//! Match quality and frecency scoring.

/// Scores a name/path match. Higher is better.
#[must_use]
pub fn match_quality(name: &str, path: &str, term: &str) -> Option<i32> {
    if term.is_empty() {
        return None;
    }

    let name = name.to_lowercase();
    let path = path.to_lowercase();

    if name == term {
        return Some(1000);
    }

    if let Some(rest) = name.strip_prefix(term) {
        return Some(900 - (rest.chars().count() as i32).min(100));
    }

    if name
        .split(|c: char| !c.is_alphanumeric())
        .any(|part| part.starts_with(term))
    {
        return Some(850);
    }

    if let Some(position) = name.find(term) {
        return Some(700 - (position as i32 * 2).min(200));
    }

    if path.contains(term) {
        return Some(500);
    }

    if is_subsequence(term, &name) {
        return Some(300);
    }

    None
}

/// Frequency + recency with a 30-day half-life, capped at 30 points.
#[must_use]
pub fn frecency(launch_count: u32, last_launched: i64, now: i64) -> f32 {
    let age_days = ((now - last_launched).max(0) as f32) / 86_400.0;
    let decay = 0.5_f32.powf(age_days / 30.0);
    (5.0 * (1.0 + launch_count as f32).ln() + 25.0 * decay).min(30.0)
}

/// Match quality dominates; usage can only add 30 points.
#[must_use]
pub fn final_score(quality: i32, frecency: f32) -> i64 {
    i64::from(quality) * 100 + frecency.min(30.0) as i64
}

fn is_subsequence(needle: &str, haystack: &str) -> bool {
    let mut chars = haystack.chars();
    needle.chars().all(|c| chars.any(|h| h == c))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn exact_beats_prefix_and_usage() {
        assert!(final_score(1000, 0.0) > final_score(900, 30.0));
    }

    #[test]
    fn match_quality_tiers_are_ordered() {
        let exact = match_quality("reports", "c:\\reports", "reports").unwrap();
        let prefix = match_quality("reports-2025", "c:\\reports-2025", "reports").unwrap();
        let substring = match_quality("annual-reports", "c:\\annual-reports", "reports").unwrap();
        let fuzzy = match_quality("r-e-p-o-r-t-s", "c:\\r-e-p-o-r-t-s", "reports");

        assert!(exact > prefix);
        assert!(prefix > substring);
        assert!(substring > fuzzy.unwrap_or(0));
    }

    #[test]
    fn frecency_is_capped() {
        assert!(frecency(1_000_000, crate::now_secs(), crate::now_secs()) <= 30.0);
    }
}
