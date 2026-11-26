//! Native folder picker used by the search settings.

use std::path::PathBuf;

use pirsa_core::util::log;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::UI::Shell::{
    FOS_FORCEFILESYSTEM, FOS_PICKFOLDERS, FileOpenDialog, IFileOpenDialog, SIGDN_FILESYSPATH,
};

use super::com::ComApartment;

/// Shows the Windows folder picker and returns the chosen folder.
///
/// Must run on a thread with its own STA apartment; the caller drives it from
/// the async task, which uses a blocking thread.
pub fn pick() -> Option<PathBuf> {
    let _com = ComApartment::new();

    unsafe {
        let dialog: IFileOpenDialog =
            CoCreateInstance(&FileOpenDialog, None, CLSCTX_INPROC_SERVER).ok()?;

        let options = dialog.GetOptions().ok()?;
        dialog
            .SetOptions(options | FOS_PICKFOLDERS | FOS_FORCEFILESYSTEM)
            .ok()?;
        dialog
            .SetTitle(windows::core::w!("Choose a folder to search"))
            .ok()?;
        dialog.Show(None).ok()?;

        let item = dialog.GetResult().ok()?;
        let name = item.GetDisplayName(SIGDN_FILESYSPATH).ok()?;
        let text = name.to_string().ok()?;

        CoTaskMemFree(Some(name.0 as *const core::ffi::c_void));

        Some(PathBuf::from(text))
    }
}

/// Async wrapper around [`pick`].
pub async fn pick_async() -> Option<PathBuf> {
    match tokio::task::spawn_blocking(pick).await {
        Ok(folder) => folder,
        Err(error) => {
            log(format!("folder picker failed: {error}"));
            None
        }
    }
}
