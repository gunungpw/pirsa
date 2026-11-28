//! Shared application models used by the core, platform, and UI crates.

use std::path::PathBuf;

use crate::util::SearchText;

/// Where an application should be launched from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LaunchTarget {
    /// A `.lnk` or `.url` file in a Start Menu folder.
    Shortcut(PathBuf),
    /// An executable path.
    Executable(PathBuf),
    /// A shell target, such as an AppUserModelID or `shell:` path.
    Shell(String),
}

/// A searchable application.
#[derive(Debug, Clone)]
pub struct AppEntry {
    name: String,
    detail: String,
    target: LaunchTarget,
    name_search: SearchText,
    detail_search: SearchText,
}

impl AppEntry {
    /// Builds an entry and precomputes the text used by search.
    #[must_use]
    pub fn new(name: String, detail: String, target: LaunchTarget) -> Self {
        let name_search = SearchText::new(&name);
        let detail_search = SearchText::new(&detail);

        Self {
            name,
            detail,
            target,
            name_search,
            detail_search,
        }
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn detail(&self) -> &str {
        &self.detail
    }

    #[must_use]
    pub fn target(&self) -> &LaunchTarget {
        &self.target
    }

    /// Normalized name used for de-duplication, usage history, and searching.
    #[must_use]
    pub fn search_key(&self) -> &str {
        self.name_search.normalized()
    }

    /// Precomputed text for the display name.
    #[must_use]
    pub fn name_search(&self) -> &SearchText {
        &self.name_search
    }

    /// Precomputed text for the category/kind detail.
    #[must_use]
    pub fn detail_search(&self) -> &SearchText {
        &self.detail_search
    }
}

/// A file or folder returned by the durable file index.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FileEntry {
    path: PathBuf,
    name: String,
    is_dir: bool,
    size: u64,
    modified: u64,
    usage_key: String,
}

impl FileEntry {
    #[must_use]
    pub fn new(
        path: PathBuf,
        name: String,
        is_dir: bool,
        size: u64,
        modified: u64,
        usage_key: String,
    ) -> Self {
        Self {
            path,
            name,
            is_dir,
            size,
            modified,
            usage_key,
        }
    }

    #[must_use]
    pub fn path(&self) -> &std::path::Path {
        &self.path
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    #[must_use]
    pub fn is_dir(&self) -> bool {
        self.is_dir
    }

    #[must_use]
    pub fn size(&self) -> u64 {
        self.size
    }

    #[must_use]
    pub fn modified(&self) -> u64 {
        self.modified
    }

    /// Stable key used by the persistent usage table.
    #[must_use]
    pub fn usage_key(&self) -> &str {
        &self.usage_key
    }
}

/// Result of a Start Menu scan.
#[derive(Debug, Default, Clone)]
pub struct ScanResult {
    pub apps: Vec<AppEntry>,
    pub warning: Option<String>,
}

/// A decoded RGBA icon bitmap.
#[derive(Debug, Clone)]
pub struct Icon {
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Straight (non-premultiplied) RGBA pixels, rows from the top.
    pub rgba: Vec<u8>,
}

/// Whether the global hotkey could be installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HotkeyStatus {
    Pending = 0,
    Ready = 1,
    Unavailable = 2,
}

impl HotkeyStatus {
    #[must_use]
    pub fn available(self) -> bool {
        self == Self::Ready
    }
}
