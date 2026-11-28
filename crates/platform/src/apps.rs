//! Start Menu application discovery and launching.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::PCWSTR;

use pirsa_core::util::{is_noise_name, log, normalize_name};
use pirsa_core::{AppEntry, LaunchTarget, ScanResult};

/// Discovers applications from the per-user and all-user Start Menu shortcut
/// folders.
///
/// Normally called by the scan worker; it is also safe to call directly from
/// tests.
pub(crate) fn scan() -> ScanResult {
    let mut apps = Vec::new();
    let mut seen = HashSet::new();
    let mut warnings = Vec::new();

    for (root, source) in shortcut_roots() {
        if !root.is_dir() {
            continue;
        }

        collect_shortcuts(&root, &root, 0, &mut apps, &mut seen, &mut warnings);
        log(format!(
            "scanned shortcut root {} ({source})",
            root.display()
        ));
    }

    apps.sort_by(|left, right| {
        left.search_key()
            .cmp(right.search_key())
            .then_with(|| left.name().cmp(right.name()))
    });

    ScanResult {
        apps,
        warning: if warnings.is_empty() {
            None
        } else {
            Some(warnings.join(" "))
        },
    }
}

/// Launches an application through the Windows shell.
pub fn launch(app: &AppEntry) -> Result<(), String> {
    let target = match app.target() {
        LaunchTarget::Shortcut(path) | LaunchTarget::Executable(path) => {
            path.to_string_lossy().into_owned()
        }
        LaunchTarget::Shell(target) => target.clone(),
    };

    let operation = wide("open");
    let file = wide(&target);

    let result = unsafe {
        windows::Win32::UI::Shell::ShellExecuteW(
            None,
            PCWSTR(operation.as_ptr()),
            PCWSTR(file.as_ptr()),
            PCWSTR::null(),
            PCWSTR::null(),
            SW_SHOWNORMAL,
        )
    };

    let code = result.0 as isize;
    if code <= 32 {
        let error = std::io::Error::last_os_error();
        return Err(format!(
            "Windows could not start it (code {code}: {error})."
        ));
    }

    log(format!("launched {} via {}", app.name(), target));
    Ok(())
}

fn shortcut_roots() -> Vec<(PathBuf, &'static str)> {
    let mut roots = Vec::new();

    if let Some(appdata) = std::env::var_os("APPDATA") {
        roots.push((
            PathBuf::from(appdata)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
            "user",
        ));
    }

    if let Some(programdata) = std::env::var_os("PROGRAMDATA") {
        roots.push((
            PathBuf::from(programdata)
                .join("Microsoft")
                .join("Windows")
                .join("Start Menu")
                .join("Programs"),
            "common",
        ));
    }

    roots
}

fn collect_shortcuts(
    directory: &Path,
    root: &Path,
    depth: usize,
    apps: &mut Vec<AppEntry>,
    seen: &mut HashSet<String>,
    warnings: &mut Vec<String>,
) {
    if depth > 6 {
        return;
    }

    let entries = match std::fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) => {
            warnings.push(format!("Could not read {}: {error}", directory.display()));
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_dir() {
            collect_shortcuts(&path, root, depth + 1, apps, seen, warnings);
            continue;
        }

        if !file_type.is_file() {
            continue;
        }

        let extension = path
            .extension()
            .and_then(|extension| extension.to_str())
            .unwrap_or_default();

        if !extension.eq_ignore_ascii_case("lnk") && !extension.eq_ignore_ascii_case("url") {
            continue;
        }

        let Some(name) = path
            .file_stem()
            .and_then(|name| name.to_str())
            .map(str::trim)
            .filter(|name| !name.is_empty() && !name.eq_ignore_ascii_case("desktop"))
        else {
            continue;
        };

        let key = normalize_name(name);

        // "Uninstall Steam", "Readme", "Website" and friends are support
        // material, not applications worth a row.
        if key.is_empty() || is_noise_name(&key) || !seen.insert(key) {
            continue;
        }

        let category = path
            .parent()
            .and_then(|parent| parent.strip_prefix(root).ok())
            .and_then(|relative| relative.components().next_back())
            .and_then(|component| component.as_os_str().to_str())
            .filter(|category| !category.is_empty())
            .map_or_else(
                || "Start Menu".to_owned(),
                |category| format!("Start Menu · {category}"),
            );

        apps.push(AppEntry::new(
            name.to_owned(),
            category,
            LaunchTarget::Shortcut(path),
        ));
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
