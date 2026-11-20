//! Windows integration: global hotkeys, single instance, Start Menu apps, and
//! native window control.

pub mod apps;
mod hotkey;
mod instance;
pub mod window;

use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};

use tokio::sync::broadcast;

pub use apps::{AppEntry, launch, scan};
pub use hotkey::{HotkeyStatus, status as hotkey_status};

static TOGGLE_REQUESTED: AtomicBool = AtomicBool::new(false);
static SHOW_REQUESTED: AtomicBool = AtomicBool::new(false);
static START_VISIBLE: AtomicBool = AtomicBool::new(false);
static SIGNAL: OnceLock<broadcast::Sender<()>> = OnceLock::new();

/// Must be called once, before the iced application is created.
///
/// If another launcher is already running, this notifies it and exits.
pub fn init() -> HotkeyStatus {
    if !instance::claim() {
        instance::signal_existing();
        std::process::exit(0);
    }

    let event = instance::create_show_event();
    instance::watch_show_event(event.unwrap_or_default());

    let status = hotkey::start();

    crate::util::log(format!("started (hotkey: {status:?}, Alt+Space)"));

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

/// Creates a subscription to launcher wake-up signals.
pub fn subscribe() -> broadcast::Receiver<()> {
    signal_sender().subscribe()
}

fn signal_sender() -> &'static broadcast::Sender<()> {
    SIGNAL.get_or_init(|| broadcast::channel(16).0)
}

fn signal() {
    let _ = signal_sender().send(());
}

/// Returns `(show_requested, toggle_requested)` and clears both flags.
pub fn take_requests() -> (bool, bool) {
    (
        SHOW_REQUESTED.swap(false, Ordering::SeqCst),
        TOGGLE_REQUESTED.swap(false, Ordering::SeqCst),
    )
}
