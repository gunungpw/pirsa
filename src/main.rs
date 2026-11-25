#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

#[cfg(not(windows))]
compile_error!("Pirsa only supports Windows.");

mod app;

use iced::{Font, window};
use pirsa_core::settings::Settings;
use pirsa_core::util;
use pirsa_platform as platform;

use crate::app::Launcher;

fn main() -> iced::Result {
    std::panic::set_hook(Box::new(|info| {
        util::log(format!("panic: {info}"));
    }));

    let hotkey = platform::init();
    let show_on_start =
        std::env::args().any(|argument| argument == "--show") || !hotkey.available();
    platform::set_start_visible(show_on_start);

    let settings = Settings::load();
    let default_font = Font::with_name(settings.font.name());
    let boot_settings = settings.clone();

    iced::daemon(
        move || Launcher::boot(boot_settings.clone()),
        Launcher::update,
        Launcher::view,
    )
    .title(|state: &Launcher, window| state.title(window))
    .subscription(Launcher::subscription)
    .theme(|state: &Launcher, _window: window::Id| Some(state.window_theme()))
    .default_font(default_font)
    .antialiasing(true)
    .run()
}
