//! Windows integration: global hotkeys, single instance, Start Menu apps, and
//! native window control.

pub mod window;

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use pirsa_core::HotkeyStatus;
use pirsa_core::settings::HotkeyChoice;
use tokio::sync::Notify;

mod apps;
mod com;
mod hotkey;
mod icons;
mod instance;
mod shell;
mod tray;

pub use apps::launch;
pub use hotkey::status as hotkey_status;
pub use shell::{extract_icons, scan};

static TOGGLE_REQUESTED: AtomicBool = AtomicBool::new(false);
static SHOW_REQUESTED: AtomicBool = AtomicBool::new(false);
static SETTINGS_REQUESTED: AtomicBool = AtomicBool::new(false);
static QUIT_REQUESTED: AtomicBool = AtomicBool::new(false);
static START_VISIBLE: AtomicBool = AtomicBool::new(false);
static SIGNAL: OnceLock<Notify> = OnceLock::new();

/// Requests collected from the hotkey thread, a second launcher process, or the
/// installer. Reading them clears each flag, so a request is handled once.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct Requests {
    pub show: bool,
    pub toggle: bool,
    pub settings: bool,
    pub quit: bool,
}

/// Must be called once, before the iced application is created.
///
/// If another launcher is already running, this notifies it and exits.
pub fn init(choice: HotkeyChoice) -> HotkeyStatus {
    if !instance::claim() {
        instance::signal_existing();
        std::process::exit(0);
    }

    shell::start();
    tray::start();

    let (show, quit) = instance::create_events();
    instance::watch(show, "show", request_show);
    instance::watch(quit, "quit", request_quit);

    let status = hotkey::start(choice);
    tray::set_tooltip(&format!("Pirsa — {}", choice.label()));

    pirsa_core::util::log(format!("started (hotkey: {status:?}, {})", choice.label()));

    status
}

/// Changes the system-wide hotkey and keeps the tray tooltip in sync.
pub fn set_hotkey(choice: HotkeyChoice) -> HotkeyStatus {
    let status = hotkey::set(choice);

    if status.available() {
        tray::set_tooltip(&format!("Pirsa — {}", choice.label()));
    }

    status
}

pub fn set_start_visible(visible: bool) {
    START_VISIBLE.store(visible, Ordering::Relaxed);
}

pub fn start_visible() -> bool {
    START_VISIBLE.load(Ordering::Relaxed)
}

pub fn request_show() {
    SHOW_REQUESTED.store(true, Ordering::SeqCst);
    signal();
}

pub fn request_toggle() {
    TOGGLE_REQUESTED.store(true, Ordering::SeqCst);
    signal();
}

pub fn request_settings() {
    SETTINGS_REQUESTED.store(true, Ordering::SeqCst);
    signal();
}

pub fn request_quit() {
    QUIT_REQUESTED.store(true, Ordering::SeqCst);
    signal();
}

/// Returns the notifier that wakes up the iced subscription.
pub fn subscribe() -> &'static Notify {
    signal_notifier()
}

/// Returns every pending request and clears the flags.
pub fn take_requests() -> Requests {
    Requests {
        show: SHOW_REQUESTED.swap(false, Ordering::SeqCst),
        toggle: TOGGLE_REQUESTED.swap(false, Ordering::SeqCst),
        settings: SETTINGS_REQUESTED.swap(false, Ordering::SeqCst),
        quit: QUIT_REQUESTED.swap(false, Ordering::SeqCst),
    }
}

fn signal_notifier() -> &'static Notify {
    SIGNAL.get_or_init(Notify::new)
}

fn signal() {
    signal_notifier().notify_one();
}
