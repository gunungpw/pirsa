//! Bounded folder search used while the folder index is still building.

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCWSTR, w};

use pirsa_core::FolderEntry;
use pirsa_core::search_scope::SearchScope;
use pirsa_core::util::log;

/// Entries inspected before the walk gives up, so a large selection cannot
/// stall the launcher.
const MAX_ENTRIES: usize = 20_000;
/// Matches kept for ranking.
const MAX_CANDIDATES: usize = 64;
/// Time budget for one search.
const MAX_ELAPSED: Duration = Duration::from_millis(250);

/// System locations never searched, whatever the scope says.
const SYSTEM_DIRECTORIES: &[&str] = &[
    "$recycle.bin",
    "system volume information",
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "recovery",
    "perflogs",
    "msocache",
];

/// Searches every root the scope selects for folders whose name contains
/// `term`. Runs on a blocking thread; the budget keeps it responsive even when
/// the scope covers whole drives.
pub(crate) fn search(scope: &SearchScope, term: &str) -> Vec<FolderEntry> {
    let needle = term.trim().to_lowercase();

    if needle.is_empty() {
        return Vec::new();
    }

    let Some(profile) = std::env::var_os("USERPROFILE").map(PathBuf::from) else {
        return Vec::new();
    };

    let roots = scope.roots(&profile, &crate::drives::infos());
    let started = Instant::now();
    let mut found = Vec::new();
    let mut visited = 0_usize;

    for root in roots {
        if found.len() >= MAX_CANDIDATES
            || visited >= MAX_ENTRIES
            || started.elapsed() >= MAX_ELAPSED
        {
            break;
        }

        let mut stack = vec![(root, 0_usize)];

        while let Some((directory, depth)) = stack.pop() {
            if depth >= scope.depth
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
                let path = entry.path();

                if scope.is_hidden_skipped(&name)
                    || is_system_directory(&name)
                    || scope.is_excluded(&path)
                {
                    continue;
                }

                if name_matches(&name, &needle) {
                    found.push(FolderEntry::new(path.clone(), name));
                }

                stack.push((path, depth + 1));
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
pub async fn search_async(scope: SearchScope, term: String) -> Vec<FolderEntry> {
    match tokio::task::spawn_blocking(move || search(&scope, &term)).await {
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

    log(format!("opened folder {target}"));
    Ok(())
}

pub(crate) fn is_system_directory(name: &str) -> bool {
    SYSTEM_DIRECTORIES
        .iter()
        .any(|system| name.eq_ignore_ascii_case(system))
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
    fn system_directories_are_always_skipped() {
        assert!(is_system_directory("Windows"));
        assert!(is_system_directory("$Recycle.Bin"));
        assert!(is_system_directory("Program Files (x86)"));
        assert!(!is_system_directory("Projects"));
    }

    #[test]
    fn blank_terms_do_not_walk_the_disk() {
        assert_eq!(
            search(&SearchScope::default(), "   "),
            [] as [FolderEntry; 0]
        );
    }

    #[test]
    fn the_default_scope_completes_against_the_real_profile() {
        // Smoke test for the walk itself: a name that cannot exist must come
        // back empty instead of panicking or hanging.
        let found = search(&SearchScope::default(), "pirsa-no-such-folder-4f7c1a");

        assert_eq!(found, [] as [FolderEntry; 0]);
    }
}
