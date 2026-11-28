//! File-search trigger parsing and path labels.
//!
//! File search is a separate mode selected by the trigger prefix stored in
//! [`crate::search_scope::SearchScope`]. [`folder_term`] strips that prefix;
//! the Turso file index does the matching and ranking.

use std::path::Path;

/// Returns the file-search term when `query` starts with `trigger`.
///
/// `None` means the query is for apps. A bare trigger returns `Some("")`, so
/// the launcher can show a hint instead of searching for nothing.
#[must_use]
pub fn folder_term<'a>(query: &'a str, trigger: &str) -> Option<&'a str> {
    if trigger.is_empty() {
        return None;
    }

    query.trim_start().strip_prefix(trigger).map(str::trim)
}

/// Short trailing part of the containing folder, for a result row.
#[must_use]
pub fn parent_label(path: &Path) -> String {
    let Some(parent) = path.parent() else {
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_trigger_selects_file_mode() {
        assert_eq!(folder_term(":reports", ":"), Some("reports"));
        assert_eq!(folder_term("  :reports", ":"), Some("reports"));
        assert_eq!(folder_term(": reports", ":"), Some("reports"));
        assert_eq!(folder_term("::reports", "::"), Some("reports"));
        assert_eq!(folder_term(":reports", "::"), None);
        assert_eq!(folder_term("reports", ":"), None);
        assert_eq!(folder_term(":", ":"), Some(""));
        assert_eq!(folder_term("  :  ", ":"), Some(""));
        assert_eq!(folder_term(":reports", ""), None);
    }

    #[test]
    fn parent_label_keeps_the_last_two_folders() {
        let path = Path::new("C:\\Users\\me\\Documents\\Projects");

        assert_eq!(
            parent_label(path),
            format!("me{sep}Documents", sep = std::path::MAIN_SEPARATOR)
        );
    }
}
