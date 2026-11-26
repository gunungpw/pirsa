//! The launcher's visual design.

use std::collections::HashMap;

use iced::border;
use iced::widget::image::Handle;
use iced::widget::text::Wrapping;
use iced::widget::{
    Image, Space, button, column, container, mouse_area, pick_list, row, scrollable, text,
    text_input,
};
use iced::{Alignment, Background, Border, Color, Element, Font, Length, Theme};

use pirsa_core::calc::{CalcKind, CalcResult};
use pirsa_core::settings::{FontChoice, HotkeyChoice, Settings, ThemeChoice};
use pirsa_core::{AppEntry, HotkeyStatus, LaunchTarget};

/// UI-originated actions.
#[derive(Debug, Clone)]
pub enum Message {
    QueryChanged(String),
    ClearQuery,
    Navigate(i32),
    /// Launch the app with this stable search key.
    Activate(String),
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
}

/// Read-only state the launcher view renders.
///
/// The view borrows one struct instead of requiring a trait implementation, so
/// adding a field does not ripple through a trait, its implementation, and the
/// test fixture.
#[derive(Debug, Clone, Copy)]
pub struct ViewState<'a> {
    pub apps: &'a [AppEntry],
    pub results: &'a [usize],
    pub calc: Option<&'a CalcResult>,
    /// Real icons keyed by [`AppEntry::search_key`].
    pub icons: &'a HashMap<String, Handle>,
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
pub const SETTINGS_WINDOW_WIDTH: f32 = 620.0;
pub const SETTINGS_WINDOW_HEIGHT: f32 = 680.0;
pub const INPUT_ID: &str = "pirsa-search-input";

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

fn semibold_font(font: Font) -> Font {
    Font {
        weight: iced::font::Weight::Semibold,
        ..font
    }
}

fn bold_font(font: Font) -> Font {
    Font {
        weight: iced::font::Weight::Bold,
        ..font
    }
}

pub fn view<'a>(app: ViewState<'a>) -> Element<'a, Message> {
    let colors = Colors::from_theme(app.theme);
    let font = app.font;
    let mut content = column![search_row(app, colors, font), Space::new().height(12)];

    if let Some(calc) = app.calc {
        content = content.push(calc_card(app, calc, colors, font));
        content = content.push(Space::new().height(CALC_GAP));
    }

    content = content
        .push(section_label(app, colors, font))
        .push(Space::new().height(6))
        .push(result_list(app, colors, font))
        .push(Space::new().height(Length::Fill))
        .push(footer(app, colors, font))
        .width(Length::Fill)
        .height(Length::Fill);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([18, 22])
        .style(root_style)
        .into()
}

fn search_row<'a>(app: ViewState<'a>, colors: Colors, font: Font) -> Element<'a, Message> {
    let field = text_input("Search apps, calculate, convert…", app.query)
        .id(INPUT_ID)
        .on_input(Message::QueryChanged)
        .padding([0, 0])
        .size(20)
        .font(font)
        .style(search_input_style)
        .width(Length::Fill);

    let clear: Element<'_, Message> = if app.query.is_empty() {
        Space::new().width(22).into()
    } else {
        button(text("×").size(15).font(font).color(colors.muted))
            .on_press(Message::ClearQuery)
            .width(24)
            .height(24)
            .padding(0)
            .style(ghost_button_style)
            .into()
    };

    row![field, clear]
        .width(Length::Fill)
        .height(54)
        .spacing(12)
        .align_y(Alignment::Center)
        .into()
}

fn calc_card<'a>(
    app: ViewState<'a>,
    calc: &'a CalcResult,
    colors: Colors,
    font: Font,
) -> Element<'a, Message> {
    let glyph = match calc.kind {
        CalcKind::Math => "=",
        CalcKind::Unit => "↔",
    };

    let badge = container(
        text(glyph)
            .size(16)
            .font(bold_font(font))
            .color(colors.on_accent),
    )
    .width(30)
    .height(30)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(calc_badge_style);

    let mut labels = column![
        text(calc.expression.as_str())
            .size(13)
            .font(medium_font(font))
            .color(colors.text)
            .wrapping(Wrapping::None)
    ]
    .spacing(2);

    if !calc.detail.is_empty() {
        labels = labels.push(
            text(calc.detail.as_str())
                .size(9.5)
                .font(font)
                .color(colors.dim)
                .wrapping(Wrapping::None),
        );
    }

    let content = row![
        badge,
        labels.width(Length::Fill),
        text(calc.primary.as_str())
            .size(17)
            .font(semibold_font(font))
            .color(colors.accent)
            .wrapping(Wrapping::None),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .align_y(Alignment::Center)
    .spacing(12);

    let selected = app.selected == 0;

    button(content)
        .on_press(Message::CopyCalc)
        .width(Length::Fill)
        .height(ROW_HEIGHT)
        .padding([0, 12])
        .style(move |theme, status| result_button_style(theme, status, selected))
        .into()
}

fn section_label<'a>(app: ViewState<'a>, colors: Colors, font: Font) -> Element<'a, Message> {
    let query_empty = app.query.trim().is_empty();
    let indexing = app.indexing && app.apps.is_empty();
    let title = if indexing {
        "Indexing"
    } else if query_empty {
        "All apps"
    } else {
        "Results"
    };
    let count = if indexing {
        "SCANNING…".to_owned()
    } else if query_empty {
        format!("{} APPS", app.apps.len())
    } else {
        format!("{} FOUND", app.results.len())
    };

    let mut details = row![Space::new().width(Length::Fill)].align_y(Alignment::Center);

    if app.warning.is_some() {
        details = details.push(
            text("PARTIAL INDEX")
                .size(9)
                .font(font)
                .color(colors.warning),
        );
        details = details.push(Space::new().width(10));
    }

    details = details.push(text(count).size(9).font(font).color(colors.dim));

    mouse_area(
        row![
            text(title)
                .size(13)
                .font(medium_font(font))
                .color(colors.muted),
            details
        ]
        .width(Length::Fill)
        .height(18)
        .align_y(Alignment::Center),
    )
    .on_press(Message::DragWindow)
    .into()
}

fn result_list<'a>(app: ViewState<'a>, colors: Colors, font: Font) -> Element<'a, Message> {
    let limit = app.result_limit;
    let height = limit as f32 * ROW_HEIGHT + (limit.saturating_sub(1)) as f32 * ROW_GAP;

    let content: Element<'_, Message> = if app.results.is_empty() {
        if app.calc.is_some() {
            Space::new().into()
        } else {
            empty_state(app, colors, font)
        }
    } else {
        let rows: Vec<Element<'_, Message>> = app
            .results
            .iter()
            .take(limit)
            .enumerate()
            .map(|(position, index)| result_row(app, *index, position, colors, font))
            .collect();

        column(rows).spacing(ROW_GAP).into()
    };

    container(content)
        .width(Length::Fill)
        .height(height)
        .align_y(Alignment::Start)
        .into()
}

fn result_row<'a>(
    app: ViewState<'a>,
    app_index: usize,
    position: usize,
    colors: Colors,
    font: Font,
) -> Element<'a, Message> {
    let Some(entry) = app.apps.get(app_index) else {
        return Space::new().into();
    };

    let selected = position + usize::from(app.calc.is_some()) == app.selected;

    // `button` places its content at the top-left, so the row has to claim the
    // full height and center its items itself. Otherwise the selected
    // highlight extends well below the icon and label.
    let content = row![
        icon_element(entry, app.icons.get(entry.search_key()), colors, font),
        row![
            text(entry.name())
                .size(15)
                .font(semibold_font(app.app_name_font))
                .color(colors.text)
                .wrapping(Wrapping::None),
            text(inline_detail(entry))
                .size(11)
                .font(app.category_font)
                .color(colors.dim)
                .wrapping(Wrapping::None),
        ]
        .spacing(9)
        .align_y(Alignment::End)
        .width(Length::Fill),
        container(
            text(entry_kind(entry))
                .size(11)
                .font(font)
                .color(if selected { colors.muted } else { colors.dim })
        )
        .width(72)
        .align_x(Alignment::End),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .align_y(Alignment::Center)
    .spacing(12);

    button(content)
        .on_press(Message::Activate(entry.search_key().to_owned()))
        .width(Length::Fill)
        .height(ROW_HEIGHT)
        .padding([0, 12])
        .style(move |theme, status| result_button_style(theme, status, selected))
        .into()
}

fn empty_state<'a>(app: ViewState<'a>, colors: Colors, font: Font) -> Element<'a, Message> {
    let (title, subtitle) = if app.indexing {
        (
            "Indexing Start Menu…".to_owned(),
            "Apps will appear in a moment.".to_owned(),
        )
    } else if app.apps.is_empty() {
        (
            "No apps indexed".to_owned(),
            "Press F5 to scan the Start Menu again.".to_owned(),
        )
    } else {
        (
            format!("No matches for “{}”", app.query.trim()),
            "Try a different name, or press Esc to close.".to_owned(),
        )
    };

    container(
        column![
            text(title)
                .size(15)
                .font(medium_font(font))
                .color(colors.text),
            text(subtitle).size(11).font(font).color(colors.dim),
        ]
        .spacing(7)
        .align_x(Alignment::Center),
    )
    .width(Length::Fill)
    .height(Length::Fill)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .into()
}

fn footer<'a>(app: ViewState<'a>, colors: Colors, font: Font) -> Element<'a, Message> {
    let settings = button(text("Settings").size(10).font(font))
        .on_press(Message::OpenSettings)
        .padding([3, 7])
        .style(ghost_button_style);

    let quit = button(text("Quit").size(10).font(font))
        .on_press(Message::Quit)
        .padding([3, 7])
        .style(ghost_button_style);

    if let Some(error) = app.error {
        return row![
            status_dot(colors.danger),
            text(error).size(10).font(font).color(colors.danger),
            Space::new().width(Length::Fill),
            settings,
            Space::new().width(8),
            quit,
        ]
        .spacing(7)
        .align_y(Alignment::Center)
        .height(24)
        .into();
    }

    if let Some(warning) = app.warning {
        return row![
            status_dot(colors.warning),
            text(shorten(warning, 72))
                .size(10)
                .font(font)
                .color(colors.warning),
            Space::new().width(Length::Fill),
            settings,
            Space::new().width(8),
            quit,
        ]
        .spacing(7)
        .align_y(Alignment::Center)
        .height(24)
        .into();
    }

    let (color, label) = match app.hotkey {
        HotkeyStatus::Ready => (
            colors.success,
            std::borrow::Cow::Borrowed(app.hotkey_choice.label()),
        ),
        HotkeyStatus::Unavailable => (
            colors.danger,
            std::borrow::Cow::Owned(format!("{} unavailable", app.hotkey_choice.label())),
        ),
        HotkeyStatus::Pending => (colors.dim, std::borrow::Cow::Borrowed("Starting…")),
    };

    let primary_action = if app.calc.is_some() && app.selected == 0 {
        "Copy"
    } else {
        "Open"
    };

    row![
        status_dot(color),
        text(label).size(10).font(font).color(colors.muted),
        Space::new().width(Length::Fill),
        text(primary_action).size(10.5).font(font).color(colors.dim),
        keycap("↵", colors, font),
        Space::new().width(10),
        text("Hide").size(10.5).font(font).color(colors.dim),
        keycap("Esc", colors, font),
        Space::new().width(14),
        settings,
        Space::new().width(8),
        quit,
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .height(24)
    .into()
}

fn keycap(label: &'static str, colors: Colors, font: Font) -> Element<'static, Message> {
    container(text(label).size(9).font(font).color(colors.muted))
        .height(20)
        .padding([2, 6])
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(keycap_style)
        .into()
}

/// Shortens a one-line status message so it cannot collide with the footer
/// buttons.
fn shorten(text: &str, max_chars: usize) -> String {
    let mut characters = text.chars();
    let shortened: String = characters.by_ref().take(max_chars).collect();

    if characters.next().is_some() {
        format!("{shortened}…")
    } else {
        shortened
    }
}

fn status_dot(color: Color) -> Element<'static, Message> {
    container(Space::new())
        .width(6)
        .height(6)
        .style(move |_theme| container::Style {
            background: Some(color.into()),
            border: border::rounded(3),
            ..container::Style::default()
        })
        .into()
}

/// The real application icon, falling back to the colored initial tile while
/// icons are still loading (or when the shell cannot provide one).
fn icon_element<'a>(
    entry: &'a AppEntry,
    icon: Option<&Handle>,
    colors: Colors,
    font: Font,
) -> Element<'a, Message> {
    match icon {
        Some(handle) => Image::new(handle.clone())
            .width(30)
            .height(30)
            .border_radius(7)
            .into(),
        None => icon_tile(entry.name(), colors, font),
    }
}

fn icon_tile(name: &str, colors: Colors, font: Font) -> Element<'_, Message> {
    let color = icon_color(name);
    let initial = initial(name);

    container(
        text(initial)
            .size(12.5)
            .font(bold_font(font))
            .color(Color::WHITE),
    )
    .width(30)
    .height(30)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_theme| container::Style {
        background: Some(color.into()),
        border: border::rounded(7).width(1).color(colors.hairline),
        ..container::Style::default()
    })
    .into()
}

fn icon_color(name: &str) -> Color {
    let mut hash = 2_166_136_261_u32;

    for byte in name.bytes() {
        hash ^= byte as u32;
        hash = hash.wrapping_mul(16_777_619);
    }

    ICON_COLORS[(hash as usize) % ICON_COLORS.len()]
}

fn initial(name: &str) -> String {
    name.chars()
        .find(|character| character.is_alphanumeric())
        .map(|character| character.to_uppercase().to_string())
        .unwrap_or_else(|| "?".to_owned())
}

fn inline_detail(entry: &AppEntry) -> &str {
    let detail = entry.detail();

    detail.strip_prefix("Start Menu · ").unwrap_or(detail)
}

fn entry_kind(entry: &AppEntry) -> &'static str {
    match entry.target() {
        LaunchTarget::Shortcut(_) => "Application",
        LaunchTarget::Executable(_) => "Desktop app",
        LaunchTarget::Shell(_) => "Store app",
    }
}

pub fn settings_view<'a>(
    settings: &'a Settings,
    theme: &'a Theme,
    notice: Option<&'a str>,
) -> Element<'a, SettingsMessage> {
    let colors = Colors::from_theme(theme);
    let font = Font::with_name(settings.font.name());
    let app_name_font = Font::with_name(settings.app_name_font.name());
    let category_font = Font::with_name(settings.category_font.name());

    let font_picker = pick_list(
        FontChoice::ALL,
        Some(settings.font),
        SettingsMessage::FontChanged,
    )
    .text_size(14)
    .padding([8, 10])
    .font(font)
    .width(Length::Fill)
    .style(pick_list_style);

    let app_name_font_picker = pick_list(
        FontChoice::ALL,
        Some(settings.app_name_font),
        SettingsMessage::AppNameFontChanged,
    )
    .text_size(14)
    .padding([8, 10])
    .font(font)
    .width(Length::Fill)
    .style(pick_list_style);

    let category_font_picker = pick_list(
        FontChoice::ALL,
        Some(settings.category_font),
        SettingsMessage::CategoryFontChanged,
    )
    .text_size(14)
    .padding([8, 10])
    .font(font)
    .width(Length::Fill)
    .style(pick_list_style);

    let theme_picker = pick_list(
        ThemeChoice::ALL,
        Some(settings.theme),
        SettingsMessage::ThemeChanged,
    )
    .text_size(14)
    .padding([8, 10])
    .font(font)
    .width(Length::Fill)
    .style(pick_list_style);

    let hotkey_picker = pick_list(
        HotkeyChoice::ALL,
        Some(settings.hotkey),
        SettingsMessage::HotkeyChanged,
    )
    .text_size(14)
    .padding([8, 10])
    .font(font)
    .width(Length::Fill)
    .style(pick_list_style);

    let notice: Element<'a, SettingsMessage> = match notice {
        Some(notice) => column![
            Space::new().height(8),
            text(notice).size(11).font(font).color(colors.danger),
        ]
        .into(),
        None => Space::new().height(0).into(),
    };

    let preview = container(
        row![
            text("Terminal")
                .size(15)
                .font(semibold_font(app_name_font))
                .color(colors.text),
            text("Windows app")
                .size(11)
                .font(category_font)
                .color(colors.dim),
        ]
        .spacing(9)
        .align_y(Alignment::End),
    )
    .width(Length::Fill)
    .padding(16)
    .style(preview_style);

    let close = button(
        text("Close")
            .size(12)
            .font(semibold_font(font))
            .color(colors.on_accent),
    )
    .on_press(SettingsMessage::Close)
    .padding([8, 18])
    .style(primary_button_style);

    let fields = column![
        settings_field("UI font", font_picker.into(), colors, font),
        Space::new().height(12),
        settings_field("App name font", app_name_font_picker.into(), colors, font),
        Space::new().height(12),
        settings_field("Category font", category_font_picker.into(), colors, font),
        Space::new().height(12),
        settings_field("Theme", theme_picker.into(), colors, font),
        Space::new().height(12),
        settings_field("Global hotkey", hotkey_picker.into(), colors, font),
        notice,
        Space::new().height(16),
        text("Preview").size(11).font(font).color(colors.dim),
        Space::new().height(6),
        preview,
    ]
    .width(Length::Fill);

    let content = column![
        text("Settings")
            .size(24)
            .font(bold_font(font))
            .color(colors.text),
        text("Changes apply immediately.")
            .size(11)
            .font(font)
            .color(colors.dim),
        Space::new().height(18),
        // The window is a fixed size, so a short screen scrolls the fields
        // instead of pushing the Close button out of reach.
        scrollable(fields).height(Length::Fill),
        Space::new().height(12),
        row![Space::new().width(Length::Fill), close]
            .width(Length::Fill)
            .align_y(Alignment::Center),
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([24, 26])
        .style(settings_root_style)
        .into()
}

fn settings_field<'a>(
    label: &'a str,
    control: Element<'a, SettingsMessage>,
    colors: Colors,
    font: Font,
) -> Element<'a, SettingsMessage> {
    column![text(label).size(11).font(font).color(colors.muted), control]
        .spacing(6)
        .width(Length::Fill)
        .into()
}

fn root_style(theme: &Theme) -> container::Style {
    let colors = Colors::from_theme(theme);
    let background = iced::gradient::Linear::new(iced::Radians(std::f32::consts::FRAC_PI_2))
        .add_stop(0.0, colors.background_top)
        .add_stop(1.0, colors.background_bottom);

    container::Style {
        background: Some(background.into()),
        text_color: Some(colors.text),
        border: border::rounded(4).width(1).color(colors.hairline),
        ..container::Style::default()
    }
}

fn settings_root_style(theme: &Theme) -> container::Style {
    let colors = Colors::from_theme(theme);
    let background = iced::gradient::Linear::new(iced::Radians(std::f32::consts::FRAC_PI_2))
        .add_stop(0.0, colors.background_top)
        .add_stop(1.0, colors.background_bottom);

    container::Style {
        background: Some(background.into()),
        text_color: Some(colors.text),
        ..container::Style::default()
    }
}

fn preview_style(theme: &Theme) -> container::Style {
    let colors = Colors::from_theme(theme);

    container::Style {
        background: Some(Background::Color(colors.surface)),
        border: border::rounded(10).width(1).color(colors.hairline),
        ..container::Style::default()
    }
}

fn calc_badge_style(theme: &Theme) -> container::Style {
    let colors = Colors::from_theme(theme);

    container::Style {
        background: Some(Background::Color(colors.accent)),
        border: border::rounded(7).width(1).color(colors.hairline),
        ..container::Style::default()
    }
}

fn search_input_style(theme: &Theme, _status: text_input::Status) -> text_input::Style {
    let colors = Colors::from_theme(theme);

    text_input::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border::default(),
        icon: colors.muted,
        placeholder: colors.dim,
        value: colors.text,
        selection: colors.accent_soft,
    }
}

fn keycap_style(theme: &Theme) -> container::Style {
    let colors = Colors::from_theme(theme);

    container::Style {
        background: Some(Background::Color(colors.hovered)),
        border: border::rounded(5).width(1).color(colors.hairline),
        ..container::Style::default()
    }
}

fn pick_list_style(theme: &Theme, _status: pick_list::Status) -> pick_list::Style {
    let colors = Colors::from_theme(theme);

    pick_list::Style {
        text_color: colors.text,
        placeholder_color: colors.dim,
        handle_color: colors.muted,
        background: Background::Color(colors.surface),
        border: border::rounded(8).width(1).color(colors.hairline),
    }
}

fn result_button_style(theme: &Theme, status: button::Status, selected: bool) -> button::Style {
    use button::Status;

    let colors = Colors::from_theme(theme);

    let background = if selected {
        match status {
            Status::Hovered | Status::Pressed => colors.pressed,
            _ => colors.selected,
        }
    } else {
        match status {
            Status::Hovered => colors.hovered,
            Status::Pressed => colors.pressed,
            _ => Color::TRANSPARENT,
        }
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color: colors.text,
        border: border::rounded(8)
            .width(if selected { 1.0 } else { 0.0 })
            .color(if selected {
                colors.row_border
            } else {
                Color::TRANSPARENT
            }),
        shadow: iced::Shadow::default(),
        snap: true,
    }
}

fn ghost_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let colors = Colors::from_theme(theme);

    button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => {
                Some(Background::Color(colors.hovered))
            }
            _ => None,
        },
        text_color: colors.muted,
        border: border::rounded(5),
        shadow: iced::Shadow::default(),
        snap: true,
    }
}

fn primary_button_style(theme: &Theme, status: button::Status) -> button::Style {
    let colors = Colors::from_theme(theme);
    let background = match status {
        button::Status::Hovered => blend(colors.accent, colors.on_accent, 0.12),
        button::Status::Pressed => blend(colors.accent, colors.on_accent, 0.22),
        _ => colors.accent,
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color: colors.on_accent,
        border: border::rounded(8),
        shadow: iced::Shadow::default(),
        snap: true,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use pirsa_core::settings::Settings;

    fn notepad() -> AppEntry {
        AppEntry::new(
            "Notepad".to_owned(),
            "Start Menu".to_owned(),
            LaunchTarget::Shell("test:notepad".to_owned()),
        )
    }

    fn app_state<'a>(
        apps: &'a [AppEntry],
        results: &'a [usize],
        icons: &'a HashMap<String, Handle>,
        theme: &'a Theme,
    ) -> ViewState<'a> {
        ViewState {
            apps,
            results,
            calc: None,
            icons,
            query: "",
            selected: 0,
            hotkey: HotkeyStatus::Ready,
            hotkey_choice: HotkeyChoice::AltSpace,
            error: None,
            warning: None,
            indexing: false,
            result_limit: MAX_RESULTS,
            theme,
            font: Font::with_name("Segoe UI"),
            app_name_font: Font::with_name("Segoe UI"),
            category_font: Font::with_name("Segoe UI"),
        }
    }

    #[test]
    fn launcher_rows_can_be_activated_headlessly() {
        let apps = [notepad()];
        let results = [0];
        let icons = HashMap::new();
        let theme = Theme::Dark;
        let mut ui = iced_tester::test::simulator(view(app_state(&apps, &results, &icons, &theme)));

        assert!(ui.find("Notepad").is_ok());
        ui.click("Notepad")
            .expect("Notepad row should be clickable");

        let messages: Vec<Message> = ui.into_messages().collect();
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, Message::Activate(key) if key == "notepad")),
            "clicking the row should activate the app by its stable key"
        );
    }

    #[test]
    fn settings_button_opens_settings_headlessly() {
        let apps = [notepad()];
        let results = [0];
        let icons = HashMap::new();
        let theme = Theme::Dark;
        let mut ui = iced_tester::test::simulator(view(app_state(&apps, &results, &icons, &theme)));

        ui.click("Settings")
            .expect("Settings button should be clickable");

        let messages: Vec<Message> = ui.into_messages().collect();
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, Message::OpenSettings)),
            "clicking Settings should emit OpenSettings"
        );
    }

    #[test]
    fn index_warning_is_shown_in_the_footer() {
        let apps = [notepad()];
        let results = [0];
        let icons = HashMap::new();
        let theme = Theme::Dark;
        let mut state = app_state(&apps, &results, &icons, &theme);
        state.warning = Some("Windows apps could not be indexed.");

        let mut ui = iced_tester::test::simulator(view(state));

        assert!(ui.find("Windows apps could not be indexed.").is_ok());
    }

    #[test]
    fn settings_view_builds_and_closes_headlessly() {
        let settings = Settings::default();
        let theme = Theme::Dark;
        let mut ui = iced_tester::test::simulator(settings_view(&settings, &theme, None));

        assert!(ui.find("UI font").is_ok());
        assert!(ui.find("App name font").is_ok());
        assert!(ui.find("Category font").is_ok());
        assert!(ui.find("Theme").is_ok());
        assert!(ui.find("Global hotkey").is_ok());
        ui.click("Close").expect("Close button should be clickable");

        let messages: Vec<SettingsMessage> = ui.into_messages().collect();
        assert!(
            messages
                .iter()
                .any(|message| matches!(message, SettingsMessage::Close)),
            "clicking Close should emit SettingsMessage::Close"
        );
    }

    #[test]
    fn settings_view_shows_hotkey_notice_headlessly() {
        let settings = Settings::default();
        let theme = Theme::Dark;
        let notice = "Ctrl+Space is already in use; select another hotkey.";
        let mut ui = iced_tester::test::simulator(settings_view(&settings, &theme, Some(notice)));

        assert!(ui.find(notice).is_ok());
    }
}
