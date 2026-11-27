//! Persistent appearance settings.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::util;

/// File name inside the launcher's data directory.
const FILE_NAME: &str = "settings.txt";
/// Schema version written to the file header.
const VERSION: u32 = 1;

/// A font offered by the settings window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FontChoice {
    SegoeUi,
    SegoeUiVariable,
    Arial,
    Calibri,
    Cambria,
    CascadiaCode,
    CascadiaMono,
    Consolas,
    Georgia,
    MicrosoftYaHei,
    Tahoma,
    TrebuchetMs,
    Verdana,
    /// A font family loaded from the user's local font directory
    /// (`~/.local/share/fonts`).
    Custom {
        family: &'static str,
        key: &'static str,
    },
}

impl FontChoice {
    pub const ALL: &'static [Self] = &[
        Self::SegoeUi,
        Self::SegoeUiVariable,
        Self::Arial,
        Self::Calibri,
        Self::Cambria,
        Self::CascadiaCode,
        Self::CascadiaMono,
        Self::Consolas,
        Self::Georgia,
        Self::MicrosoftYaHei,
        Self::Tahoma,
        Self::TrebuchetMs,
        Self::Verdana,
    ];

    /// The Windows font family name. Always static so it can feed
    /// `iced::Font::with_name` directly.
    #[must_use]
    pub const fn name(self) -> &'static str {
        match self {
            Self::SegoeUi => "Segoe UI",
            Self::SegoeUiVariable => "Segoe UI Variable",
            Self::Arial => "Arial",
            Self::Calibri => "Calibri",
            Self::Cambria => "Cambria",
            Self::CascadiaCode => "Cascadia Code",
            Self::CascadiaMono => "Cascadia Mono",
            Self::Consolas => "Consolas",
            Self::Georgia => "Georgia",
            Self::MicrosoftYaHei => "Microsoft YaHei",
            Self::Tahoma => "Tahoma",
            Self::TrebuchetMs => "Trebuchet MS",
            Self::Verdana => "Verdana",
            Self::Custom { family, .. } => family,
        }
    }

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::SegoeUi => "segoe-ui",
            Self::SegoeUiVariable => "segoe-ui-variable",
            Self::Arial => "arial",
            Self::Calibri => "calibri",
            Self::Cambria => "cambria",
            Self::CascadiaCode => "cascadia-code",
            Self::CascadiaMono => "cascadia-mono",
            Self::Consolas => "consolas",
            Self::Georgia => "georgia",
            Self::MicrosoftYaHei => "microsoft-yahei",
            Self::Tahoma => "tahoma",
            Self::TrebuchetMs => "trebuchet-ms",
            Self::Verdana => "verdana",
            Self::Custom { key, .. } => key,
        }
    }

    /// Creates a choice for a font family discovered in the user's local font
    /// directory. The family name is kept for the lifetime of the process so
    /// it can feed `iced::Font::with_name`, which requires `'static` names.
    #[must_use]
    pub fn custom(family: &str) -> Self {
        let family = Box::leak(family.trim().to_owned().into_boxed_str());
        let key = Box::leak(format!("custom:{family}").into_boxed_str());

        Self::Custom { family, key }
    }

    fn from_key(value: &str) -> Option<Self> {
        let value = value.trim();

        if let Some(family) = value.strip_prefix("custom:") {
            let family = family.trim();

            return (!family.is_empty()).then(|| Self::custom(family));
        }

        let normalized = value.to_ascii_lowercase().replace(' ', "-");

        Self::ALL
            .iter()
            .copied()
            .find(|choice| choice.key() == normalized)
    }
}

impl fmt::Display for FontChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.name())
    }
}

/// A theme offered in the settings window. There is intentionally no
/// "follow system" option.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ThemeChoice {
    Light,
    Dark,
    Nord,
    Dracula,
    GruvboxDark,
    CatppuccinMocha,
    TokyoNight,
    SolarizedLight,
    GruvboxLight,
    CatppuccinLatte,
    TokyoNightLight,
}

impl ThemeChoice {
    pub const ALL: &'static [Self] = &[
        Self::Light,
        Self::Dark,
        Self::Nord,
        Self::Dracula,
        Self::GruvboxDark,
        Self::CatppuccinMocha,
        Self::TokyoNight,
        Self::SolarizedLight,
        Self::GruvboxLight,
        Self::CatppuccinLatte,
        Self::TokyoNightLight,
    ];

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::Light => "light",
            Self::Dark => "dark",
            Self::Nord => "nord",
            Self::Dracula => "dracula",
            Self::GruvboxDark => "gruvbox-dark",
            Self::CatppuccinMocha => "catppuccin-mocha",
            Self::TokyoNight => "tokyo-night",
            Self::SolarizedLight => "solarized-light",
            Self::GruvboxLight => "gruvbox-light",
            Self::CatppuccinLatte => "catppuccin-latte",
            Self::TokyoNightLight => "tokyo-night-light",
        }
    }

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Light => "Light",
            Self::Dark => "Dark",
            Self::Nord => "Nord",
            Self::Dracula => "Dracula",
            Self::GruvboxDark => "Gruvbox Dark",
            Self::CatppuccinMocha => "Catppuccin Mocha",
            Self::TokyoNight => "Tokyo Night",
            Self::SolarizedLight => "Solarized Light",
            Self::GruvboxLight => "Gruvbox Light",
            Self::CatppuccinLatte => "Catppuccin Latte",
            Self::TokyoNightLight => "Tokyo Night Light",
        }
    }

    fn from_key(value: &str) -> Option<Self> {
        let normalized = value.trim().to_ascii_lowercase().replace(' ', "-");

        Self::ALL
            .iter()
            .copied()
            .find(|choice| choice.key() == normalized)
    }
}

impl fmt::Display for ThemeChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// A system-wide hotkey offered by the settings window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HotkeyChoice {
    AltSpace,
    CtrlSpace,
    CtrlAltSpace,
    AltShiftSpace,
    CtrlShiftSpace,
    AltBacktick,
    CtrlBacktick,
    AltQ,
    CtrlAltQ,
    CtrlAltP,
}

impl HotkeyChoice {
    pub const ALL: &'static [Self] = &[
        Self::AltSpace,
        Self::CtrlSpace,
        Self::CtrlAltSpace,
        Self::AltShiftSpace,
        Self::CtrlShiftSpace,
        Self::AltBacktick,
        Self::CtrlBacktick,
        Self::AltQ,
        Self::CtrlAltQ,
        Self::CtrlAltP,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::AltSpace => "Alt+Space",
            Self::CtrlSpace => "Ctrl+Space",
            Self::CtrlAltSpace => "Ctrl+Alt+Space",
            Self::AltShiftSpace => "Alt+Shift+Space",
            Self::CtrlShiftSpace => "Ctrl+Shift+Space",
            Self::AltBacktick => "Alt+`",
            Self::CtrlBacktick => "Ctrl+`",
            Self::AltQ => "Alt+Q",
            Self::CtrlAltQ => "Ctrl+Alt+Q",
            Self::CtrlAltP => "Ctrl+Alt+P",
        }
    }

    #[must_use]
    pub const fn key(self) -> &'static str {
        match self {
            Self::AltSpace => "alt-space",
            Self::CtrlSpace => "ctrl-space",
            Self::CtrlAltSpace => "ctrl-alt-space",
            Self::AltShiftSpace => "alt-shift-space",
            Self::CtrlShiftSpace => "ctrl-shift-space",
            Self::AltBacktick => "alt-backtick",
            Self::CtrlBacktick => "ctrl-backtick",
            Self::AltQ => "alt-q",
            Self::CtrlAltQ => "ctrl-alt-q",
            Self::CtrlAltP => "ctrl-alt-p",
        }
    }

    fn from_key(value: &str) -> Option<Self> {
        let normalized = value
            .trim()
            .to_ascii_lowercase()
            .replace('`', "backtick")
            .replace([' ', '+', '-'], "");

        Self::ALL
            .iter()
            .copied()
            .find(|choice| choice.key().replace([' ', '+', '-'], "") == normalized)
    }
}

impl fmt::Display for HotkeyChoice {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.label())
    }
}

/// User appearance settings.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Settings {
    /// Font used by the search field, headings, footer, and settings window.
    pub font: FontChoice,
    /// Font used for application names in the result list.
    pub app_name_font: FontChoice,
    /// Font used for the app category/detail text.
    pub category_font: FontChoice,
    pub theme: ThemeChoice,
    /// The system-wide hotkey that opens the launcher.
    pub hotkey: HotkeyChoice,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            font: FontChoice::SegoeUi,
            app_name_font: FontChoice::SegoeUi,
            category_font: FontChoice::SegoeUi,
            theme: ThemeChoice::Dark,
            hotkey: HotkeyChoice::AltSpace,
        }
    }
}

impl Settings {
    /// Loads settings from `%LOCALAPPDATA%\Pirsa\settings.txt`.
    #[must_use]
    pub fn load() -> Self {
        Self::load_from(&settings_path())
    }

    /// Loads settings from an explicit path.
    #[must_use]
    pub fn load_from(path: &Path) -> Self {
        let Ok(contents) = std::fs::read_to_string(path) else {
            return Self::default();
        };

        let mut settings = Self::default();
        let mut app_name_font = None;
        let mut category_font = None;

        for line in contents.lines() {
            let line = line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };

            match key.trim() {
                "font" => {
                    if let Some(font) = FontChoice::from_key(value) {
                        settings.font = font;
                    }
                }
                "app-name-font" => {
                    if let Some(font) = FontChoice::from_key(value) {
                        app_name_font = Some(font);
                    }
                }
                "category-font" => {
                    if let Some(font) = FontChoice::from_key(value) {
                        category_font = Some(font);
                    }
                }
                "theme" => {
                    if let Some(theme) = ThemeChoice::from_key(value) {
                        settings.theme = theme;
                    }
                }
                "hotkey" => {
                    if let Some(hotkey) = HotkeyChoice::from_key(value) {
                        settings.hotkey = hotkey;
                    }
                }
                // `version` is reserved for future migrations and unknown keys
                // are ignored; old files simply lack it.
                _ => {}
            }
        }

        // Older settings files only had one font. It applied to everything.
        settings.app_name_font = app_name_font.unwrap_or(settings.font);
        settings.category_font = category_font.unwrap_or(settings.font);

        settings
    }

    /// Persists settings to `%LOCALAPPDATA%\Pirsa\settings.txt`.
    pub fn save(&self) {
        // Unit tests share the real user data directory; never overwrite the
        // installed launcher's settings from a test run.
        if cfg!(test) {
            return;
        }

        self.save_to(&settings_path());
    }

    /// Persists settings to an explicit path, replacing it atomically.
    pub fn save_to(&self, path: &Path) {
        let body = format!(
            "version={VERSION}\nfont={}\napp-name-font={}\ncategory-font={}\ntheme={}\nhotkey={}\n",
            self.font.key(),
            self.app_name_font.key(),
            self.category_font.key(),
            self.theme.key(),
            self.hotkey.key()
        );

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let temporary = path.with_extension("tmp");

        if let Err(error) = util::write_atomic(&temporary, path, body.as_bytes()) {
            util::log(format!("could not save settings: {error}"));
        }
    }

    /// Replaces font choices that are not in `available` with the first
    /// available choice. Returns `true` when a value changed.
    ///
    /// Settings can outlive an uninstalled font. Applying such a family
    /// silently falls back to Segoe UI, which makes the font picker look
    /// broken, so the launcher normalizes the stored choices first.
    pub fn use_available_fonts(&mut self, available: &[FontChoice]) -> bool {
        let Some(fallback) = available.first().copied() else {
            return false;
        };

        let mut changed = false;

        for font in [
            &mut self.font,
            &mut self.app_name_font,
            &mut self.category_font,
        ] {
            if available.contains(font) {
                continue;
            }

            // A built-in choice can point at the same family as a font loaded
            // from the local directory (or vice versa). Keep the family when
            // it is available under another choice.
            if let Some(replacement) = available
                .iter()
                .find(|choice| choice.name().eq_ignore_ascii_case(font.name()))
            {
                *font = *replacement;
            } else {
                *font = fallback;
            }

            changed = true;
        }

        changed
    }
}

fn settings_path() -> PathBuf {
    util::data_directory().map_or_else(
        || PathBuf::from(FILE_NAME),
        |directory| directory.join(FILE_NAME),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::sync::atomic::{AtomicU32, Ordering};

    static COUNTER: AtomicU32 = AtomicU32::new(0);

    fn temp_path() -> PathBuf {
        let unique = COUNTER.fetch_add(1, Ordering::Relaxed);
        std::env::temp_dir().join(format!(
            "pirsa-settings-{}-{unique}.txt",
            std::process::id()
        ))
    }

    #[test]
    fn missing_file_uses_defaults() {
        let settings = Settings::load_from(&temp_path());

        assert_eq!(settings.font, FontChoice::SegoeUi);
        assert_eq!(settings.theme, ThemeChoice::Dark);
        assert_eq!(settings.hotkey, HotkeyChoice::AltSpace);
    }

    #[test]
    fn settings_round_trip() {
        let path = temp_path();
        let settings = Settings {
            font: FontChoice::Consolas,
            app_name_font: FontChoice::Georgia,
            category_font: FontChoice::CascadiaMono,
            theme: ThemeChoice::CatppuccinMocha,
            hotkey: HotkeyChoice::CtrlAltP,
        };

        settings.save_to(&path);
        let loaded = Settings::load_from(&path);

        assert_eq!(loaded, settings);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn custom_fonts_round_trip() {
        let path = temp_path();
        let settings = Settings {
            app_name_font: FontChoice::custom("JetBrains Mono"),
            ..Settings::default()
        };

        settings.save_to(&path);
        let loaded = Settings::load_from(&path);

        assert_eq!(loaded.app_name_font, settings.app_name_font);
        assert_eq!(loaded.app_name_font.name(), "JetBrains Mono");
        assert_eq!(loaded.app_name_font.key(), "custom:JetBrains Mono");
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn legacy_single_font_applies_to_app_and_category() {
        let path = temp_path();
        std::fs::write(&path, "font=consolas\ntheme=dark\n").unwrap();

        let settings = Settings::load_from(&path);

        assert_eq!(settings.font, FontChoice::Consolas);
        assert_eq!(settings.app_name_font, FontChoice::Consolas);
        assert_eq!(settings.category_font, FontChoice::Consolas);
        assert_eq!(settings.theme, ThemeChoice::Dark);
        assert_eq!(settings.hotkey, HotkeyChoice::AltSpace);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn hotkey_accepts_legacy_spellings() {
        let path = temp_path();
        std::fs::write(&path, "hotkey=Alt+`\n").unwrap();

        let settings = Settings::load_from(&path);

        assert_eq!(settings.hotkey, HotkeyChoice::AltBacktick);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn invalid_values_fall_back() {
        let path = temp_path();
        std::fs::write(
            &path,
            "font=missing\ntheme=system\nhotkey=none\nunknown=value\n",
        )
        .unwrap();

        let settings = Settings::load_from(&path);

        assert_eq!(settings.font, FontChoice::SegoeUi);
        assert_eq!(settings.app_name_font, FontChoice::SegoeUi);
        assert_eq!(settings.category_font, FontChoice::SegoeUi);
        assert_eq!(settings.theme, ThemeChoice::Dark);
        assert_eq!(settings.hotkey, HotkeyChoice::AltSpace);
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn labels_are_distinct_and_have_no_system_option() {
        let mut labels: Vec<&str> = ThemeChoice::ALL.iter().map(|t| t.label()).collect();
        labels.sort_unstable();
        labels.dedup();

        assert_eq!(labels.len(), ThemeChoice::ALL.len());
        assert!(
            !labels
                .iter()
                .any(|label| label.eq_ignore_ascii_case("system"))
        );
    }

    #[test]
    fn hotkey_labels_and_keys_are_distinct() {
        let mut labels: Vec<&str> = HotkeyChoice::ALL.iter().map(|h| h.label()).collect();
        labels.sort_unstable();
        labels.dedup();

        let mut keys: Vec<&str> = HotkeyChoice::ALL.iter().map(|h| h.key()).collect();
        keys.sort_unstable();
        keys.dedup();

        assert_eq!(labels.len(), HotkeyChoice::ALL.len());
        assert_eq!(keys.len(), HotkeyChoice::ALL.len());
    }

    #[test]
    fn unavailable_fonts_are_replaced_with_an_installed_one() {
        let mut settings = Settings {
            font: FontChoice::CascadiaMono,
            app_name_font: FontChoice::CascadiaCode,
            category_font: FontChoice::Arial,
            ..Settings::default()
        };

        let changed = settings.use_available_fonts(&[FontChoice::SegoeUi, FontChoice::Arial]);

        assert!(changed);
        assert_eq!(settings.font, FontChoice::SegoeUi);
        assert_eq!(settings.app_name_font, FontChoice::SegoeUi);
        assert_eq!(settings.category_font, FontChoice::Arial);
    }

    #[test]
    fn available_fonts_are_kept() {
        let mut settings = Settings::default();
        let original = settings.clone();

        assert!(!settings.use_available_fonts(&[FontChoice::SegoeUi, FontChoice::Georgia]));
        assert_eq!(settings, original);
    }

    #[test]
    fn local_font_choices_are_kept_when_available() {
        let custom = FontChoice::custom("JetBrains Mono");
        let mut settings = Settings {
            app_name_font: custom,
            ..Settings::default()
        };

        assert!(!settings.use_available_fonts(&[FontChoice::SegoeUi, custom]));
        assert_eq!(settings.app_name_font, custom);
    }

    #[test]
    fn a_local_font_can_replace_a_builtin_with_the_same_family() {
        let local = FontChoice::custom("Cascadia Code");
        let mut settings = Settings {
            app_name_font: FontChoice::CascadiaCode,
            ..Settings::default()
        };

        let changed = settings.use_available_fonts(&[FontChoice::SegoeUi, local]);

        assert!(changed);
        assert_eq!(settings.app_name_font, local);
        assert_eq!(settings.app_name_font.name(), "Cascadia Code");
    }

    #[test]
    fn no_available_fonts_leaves_settings_alone() {
        let mut settings = Settings {
            app_name_font: FontChoice::Georgia,
            ..Settings::default()
        };
        let original = settings.clone();

        assert!(!settings.use_available_fonts(&[]));
        assert_eq!(settings, original);
    }
}
