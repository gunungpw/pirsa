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
        let profile = if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        };
        let version = env!("CARGO_PKG_VERSION");
        let backtrace = std::backtrace::Backtrace::force_capture();

        util::log(format!(
            "panic in Pirsa {version} ({profile}): {info}\n{backtrace}"
        ));

        // Release builds abort on panic, so show why before dying. Debug builds
        // keep the terminal backtrace instead of a modal dialog.
        #[cfg(not(debug_assertions))]
        platform::show_fatal_error(&format!(
            "Pirsa {version} crashed and must close.\n\n{info}\n\nDetails were written to:\n%LOCALAPPDATA%\\Pirsa\\launcher.log"
        ));
    }));

    if let Some(directory) = util::data_directory() {
        util::log(format!("data directory: {}", directory.display()));
    }

    let settings = Settings::load();
    let hotkey = platform::init(settings.hotkey);
    let show_on_start =
        std::env::args().any(|argument| argument == "--show") || !hotkey.available();
    platform::set_start_visible(show_on_start);

    let default_font = Font::with_name(settings.font.name());
    let boot_settings = settings.clone();
    let local_fonts = platform::local_fonts();

    let mut daemon = iced::daemon(
        move || Launcher::boot(boot_settings.clone()),
        Launcher::update,
        Launcher::view,
    )
    .title(|state: &Launcher, window| state.title(window))
    .subscription(Launcher::subscription)
    .theme(|state: &Launcher, _window: window::Id| Some(state.window_theme()))
    .default_font(default_font)
    .antialiasing(true);

    // Families in ~/.local/share/fonts are loaded alongside the system fonts.
    for font in &local_fonts.files {
        daemon = daemon.font(font.as_slice());
    }

    daemon.run()
}
