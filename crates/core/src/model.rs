//! Shared application models used by the core, platform, and UI crates.

use std::path::PathBuf;

use crate::util::SearchText;

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
    name: String,
    detail: String,
    target: LaunchTarget,
    name_search: SearchText,
    detail_search: SearchText,
}

impl AppEntry {
    /// Builds an entry and precomputes the text used by search.
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

    pub fn name(&self) -> &str {
        &self.name
    }

    pub fn detail(&self) -> &str {
        &self.detail
    }

    pub fn target(&self) -> &LaunchTarget {
        &self.target
    }

    /// Normalized name used for de-duplication, usage history, and searching.
    pub fn search_key(&self) -> &str {
        self.name_search.normalized()
    }

    /// Precomputed text for the display name.
    pub fn name_search(&self) -> &SearchText {
        &self.name_search
    }

    /// Precomputed text for the category/kind detail.
    pub fn detail_search(&self) -> &SearchText {
        &self.detail_search
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
    pub fn available(self) -> bool {
        self == Self::Ready
    }
}
