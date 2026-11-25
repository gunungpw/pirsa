//! Persistent appearance settings.

use std::fmt;
use std::path::{Path, PathBuf};

use crate::util;

/// File name inside the launcher's data directory.
const FILE_NAME: &str = "settings.txt";

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
        }
    }

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
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            font: FontChoice::SegoeUi,
            app_name_font: FontChoice::SegoeUi,
            category_font: FontChoice::SegoeUi,
            theme: ThemeChoice::Dark,
        }
    }
}

impl Settings {
    /// Loads settings from `%LOCALAPPDATA%\Pirsa\settings.txt`.
    pub fn load() -> Self {
        Self::load_from(&settings_path())
    }

    /// Loads settings from an explicit path.
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
        self.save_to(&settings_path());
    }

    /// Persists settings to an explicit path, replacing it atomically.
    pub fn save_to(&self, path: &Path) {
        let body = format!(
            "font={}\napp-name-font={}\ncategory-font={}\ntheme={}\n",
            self.font.key(),
            self.app_name_font.key(),
            self.category_font.key(),
            self.theme.key()
        );

        if let Some(parent) = path.parent() {
            let _ = std::fs::create_dir_all(parent);
        }

        let temporary = path.with_extension("tmp");

        if let Err(error) = util::write_atomic(&temporary, path, body.as_bytes()) {
            util::log(format!("could not save settings: {error}"));
        }
    }
}

fn settings_path() -> PathBuf {
    util::data_directory()
        .map(|directory| directory.join(FILE_NAME))
        .unwrap_or_else(|| PathBuf::from(FILE_NAME))
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
    }

    #[test]
    fn settings_round_trip() {
        let path = temp_path();
        let settings = Settings {
            font: FontChoice::Consolas,
            app_name_font: FontChoice::Georgia,
            category_font: FontChoice::CascadiaMono,
            theme: ThemeChoice::CatppuccinMocha,
        };

        settings.save_to(&path);
        let loaded = Settings::load_from(&path);

        assert_eq!(loaded, settings);
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
        let _ = std::fs::remove_file(path);
    }

    #[test]
    fn invalid_values_fall_back() {
        let path = temp_path();
        std::fs::write(&path, "font=missing\ntheme=system\nunknown=value\n").unwrap();

        let settings = Settings::load_from(&path);

        assert_eq!(settings.font, FontChoice::SegoeUi);
        assert_eq!(settings.app_name_font, FontChoice::SegoeUi);
        assert_eq!(settings.category_font, FontChoice::SegoeUi);
        assert_eq!(settings.theme, ThemeChoice::Dark);
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
}
