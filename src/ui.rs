//! The launcher's visual design.

use iced::border;
use iced::widget::image::Handle;
use iced::widget::text::Wrapping;
use iced::widget::{Image, Space, button, column, container, mouse_area, row, text, text_input};
use iced::{Alignment, Background, Border, Color, Element, Font, Length, Theme};

use crate::app::{Launcher, Message};
use crate::platform::HotkeyStatus;
use crate::platform::apps::{AppEntry, LaunchTarget};

pub const WINDOW_WIDTH: f32 = 820.0;
pub const WINDOW_HEIGHT: f32 = 540.0;
pub const INPUT_ID: &str = "pirsa-search-input";

pub const MAX_RESULTS: usize = 8;
const ROW_HEIGHT: f32 = 46.0;
const ROW_GAP: f32 = 2.0;

const BG_TOP: Color = Color::from_rgb8(0x15, 0x18, 0x20);
const BG_BOTTOM: Color = Color::from_rgb8(0x0a, 0x0c, 0x11);
const TEXT: Color = Color::from_rgb8(0xf3, 0xf5, 0xf9);
const MUTED: Color = Color::from_rgb8(0xb8, 0xbf, 0xcc);
const DIM: Color = Color::from_rgb8(0x8a, 0x93, 0xa3);
const ACCENT_SOFT: Color = Color::from_rgba(0.478, 0.635, 0.968, 0.20);
const SUCCESS: Color = Color::from_rgb8(0x73, 0xd8, 0xa3);
const WARNING: Color = Color::from_rgb8(0xe0, 0xaf, 0x68);
const DANGER: Color = Color::from_rgb8(0xf0, 0x7a, 0x8c);

const SELECTED: Color = Color::from_rgba8(255, 255, 255, 0.075);
const HOVERED: Color = Color::from_rgba8(255, 255, 255, 0.045);
const PRESSED: Color = Color::from_rgba8(255, 255, 255, 0.11);
const ROW_BORDER: Color = Color::from_rgba8(255, 255, 255, 0.07);
const HAIRLINE: Color = Color::from_rgba8(255, 255, 255, 0.08);

const UI_FONT: Font = Font::with_name("Segoe UI");
const MONO_FONT: Font = Font::with_name("Cascadia Mono");

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

pub fn view(app: &Launcher) -> Element<'_, Message> {
    let content = column![
        search_row(app),
        Space::new().height(14),
        section_label(app),
        Space::new().height(6),
        result_list(app),
        Space::new().height(Length::Fill),
        footer(app),
    ]
    .width(Length::Fill)
    .height(Length::Fill);

    container(content)
        .width(Length::Fill)
        .height(Length::Fill)
        .padding([18, 22])
        .style(root_style)
        .into()
}

fn search_row(app: &Launcher) -> Element<'_, Message> {
    let field = text_input("Search apps…", &app.query)
        .id(INPUT_ID)
        .on_input(Message::QueryChanged)
        .padding([0, 0])
        .size(20)
        .font(UI_FONT)
        .style(search_input_style)
        .width(Length::Fill);

    let clear: Element<'_, Message> = if app.query.is_empty() {
        Space::new().width(22).into()
    } else {
        button(text("×").size(15).font(UI_FONT).color(MUTED))
            .on_press(Message::ClearQuery)
            .width(24)
            .height(24)
            .padding(0)
            .style(ghost_button_style)
            .into()
    };

    row![
        field,
        clear,
        hotkey_keycap(app.hotkey),
        button(text("×").size(17).font(UI_FONT))
            .on_press(Message::Hide)
            .width(28)
            .height(28)
            .padding(0)
            .style(close_button_style),
    ]
    .width(Length::Fill)
    .height(54)
    .spacing(12)
    .align_y(Alignment::Center)
    .into()
}

fn section_label(app: &Launcher) -> Element<'_, Message> {
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
        details = details.push(text("PARTIAL INDEX").size(9).font(MONO_FONT).color(WARNING));
        details = details.push(Space::new().width(10));
    }

    details = details.push(text(count).size(9).font(MONO_FONT).color(DIM));

    mouse_area(
        row![
            text(title).size(13).font(medium_font()).color(MUTED),
            details
        ]
        .width(Length::Fill)
        .height(18)
        .align_y(Alignment::Center),
    )
    .on_press(Message::DragWindow)
    .into()
}

fn result_list(app: &Launcher) -> Element<'_, Message> {
    let height = MAX_RESULTS as f32 * ROW_HEIGHT + (MAX_RESULTS.saturating_sub(1)) as f32 * ROW_GAP;

    let content: Element<'_, Message> = if app.results.is_empty() {
        empty_state(app)
    } else {
        let rows: Vec<Element<'_, Message>> = app
            .results
            .iter()
            .take(MAX_RESULTS)
            .enumerate()
            .map(|(position, index)| result_row(app, *index, position))
            .collect();

        column(rows).spacing(ROW_GAP).into()
    };

    container(content)
        .width(Length::Fill)
        .height(height)
        .align_y(Alignment::Start)
        .into()
}

fn result_row(app: &Launcher, app_index: usize, position: usize) -> Element<'_, Message> {
    let Some(entry) = app.apps.get(app_index) else {
        return Space::new().into();
    };

    let selected = position == app.selected;

    // `button` places its content at the top-left, so the row has to claim the
    // full height and center its items itself. Otherwise the selected
    // highlight extends well below the icon and label.
    let content = row![
        icon_element(entry, app.icons.get(&entry.name)),
        row![
            text(entry.name.as_str())
                .size(15)
                .font(semibold_font())
                .color(TEXT)
                .wrapping(Wrapping::None),
            text(inline_detail(entry))
                .size(11)
                .font(UI_FONT)
                .color(DIM)
                .wrapping(Wrapping::None),
        ]
        .spacing(9)
        .align_y(Alignment::Center)
        .width(Length::Fill),
        container(
            text(entry_kind(entry))
                .size(11)
                .font(UI_FONT)
                .color(if selected { MUTED } else { DIM })
        )
        .width(72)
        .align_x(Alignment::End),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .align_y(Alignment::Center)
    .spacing(12);

    button(content)
        .on_press(Message::Activate(position))
        .width(Length::Fill)
        .height(ROW_HEIGHT)
        .padding([0, 12])
        .style(move |theme, status| result_button_style(theme, status, selected))
        .into()
}

fn empty_state(app: &Launcher) -> Element<'_, Message> {
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
            text(title).size(15).font(medium_font()).color(TEXT),
            text(subtitle).size(11).font(UI_FONT).color(DIM),
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

fn footer(app: &Launcher) -> Element<'_, Message> {
    let quit = button(text("Quit").size(10).font(UI_FONT))
        .on_press(Message::Quit)
        .padding([3, 7])
        .style(ghost_button_style);

    if let Some(error) = &app.error {
        return row![
            status_dot(DANGER),
            text(error.as_str()).size(10).font(UI_FONT).color(DANGER),
            Space::new().width(Length::Fill),
            quit,
        ]
        .spacing(7)
        .align_y(Alignment::Center)
        .height(24)
        .into();
    }

    let (color, label) = match app.hotkey {
        HotkeyStatus::Ready => (SUCCESS, "Alt+Space"),
        HotkeyStatus::Unavailable => (DANGER, "Alt+Space unavailable"),
        HotkeyStatus::Pending => (DIM, "Starting…"),
    };

    row![
        status_dot(color),
        text(label).size(10).font(UI_FONT).color(MUTED),
        Space::new().width(Length::Fill),
        text("Open").size(10.5).font(UI_FONT).color(DIM),
        keycap("↵"),
        Space::new().width(10),
        text("Hide").size(10.5).font(UI_FONT).color(DIM),
        keycap("Esc"),
        Space::new().width(14),
        quit,
    ]
    .spacing(6)
    .align_y(Alignment::Center)
    .height(24)
    .into()
}

fn hotkey_keycap(status: HotkeyStatus) -> Element<'static, Message> {
    let label = if status.available() {
        "ALT + SPACE"
    } else {
        "NO HOTKEY"
    };

    keycap(label)
}

fn keycap(label: &'static str) -> Element<'static, Message> {
    container(text(label).size(9).font(MONO_FONT).color(MUTED))
        .height(20)
        .padding([2, 6])
        .align_x(Alignment::Center)
        .align_y(Alignment::Center)
        .style(keycap_style)
        .into()
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
fn icon_element<'a>(entry: &'a AppEntry, icon: Option<&Handle>) -> Element<'a, Message> {
    match icon {
        Some(handle) => Image::new(handle.clone())
            .width(30)
            .height(30)
            .border_radius(7)
            .into(),
        None => icon_tile(&entry.name),
    }
}

fn icon_tile(name: &str) -> Element<'_, Message> {
    let color = icon_color(name);
    let initial = initial(name);

    container(
        text(initial)
            .size(12.5)
            .font(bold_font())
            .color(Color::WHITE),
    )
    .width(30)
    .height(30)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(move |_theme| container::Style {
        background: Some(color.into()),
        border: border::rounded(7)
            .width(1)
            .color(Color::from_rgba(1.0, 1.0, 1.0, 0.08)),
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
    entry
        .detail
        .strip_prefix("Start Menu · ")
        .unwrap_or(entry.detail.as_str())
}

fn entry_kind(entry: &AppEntry) -> &'static str {
    match &entry.target {
        LaunchTarget::Shortcut(_) => "Application",
        LaunchTarget::Executable(_) => "Desktop app",
        LaunchTarget::Shell(_) => "Store app",
    }
}

fn root_style(_theme: &Theme) -> container::Style {
    let background = iced::gradient::Linear::new(iced::Radians(std::f32::consts::FRAC_PI_2))
        .add_stop(0.0, BG_TOP)
        .add_stop(1.0, BG_BOTTOM);

    container::Style {
        background: Some(background.into()),
        text_color: Some(TEXT),
        border: border::rounded(4)
            .width(1)
            .color(Color::from_rgba8(255, 255, 255, 0.08)),
        ..container::Style::default()
    }
}

fn search_input_style(_theme: &Theme, _status: text_input::Status) -> text_input::Style {
    text_input::Style {
        background: Background::Color(Color::TRANSPARENT),
        border: Border::default(),
        icon: MUTED,
        placeholder: DIM,
        value: TEXT,
        selection: ACCENT_SOFT,
    }
}

fn keycap_style(_theme: &Theme) -> container::Style {
    container::Style {
        background: Some(Background::Color(Color::from_rgba8(255, 255, 255, 0.045))),
        border: border::rounded(5).width(1).color(HAIRLINE),
        ..container::Style::default()
    }
}

fn result_button_style(_theme: &Theme, status: button::Status, selected: bool) -> button::Style {
    use button::Status;

    let background = if selected {
        match status {
            Status::Hovered | Status::Pressed => PRESSED,
            _ => SELECTED,
        }
    } else {
        match status {
            Status::Hovered => HOVERED,
            Status::Pressed => PRESSED,
            _ => Color::TRANSPARENT,
        }
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color: TEXT,
        border: border::rounded(8)
            .width(if selected { 1.0 } else { 0.0 })
            .color(if selected {
                ROW_BORDER
            } else {
                Color::TRANSPARENT
            }),
        shadow: iced::Shadow::default(),
        snap: true,
    }
}

fn close_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    let background = match status {
        button::Status::Hovered => {
            Some(Background::Color(Color::from_rgba(0.94, 0.48, 0.55, 0.16)))
        }
        button::Status::Pressed => {
            Some(Background::Color(Color::from_rgba(0.94, 0.48, 0.55, 0.24)))
        }
        _ => None,
    };

    button::Style {
        background,
        text_color: if matches!(status, button::Status::Hovered | button::Status::Pressed) {
            DANGER
        } else {
            MUTED
        },
        border: border::rounded(6),
        shadow: iced::Shadow::default(),
        snap: true,
    }
}

fn ghost_button_style(_theme: &Theme, status: button::Status) -> button::Style {
    button::Style {
        background: match status {
            button::Status::Hovered | button::Status::Pressed => Some(Background::Color(HOVERED)),
            _ => None,
        },
        text_color: MUTED,
        border: border::rounded(5),
        shadow: iced::Shadow::default(),
        snap: true,
    }
}

fn medium_font() -> Font {
    Font {
        weight: iced::font::Weight::Medium,
        ..UI_FONT
    }
}

fn semibold_font() -> Font {
    Font {
        weight: iced::font::Weight::Semibold,
        ..UI_FONT
    }
}

fn bold_font() -> Font {
    Font {
        weight: iced::font::Weight::Bold,
        ..UI_FONT
    }
}
