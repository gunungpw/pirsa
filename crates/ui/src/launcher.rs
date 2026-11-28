//! The launcher window: search field, calculation card, and ranked rows.

use iced::border;
use iced::widget::image::Handle;
use iced::widget::text::Wrapping;
use iced::widget::{
    Image, Space, button, column, container, mouse_area, row, svg, text, text_input,
};
use iced::{Alignment, Color, Element, Font, Length};

use pirsa_core::calc::{CalcKind, CalcResult};
use pirsa_core::folders::{self, FolderEntry};
use pirsa_core::{AppEntry, LaunchTarget, ResultRow};

use super::style::*;
use super::*;

/// Material Symbols folder glyph, centred in the same 30 px slot as app icons.
const FOLDER_ICON: &[u8] = include_bytes!("../../../assets/material-symbols--folder.svg");

pub fn view(app: ViewState<'_>) -> Element<'_, Message> {
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

fn search_row(app: ViewState<'_>, colors: Colors, font: Font) -> Element<'_, Message> {
    let folder_mode = folders::folder_term(app.query, &app.scope.trigger).is_some();
    let placeholder = if folder_mode {
        "Search folders…".to_owned()
    } else {
        format!(
            "Search apps, calculate, convert…  ({} for folders)",
            app.scope.trigger
        )
    };
    let field = text_input(&placeholder, app.query)
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

fn section_label(app: ViewState<'_>, colors: Colors, font: Font) -> Element<'_, Message> {
    let folder_term = folders::folder_term(app.query, &app.scope.trigger);
    let folder_mode = folder_term.is_some();
    let query_empty = app.query.trim().is_empty();
    let indexing = app.indexing && app.apps.is_empty() && !folder_mode;

    let title = if folder_mode {
        "Folders"
    } else if indexing {
        "Indexing"
    } else if query_empty {
        "All apps"
    } else {
        "Results"
    };

    let count = if folder_mode {
        match folder_term {
            Some("") => "TYPE TO SEARCH".to_owned(),
            _ => format!("{} FOUND", app.rows.len()),
        }
    } else if indexing {
        "SCANNING…".to_owned()
    } else if query_empty {
        format!("{} APPS", app.apps.len())
    } else {
        format!("{} FOUND", app.rows.len())
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

fn result_list(app: ViewState<'_>, colors: Colors, font: Font) -> Element<'_, Message> {
    let limit = app.result_limit;
    let height = limit as f32 * ROW_HEIGHT + (limit.saturating_sub(1)) as f32 * ROW_GAP;

    let content: Element<'_, Message> = if app.rows.is_empty() {
        if app.calc.is_some() {
            Space::new().into()
        } else {
            empty_state(app, colors, font)
        }
    } else {
        let rows: Vec<Element<'_, Message>> = app
            .rows
            .iter()
            .take(limit)
            .enumerate()
            .filter_map(|(position, row)| match *row {
                ResultRow::App(index) => Some(result_row(app, index, position, colors, font)),
                ResultRow::Folder(index) => app
                    .folders
                    .get(index)
                    .map(|folder| folder_row(app, folder, position, colors, font)),
            })
            .collect();

        column(rows).spacing(ROW_GAP).into()
    };

    container(content)
        .width(Length::Fill)
        .height(height)
        .align_y(Alignment::Start)
        .into()
}

fn folder_row<'a>(
    app: ViewState<'a>,
    folder: &'a FolderEntry,
    position: usize,
    colors: Colors,
    font: Font,
) -> Element<'a, Message> {
    let selected = position == app.selected;

    let content = row![
        folder_icon(colors),
        row![
            text(folder.name())
                .size(15)
                .font(semibold_font(app.app_name_font))
                .color(colors.text)
                .wrapping(Wrapping::None),
            text(folder.parent_label())
                .size(11)
                .font(app.category_font)
                .color(colors.dim)
                .wrapping(Wrapping::None),
        ]
        .spacing(9)
        .align_y(Alignment::End)
        .width(Length::Fill),
        container(text("Folder").size(11).font(font).color(if selected {
            colors.muted
        } else {
            colors.dim
        }))
        .width(72)
        .align_x(Alignment::End),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .align_y(Alignment::Center)
    .spacing(12);

    button(content)
        .on_press(Message::ActivateFolder(folder.path().to_path_buf()))
        .width(Length::Fill)
        .height(ROW_HEIGHT)
        .padding([0, 12])
        .style(move |theme, status| result_button_style(theme, status, selected))
        .into()
}

fn result_row(
    app: ViewState<'_>,
    app_index: usize,
    position: usize,
    colors: Colors,
    font: Font,
) -> Element<'_, Message> {
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

fn empty_state(app: ViewState<'_>, colors: Colors, font: Font) -> Element<'_, Message> {
    let folder_term = folders::folder_term(app.query, &app.scope.trigger);
    let (title, subtitle) = if let Some(term) = folder_term {
        if term.is_empty() {
            (
                format!("Type a folder name after “{}”", app.scope.trigger),
                "Folder search is separate from app search.".to_owned(),
            )
        } else {
            (
                format!("No folders for “{term}”"),
                "Try a different folder name, or press Esc to close.".to_owned(),
            )
        }
    } else if app.indexing {
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

fn footer(app: ViewState<'_>, colors: Colors, font: Font) -> Element<'_, Message> {
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

/// A folder result uses the Material Symbols folder glyph rather than an
/// initial tile.
fn folder_icon(colors: Colors) -> Element<'static, Message> {
    container(
        svg(svg::Handle::from_memory(FOLDER_ICON))
            .width(22)
            .height(22)
            .style(move |_theme, _status| svg::Style {
                color: Some(colors.accent),
            }),
    )
    .width(30)
    .height(30)
    .align_x(Alignment::Center)
    .align_y(Alignment::Center)
    .style(folder_tile_style)
    .into()
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
        .map_or_else(
            || "?".to_owned(),
            |character| character.to_uppercase().to_string(),
        )
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
