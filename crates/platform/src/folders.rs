//! Opening files and folders in their default Windows handler.

use std::path::Path;

use windows::Win32::UI::Shell::ShellExecuteW;
use windows::Win32::UI::WindowsAndMessaging::SW_SHOWNORMAL;
use windows::core::{PCWSTR, w};

use pirsa_core::util::log;

/// Opens a file or folder with its default Windows handler.
pub fn open_path(path: &Path) -> Result<(), String> {
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

    log(format!("opened {target}"));
    Ok(())
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}
