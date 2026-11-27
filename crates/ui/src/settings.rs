//! The settings window.

use iced::widget::{
    Space, button, checkbox, column, container, mouse_area, pick_list, row, scrollable, slider,
    text, text_input,
};
use iced::{Alignment, Element, Font, Length};

use pirsa_core::search_scope::{DriveDetails, DriveKind, DriveMode};
use pirsa_core::settings::{HotkeyChoice, ThemeChoice};

use super::style::*;
use super::*;

pub fn settings_view<'a>(view: SettingsView<'a>) -> Element<'a, SettingsMessage> {
    let SettingsView {
        settings,
        fonts,
        scope,
        drives,
        exclude_draft,
        index_status,
        theme,
        notice,
        category,
    } = view;

    let colors = Colors::from_theme(theme);
    let font = Font::with_name(settings.font.name());
    let app_name_font = Font::with_name(settings.app_name_font.name());
    let category_font = Font::with_name(settings.category_font.name());

    let font_picker = pick_list(fonts, Some(settings.font), SettingsMessage::FontChanged)
        .text_size(14)
        .padding([8, 10])
        .font(font)
        .width(Length::Fill)
        .style(pick_list_style);

    let app_name_font_picker = pick_list(
        fonts,
        Some(settings.app_name_font),
        SettingsMessage::AppNameFontChanged,
    )
    .text_size(14)
    .padding([8, 10])
    .font(font)
    .width(Length::Fill)
    .style(pick_list_style);

    let category_font_picker = pick_list(
        fonts,
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

    let notice: Option<Element<'a, SettingsMessage>> =
        notice.map(|notice| text(notice).size(11).font(font).color(colors.danger).into());

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

    let appearance = settings_pane(
        "Fonts and colors for the launcher. Custom fonts come from ~/.local/share/fonts.",
        vec![
            settings_field(
                "UI font",
                "Used across the launcher and this window.",
                font_picker.into(),
                colors,
                font,
            ),
            settings_field(
                "App name font",
                "Applied to the app name in each result.",
                app_name_font_picker.into(),
                colors,
                font,
            ),
            settings_field(
                "Category font",
                "Applied to the category hint beside each name.",
                category_font_picker.into(),
                colors,
                font,
            ),
            settings_field(
                "Theme",
                "Colors for the launcher and this window.",
                theme_picker.into(),
                colors,
                font,
            ),
        ],
        colors,
        font,
    );

    let mut behavior_items = vec![settings_field(
        "Global hotkey",
        "Summons the launcher from anywhere.",
        hotkey_picker.into(),
        colors,
        font,
    )];

    if let Some(notice) = notice {
        behavior_items.push(notice);
    }

    let behavior = settings_pane(
        "How the launcher responds to input.",
        behavior_items,
        colors,
        font,
    );

    let folder_search = settings_pane(
        "Which folders the search index covers.",
        search_section_items(scope, drives, exclude_draft, index_status, colors, font),
        colors,
        font,
    );

    let preview_pane = settings_pane(
        "A sample row using the current fonts and theme.",
        vec![preview.into()],
        colors,
        font,
    );

    // First column: the categories. Second column: the settings of the selected
    // category, scrolled because the window has a fixed size.
    let mut nav_items: Vec<Element<'a, SettingsMessage>> = Vec::new();

    for item in SettingsCategory::ALL {
        let is_selected = *item == category;
        let label_font = if is_selected {
            semibold_font(font)
        } else {
            font
        };

        nav_items.push(
            button(
                text(item.label())
                    .size(12)
                    .font(label_font)
                    .width(Length::Fill),
            )
            .on_press(SettingsMessage::CategorySelected(*item))
            .width(Length::Fill)
            .padding([7, 10])
            .style(move |theme, status| settings_nav_style(theme, status, is_selected))
            .into(),
        );
    }

    let nav = container(column(nav_items).spacing(2).width(Length::Fill))
        .width(SETTINGS_NAV_WIDTH)
        .height(Length::Fill)
        .padding(6)
        .style(settings_card_style);

    let detail: Element<'a, SettingsMessage> = match category {
        SettingsCategory::Appearance => appearance,
        SettingsCategory::Behavior => behavior,
        SettingsCategory::FolderSearch => folder_search,
        SettingsCategory::Preview => preview_pane,
    };

    let body = row![
        nav,
        Space::new().width(14),
        scrollable(detail).width(Length::Fill).height(Length::Fill),
    ]
    .width(Length::Fill)
    .height(Length::Fill)
    .align_y(Alignment::Start);

    // The window has no title bar, so the header doubles as the drag handle and
    // carries its own dismiss button.
    let header_title = mouse_area(
        column![
            text("Settings")
                .size(24)
                .font(bold_font(font))
                .color(colors.text),
            text("Changes apply immediately.")
                .size(11)
                .font(font)
                .color(colors.dim),
        ]
        .spacing(2)
        .width(Length::Fill),
    )
    .on_press(SettingsMessage::DragWindow);

    let dismiss = button(text("×").size(18).font(font).color(colors.muted))
        .on_press(SettingsMessage::Close)
        .padding([2, 10])
        .style(ghost_button_style);

    let content = column![
        row![header_title, Space::new().width(12), dismiss]
            .width(Length::Fill)
            .align_y(Alignment::Start),
        Space::new().height(16),
        body,
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
        .padding([20, 22])
        .style(settings_root_style)
        .into()
}

/// Folder-search configuration: scope, drives, extra folders, exclusions,
/// depth, hidden folders, and the index state.
fn search_section_items<'a>(
    scope: &'a SearchScope,
    drives: &'a [DriveDetails],
    exclude_draft: &'a str,
    index_status: IndexStatus,
    colors: Colors,
    font: Font,
) -> Vec<Element<'a, SettingsMessage>> {
    let preset_picker = pick_list(
        ScopePreset::ALL,
        Some(ScopePreset::of(&scope.drives)),
        SettingsMessage::ScopePresetChanged,
    )
    .text_size(14)
    .padding([8, 10])
    .font(font)
    .width(Length::Fill)
    .style(pick_list_style);

    let mut drive_rows: Vec<Element<'a, SettingsMessage>> = Vec::new();

    for drive in drives {
        let letter = drive.info.letter;
        let selected = drive_selected(&scope.drives, drive);

        drive_rows.push(
            checkbox(selected)
                .label(drive.summary())
                .on_toggle(move |checked| SettingsMessage::DriveToggled(letter, checked))
                .text_size(12)
                .font(font)
                .into(),
        );
    }

    if drive_rows.is_empty() {
        drive_rows.push(
            text("No searchable drives were found.")
                .size(11)
                .font(font)
                .color(colors.dim)
                .into(),
        );
    }

    let mut include_rows: Vec<Element<'a, SettingsMessage>> = Vec::new();

    for path in &scope.include {
        include_rows.push(
            row![
                text(path.display().to_string())
                    .size(11)
                    .font(font)
                    .color(colors.muted)
                    .width(Length::Fill),
                remove_button(font, colors, SettingsMessage::IncludeRemoved(path.clone())),
            ]
            .width(Length::Fill)
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        );
    }

    let add_folder = button(text("Add folder…").size(11).font(font).color(colors.muted))
        .on_press(SettingsMessage::AddFolderRequested)
        .padding([3, 8])
        .style(ghost_button_style);

    let exclude_input = text_input("Add an exclusion…", exclude_draft)
        .on_input(SettingsMessage::ExcludeDraftChanged)
        .on_submit(SettingsMessage::ExcludeAdded)
        .padding([7, 9])
        .size(12)
        .font(font)
        .style(field_input_style)
        .width(Length::Fill);

    let add_exclude = button(text("Add").size(11).font(font).color(colors.muted))
        .on_press(SettingsMessage::ExcludeAdded)
        .padding([3, 10])
        .style(ghost_button_style);

    let mut rule_rows: Vec<Element<'a, SettingsMessage>> = Vec::new();

    for rule in &scope.exclude {
        rule_rows.push(
            row![
                text(rule.clone())
                    .size(11)
                    .font(font)
                    .color(colors.muted)
                    .width(Length::Fill),
                remove_button(font, colors, SettingsMessage::ExcludeRemoved(rule.clone())),
            ]
            .width(Length::Fill)
            .spacing(8)
            .align_y(Alignment::Center)
            .into(),
        );
    }

    let depth = slider(1.0_f32..=12.0_f32, scope.depth as f32, |value: f32| {
        SettingsMessage::DepthChanged(value.round() as u8)
    })
    .step(1.0_f32)
    .on_release(SettingsMessage::ScopeCommitted)
    .width(Length::Fill);

    let hidden = checkbox(scope.include_hidden)
        .label("Include hidden folders")
        .on_toggle(SettingsMessage::HiddenToggled)
        .text_size(12)
        .font(font);

    let status_text = if index_status.building {
        format!("Indexing… {} folders so far", index_status.folders)
    } else if index_status.ready {
        format!("{} folders indexed", index_status.folders)
    } else {
        "Index not built yet".to_owned()
    };

    let rebuild = button(text("Rebuild").size(11).font(font).color(colors.muted))
        .on_press(SettingsMessage::RebuildIndex)
        .padding([3, 10])
        .style(ghost_button_style);

    let items: Vec<Element<'a, SettingsMessage>> = vec![
        settings_field(
            "Scope",
            "Which drives the folder index searches.",
            preset_picker.into(),
            colors,
            font,
        ),
        settings_subgroup(
            "Drives",
            vec![column(drive_rows).spacing(4).into()],
            colors,
            font,
        ),
        settings_subgroup(
            "Extra folders",
            vec![column(include_rows).spacing(2).into(), add_folder.into()],
            colors,
            font,
        ),
        settings_subgroup(
            "Excluded names or paths",
            vec![
                row![exclude_input, add_exclude]
                    .width(Length::Fill)
                    .spacing(6)
                    .align_y(Alignment::Center)
                    .into(),
                column(rule_rows).spacing(2).into(),
            ],
            colors,
            font,
        ),
        settings_field(
            "Search depth",
            "How many folder levels the index descends.",
            depth.into(),
            colors,
            font,
        ),
        hidden.into(),
        row![
            text(status_text)
                .size(10)
                .font(font)
                .color(colors.dim)
                .width(Length::Fill),
            rebuild,
        ]
        .width(Length::Fill)
        .align_y(Alignment::Center)
        .into(),
    ];

    items
}

fn drive_selected(mode: &DriveMode, drive: &DriveDetails) -> bool {
    match mode {
        DriveMode::Profile => false,
        DriveMode::All => true,
        DriveMode::Fixed => drive.info.kind == DriveKind::Fixed,
        DriveMode::Selected(letters) => letters.contains(&drive.info.letter),
    }
}

fn remove_button<'a>(
    font: Font,
    colors: Colors,
    message: SettingsMessage,
) -> Element<'a, SettingsMessage> {
    button(text("×").size(13).font(font).color(colors.muted))
        .on_press(message)
        .padding([0, 6])
        .style(ghost_button_style)
        .into()
}

fn settings_field<'a>(
    label: &'a str,
    hint: &'a str,
    control: Element<'a, SettingsMessage>,
    colors: Colors,
    font: Font,
) -> Element<'a, SettingsMessage> {
    let mut rows = column![text(label).size(11).font(font).color(colors.muted)]
        .spacing(3)
        .width(Length::Fill);

    if !hint.is_empty() {
        rows = rows.push(text(hint).size(10).font(font).color(colors.dim));
    }

    rows.push(Space::new().height(3))
        .push(control)
        .width(Length::Fill)
        .into()
}

/// A labelled group inside a category, used for the list-style controls in the
/// folder-search section.
fn settings_subgroup<'a>(
    label: &'a str,
    items: Vec<Element<'a, SettingsMessage>>,
    colors: Colors,
    font: Font,
) -> Element<'a, SettingsMessage> {
    let mut rows = column![text(label).size(11).font(font).color(colors.muted)]
        .spacing(4)
        .width(Length::Fill);

    for item in items {
        rows = rows.push(item);
    }

    rows.into()
}

/// The card that holds the settings of the selected category.
fn settings_pane<'a>(
    description: &'a str,
    items: Vec<Element<'a, SettingsMessage>>,
    colors: Colors,
    font: Font,
) -> Element<'a, SettingsMessage> {
    let mut rows = column![
        text(description).size(11).font(font).color(colors.dim),
        Space::new().height(14),
    ]
    .width(Length::Fill);

    for (position, item) in items.into_iter().enumerate() {
        if position > 0 {
            rows = rows.push(Space::new().height(12));
        }

        rows = rows.push(item);
    }

    container(rows)
        .width(Length::Fill)
        .padding(14)
        .style(settings_card_style)
        .into()
}
