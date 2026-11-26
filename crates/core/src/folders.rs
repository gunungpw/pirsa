//! Folder matches for the launcher's search field.
//!
//! Folder search is not a separate mode: every query matches folders as well
//! as apps, and [`crate::results::merge`] interleaves both into one list.

use std::path::{Path, PathBuf};

/// A folder found by the filesystem search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderEntry {
    path: PathBuf,
    name: String,
}

impl FolderEntry {
    #[must_use]
    pub fn new(path: PathBuf, name: String) -> Self {
        Self { path, name }
    }

    #[must_use]
    pub fn path(&self) -> &Path {
        &self.path
    }

    #[must_use]
    pub fn name(&self) -> &str {
        &self.name
    }

    /// Short trailing part of the containing folder, for the result row.
    #[must_use]
    pub fn parent_label(&self) -> String {
        let Some(parent) = self.path.parent() else {
            return String::new();
        };

        let components: Vec<&str> = parent
            .components()
            .filter_map(|component| component.as_os_str().to_str())
            .collect();

        match components.as_slice() {
            [] => String::new(),
            [only] => (*only).to_owned(),
            [.., second_last, last] => {
                format!("{second_last}{sep}{last}", sep = std::path::MAIN_SEPARATOR)
            }
        }
    }
}

/// Folder matches with their scores, best first.
///
/// The scores sit on the same scale as [`crate::search::rank_scores`] — exact
/// matches beat prefixes, which beat substrings — so both kinds of result can
/// share one ranked list. Names that only match case-insensitively as a
/// subsequence are left to the app ranker; a folder row has to read as a real
/// name match.
#[must_use]
pub fn rank_scores(folders: &[FolderEntry], term: &str) -> Vec<(i32, usize)> {
    let needle = term.trim().to_lowercase();

    if needle.is_empty() {
        return Vec::new();
    }

    let needle_len = needle.chars().count() as i32;
    let mut matches: Vec<(i32, usize)> = folders
        .iter()
        .enumerate()
        .filter_map(|(index, folder)| {
            let name = folder.name.to_lowercase();
            let score = if name == needle {
                2_000
            } else if name.starts_with(&needle) {
                let extra = name.chars().count() as i32 - needle_len;
                1_500 - extra.min(300)
            } else {
                let position = name.find(&needle)?;
                1_050 - position as i32 * 4
            };

            Some((score, index))
        })
        .collect();

    matches.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));

    matches
}

#[cfg(test)]
mod tests {
    use super::*;

    fn entry(path: &str) -> FolderEntry {
        let path = PathBuf::from(path);
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().into_owned())
            .unwrap_or_default();

        FolderEntry::new(path, name)
    }

    #[test]
    fn ranking_prefers_exact_then_prefix_then_substring() {
        let folders = vec![
            entry(r"C:\Users\me\annual-reports"),
            entry(r"C:\Users\me\reports"),
            entry(r"C:\Users\me\reports-archive"),
        ];

        let ranked: Vec<usize> = rank_scores(&folders, "reports")
            .into_iter()
            .map(|(_, index)| index)
            .collect();

        assert_eq!(ranked, vec![1, 2, 0]);
    }

    #[test]
    fn scores_are_comparable_with_the_app_ranker() {
        let folders = vec![entry(r"C:\Users\me\reports")];

        // Same tiers as `search::rank_scores`: exact, prefix, substring.
        assert_eq!(rank_scores(&folders, "reports"), vec![(2_000, 0)]);
        assert_eq!(rank_scores(&folders, "repo"), vec![(1_500 - 3, 0)]);
        assert_eq!(rank_scores(&folders, "ports"), vec![(1_050 - 2 * 4, 0)]);
    }

    #[test]
    fn unrelated_folders_are_dropped() {
        let folders = vec![entry(r"C:\Users\me\notes")];

        assert!(rank_scores(&folders, "zzz").is_empty());
    }

    #[test]
    fn an_empty_term_matches_nothing() {
        let folders = vec![entry(r"C:\Users\me\beta"), entry(r"C:\Users\me\alpha")];

        assert!(rank_scores(&folders, "  ").is_empty());
    }

    #[test]
    fn parent_label_keeps_the_last_two_folders() {
        let folder = entry("C:\\Users\\me\\Documents\\Projects");

        assert_eq!(
            folder.parent_label(),
            format!("me{sep}Documents", sep = std::path::MAIN_SEPARATOR)
        );
    }
}
