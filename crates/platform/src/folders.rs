//! Bounded folder search behind the `~` / `>` query prefixes.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCWSTR, w};

use pirsa_core::FolderEntry;
use pirsa_core::util::log;

/// Deepest folder level the walk descends into.
const MAX_DEPTH: usize = 6;
/// Entries inspected before the walk gives up, so a huge profile cannot stall
/// the launcher.
const MAX_ENTRIES: usize = 20_000;
/// Matches kept for ranking.
const MAX_CANDIDATES: usize = 64;
/// Time budget for one search.
const MAX_ELAPSED: Duration = Duration::from_millis(250);

/// Folders that never hold user documents worth searching.
const SKIPPED_DIRECTORIES: &[&str] = &[
    "AppData",
    "Application Data",
    "Local Settings",
    "$Recycle.Bin",
    "OneDriveTemp",
    "node_modules",
    ".git",
    "target",
    ".cargo",
    ".rustup",
    ".nuget",
    ".gradle",
    "Windows",
    "Program Files",
    "Program Files (x86)",
];

/// Searches the user's profile for folders whose name contains `term`. Runs on
/// a blocking thread; keep the caller's expectations bounded.
pub(crate) fn search(term: &str) -> Vec<FolderEntry> {
    let needle = term.trim().to_lowercase();

    if needle.is_empty() {
        return Vec::new();
    }

    let Some(root) = std::env::var_os("USERPROFILE").map(PathBuf::from) else {
        return Vec::new();
    };

    let started = Instant::now();
    let mut found = Vec::new();
    let mut visited = 0_usize;
    let mut stack = vec![(root, 0_usize)];

    while let Some((directory, depth)) = stack.pop() {
        if depth > MAX_DEPTH
            || found.len() >= MAX_CANDIDATES
            || visited >= MAX_ENTRIES
            || started.elapsed() >= MAX_ELAPSED
        {
            break;
        }

        let Ok(entries) = std::fs::read_dir(&directory) else {
            continue;
        };

        for entry in entries.flatten() {
            visited += 1;

            if visited >= MAX_ENTRIES || found.len() >= MAX_CANDIDATES {
                break;
            }

            let Ok(file_type) = entry.file_type() else {
                continue;
            };

            // Folder search only: files are never collected.
            if !file_type.is_dir() {
                continue;
            }

            let name = entry.file_name().to_string_lossy().into_owned();

            // Hidden folders are shell bookkeeping, not something to open.
            if name.starts_with('.') {
                continue;
            }

            if !SKIPPED_DIRECTORIES
                .iter()
                .any(|skip| name.eq_ignore_ascii_case(skip))
            {
                stack.push((entry.path(), depth + 1));
            }

            if name_matches(&name, &needle) {
                found.push(FolderEntry::new(entry.path(), name));
            }
        }
    }

    log(format!(
        "folder search for {needle:?}: {} matches from {visited} entries in {:?}",
        found.len(),
        started.elapsed()
    ));

    found
}

/// Runs [`search`] on the blocking pool.
pub async fn search_async(term: String) -> Vec<FolderEntry> {
    match tokio::task::spawn_blocking(move || search(&term)).await {
        Ok(folders) => folders,
        Err(error) => {
            log(format!("folder search task failed: {error}"));
            Vec::new()
        }
    }
}

/// Opens a folder in Explorer.
pub fn open_folder(path: &Path) -> Result<(), String> {
    let target = path.to_string_lossy();
    let path = wide(&target);

    let result = unsafe {
        ShellExecuteW(
            None,
            w!("open"),
            PCWSTR(path.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };

    let code = result.0 as isize;

    if code <= 32 {
        let error = std::io::Error::last_os_error();
        return Err(format!("Windows could not open it (code {code}: {error})."));
    }

    log(format!("opened folder {}", target));
    Ok(())
}

fn name_matches(name: &str, needle: &str) -> bool {
    name.to_lowercase().contains(needle)
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn matching_ignores_case() {
        assert!(name_matches("Annual Report", "report"));
        assert!(!name_matches("notes", "report"));
    }

    #[test]
    fn blank_terms_do_not_walk_the_disk() {
        assert!(search("   ").is_empty());
    }

    #[test]
    fn shell_folders_are_skipped() {
        assert!(
            SKIPPED_DIRECTORIES
                .iter()
                .any(|name| name.eq_ignore_ascii_case("appdata"))
        );
        assert!(
            !SKIPPED_DIRECTORIES
                .iter()
                .any(|name| name.eq_ignore_ascii_case("documents"))
        );
    }

    #[test]
    fn a_search_completes_against_the_real_profile() {
        // Smoke test for the walk itself: a name that cannot exist must come
        // back empty instead of panicking or hanging.
        let found = search("pirsa-no-such-folder-4f7c1a");

        assert!(found.is_empty());
    }
}
