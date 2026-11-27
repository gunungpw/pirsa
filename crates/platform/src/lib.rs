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
pub mod drives;
mod folder_index;
mod folder_picker;
mod folders;
mod hotkey;
mod icons;
mod instance;
mod local_fonts;
mod shell;
mod tray;
mod window_memory;

use pirsa_core::FolderEntry;
use pirsa_core::search_scope::SearchScope;

pub use apps::launch;
pub use folder_index::{
    ensure as ensure_folder_index, invalidate as invalidate_folder_index,
    rebuild as rebuild_folder_index, status as folder_index_status,
};
pub use folder_picker::pick_async as pick_folder;
pub use folders::{open_folder, search_async as live_folder_search};
pub use hotkey::status as hotkey_status;
pub use local_fonts::{LocalFonts, local_fonts};
pub use shell::{extract_icons, scan};
pub use window::show_fatal_error;

/// Searches folders in `scope`, preferring the persistent index and falling
/// back to the bounded live walk while it is not ready.
pub async fn search_folders(scope: SearchScope, term: String) -> Vec<FolderEntry> {
    if let Some(found) = folder_index::search(&term, scope.max_results) {
        return found;
    }

    live_folder_search(scope, term).await
}

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
///
/// The change round-trips through the hotkey thread, so this is a future the
/// caller should run as a task instead of blocking the UI thread.
pub async fn set_hotkey(choice: HotkeyChoice) -> HotkeyStatus {
    let status = hotkey::set(choice).await;

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
#[must_use]
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
