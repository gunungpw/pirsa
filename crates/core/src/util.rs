//! Small utilities shared by the launcher.

use std::fs::File;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::{SystemTime, UNIX_EPOCH};

/// Log files larger than this are rotated (deleted and reopened).
const MAX_LOG_BYTES: u64 = 512 * 1024;

/// The log file is opened once and reused, so a log write in the update loop
/// costs a single `write` instead of an open/metadata/close cycle.
static LOG_FILE: Mutex<Option<File>> = Mutex::new(None);
static LOG_SIZE: AtomicU64 = AtomicU64::new(0);

/// Lowercases a name and reduces every run of non-alphanumeric characters to a
/// single space. Used for de-duplication and fuzzy matching so both paths agree
/// on what "the same name" means.
#[must_use]
pub fn normalize_name(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());

    for character in value.chars() {
        if character.is_alphanumeric() {
            normalized.extend(character.to_lowercase());
        } else if !normalized.ends_with(' ') {
            normalized.push(' ');
        }
    }

    normalized.trim().to_owned()
}

/// Precomputed text used by the fuzzy ranker.
///
/// Normalizing every application name and detail on every keystroke was the
/// largest avoidable cost in the search path. `SearchText` computes the
/// normalized string, character vector, and acronym once, when the app is
/// indexed.
#[derive(Debug, Clone)]
pub struct SearchText {
    normalized: String,
    characters: Vec<char>,
    acronym: String,
    acronym_len: usize,
}

impl SearchText {
    #[must_use]
    pub fn new(value: &str) -> Self {
        let normalized = normalize_name(value);
        let characters: Vec<char> = normalized.chars().collect();
        let acronym: String = normalized
            .split_whitespace()
            .filter_map(|word| word.chars().next())
            .collect();
        let acronym_len = acronym.chars().count();

        Self {
            normalized,
            characters,
            acronym,
            acronym_len,
        }
    }

    #[must_use]
    pub fn normalized(&self) -> &str {
        &self.normalized
    }

    #[must_use]
    pub fn characters(&self) -> &[char] {
        &self.characters
    }

    #[must_use]
    pub fn acronym(&self) -> &str {
        &self.acronym
    }

    #[must_use]
    pub fn acronym_len(&self) -> usize {
        self.acronym_len
    }

    /// Number of characters in the normalized text, not bytes.
    #[must_use]
    pub fn len(&self) -> usize {
        self.characters.len()
    }

    #[must_use]
    pub fn is_empty(&self) -> bool {
        self.characters.is_empty()
    }
}

/// Start Menu entries that are support material rather than applications.
const NOISE_WORDS: &[&str] = &[
    "uninstall",
    "readme",
    "help",
    "documentation",
    "manual",
    "release notes",
    "changelog",
    "license",
    "website",
    "support",
    "repair",
    "modify",
    "remove",
];

/// Whether a normalized shortcut name is shell noise rather than an app.
///
/// Only whole leading words count, so "Helper" and "Removeable Drive" survive
/// while "Help", "Readme.txt" and "Uninstall Steam" are filtered out.
#[must_use]
pub fn is_noise_name(normalized: &str) -> bool {
    NOISE_WORDS.iter().any(|word| {
        normalized == *word
            || normalized
                .strip_prefix(word)
                .is_some_and(|rest| rest.starts_with(' '))
    })
}

/// Appends a line to the launcher log. Logging is best-effort and must never
/// take the launcher down.
pub fn log(message: impl AsRef<str>) {
    let message = message.as_ref();

    #[cfg(debug_assertions)]
    eprintln!("[pirsa] {message}");

    // Unit tests share the real user data directory; keep test output out of
    // the installed launcher's log.
    if cfg!(test) {
        return;
    }

    let Some(directory) = data_directory() else {
        return;
    };

    let _ = std::fs::create_dir_all(&directory);
    let path = directory.join("launcher.log");

    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default();
    let line = format!("[{timestamp}] {message}\n");

    let mut file = LOG_FILE
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner);

    let rotate =
        file.is_some() && LOG_SIZE.load(Ordering::Relaxed) + line.len() as u64 > MAX_LOG_BYTES;

    if rotate {
        // Drop the handle before deleting so the next open starts fresh.
        *file = None;
        let _ = std::fs::remove_file(&path);
        LOG_SIZE.store(0, Ordering::Relaxed);
    }

    if file.is_none() {
        match std::fs::OpenOptions::new()
            .create(true)
            .append(true)
            .open(&path)
        {
            Ok(opened) => {
                LOG_SIZE.store(
                    opened.metadata().map_or(0, |meta| meta.len()),
                    Ordering::Relaxed,
                );
                *file = Some(opened);
            }
            Err(_) => return,
        }
    }

    if let Some(handle) = file.as_mut()
        && handle.write_all(line.as_bytes()).is_ok()
    {
        LOG_SIZE.fetch_add(line.len() as u64, Ordering::Relaxed);
    }
}

/// The per-user data directory for the launcher.
#[must_use]
pub fn data_directory() -> Option<PathBuf> {
    std::env::var_os("LOCALAPPDATA").map(|base| PathBuf::from(base).join("Pirsa"))
}

/// Writes `body` to `temporary`, then atomically renames it over `destination`.
pub(crate) fn write_atomic(
    temporary: &Path,
    destination: &Path,
    body: &[u8],
) -> std::io::Result<()> {
    {
        let mut file = File::create(temporary)?;
        file.write_all(body)?;
        file.flush()?;
    }

    std::fs::rename(temporary, destination).inspect_err(|_| {
        let _ = std::fs::remove_file(temporary);
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names_are_lowercased_and_collapsed() {
        assert_eq!(
            normalize_name("  Visual Studio CODE  "),
            "visual studio code"
        );
        assert_eq!(normalize_name("Code::Blocks"), "code blocks");
        assert_eq!(normalize_name("Café"), "café");
        assert_eq!(normalize_name("!!!"), "");
    }

    #[test]
    fn shell_noise_is_filtered_by_whole_word() {
        assert!(is_noise_name("uninstall steam"));
        assert!(is_noise_name("help"));
        assert!(is_noise_name("readme txt"));
        assert!(is_noise_name("release notes"));
        assert!(is_noise_name("modify installation"));

        assert!(!is_noise_name("helper"));
        assert!(!is_noise_name("removeable drive"));
        assert!(!is_noise_name("notepad"));
        assert!(!is_noise_name("visual studio code"));
    }

    #[test]
    fn search_text_is_precomputed_once() {
        let text = SearchText::new("Visual Studio Code");

        assert_eq!(text.normalized(), "visual studio code");
        assert_eq!(text.acronym(), "vsc");
        assert_eq!(text.acronym_len(), 3);
        assert_eq!(text.len(), 18);
        assert_eq!(text.characters().first(), Some(&'v'));
        assert!(!text.is_empty());
        assert!(SearchText::new("!!").is_empty());
    }
}
