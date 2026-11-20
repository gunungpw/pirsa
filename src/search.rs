//! Ranking for the launcher's search field.

use crate::platform::apps::AppEntry;

/// Returns application indices ordered from best to worst match.
pub fn rank(apps: &[AppEntry], query: &str) -> Vec<usize> {
    let query = normalize(query);

    if query.is_empty() {
        return (0..apps.len()).collect();
    }

    let tokens: Vec<&str> = query.split_whitespace().collect();
    let mut matches = Vec::with_capacity(apps.len());

    for (index, app) in apps.iter().enumerate() {
        let name = normalize(&app.name);
        let detail = normalize(&app.detail);

        if name.is_empty() {
            continue;
        }

        let mut score = score_name(&name, &query).unwrap_or(0);
        let mut matched = score > 0;

        for token in &tokens {
            let token_score = score_name(&name, token)
                .map(|score| score + 40)
                .or_else(|| score_name(&detail, token));

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

        // Prefer shorter, more specific application names when scores tie.
        score -= (name.len() as i32).min(40);
        matches.push((score, name.len(), index));
    }

    matches.sort_by(|left, right| {
        right
            .0
            .cmp(&left.0)
            .then_with(|| left.1.cmp(&right.1))
            .then_with(|| apps[left.2].name.cmp(&apps[right.2].name))
    });

    matches.into_iter().map(|(_, _, index)| index).collect()
}

fn score_name(name: &str, query: &str) -> Option<i32> {
    if query.is_empty() {
        return Some(0);
    }

    if name == query {
        return Some(2_000);
    }

    if name.starts_with(query) {
        return Some(1_500 - (name.len() as i32 - query.len() as i32).min(300));
    }

    if let Some(position) = name.find(query) {
        let boundary = if position == 0 || is_word_boundary(name, position) {
            250
        } else {
            0
        };

        return Some(1_050 + boundary - position as i32 * 4);
    }

    let acronym = acronym(name);
    if acronym == query {
        return Some(1_000);
    }

    if acronym.starts_with(query) {
        return Some(850 - (acronym.len() as i32 - query.len() as i32).min(200));
    }

    subsequence_score(name, query)
}

fn subsequence_score(name: &str, query: &str) -> Option<i32> {
    let name_chars: Vec<char> = name.chars().collect();
    let query_chars: Vec<char> = query.chars().collect();
    let mut positions = Vec::with_capacity(query_chars.len());
    let mut name_index = 0;

    for query_char in &query_chars {
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
        .filter(|position| is_word_boundary(name, **position))
        .count() as i32
        * 35;

    Some(550 + boundary_bonus - gaps * 9 - positions.first().copied().unwrap_or(0) as i32 * 2)
}

fn acronym(name: &str) -> String {
    name.split_whitespace()
        .filter_map(|word| word.chars().next())
        .collect()
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

fn normalize(value: &str) -> String {
    let mut normalized = String::with_capacity(value.len());

    for character in value.trim().chars() {
        if character.is_alphanumeric() {
            normalized.extend(character.to_lowercase());
        } else if !normalized.ends_with(' ') {
            normalized.push(' ');
        }
    }

    normalized.trim().to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::apps::LaunchTarget;

    fn app(name: &str) -> AppEntry {
        AppEntry {
            name: name.to_owned(),
            detail: "Start Menu".to_owned(),
            target: LaunchTarget::Shell(format!("test:{name}")),
        }
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
        let mut settings = app("Settings");
        settings.detail = "Windows app".to_owned();
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
}
