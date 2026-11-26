//! The launcher's unified result list.
//!
//! Apps and folders share one ranked list, so a plain query searches both
//! without a prefix. [`merge`] interleaves the two scored streams without
//! re-ranking them.

/// One row of the launcher's result list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultRow {
    /// Index into the indexed app list.
    App(usize),
    /// Index into the current folder matches.
    Folder(usize),
}

/// Merges app and folder matches into one list, best first.
///
/// Both inputs are `(score, index)` pairs already ordered best first and scored
/// on the same scale, which [`crate::search::rank_scores`] and
/// [`crate::folders::rank_scores`] guarantee. On a tie the app wins, so a name
/// shared by an app and a folder always lists the app first, and the two
/// streams keep their own relative order.
#[must_use]
pub fn merge(apps: &[(i32, usize)], folders: &[(i32, usize)], limit: usize) -> Vec<ResultRow> {
    let mut rows = Vec::with_capacity((apps.len() + folders.len()).min(limit));
    let mut next_app = 0;
    let mut next_folder = 0;

    while rows.len() < limit && (next_app < apps.len() || next_folder < folders.len()) {
        let take_app = match (apps.get(next_app), folders.get(next_folder)) {
            (Some((app_score, _)), Some((folder_score, _))) => app_score >= folder_score,
            (Some(_), None) => true,
            (None, Some(_)) => false,
            (None, None) => break,
        };

        if take_app {
            rows.push(ResultRow::App(apps[next_app].1));
            next_app += 1;
        } else {
            rows.push(ResultRow::Folder(folders[next_folder].1));
            next_folder += 1;
        }
    }

    rows
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn higher_scores_come_first() {
        let apps = [(1_500, 0), (1_050, 1)];
        let folders = [(2_000, 0), (600, 1)];

        assert_eq!(
            merge(&apps, &folders, 10),
            vec![
                ResultRow::Folder(0),
                ResultRow::App(0),
                ResultRow::App(1),
                ResultRow::Folder(1),
            ]
        );
    }

    #[test]
    fn ties_keep_apps_ahead_of_folders() {
        let apps = [(1_000, 3)];
        let folders = [(1_000, 7)];

        assert_eq!(
            merge(&apps, &folders, 10),
            vec![ResultRow::App(3), ResultRow::Folder(7)]
        );
    }

    #[test]
    fn the_limit_applies_to_the_merged_list() {
        let apps = [(900, 0), (800, 1), (700, 2)];
        let folders = [(950, 0)];

        assert_eq!(
            merge(&apps, &folders, 2),
            vec![ResultRow::Folder(0), ResultRow::App(0)]
        );
    }

    #[test]
    fn a_missing_side_still_lists_the_other() {
        assert_eq!(
            merge(&[], &[(500, 4)], 8),
            vec![ResultRow::Folder(4)],
            "folder-only matches are listed"
        );
        assert_eq!(
            merge(&[(500, 2)], &[], 8),
            vec![ResultRow::App(2)],
            "app-only matches are listed"
        );
        assert!(merge(&[], &[], 8).is_empty());
    }
}
