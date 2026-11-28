//! The launcher's visual design.
//!
//! The types and shared styling live here; the launcher window lives in
//! [`launcher`], the settings window in [`settings`], and the widget styles in
//! [`style`].

use std::collections::{HashMap, HashSet};
use std::path::PathBuf;
use std::sync::OnceLock;

use iced::widget::image::Handle;
use iced::{Color, Font, Theme};

use pirsa_core::calc::CalcResult;
use pirsa_core::search_scope::{DriveDetails, DriveMode, IndexStatus, SearchScope};
use pirsa_core::settings::{FontChoice, HotkeyChoice, Settings, ThemeChoice};
use pirsa_core::{AppEntry, FileEntry, HotkeyStatus, ResultRow};

/// UI-originated actions.
#[derive(Debug, Clone)]
pub enum Message {
    QueryChanged(String),
    ClearQuery,
    Navigate(i32),
    /// Launch the app with this stable search key.
    Activate(String),
    /// Open the file or folder at this path.
    ActivateFile(PathBuf),
    CopyCalc,
    OpenSettings,
    Quit,
    DragWindow,
}

/// Actions from the settings window.
#[derive(Debug, Clone)]
pub enum SettingsMessage {
    FontChanged(FontChoice),
    AppNameFontChanged(FontChoice),
    CategoryFontChanged(FontChoice),
    ThemeChanged(ThemeChoice),
    HotkeyChanged(HotkeyChoice),
    Close,
    ScopePresetChanged(ScopePreset),
    DriveToggled(char, bool),
    AddFolderRequested,
    IncludeRemoved(PathBuf),
    ExcludeDraftChanged(String),
    ExcludeAdded,
    ExcludeRemoved(String),
    DepthChanged(u8),
    HiddenToggled(bool),
    TriggerDraftChanged(String),
    TriggerCommitted,
    /// The depth slider was released; rebuild the index for the new depth.
    ScopeCommitted,
    RebuildIndex,
    /// Drag the borderless settings window by its header.
    DragWindow,
    /// Show the settings of another category in the right column.
    CategorySelected(SettingsCategory),
}

/// The categories listed in the settings window's first column.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum SettingsCategory {
    #[default]
    Appearance,
    Behavior,
    FileSearch,
    Preview,
}

impl SettingsCategory {
    pub const ALL: &'static [Self] = &[
        Self::Appearance,
        Self::Behavior,
        Self::FileSearch,
        Self::Preview,
    ];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Appearance => "Appearance",
            Self::Behavior => "Behavior",
            Self::FileSearch => "File search",
            Self::Preview => "Preview",
        }
    }
}

/// The scope choices offered by the settings window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScopePreset {
    Profile,
    Fixed,
    All,
    Selected,
}

impl ScopePreset {
    pub const ALL: &'static [Self] = &[Self::Profile, Self::Fixed, Self::All, Self::Selected];

    #[must_use]
    pub const fn label(self) -> &'static str {
        match self {
            Self::Profile => "Profile only",
            Self::Fixed => "Fixed drives",
            Self::All => "All drives",
            Self::Selected => "Selected drives",
        }
    }

    fn of(mode: &DriveMode) -> Self {
        match mode {
            DriveMode::Profile => Self::Profile,
            DriveMode::Fixed => Self::Fixed,
            DriveMode::All => Self::All,
            DriveMode::Selected(_) => Self::Selected,
        }
    }
}

impl std::fmt::Display for ScopePreset {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.label())
    }
}

/// Read-only state the launcher view renders.
///
/// The view borrows one struct instead of requiring a trait implementation, so
/// adding a field does not ripple through a trait, its implementation, and the
/// test fixture.
#[derive(Debug, Clone, Copy)]
pub struct ViewState<'a> {
    pub apps: &'a [AppEntry],
    /// Rows for the active query: apps in app mode, files and folders in file
    /// mode. The view shows the first [`ViewState::result_limit`] rows.
    pub rows: &'a [ResultRow],
    pub calc: Option<&'a CalcResult>,
    /// Real icons keyed by [`AppEntry::search_key`].
    pub icons: &'a HashMap<String, Handle>,
    /// File and folder matches, indexed by [`ResultRow::File`].
    pub files: &'a [FileEntry],
    /// File search configuration shown in the settings window.
    pub scope: &'a SearchScope,
    pub drives: &'a [DriveDetails],
    pub exclude_draft: &'a str,
    pub index_status: IndexStatus,
    pub query: &'a str,
    pub selected: usize,
    pub hotkey: HotkeyStatus,
    pub hotkey_choice: HotkeyChoice,
    pub error: Option<&'a str>,
    pub warning: Option<&'a str>,
    pub indexing: bool,
    pub result_limit: usize,
    pub theme: &'a Theme,
    pub font: Font,
    pub app_name_font: Font,
    pub category_font: Font,
}

pub const WINDOW_WIDTH: f32 = 820.0;
pub const WINDOW_HEIGHT: f32 = 540.0;
pub const SETTINGS_WINDOW_WIDTH: f32 = 780.0;
pub const SETTINGS_WINDOW_HEIGHT: f32 = 600.0;
pub const INPUT_ID: &str = "pirsa-search-input";

/// Width of the settings window's category column.
const SETTINGS_NAV_WIDTH: f32 = 168.0;

pub const MAX_RESULTS: usize = 8;
pub const MAX_RESULTS_WITH_CALC: usize = MAX_RESULTS - 1;
const ROW_HEIGHT: f32 = 46.0;
const ROW_GAP: f32 = 2.0;
const CALC_GAP: f32 = 8.0;

const ICON_COLORS: [Color; 8] = [
    Color::from_rgb8(0x4c, 0x7d, 0xf2),
    Color::from_rgb8(0x9a, 0x6a, 0xe8),
    Color::from_rgb8(0x2f, 0x9e, 0x8f),
    Color::from_rgb8(0xd1, 0x7a, 0x3d),
    Color::from_rgb8(0xc4, 0x5d, 0x86),
    Color::from_rgb8(0x3d, 0x8e, 0xc9),
    Color::from_rgb8(0x8a, 0x9b, 0x3e),
    Color::from_rgb8(0xbf, 0x6b, 0x5b),
];

/// Colours derived from the active iced theme.
#[derive(Debug, Clone, Copy)]
struct Colors {
    background_top: Color,
    background_bottom: Color,
    surface: Color,
    text: Color,
    muted: Color,
    dim: Color,
    accent: Color,
    accent_soft: Color,
    on_accent: Color,
    success: Color,
    warning: Color,
    danger: Color,
    selected: Color,
    hovered: Color,
    pressed: Color,
    row_border: Color,
    hairline: Color,
}

impl Colors {
    fn from_theme(theme: &Theme) -> Self {
        let palette = theme.palette();
        let background = palette.background;
        let is_light = luminance(background) > 0.5;
        let text = palette.text;
        let accent = palette.primary;

        Self {
            background_top: if is_light {
                blend(background, Color::WHITE, 0.35)
            } else {
                blend(background, Color::WHITE, 0.06)
            },
            background_bottom: if is_light {
                blend(background, Color::BLACK, 0.08)
            } else {
                blend(background, Color::BLACK, 0.18)
            },
            surface: if is_light {
                blend(background, Color::BLACK, 0.04)
            } else {
                blend(background, Color::WHITE, 0.05)
            },
            text,
            muted: blend(text, background, 0.35),
            dim: blend(text, background, 0.55),
            accent,
            accent_soft: Color::from_rgba(accent.r, accent.g, accent.b, 0.20),
            on_accent: if luminance(accent) > 0.55 {
                Color::BLACK
            } else {
                Color::WHITE
            },
            success: palette.success,
            warning: palette.warning,
            danger: palette.danger,
            selected: overlay(is_light, 0.065),
            hovered: overlay(is_light, 0.035),
            pressed: overlay(is_light, 0.10),
            row_border: overlay(is_light, 0.08),
            hairline: overlay(is_light, 0.10),
        }
    }
}

fn blend(base: Color, overlay: Color, amount: f32) -> Color {
    let amount = amount.clamp(0.0, 1.0);
    Color::from_rgba(
        base.r + (overlay.r - base.r) * amount,
        base.g + (overlay.g - base.g) * amount,
        base.b + (overlay.b - base.b) * amount,
        1.0,
    )
}

fn overlay(is_light: bool, alpha: f32) -> Color {
    let color = if is_light { Color::BLACK } else { Color::WHITE };
    Color::from_rgba(color.r, color.g, color.b, alpha)
}

fn luminance(color: Color) -> f32 {
    0.2126 * color.r + 0.7152 * color.g + 0.0722 * color.b
}

fn medium_font(font: Font) -> Font {
    Font {
        weight: iced::font::Weight::Medium,
        ..font
    }
}

/// Applies emphasis without losing the selected font family.
///
/// cosmic-text looks for a face with the exact requested weight before it
/// considers the family. Only Segoe UI ships a real semibold (600) face; for
/// every other family a 600 request falls back to Segoe UI Semibold, so the
/// app-name font appears to be ignored. Medium (500) has no exact-weight face
/// to hijack it, keeping the chosen family (including regular-only fonts from
/// the local font directory).
fn semibold_font(font: Font) -> Font {
    let weight = match font.family {
        iced::font::Family::Name("Segoe UI") => iced::font::Weight::Semibold,
        _ => iced::font::Weight::Medium,
    };

    Font { weight, ..font }
}

fn bold_font(font: Font) -> Font {
    let weight = match font.family {
        iced::font::Family::Name("Segoe UI") => iced::font::Weight::Bold,
        _ => iced::font::Weight::Medium,
    };

    Font { weight, ..font }
}

/// Font choices whose family is installed on this system.
///
/// Offering a missing family is misleading: the text silently falls back to
/// Segoe UI, so the picker appears to do nothing. The set is cached and reads
/// the same font database iced uses for rendering.
pub fn available_fonts() -> &'static [FontChoice] {
    static AVAILABLE: OnceLock<Vec<FontChoice>> = OnceLock::new();

    AVAILABLE.get_or_init(|| {
        let installed = installed_families();
        let fonts: Vec<FontChoice> = FontChoice::ALL
            .iter()
            .copied()
            .filter(|choice| installed.contains(&choice.name().to_ascii_lowercase()))
            .collect();

        if fonts.is_empty() {
            FontChoice::ALL.to_vec()
        } else {
            fonts
        }
    })
}

fn installed_families() -> HashSet<String> {
    let mut font_system = iced::advanced::graphics::text::font_system()
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    let database = font_system.raw().db_mut();

    database
        .faces()
        .flat_map(|face| face.families.iter())
        .map(|(name, _)| name.to_ascii_lowercase())
        .collect()
}

/// Read-only state the settings window renders.
///
/// Mirrors [`ViewState`]: the view borrows one struct instead of taking a long
/// argument list, so adding a field does not ripple through every call site.
#[derive(Debug, Clone, Copy)]
pub struct SettingsView<'a> {
    pub settings: &'a Settings,
    /// Font choices installed on this system; the pickers only offer these.
    pub fonts: &'a [FontChoice],
    pub scope: &'a SearchScope,
    pub drives: &'a [DriveDetails],
    pub trigger_draft: &'a str,
    pub exclude_draft: &'a str,
    pub index_status: IndexStatus,
    pub theme: &'a Theme,
    pub notice: Option<&'a str>,
    /// Category selected in the first column.
    pub category: SettingsCategory,
}

mod launcher;
mod settings;
mod style;

#[cfg(test)]
mod tests;

pub use launcher::view;
pub use settings::settings_view;
