//! Where folder search looks and what it skips.
//!
//! The scope is persisted in `%LOCALAPPDATA%\Pirsa\search.txt` as line-based
//! `key=value` pairs, matching the launcher's other configuration files. The
//! default scope is exactly the launcher's original behaviour: the user
//! profile, six levels deep, with development and shell noise excluded.

use std::path::{Path, PathBuf};

use crate::util;

/// File name inside the launcher's data directory.
const FILE_NAME: &str = "search.txt";
const VERSION: u32 = 1;

pub const MIN_DEPTH: usize = 1;
pub const MAX_DEPTH: usize = 12;
pub const DEFAULT_DEPTH: usize = 6;
pub const MIN_RESULTS: usize = 1;
pub const MAX_RESULTS: usize = 16;
pub const DEFAULT_RESULTS: usize = 8;

/// How a drive is classified by Windows.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DriveKind {
    Fixed,
    Removable,
    Remote,
    Other,
}

impl DriveKind {
    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Fixed => "Fixed",
            Self::Removable => "Removable",
            Self::Remote => "Network",
            Self::Other => "Other",
        }
    }
}

/// A drive the scope can search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveInfo {
    pub letter: char,
    pub kind: DriveKind,
}

impl DriveInfo {
    #[must_use]
    pub fn root(&self) -> PathBuf {
        PathBuf::from(format!("{letter}:\\", letter = self.letter))
    }
}

/// A drive with the details the settings window shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DriveDetails {
    pub info: DriveInfo,
    pub label: String,
    pub free_bytes: u64,
    pub total_bytes: u64,
}

impl DriveDetails {
    /// `"C: · Local Disk · 123 GB free"`, used by the settings checkboxes.
    #[must_use]
    pub fn summary(&self) -> String {
        let mut summary = format!("{}:", self.info.letter);

        if !self.label.is_empty() {
            summary.push_str(" · ");
            summary.push_str(&self.label);
        }

        if self.total_bytes > 0 {
            summary.push_str(&format!(" · {} free", format_bytes(self.free_bytes)));
        }

        summary
    }
}

fn format_bytes(bytes: u64) -> String {
    const GIB: f64 = 1024.0 * 1024.0 * 1024.0;
    const MIB: f64 = 1024.0 * 1024.0;

    let bytes = bytes as f64;

    if bytes >= GIB {
        format!("{:.0} GB", bytes / GIB)
    } else {
        format!("{:.0} MB", (bytes / MIB).max(1.0))
    }
}

/// Build state of the persistent folder index.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct IndexStatus {
    pub ready: bool,
    pub building: bool,
    pub folders: u64,
}

/// Which drives participate in the search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DriveMode {
    /// The user profile only: the launcher's original behaviour.
    Profile,
    /// Every attached drive.
    All,
    /// Local fixed drives only.
    Fixed,
    /// The listed drive letters, in the order the user chose them.
    Selected(Vec<char>),
}

impl DriveMode {
    #[must_use]
    pub const fn label(&self) -> &'static str {
        match self {
            Self::Profile => "Profile only",
            Self::All => "All drives",
            Self::Fixed => "Fixed drives",
            Self::Selected(_) => "Selected drives",
        }
    }

    pub fn key(&self) -> String {
        match self {
            Self::Profile => "profile".to_owned(),
            Self::All => "all".to_owned(),
            Self::Fixed => "fixed".to_owned(),
            Self::Selected(letters) => {
                let list: Vec<String> = letters.iter().map(char::to_string).collect();
                format!("selected:{}", list.join(","))
            }
        }
    }

    fn from_key(value: &str) -> Option<Self> {
        match value.trim() {
            "profile" => Some(Self::Profile),
            "all" => Some(Self::All),
            "fixed" => Some(Self::Fixed),
            other => {
                let letters = other.strip_prefix("selected:")?;
                let letters: Vec<char> = letters
                    .split(',')
                    .filter_map(|letter| letter.trim().chars().next())
                    .map(|letter| letter.to_ascii_uppercase())
                    .filter(char::is_ascii_uppercase)
                    .collect();

                if letters.is_empty() {
                    None
                } else {
                    Some(Self::Selected(letters))
                }
            }
        }
    }
}

/// A configured folder search.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SearchScope {
    pub drives: DriveMode,
    /// Extra roots searched in addition to the drive selection.
    pub include: Vec<PathBuf>,
    /// Name globs (`node_modules`, `temp*`) and path suffixes (`*\bin`).
    pub exclude: Vec<String>,
    pub depth: usize,
    pub max_results: usize,
    pub include_hidden: bool,
}

impl Default for SearchScope {
    fn default() -> Self {
        Self {
            drives: DriveMode::Profile,
            include: Vec::new(),
            exclude: default_excludes(),
            depth: DEFAULT_DEPTH,
            max_results: DEFAULT_RESULTS,
            include_hidden: false,
        }
    }
}

impl SearchScope {
    /// Replaces the drive selection, keeping every other rule.
    #[must_use]
    pub fn with_drives(mut self, drives: DriveMode) -> Self {
        self.drives = drives;
        self
    }

    pub fn set_depth(&mut self, depth: usize) {
        self.depth = depth.clamp(MIN_DEPTH, MAX_DEPTH);
    }

    pub fn set_max_results(&mut self, results: usize) {
        self.max_results = results.clamp(MIN_RESULTS, MAX_RESULTS);
    }

    pub fn add_include(&mut self, path: PathBuf) {
        if path.as_os_str().is_empty() {
            return;
        }

        let key = path_key(&path);

        if self
            .include
            .iter()
            .any(|existing| path_key(existing) == key)
        {
            return;
        }

        self.include.push(path);
        self.include.sort_by_key(|path| path_key(path));
    }

    pub fn remove_include(&mut self, path: &Path) {
        let key = path_key(path);
        self.include.retain(|existing| path_key(existing) != key);
    }

    /// Adds one exclusion rule. Returns whether it was new.
    pub fn add_exclude(&mut self, rule: &str) -> bool {
        let rule = rule.trim().to_lowercase();

        if rule.is_empty() || self.exclude.contains(&rule) {
            return false;
        }

        self.exclude.push(rule);
        self.exclude.sort();
        true
    }

    pub fn remove_exclude(&mut self, rule: &str) {
        self.exclude.retain(|existing| existing != rule);
    }

    /// Whether `path` is excluded by name or by a `*\suffix` rule.
    #[must_use]
    pub fn is_excluded(&self, path: &Path) -> bool {
        let name = path
            .file_name()
            .map(|name| name.to_string_lossy().to_lowercase())
            .unwrap_or_default();
        let lower_path = path.to_string_lossy().to_lowercase();

        self.exclude.iter().any(|rule| {
            if let Some(suffix) = rule.strip_prefix("*\\") {
                let suffix = format!("\\{suffix}");
                lower_path.ends_with(&suffix)
            } else {
                glob_match(rule, &name)
            }
        })
    }

    /// Whether a hidden entry should be skipped.
    #[must_use]
    pub fn is_hidden_skipped(&self, name: &str) -> bool {
        !self.include_hidden && name.starts_with('.')
    }

    /// Expands the selection into concrete roots, dropping nested duplicates
    /// and preserving a stable order.
    pub fn roots(&self, profile: &Path, drives: &[DriveInfo]) -> Vec<PathBuf> {
        let mut roots = Vec::new();

        match &self.drives {
            DriveMode::Profile => roots.push(profile.to_path_buf()),
            DriveMode::All => roots.extend(drives.iter().map(DriveInfo::root)),
            DriveMode::Fixed => roots.extend(
                drives
                    .iter()
                    .filter(|drive| drive.kind == DriveKind::Fixed)
                    .map(DriveInfo::root),
            ),
            DriveMode::Selected(letters) => roots.extend(
                drives
                    .iter()
                    .filter(|drive| letters.contains(&drive.letter))
                    .map(DriveInfo::root),
            ),
        }

        roots.extend(self.include.iter().cloned());

        dedupe_roots(roots)
    }

    /// Stable hash of everything that changes what the index contains.
    #[must_use]
    pub fn fingerprint(&self) -> u64 {
        let mut hash = 0xcbf2_9ce4_8422_2325_u64;
        let mut feed = |text: &str| {
            for byte in text.bytes() {
                hash ^= u64::from(byte);
                hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
            }
            // Separator so adjacent fields cannot collide.
            hash ^= 0xff;
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        };

        feed(&self.drives.key());
        feed(&self.depth.to_string());
        feed(&self.max_results.to_string());
        feed(if self.include_hidden {
            "hidden"
        } else {
            "visible"
        });

        for path in &self.include {
            feed(&path_key(path));
        }

        for rule in &self.exclude {
            feed(rule);
        }

        hash
    }

    /// Loads the scope from `%LOCALAPPDATA%\Pirsa\search.txt`.
    #[must_use]
    pub fn load() -> Self {
        Self::load_from(&scope_path())
    }

    /// Loads the scope from an explicit path.
    #[must_use]
    pub fn load_from(path: &Path) -> Self {
        let Ok(contents) = std::fs::read_to_string(path) else {
            return Self::default();
        };

        Self::parse(&contents)
    }

    /// Parses the file format. Missing or damaged entries fall back to defaults
    /// one field at a time.
    #[must_use]
    #[allow(clippy::match_same_arms)] // unknown and ignored keys both fall through by design
    pub fn parse(contents: &str) -> Self {
        let mut scope = Self {
            // Start with an empty exclusion list so the file's rules replace the
            // defaults instead of being merged with them.
            exclude: Vec::new(),
            ..Self::default()
        };
        let mut saw_exclude = false;

        for line in contents.lines() {
            let line = line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };

            let key = key.trim();
            let value = value.trim();

            match key {
                "version" => {}
                "drives" => {
                    if let Some(mode) = DriveMode::from_key(value) {
                        scope.drives = mode;
                    }
                }
                "include" if !value.is_empty() => scope.include.push(PathBuf::from(value)),
                "exclude" if !value.is_empty() => {
                    let rule = value.to_lowercase();

                    if !scope.exclude.contains(&rule) {
                        scope.exclude.push(rule);
                    }

                    saw_exclude = true;
                }
                "depth" => {
                    if let Ok(depth) = value.parse::<usize>() {
                        scope.set_depth(depth);
                    }
                }
                "max-results" => {
                    if let Ok(results) = value.parse::<usize>() {
                        scope.set_max_results(results);
                    }
                }
                "include-hidden" => scope.include_hidden = value.eq_ignore_ascii_case("true"),
                _ => {}
            }
        }

        // A future format may rename fields; an older launcher still reads the
        // parts it understands.
        if !saw_exclude {
            scope.exclude = default_excludes();
        }

        scope.include.sort_by_key(|path| path_key(path));
        scope.exclude.sort();

        scope
    }

    /// Renders the current format.
    #[must_use]
    pub fn render(&self) -> String {
        let mut body = String::from("# Pirsa folder search scope\n");
        body.push_str(&format!("version={VERSION}\n"));
        body.push_str(&format!("drives={}\n", self.drives.key()));

        for path in &self.include {
            body.push_str(&format!("include={}\n", path.display()));
        }

        for rule in &self.exclude {
            body.push_str(&format!("exclude={rule}\n"));
        }

        body.push_str(&format!("depth={}\n", self.depth));
        body.push_str(&format!("max-results={}\n", self.max_results));
        body.push_str(&format!("include-hidden={}\n", self.include_hidden));

        body
    }

    /// Persists to `%LOCALAPPDATA%\Pirsa\search.txt`.
    pub fn save(&self) {
        // Unit tests share the real user data directory; never overwrite the
        // installed launcher's scope from a test run.
        if cfg!(test) {
            return;
        }

        self.save_to(&scope_path());
    }

    pub fn save_to(&self, path: &Path) {
        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let temporary = path.with_extension("tmp");

        if let Err(error) = util::write_atomic(&temporary, path, self.render().as_bytes()) {
            util::log(format!("could not save the search scope: {error}"));
        }
    }
}

fn default_excludes() -> Vec<String> {
    let mut excludes: Vec<String> = [
        "node_modules",
        ".git",
        "target",
        ".cargo",
        ".rustup",
        ".nuget",
        ".gradle",
        "appdata",
        "application data",
        "local settings",
        "onedrivetemp",
        "$recycle.bin",
        "system volume information",
    ]
    .into_iter()
    .map(str::to_owned)
    .collect();

    excludes.sort();

    excludes
}

/// Case-insensitive key used for comparisons and the index fingerprint.
fn path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

fn dedupe_roots(roots: Vec<PathBuf>) -> Vec<PathBuf> {
    let keyed: Vec<(String, PathBuf)> = roots
        .into_iter()
        .map(|path| (path_key(&path), path))
        .collect();

    let mut kept: Vec<(String, PathBuf)> = Vec::new();

    for (key, path) in keyed {
        if key.is_empty() || kept.iter().any(|(parent, _)| is_within(parent, &key)) {
            continue;
        }

        // A later, broader root replaces anything it now covers.
        kept.retain(|(child, _)| !is_within(&key, child));
        kept.push((key, path));
    }

    kept.into_iter().map(|(_, path)| path).collect()
}

/// Component-aware containment check so `C:\work` does not swallow `C:\workshop`.
fn is_within(parent: &str, child: &str) -> bool {
    if parent.is_empty() {
        return false;
    }

    if child == parent {
        return true;
    }

    if parent.ends_with('\\') {
        return child.starts_with(parent);
    }

    child.starts_with(parent) && child.as_bytes().get(parent.len()) == Some(&b'\\')
}

/// Glob match with `*` as the only wildcard.
fn glob_match(pattern: &str, text: &str) -> bool {
    let pattern: Vec<char> = pattern.chars().collect();
    let text: Vec<char> = text.chars().collect();

    let (mut pattern_index, mut text_index) = (0_usize, 0_usize);
    let mut star: Option<(usize, usize)> = None;

    while text_index < text.len() {
        if pattern_index < pattern.len() && pattern[pattern_index] == text[text_index] {
            pattern_index += 1;
            text_index += 1;
        } else if pattern_index < pattern.len() && pattern[pattern_index] == '*' {
            star = Some((pattern_index, text_index));
            pattern_index += 1;
        } else if let Some((star_index, star_text)) = star {
            pattern_index = star_index + 1;
            text_index = star_text + 1;
            star = Some((star_index, star_text + 1));
        } else {
            return false;
        }
    }

    while pattern_index < pattern.len() && pattern[pattern_index] == '*' {
        pattern_index += 1;
    }

    pattern_index == pattern.len()
}

fn scope_path() -> PathBuf {
    util::data_directory().map_or_else(
        || PathBuf::from(FILE_NAME),
        |directory| directory.join(FILE_NAME),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn drives() -> Vec<DriveInfo> {
        vec![
            DriveInfo {
                letter: 'C',
                kind: DriveKind::Fixed,
            },
            DriveInfo {
                letter: 'D',
                kind: DriveKind::Fixed,
            },
            DriveInfo {
                letter: 'E',
                kind: DriveKind::Removable,
            },
        ]
    }

    fn profile() -> PathBuf {
        PathBuf::from("C:\\Users\\me")
    }

    #[test]
    fn defaults_match_the_original_behaviour() {
        let scope = SearchScope::default();

        assert_eq!(scope.drives, DriveMode::Profile);
        assert_eq!(scope.depth, DEFAULT_DEPTH);
        assert_eq!(scope.roots(&profile(), &[]), vec![profile()]);
        assert!(scope.is_excluded(Path::new("C:\\Users\\me\\node_modules")));
        assert!(!scope.is_excluded(Path::new("C:\\Users\\me\\my.git")));
    }

    #[test]
    fn drive_modes_expand_to_roots() {
        let profile = profile();

        assert_eq!(
            SearchScope::default().roots(&profile, &drives()),
            vec![profile.clone()]
        );

        let fixed = SearchScope::default().with_drives(DriveMode::Fixed);
        assert_eq!(
            fixed.roots(&profile, &drives()),
            vec![PathBuf::from("C:\\"), PathBuf::from("D:\\")]
        );

        let all = SearchScope::default().with_drives(DriveMode::All);
        assert_eq!(all.roots(&profile, &drives()).len(), 3);

        let selected = SearchScope::default().with_drives(DriveMode::Selected(vec!['D', 'E']));
        assert_eq!(
            selected.roots(&profile, &drives()),
            vec![PathBuf::from("D:\\"), PathBuf::from("E:\\")]
        );
    }

    #[test]
    fn nested_roots_are_deduplicated() {
        let scope = SearchScope {
            drives: DriveMode::Fixed,
            include: vec![PathBuf::from("C:\\Users\\me\\Projects")],
            ..SearchScope::default()
        };

        // C:\ already covers the include.
        assert_eq!(
            scope.roots(&profile(), &drives()),
            vec![PathBuf::from("C:\\"), PathBuf::from("D:\\")]
        );
    }

    #[test]
    fn includes_are_added_after_drive_roots() {
        let scope = SearchScope {
            include: vec![PathBuf::from("D:\\Work")],
            ..SearchScope::default()
        };

        assert_eq!(
            scope.roots(&profile(), &drives()),
            vec![profile(), PathBuf::from("D:\\Work")]
        );
    }

    #[test]
    fn include_add_ignores_duplicates() {
        let mut scope = SearchScope::default();
        scope.add_include(PathBuf::from("D:\\Work"));
        scope.add_include(PathBuf::from("d:\\work\\"));

        assert_eq!(scope.include.len(), 1);

        scope.remove_include(Path::new("D:\\WORK"));

        assert!(scope.include.is_empty());
    }

    #[test]
    fn exclusion_rules_cover_names_globs_and_suffixes() {
        let mut scope = SearchScope {
            exclude: Vec::new(),
            ..SearchScope::default()
        };
        scope.add_exclude("node_modules");
        scope.add_exclude("temp*");
        scope.add_exclude("*\\bin");

        assert!(scope.is_excluded(Path::new("C:\\work\\node_modules")));
        assert!(scope.is_excluded(Path::new("C:\\work\\temporary")));
        assert!(scope.is_excluded(Path::new("D:\\repo\\bin")));
        assert!(!scope.is_excluded(Path::new("D:\\repo\\binary")));
        assert!(!scope.is_excluded(Path::new("D:\\repo\\src")));
    }

    #[test]
    fn hidden_entries_are_skipped_unless_requested() {
        let scope = SearchScope::default();
        assert!(scope.is_hidden_skipped(".config"));

        let scope = SearchScope {
            include_hidden: true,
            ..SearchScope::default()
        };
        assert!(!scope.is_hidden_skipped(".config"));
    }

    #[test]
    fn round_trip_preserves_every_field() {
        let mut scope = SearchScope::default().with_drives(DriveMode::Selected(vec!['C', 'D']));
        scope.include.push(PathBuf::from("D:\\Work"));
        scope.add_exclude("*.bak");
        scope.set_depth(9);
        scope.set_max_results(12);
        scope.include_hidden = true;

        let parsed = SearchScope::parse(&scope.render());

        assert_eq!(parsed, scope);
    }

    #[test]
    fn damaged_lines_fall_back_per_field() {
        let scope = SearchScope::parse(
            "version=1\ndrives=selected:\ndepth=oops\nmax-results=999\ninclude=\nexclude=\n",
        );

        assert_eq!(scope.drives, DriveMode::Profile);
        assert_eq!(scope.depth, DEFAULT_DEPTH);
        assert_eq!(scope.max_results, MAX_RESULTS);
        assert!(scope.include.is_empty());
        // No exclusion line survived, so the defaults come back.
        assert_eq!(scope.exclude, default_excludes());
    }

    #[test]
    fn depth_and_results_are_clamped() {
        let mut scope = SearchScope::default();
        scope.set_depth(0);
        scope.set_max_results(0);
        assert_eq!(scope.depth, MIN_DEPTH);
        assert_eq!(scope.max_results, MIN_RESULTS);

        scope.set_depth(999);
        scope.set_max_results(999);
        assert_eq!(scope.depth, MAX_DEPTH);
        assert_eq!(scope.max_results, MAX_RESULTS);
    }

    #[test]
    fn fingerprint_changes_with_the_scope() {
        let base = SearchScope::default();

        let mut deeper = base.clone();
        deeper.set_depth(base.depth + 1);

        let mut excluded = base.clone();
        excluded.add_exclude("scratch");

        let mut driven = base.clone();
        driven.drives = DriveMode::Fixed;

        assert_ne!(base.fingerprint(), deeper.fingerprint());
        assert_ne!(base.fingerprint(), excluded.fingerprint());
        assert_ne!(base.fingerprint(), driven.fingerprint());
        assert_eq!(base.fingerprint(), SearchScope::default().fingerprint());
    }

    #[test]
    fn glob_matching_handles_stars() {
        assert!(glob_match("temp*", "temporary"));
        assert!(glob_match("*cache*", "mycacheold"));
        assert!(glob_match("a*b*c", "axxbyyc"));
        assert!(!glob_match("temp*", "attempt"));
        assert!(!glob_match("a*b", "abx"));
    }

    #[test]
    fn drive_summaries_are_human_readable() {
        let details = DriveDetails {
            info: DriveInfo {
                letter: 'C',
                kind: DriveKind::Fixed,
            },
            label: "System".to_owned(),
            free_bytes: 100 * 1024 * 1024 * 1024,
            total_bytes: 500 * 1024 * 1024 * 1024,
        };

        let summary = details.summary();

        assert!(summary.starts_with("C:"));
        assert!(summary.contains("System"));
        assert!(summary.contains("100 GB free"));

        let unlabelled = DriveDetails {
            label: String::new(),
            ..details
        };

        assert_eq!(unlabelled.summary(), "C: · 100 GB free");
    }
}
