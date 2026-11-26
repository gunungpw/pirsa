//! Folder search triggered from the launcher's search field.
//!
//! Typing `~projects` or `>projects` switches the launcher from app search to
//! folder search. Both prefixes are aliases: `~` reads as "home directory" and
//! `>` reads as "filesystem", and neither is ever part of the search term.

use std::path::{Path, PathBuf};

/// Prefix that switches the search field into folder mode.
const PREFIXES: [char; 2] = ['~', '>'];

/// A folder found by the filesystem search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderEntry {
    path: PathBuf,
    name: String,
}

impl FolderEntry {
    pub fn new(path: PathBuf, name: String) -> Self {
        Self { path, name }
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    pub fn name(&self) -> &str {
        &self.name
    }

    /// Short trailing part of the containing folder, for the result row.
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

/// A parsed `~term` / `>term` query.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FolderQuery {
    prefix: char,
    term: String,
}

impl FolderQuery {
    pub fn prefix(&self) -> char {
        self.prefix
    }

    pub fn term(&self) -> &str {
        &self.term
    }

    pub fn is_empty(&self) -> bool {
        self.term.is_empty()
    }
}

/// Parses a folder-mode query, or `None` when the field holds an app query.
///
/// The prefix only counts when it is the first non-space character; `a ~ b`
/// stays an app search.
pub fn parse_query(input: &str) -> Option<FolderQuery> {
    let trimmed = input.trim_start();
    let mut characters = trimmed.chars();
    let prefix = characters.next()?;

    if !PREFIXES.contains(&prefix) {
        return None;
    }

    Some(FolderQuery {
        prefix,
        term: characters.as_str().trim().to_owned(),
    })
}

/// Orders folder matches from best to worst for `term`.
///
/// An empty term keeps the order the filesystem walk produced, so `~` alone
/// lists nothing and shows the hint instead.
pub fn rank(folders: &[FolderEntry], term: &str) -> Vec<usize> {
    let needle = term.trim().to_lowercase();

    if needle.is_empty() {
        return (0..folders.len()).collect();
    }

    let mut matches: Vec<(i32, usize)> = folders
        .iter()
        .enumerate()
        .filter_map(|(index, folder)| {
            let name = folder.name.to_lowercase();
            let score = if name == needle {
                1_200
            } else if name.starts_with(&needle) {
                1_000 - name.chars().count().saturating_sub(needle.chars().count()) as i32
            } else {
                let position = name.find(&needle)?;
                600 - position as i32 * 4
            };

            Some((score, index))
        })
        .collect();

    matches.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));

    matches.into_iter().map(|(_, index)| index).collect()
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
    fn tilde_and_angle_prefixes_switch_to_folder_mode() {
        let tilde = parse_query("~projects").expect("~ is a folder prefix");
        assert_eq!(tilde.prefix(), '~');
        assert_eq!(tilde.term(), "projects");

        let angle = parse_query("> notes").expect("> is a folder prefix");
        assert_eq!(angle.prefix(), '>');
        assert_eq!(angle.term(), "notes");

        assert_eq!(parse_query("  ~ spaced").unwrap().term(), "spaced");
    }

    #[test]
    fn plain_queries_stay_app_queries() {
        assert!(parse_query("projects").is_none());
        assert!(parse_query("a ~ b").is_none());
        assert!(parse_query("").is_none());
        assert!(parse_query("   ").is_none());
    }

    #[test]
    fn an_empty_term_is_a_valid_folder_query() {
        let query = parse_query("~").expect("a bare prefix is still folder mode");
        assert!(query.is_empty());
    }

    #[test]
    fn ranking_prefers_exact_then_prefix_then_substring() {
        let folders = vec![
            entry("C:\\Users\\me\\annual-reports"),
            entry("C:\\Users\\me\\reports"),
            entry("C:\\Users\\me\\reports-archive"),
        ];

        assert_eq!(rank(&folders, "reports"), vec![1, 2, 0]);
    }

    #[test]
    fn unrelated_folders_are_dropped() {
        let folders = vec![entry("C:\\Users\\me\\notes")];

        assert!(rank(&folders, "zzz").is_empty());
    }

    #[test]
    fn empty_term_keeps_walk_order() {
        let folders = vec![entry("C:\\Users\\me\\beta"), entry("C:\\Users\\me\\alpha")];

        assert_eq!(rank(&folders, "  "), vec![0, 1]);
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
