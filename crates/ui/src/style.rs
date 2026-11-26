//! Widget styles, all derived from the active iced theme.

use iced::widget::{button, container, pick_list, text_input};
use iced::{Background, Border, Color, Theme, border};

use super::*;

pub(super) fn field_input_style(theme: &Theme, _status: text_input::Status) -> text_input::Style {
    let colors = Colors::from_theme(theme);

    text_input::Style {
        background: Background::Color(colors.surface),
        border: border::rounded(8).width(1).color(colors.hairline),
        icon: colors.muted,
        placeholder: colors.dim,
        value: colors.text,
        selection: colors.accent_soft,
    }
}

pub(super) fn settings_nav_style(
    theme: &Theme,
    status: button::Status,
    selected: bool,
) -> button::Style {
    use button::Status;

    let colors = Colors::from_theme(theme);

    let background = if selected {
        colors.accent_soft
    } else {
        match status {
            Status::Hovered => colors.hovered,
            Status::Pressed => colors.pressed,
            _ => Color::TRANSPARENT,
        }
    };

    button::Style {
        background: Some(Background::Color(background)),
        text_color: if selected {
            colors.accent
        } else {
            colors.muted
        },
        border: border::rounded(7),
        shadow: iced::Shadow::default(),
        snap: true,
    }
}

pub(super) fn root_style(theme: &Theme) -> container::Style {
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

pub(super) fn settings_root_style(theme: &Theme) -> container::Style {
    let colors = Colors::from_theme(theme);
    let background = iced::gradient::Linear::new(iced::Radians(std::f32::consts::FRAC_PI_2))
        .add_stop(0.0, colors.background_top)
        .add_stop(1.0, colors.background_bottom);

    container::Style {
        background: Some(background.into()),
        text_color: Some(colors.text),
        border: border::rounded(10).width(1).color(colors.hairline),
        ..container::Style::default()
    }
}

pub(super) fn settings_card_style(theme: &Theme) -> container::Style {
    let colors = Colors::from_theme(theme);

    container::Style {
        background: Some(Background::Color(colors.surface)),
        text_color: Some(colors.text),
        border: border::rounded(10).width(1).color(colors.hairline),
        ..container::Style::default()
    }
}

pub(super) fn preview_style(theme: &Theme) -> container::Style {
    let colors = Colors::from_theme(theme);

    container::Style {
        background: Some(Background::Color(colors.surface)),
        border: border::rounded(10).width(1).color(colors.hairline),
        ..container::Style::default()
    }
}

pub(super) fn calc_badge_style(theme: &Theme) -> container::Style {
    let colors = Colors::from_theme(theme);

    container::Style {
        background: Some(Background::Color(colors.accent)),
        border: border::rounded(7).width(1).color(colors.hairline),
        ..container::Style::default()
    }
}

pub(super) fn search_input_style(theme: &Theme, _status: text_input::Status) -> text_input::Style {
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

pub(super) fn keycap_style(theme: &Theme) -> container::Style {
    let colors = Colors::from_theme(theme);

    container::Style {
        background: Some(Background::Color(colors.hovered)),
        border: border::rounded(5).width(1).color(colors.hairline),
        ..container::Style::default()
    }
}

pub(super) fn pick_list_style(theme: &Theme, _status: pick_list::Status) -> pick_list::Style {
    let colors = Colors::from_theme(theme);

    pick_list::Style {
        text_color: colors.text,
        placeholder_color: colors.dim,
        handle_color: colors.muted,
        background: Background::Color(colors.surface),
        border: border::rounded(8).width(1).color(colors.hairline),
    }
}

pub(super) fn result_button_style(
    theme: &Theme,
    status: button::Status,
    selected: bool,
) -> button::Style {
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

pub(super) fn ghost_button_style(theme: &Theme, status: button::Status) -> button::Style {
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

pub(super) fn primary_button_style(theme: &Theme, status: button::Status) -> button::Style {
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
