//! Persistent launch history.
//!
//! Remembering how often an application is launched lets the launcher surface
//! frequently used apps first in the unfiltered list and lets them win close
//! search ties. Only a launch count and the last launch time are stored, keyed
//! by the normalized application name.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

use crate::util;

/// File name inside the launcher's data directory.
const FILE_NAME: &str = "usage.txt";
/// History is capped so the file stays small and loading stays instant.
const MAX_ENTRIES: usize = 500;
/// Launch count that earns the maximum ranking bonus.
const BONUS_LAUNCHES: u32 = 10;
/// Points added per launch. Capped so relevance still dominates ranking: this
/// only decides matches that are already close.
const BONUS_PER_LAUNCH: i32 = 8;

/// What is remembered about a single application.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct UsageEntry {
    /// Successful launches, saturating at [`u32::MAX`].
    pub launches: u32,
    /// Unix timestamp in seconds of the most recent launch.
    pub last_used: u64,
}

impl UsageEntry {
    /// Ranking bonus for this entry, between 0 and
    /// `BONUS_LAUNCHES * BONUS_PER_LAUNCH`.
    pub fn score_bonus(&self) -> i32 {
        self.launches.min(BONUS_LAUNCHES) as i32 * BONUS_PER_LAUNCH
    }
}

/// In-memory launch history.
#[derive(Debug, Clone, Default)]
pub struct Usage {
    entries: HashMap<String, UsageEntry>,
}

impl Usage {
    /// Loads the history from `%LOCALAPPDATA%\Pirsa\usage.txt`.
    ///
    /// A missing, unreadable, or damaged file simply yields empty history:
    /// usage tracking must never take the launcher down.
    pub fn load() -> Self {
        Self::load_from(&history_path())
    }

    /// Loads history from an explicit path.
    pub fn load_from(path: &Path) -> Self {
        let mut usage = Self::default();

        let Ok(contents) = std::fs::read_to_string(path) else {
            return usage;
        };

        for line in contents.lines() {
            let line = line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let mut fields = line.splitn(3, '\t');
            let (Some(launches), Some(last_used), Some(key)) =
                (fields.next(), fields.next(), fields.next())
            else {
                continue;
            };

            let (Ok(launches), Ok(last_used)) = (
                launches.trim().parse::<u32>(),
                last_used.trim().parse::<u64>(),
            ) else {
                continue;
            };

            usage.insert(
                key,
                UsageEntry {
                    launches,
                    last_used,
                },
            );
        }

        usage
    }

    /// Persists the history to `%LOCALAPPDATA%\Pirsa\usage.txt`.
    pub fn save(&self) {
        self.save_to(&history_path());
    }

    /// Persists the history to an explicit path, replacing it atomically.
    pub fn save_to(&self, path: &Path) {
        let mut entries: Vec<(&str, UsageEntry)> = self
            .entries
            .iter()
            .filter(|(_, entry)| entry.launches > 0)
            .map(|(key, entry)| (key.as_str(), *entry))
            .collect();

        entries.sort_by(|left, right| {
            right
                .1
                .launches
                .cmp(&left.1.launches)
                .then_with(|| right.1.last_used.cmp(&left.1.last_used))
                .then_with(|| left.0.cmp(right.0))
        });
        entries.truncate(MAX_ENTRIES);

        let mut body = String::from("# Pirsa usage history: launches<TAB>last-used<TAB>app\n");

        for (key, entry) in entries {
            body.push_str(&format!(
                "{}\t{}\t{}\n",
                entry.launches, entry.last_used, key
            ));
        }

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let temporary = path.with_extension("tmp");

        if let Err(error) = util::write_atomic(&temporary, path, body.as_bytes()) {
            util::log(format!("could not save usage history: {error}"));
        }
    }

    /// Remembers one successful launch and returns the updated entry.
    pub fn record(&mut self, key: &str) -> UsageEntry {
        let entry = UsageEntry {
            launches: self.entry(key).launches.saturating_add(1),
            last_used: now(),
        };

        self.insert(key, entry);
        entry
    }

    /// Stores an entry, keeping the busiest history when a key repeats.
    pub fn insert(&mut self, key: &str, entry: UsageEntry) {
        let key = key.trim();

        if key.is_empty() || entry.launches == 0 {
            return;
        }

        self.entries
            .entry(key.to_owned())
            .and_modify(|existing| {
                existing.launches = existing.launches.max(entry.launches);
                existing.last_used = existing.last_used.max(entry.last_used);
            })
            .or_insert(entry);
    }

    /// The remembered history for `key`, or empty history when unknown.
    pub fn entry(&self, key: &str) -> UsageEntry {
        self.entries.get(key).copied().unwrap_or_default()
    }
}

fn history_path() -> PathBuf {
    util::data_directory()
        .map(|directory| directory.join(FILE_NAME))
        .unwrap_or_else(|| PathBuf::from(FILE_NAME))
}

fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|duration| duration.as_secs())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_path() -> PathBuf {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!("pirsa-usage-{}-{unique}.txt", std::process::id()))
    }

    #[test]
    fn missing_file_starts_empty() {
        let path = temp_path();
        let usage = Usage::load_from(&path);

        assert_eq!(usage.entry("notepad"), UsageEntry::default());
        assert_eq!(usage.entry("notepad").score_bonus(), 0);
    }

    #[test]
    fn history_round_trips() {
        let path = temp_path();
        let mut usage = Usage::default();

        usage.record("visual studio code");
        usage.record("visual studio code");
        usage.record("notepad");
        usage.save_to(&path);

        let loaded = Usage::load_from(&path);

        assert_eq!(loaded.entry("visual studio code").launches, 2);
        assert_eq!(
            loaded.entry("visual studio code").last_used,
            usage.entry("visual studio code").last_used
        );
        assert_eq!(loaded.entry("notepad").launches, 1);
        assert_eq!(loaded.entry("unknown"), UsageEntry::default());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn duplicate_entries_keep_the_busiest_history() {
        let mut usage = Usage::default();

        usage.insert(
            "notepad",
            UsageEntry {
                launches: 2,
                last_used: 10,
            },
        );
        usage.insert(
            "notepad",
            UsageEntry {
                launches: 5,
                last_used: 4,
            },
        );

        assert_eq!(
            usage.entry("notepad"),
            UsageEntry {
                launches: 5,
                last_used: 10
            }
        );
    }

    #[test]
    fn empty_keys_are_ignored() {
        let mut usage = Usage::default();

        usage.record("");
        usage.record("   ");

        assert_eq!(usage.entry(""), UsageEntry::default());
        assert_eq!(usage.entry("   "), UsageEntry::default());
    }

    #[test]
    fn damaged_lines_are_ignored() {
        let path = temp_path();
        std::fs::write(
            &path,
            "garbage\n# comment\n2\t99\tnotepad\n1\tnope\tbroken\n\n4\t7\t\n",
        )
        .unwrap();

        let usage = Usage::load_from(&path);

        assert_eq!(
            usage.entry("notepad"),
            UsageEntry {
                launches: 2,
                last_used: 99
            }
        );
        assert_eq!(usage.entry("broken"), UsageEntry::default());

        let _ = std::fs::remove_file(&path);
    }

    #[test]
    fn bonus_is_bounded() {
        let mut usage = Usage::default();
        usage.insert(
            "notepad",
            UsageEntry {
                launches: 10_000,
                last_used: 1,
            },
        );

        assert_eq!(
            usage.entry("notepad").score_bonus(),
            BONUS_LAUNCHES as i32 * BONUS_PER_LAUNCH
        );
        assert_eq!(usage.entry("missing").score_bonus(), 0);
    }
}
