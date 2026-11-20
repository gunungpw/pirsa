//! Small utilities shared by the launcher.

use std::io::Write;
use std::path::PathBuf;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

static LOG_LOCK: Mutex<()> = Mutex::new(());

/// Appends a line to the launcher log. Logging is best-effort and must never
/// take the launcher down.
pub fn log(message: impl AsRef<str>) {
    let message = message.as_ref();
    let _guard = LOG_LOCK
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner());

    #[cfg(debug_assertions)]
    eprintln!("[pirsa] {message}");

    let Some(directory) = log_directory() else {
        return;
    };

    let _ = std::fs::create_dir_all(&directory);
    let path = directory.join("launcher.log");

    if let Ok(metadata) = std::fs::metadata(&path)
        && metadata.len() > 512 * 1024
    {
        let _ = std::fs::remove_file(&path);
    }

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();

    if let Ok(mut file) = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
    {
        let _ = writeln!(file, "[{timestamp}] {message}");
    }
}

/// The per-user data directory for the launcher.
pub fn data_directory() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join("Pirsa"))
}

fn log_directory() -> Option<PathBuf> {
    data_directory()
}
