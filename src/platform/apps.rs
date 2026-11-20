//! Start Menu application discovery and launching.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};
use windows::Win32::UI::Shell::Common::{ITEMIDLIST, STRRET};
use windows::Win32::UI::Shell::{
    ILFree, IShellFolder, SHCONTF_FOLDERS, SHCONTF_NONFOLDERS, SHGDN_FORPARSING, SHGDN_NORMAL,
    SHGetDesktopFolder, SHParseDisplayName, StrRetToBufW,
};
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCWSTR, w};

use crate::util::log;

/// Where an application should be launched from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchTarget {
    /// A `.lnk` or `.url` file in a Start Menu folder.
    Shortcut(PathBuf),
    /// An executable, usually reported by the AppsFolder namespace.
    Executable(PathBuf),
    /// A shell path such as `shell:AppsFolder\Package!App`.
    Shell(String),
}

/// A searchable application.
#[derive(Debug, Clone)]
pub struct AppEntry {
    pub name: String,
    pub detail: String,
    pub target: LaunchTarget,
}

impl AppEntry {
    /// Normalized name used for de-duplication and searching.
    pub fn search_key(&self) -> String {
        normalize_name(&self.name)
    }
}

/// Result of a Start Menu scan.
#[derive(Debug, Default, Clone)]
pub struct ScanResult {
    pub apps: Vec<AppEntry>,
    pub warning: Option<String>,
}

/// Discovers Start Menu applications from both shortcut folders and the
/// Windows `AppsFolder` shell namespace (which includes packaged/UWP apps).
pub fn scan() -> ScanResult {
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

    let shortcut_count = apps.len();

    match scan_apps_folder() {
        Ok(folder_apps) => {
            let mut added = 0;

            for app in folder_apps {
                let key = app.search_key();
                if key.is_empty() || !seen.insert(key) {
                    continue;
                }

                apps.push(app);
                added += 1;
            }

            log(format!(
                "AppsFolder added {added} apps ({shortcut_count} shortcuts found)"
            ));
        }
        Err(error) => {
            warnings.push(format!("Windows apps could not be indexed: {error}"));
            log(format!("AppsFolder scan failed: {error}"));
        }
    }

    apps.sort_by(|left, right| {
        left.name
            .to_lowercase()
            .cmp(&right.name.to_lowercase())
            .then_with(|| left.name.cmp(&right.name))
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
    let target = match &app.target {
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

    log(format!("launched {} via {}", app.name, target));
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
        if key.is_empty() || !seen.insert(key) {
            continue;
        }

        let category = path
            .parent()
            .and_then(|parent| parent.strip_prefix(root).ok())
            .and_then(|relative| relative.components().next_back())
            .and_then(|component| component.as_os_str().to_str())
            .filter(|category| !category.is_empty())
            .map(|category| format!("Start Menu · {category}"))
            .unwrap_or_else(|| "Start Menu".to_owned());

        apps.push(AppEntry {
            name: name.to_owned(),
            detail: category,
            target: LaunchTarget::Shortcut(path),
        });
    }
}

fn scan_apps_folder() -> Result<Vec<AppEntry>, String> {
    let initialized = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) };
    let should_uninitialize = initialized.0 >= 0;

    let result = scan_apps_folder_inner();

    if should_uninitialize {
        unsafe { CoUninitialize() };
    }

    result
}

fn scan_apps_folder_inner() -> Result<Vec<AppEntry>, String> {
    unsafe {
        let desktop = SHGetDesktopFolder().map_err(|error| error.to_string())?;

        let mut folder_pidl: *mut ITEMIDLIST = std::ptr::null_mut();
        SHParseDisplayName(w!("shell:AppsFolder"), None, &mut folder_pidl, 0, None)
            .map_err(|error| error.to_string())?;

        if folder_pidl.is_null() {
            return Err("AppsFolder returned an empty namespace.".to_owned());
        }

        let folder_result: Result<IShellFolder, _> = desktop.BindToObject(folder_pidl, None);
        ILFree(Some(folder_pidl));
        let folder = folder_result.map_err(|error| error.to_string())?;

        let mut enumerator = None;
        let flags = (SHCONTF_FOLDERS.0 | SHCONTF_NONFOLDERS.0) as u32;
        let enumeration = folder.EnumObjects(HWND::default(), flags, &mut enumerator);

        if enumeration.is_err() {
            return Err(format!(
                "Could not enumerate AppsFolder (HRESULT {:#x}).",
                enumeration.0
            ));
        }

        let enumerator =
            enumerator.ok_or_else(|| "AppsFolder enumeration was empty.".to_owned())?;
        let mut apps = Vec::new();
        let mut seen = HashSet::new();
        let mut enumerated = 0;

        for _ in 0..5000 {
            let mut item: *mut ITEMIDLIST = std::ptr::null_mut();
            let mut item_slice = [item];
            let next = enumerator.Next(&mut item_slice, None);
            item = item_slice[0];

            if item.is_null() || next.is_err() {
                break;
            }

            enumerated += 1;

            // Child PIDLs from EnumObjects are relative to the AppsFolder, so
            // GetDisplayNameOf is the correct API here (SHGetNameFromIDList
            // expects an absolute PIDL).
            let display_name = folder_display_name(&folder, item, SHGDN_NORMAL);
            let parsing_name = folder_display_name(&folder, item, SHGDN_FORPARSING);

            ILFree(Some(item));

            let (Some(display_name), Some(parsing_name)) = (display_name, parsing_name) else {
                continue;
            };

            let display_name = display_name.trim();
            let parsing_name = parsing_name.trim();

            if display_name.is_empty()
                || parsing_name.is_empty()
                || display_name.starts_with('@')
                || parsing_name.starts_with("http:")
                || parsing_name.starts_with("https:")
                || parsing_name.starts_with("mailto:")
            {
                continue;
            }

            let key = normalize_name(display_name);
            if key.is_empty() || !seen.insert(key) {
                continue;
            }

            let (target, detail) = classify_apps_folder_target(parsing_name);

            apps.push(AppEntry {
                name: display_name.to_owned(),
                detail,
                target,
            });
        }

        log(format!(
            "enumerated {enumerated} AppsFolder items, keeping {}",
            apps.len()
        ));

        Ok(apps)
    }
}

fn classify_apps_folder_target(parsing_name: &str) -> (LaunchTarget, String) {
    if parsing_name.starts_with("shell:") {
        return (
            LaunchTarget::Shell(parsing_name.to_owned()),
            "Windows app".to_owned(),
        );
    }

    let path = Path::new(parsing_name);

    if path.is_absolute() {
        let detail = if path.is_file() {
            "Desktop app"
        } else {
            "Windows app"
        };

        return (
            LaunchTarget::Executable(path.to_path_buf()),
            detail.to_owned(),
        );
    }

    (
        LaunchTarget::Shell(format!("shell:AppsFolder\\{parsing_name}")),
        "Windows app".to_owned(),
    )
}

fn folder_display_name(
    folder: &IShellFolder,
    pidl: *const ITEMIDLIST,
    flags: windows::Win32::UI::Shell::SHGDNF,
) -> Option<String> {
    unsafe {
        let mut strret = STRRET::default();
        folder.GetDisplayNameOf(pidl, flags, &mut strret).ok()?;

        let mut buffer = [0_u16; 1024];
        StrRetToBufW(&mut strret, Some(pidl), &mut buffer).ok()?;

        let length = buffer
            .iter()
            .position(|character| *character == 0)
            .unwrap_or(buffer.len());

        String::from_utf16(&buffer[..length]).ok()
    }
}

fn normalize_name(name: &str) -> String {
    let mut normalized = String::with_capacity(name.len());

    for character in name.chars() {
        if character.is_alphanumeric() {
            normalized.extend(character.to_lowercase());
        } else if !normalized.ends_with(' ') {
            normalized.push(' ');
        }
    }

    normalized.trim().to_owned()
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
