//! Ranking for the launcher's search field.

use crate::model::AppEntry;
use crate::usage::{Usage, UsageEntry};
use crate::util::SearchText;

/// Returns application indices ordered from best to worst match, preferring
/// apps that are launched often.
///
/// With an empty query, frequently used apps move to the front while
/// everything else keeps its Start Menu order. Search keeps its usual scoring
/// and only lets usage decide close matches.
pub fn rank_with_usage(apps: &[AppEntry], query: &str, usage: &Usage) -> Vec<usize> {
    let query = SearchText::new(query);

    if query.is_empty() {
        return popular(apps, usage);
    }

    let tokens: Vec<SearchText> = query
        .normalized()
        .split_whitespace()
        .map(SearchText::new)
        .collect();
    let mut matches = Vec::with_capacity(apps.len());

    for (index, app) in apps.iter().enumerate() {
        let name = app.name_search();
        let detail = app.detail_search();

        if name.is_empty() {
            continue;
        }

        let mut score = score_name(name, &query).unwrap_or(0);
        let mut matched = score > 0;

        for token in &tokens {
            let token_score = score_name(name, token)
                .map(|score| score + 40)
                .or_else(|| score_name(detail, token));

            match token_score {
                Some(token_score) => {
                    score += token_score;
                    matched = true;
                }
                None => {
                    matched = false;
                    break;
                }
            }
        }

        if !matched {
            continue;
        }

        // Prefer shorter, more specific application names when scores tie,
        // then apps the user launches often.
        let usage_entry = usage.entry(name.normalized());
        score -= (name.len() as i32).min(40);
        score += usage_entry.score_bonus();
        matches.push((score, name.len(), index, usage_entry));
    }

    matches.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| right.3.launches.cmp(&left.3.launches))
            .then_with(|| right.3.last_used.cmp(&left.3.last_used))
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| apps[left.2].name().cmp(apps[right.2].name()))
    });

    matches.into_iter().map(|(_, _, index, _)| index).collect()
}

/// Orders every app with the most-used ones first.
///
/// `sort_by` is stable, so apps without history keep the Start Menu order
/// (alphabetical) they were indexed with.
fn popular(apps: &[AppEntry], usage: &Usage) -> Vec<usize> {
    let mut ranked: Vec<(usize, UsageEntry)> = apps
        .iter()
        .enumerate()
        .map(|(index, app)| (index, usage.entry(app.search_key())))
        .collect();

    ranked.sort_by(|left, right| {
        right
            .1
            .launches
            .cmp(&left.1.launches)
            .then_with(|| right.1.last_used.cmp(&left.1.last_used))
    });

    ranked.into_iter().map(|(index, _)| index).collect()
}

fn score_name(text: &SearchText, query: &SearchText) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }

    let name = text.normalized();
    let query_text = query.normalized();

    if name == query_text {
        return Some(2_000);
    }

    if name.starts_with(query_text) {
        return Some(1_500 - text.len().saturating_sub(query.len()).min(300) as i32);
    }

    if let Some(position) = name.find(query_text) {
        let boundary = if position == 0 || is_word_boundary(name, position) {
            250
        } else {
            0
        };

        return Some(1_050 + boundary - position as i32 * 4);
    }

    let acronym = text.acronym();
    if acronym == query_text {
        return Some(1_000);
    }

    if acronym.starts_with(query_text) {
        return Some(850 - text.acronym_len().saturating_sub(query.len()).min(200) as i32);
    }

    subsequence_score(text.characters(), query.characters())
}

fn subsequence_score(name_chars: &[char], query_chars: &[char]) -> Option<i32> {
    let mut positions = Vec::with_capacity(query_chars.len());
    let mut name_index = 0;

    for query_char in query_chars {
        let found = name_chars[name_index..]
            .iter()
            .position(|candidate| candidate == query_char)?;
        let position = name_index + found;
        positions.push(position);
        name_index = position + 1;
    }

    let span = positions.last().copied().unwrap_or(0) - positions.first().copied().unwrap_or(0) + 1;
    let gaps = span.saturating_sub(query_chars.len()) as i32;
    let boundary_bonus = positions
        .iter()
        .filter(|position| is_char_boundary(name_chars, **position))
        .count() as i32
        * 35;

    Some(550 + boundary_bonus - gaps * 9 - positions.first().copied().unwrap_or(0) as i32 * 2)
}

fn is_word_boundary(text: &str, byte_position: usize) -> bool {
    if byte_position == 0 {
        return true;
    }

    text[..byte_position]
        .chars()
        .next_back()
        .is_none_or(|character| !character.is_alphanumeric())
}

/// Word-boundary check for a position in a char slice. Subsequence matches track
/// char indices, which are not byte offsets once a name contains non-ASCII text.
fn is_char_boundary(characters: &[char], index: usize) -> bool {
    index == 0 || !characters[index - 1].is_alphanumeric()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::LaunchTarget;

    fn app(name: &str) -> AppEntry {
        AppEntry::new(
            name.to_owned(),
            "Start Menu".to_owned(),
            LaunchTarget::Shell(format!("test:{name}")),
        )
    }

    /// Ranking without any launch history, i.e. the relevance-only order.
    fn rank(apps: &[AppEntry], query: &str) -> Vec<usize> {
        rank_with_usage(apps, query, &Usage::default())
    }

    #[test]
    fn empty_query_keeps_every_app() {
        let apps = vec![app("Alpha"), app("Beta")];
        assert_eq!(rank(&apps, ""), vec![0, 1]);
        assert_eq!(rank(&apps, "   "), vec![0, 1]);
    }

    #[test]
    fn exact_and_prefix_matches_rank_first() {
        let apps = vec![app("Code"), app("Visual Studio Code"), app("Code::Blocks")];
        let ranked = rank(&apps, "code");

        assert_eq!(ranked.first().copied(), Some(0));
        assert!(ranked.contains(&1));
        assert!(ranked.contains(&2));
    }

    #[test]
    fn acronym_matches_work() {
        let apps = vec![app("Visual Studio Code"), app("Spotify"), app("Discord")];

        assert_eq!(rank(&apps, "vsc").first().copied(), Some(0));
        assert_eq!(rank(&apps, "disc").first().copied(), Some(2));
    }

    #[test]
    fn multiple_tokens_must_all_match() {
        let apps = vec![app("Visual Studio Code"), app("Visual Studio Installer")];
        let ranked = rank(&apps, "studio code");

        assert_eq!(ranked, vec![0]);
    }

    #[test]
    fn query_is_case_insensitive() {
        let apps = vec![app("Microsoft Edge")];
        assert_eq!(rank(&apps, "EDGE"), vec![0]);
    }

    #[test]
    fn details_can_match() {
        let settings = AppEntry::new(
            "Settings".to_owned(),
            "Windows app".to_owned(),
            LaunchTarget::Shell("test:settings".to_owned()),
        );
        let apps = vec![settings, app("Notepad")];

        assert_eq!(rank(&apps, "windows"), vec![0]);
    }

    #[test]
    fn fuzzy_subsequence_matches_are_available() {
        let apps = vec![app("Visual Studio Code")];
        assert_eq!(rank(&apps, "vscode"), vec![0]);
    }

    #[test]
    fn unrelated_query_returns_nothing() {
        let apps = vec![app("Notepad"), app("Calculator")];
        assert!(rank(&apps, "zzzz").is_empty());
    }

    #[test]
    fn unicode_names_do_not_panic_in_subsequence_matching() {
        let apps = vec![app("Über"), app("Café")];

        // "br" only matches "Über" as a subsequence, which used to index the
        // name by char position into a byte slice.
        assert_eq!(rank(&apps, "br"), vec![0]);
    }

    fn usage_with(entries: &[(&str, u32, u64)]) -> Usage {
        let mut usage = Usage::default();

        for (key, launches, last_used) in entries {
            usage.insert(
                key,
                UsageEntry {
                    launches: *launches,
                    last_used: *last_used,
                },
            );
        }

        usage
    }

    #[test]
    fn frequently_used_apps_lead_the_empty_list() {
        let apps = vec![app("Alpha"), app("Beta"), app("Gamma"), app("Delta")];
        let usage = usage_with(&[("gamma", 4, 10), ("beta", 1, 20)]);

        assert_eq!(rank_with_usage(&apps, "", &usage), vec![2, 1, 0, 3]);
    }

    #[test]
    fn equally_used_apps_are_ordered_by_recency() {
        let apps = vec![app("Alpha"), app("Beta")];
        let usage = usage_with(&[("alpha", 3, 10), ("beta", 3, 20)]);

        assert_eq!(rank_with_usage(&apps, "", &usage), vec![1, 0]);
    }

    #[test]
    fn empty_query_order_is_unchanged_without_history() {
        let apps = vec![app("Alpha"), app("Beta")];
        let usage = usage_with(&[("unknown", 9, 10)]);

        assert_eq!(rank_with_usage(&apps, "", &usage), vec![0, 1]);
    }

    #[test]
    fn usage_breaks_close_search_ties() {
        let apps = vec![app("Plan A"), app("Plan B")];

        assert_eq!(rank(&apps, "plan"), vec![0, 1]);
        assert_eq!(
            rank_with_usage(&apps, "plan", &usage_with(&[("plan b", 3, 5)])),
            vec![1, 0]
        );
    }

    #[test]
    fn usage_does_not_override_an_exact_match() {
        let apps = vec![app("Code"), app("Codex")];
        let usage = usage_with(&[("codex", 10, 5)]);

        assert_eq!(rank_with_usage(&apps, "code", &usage), vec![0, 1]);
    }
}
