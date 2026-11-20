//! Global hotkey handling.
//!
//! Alt+Space is registered as a system-wide hotkey on a dedicated thread with
//! its own message loop. This is the same mechanism Windows exposes through
//! `RegisterHotKey`, so it is reliable and needs no keyboard hook.

use std::sync::atomic::{AtomicU8, Ordering};
use std::sync::mpsc;
use std::thread;
use std::time::Duration;

use windows::Win32::UI::Input::KeyboardAndMouse::{
    MOD_ALT, MOD_NOREPEAT, RegisterHotKey, VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, TranslateMessage, WM_HOTKEY,
};

use crate::platform::request_toggle;
use crate::util::log;

const HOTKEY_ID: i32 = 0x4C55;

static STATUS: AtomicU8 = AtomicU8::new(HotkeyStatus::Pending as u8);

/// Whether the global hotkey could be installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum HotkeyStatus {
    Pending = 0,
    Ready = 1,
    Unavailable = 2,
}

impl HotkeyStatus {
    pub fn available(self) -> bool {
        self == Self::Ready
    }
}

pub fn status() -> HotkeyStatus {
    match STATUS.load(Ordering::Relaxed) {
        1 => HotkeyStatus::Ready,
        2 => HotkeyStatus::Unavailable,
        _ => HotkeyStatus::Pending,
    }
}

/// Starts the hotkey thread and waits briefly for it to report readiness.
pub fn start() -> HotkeyStatus {
    let (sender, receiver) = mpsc::channel();

    let spawn = thread::Builder::new()
        .name("pirsa-hotkeys".to_owned())
        .spawn(move || hotkey_thread(sender));

    if let Err(error) = spawn {
        log(format!("Could not start the hotkey thread: {error}"));
        STATUS.store(HotkeyStatus::Unavailable as u8, Ordering::Relaxed);
        return HotkeyStatus::Unavailable;
    }

    let status = receiver
        .recv_timeout(Duration::from_millis(1_500))
        .unwrap_or(HotkeyStatus::Unavailable);

    STATUS.store(status as u8, Ordering::Relaxed);
    status
}

fn hotkey_thread(sender: mpsc::Sender<HotkeyStatus>) {
    let registered = unsafe {
        RegisterHotKey(None, HOTKEY_ID, MOD_ALT | MOD_NOREPEAT, VK_SPACE.0 as u32).is_ok()
    };

    let status = if registered {
        HotkeyStatus::Ready
    } else {
        log(format!(
            "Alt+Space could not be registered: {}",
            std::io::Error::last_os_error()
        ));
        HotkeyStatus::Unavailable
    };

    let _ = sender.send(status);
    log(format!("hotkey status: {status:?} (Alt+Space)"));

    if !registered {
        return;
    }

    let mut message = MSG::default();

    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };

        if result.0 <= 0 {
            break;
        }

        if message.message == WM_HOTKEY && message.wParam.0 as i32 == HOTKEY_ID {
            request_toggle();
        }

        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }
}
