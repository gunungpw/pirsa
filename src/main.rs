#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(windows))]
compile_error!("Pirsa only supports Windows.");

mod app;
mod platform;
mod search;
mod ui;
mod util;

use iced::window;
use iced::{Size, Theme};

use crate::app::Launcher;

fn main() -> iced::Result {
    std::panic::set_hook(Box::new(|info| {
        util::log(format!("panic: {info}"));
    }));

    let hotkey = platform::init();
    let show_on_start =
        std::env::args().any(|argument| argument == "--show") || !hotkey.available();
    platform::set_start_visible(show_on_start);

    iced::application(Launcher::boot, Launcher::update, Launcher::view)
        .title("Pirsa")
        .subscription(Launcher::subscription)
        .theme(theme)
        .default_font(iced::Font::with_name("Segoe UI"))
        .antialiasing(true)
        .window(window_settings(show_on_start))
        .run()
}

fn theme(_state: &Launcher) -> Theme {
    Theme::Dark
}

fn window_settings(visible: bool) -> window::Settings {
    let icon =
        window::icon::from_rgba(include_bytes!("../assets/icon-64.rgba").to_vec(), 64, 64).ok();

    let platform_specific = window::settings::PlatformSpecific {
        skip_taskbar: true,
        undecorated_shadow: true,
        corner_preference: window::settings::platform::CornerPreference::RoundSmall,
        ..window::settings::PlatformSpecific::default()
    };

    window::Settings {
        size: Size::new(ui::WINDOW_WIDTH, ui::WINDOW_HEIGHT),
        position: window::Position::Centered,
        resizable: false,
        minimizable: false,
        closeable: false,
        decorations: false,
        transparent: false,
        visible,
        icon,
        exit_on_close_request: false,
        platform_specific,
        ..window::Settings::default()
    }
}
