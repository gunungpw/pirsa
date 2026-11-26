//! Global hotkey handling.
//!
//! The chosen hotkey is registered system-wide on a dedicated thread with its
//! own message loop. This is the same mechanism Windows exposes through
//! `RegisterHotKey`, so it is reliable and needs no keyboard hook.
//!
//! The settings window can change the hotkey at runtime. The thread that owns
//! the registration receives a posted message, unregisters the old binding,
//! and registers the new one. If the new binding is already taken, the old one
//! is restored so the launcher stays reachable.

use std::sync::atomic::{AtomicU8, AtomicU32, Ordering};
use std::sync::mpsc;
use std::sync::{Mutex, MutexGuard};
use std::thread;
use std::time::Duration;

use windows::Win32::Foundation::{LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Input::KeyboardAndMouse::{
    HOT_KEY_MODIFIERS, MOD_ALT, MOD_CONTROL, MOD_NOREPEAT, MOD_SHIFT, RegisterHotKey,
    UnregisterHotKey, VK_OEM_3, VK_P, VK_Q, VK_SPACE,
};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, GetMessageW, MSG, PM_NOREMOVE, PeekMessageW, PostThreadMessageW,
    TranslateMessage, WM_APP, WM_HOTKEY,
};

use pirsa_core::HotkeyStatus;
use pirsa_core::settings::HotkeyChoice;
use pirsa_core::util::log;

use crate::request_toggle;

const HOTKEY_ID: i32 = 0x4C55;
const CHANGE_HOTKEY_MESSAGE: u32 = WM_APP + 1;
const CHANGE_TIMEOUT: Duration = Duration::from_millis(1_500);

static STATUS: AtomicU8 = AtomicU8::new(HotkeyStatus::Pending as u8);
static THREAD_ID: AtomicU32 = AtomicU32::new(0);
static PENDING: Mutex<Option<(HotkeyChoice, mpsc::Sender<HotkeyStatus>)>> = Mutex::new(None);

pub fn status() -> HotkeyStatus {
    match STATUS.load(Ordering::Relaxed) {
        1 => HotkeyStatus::Ready,
        2 => HotkeyStatus::Unavailable,
        _ => HotkeyStatus::Pending,
    }
}

/// Starts the hotkey thread with `initial` and waits briefly for it to report
/// readiness.
pub fn start(initial: HotkeyChoice) -> HotkeyStatus {
    let (sender, receiver) = mpsc::channel();

    let spawn = thread::Builder::new()
        .name("pirsa-hotkeys".to_owned())
        .spawn(move || hotkey_thread(initial, sender));

    if let Err(error) = spawn {
        log(format!("Could not start the hotkey thread: {error}"));
        STATUS.store(HotkeyStatus::Unavailable as u8, Ordering::Relaxed);
        return HotkeyStatus::Unavailable;
    }

    match receiver.recv_timeout(CHANGE_TIMEOUT) {
        Ok(status) => status,
        Err(_) => {
            STATUS.store(HotkeyStatus::Unavailable as u8, Ordering::Relaxed);
            HotkeyStatus::Unavailable
        }
    }
}

/// Changes the global hotkey while the launcher is running.
///
/// Returns `Ready` when the new binding was registered. If Windows rejects it
/// (usually because another app owns the same combination), the previous
/// binding is restored and `Unavailable` is returned.
pub fn set(choice: HotkeyChoice) -> HotkeyStatus {
    let thread_id = THREAD_ID.load(Ordering::Relaxed);

    if thread_id == 0 {
        log("hotkey thread is not running; cannot change the hotkey");
        return HotkeyStatus::Unavailable;
    }

    let (sender, receiver) = mpsc::channel();
    *pending() = Some((choice, sender));

    let posted =
        unsafe { PostThreadMessageW(thread_id, CHANGE_HOTKEY_MESSAGE, WPARAM(0), LPARAM(0)) };

    if let Err(error) = posted {
        let _ = pending().take();
        log(format!(
            "could not ask the hotkey thread to change: {error}"
        ));
        return HotkeyStatus::Unavailable;
    }

    receiver.recv_timeout(CHANGE_TIMEOUT).unwrap_or_else(|_| {
        log("hotkey thread did not answer the change request");
        HotkeyStatus::Unavailable
    })
}

fn pending() -> MutexGuard<'static, Option<(HotkeyChoice, mpsc::Sender<HotkeyStatus>)>> {
    PENDING.lock().unwrap_or_else(|poison| poison.into_inner())
}

fn hotkey_thread(initial: HotkeyChoice, sender: mpsc::Sender<HotkeyStatus>) {
    // Force this thread's message queue to exist before publishing its id so
    // a later `PostThreadMessageW` cannot race the first `GetMessageW`.
    let mut queue_message = MSG::default();
    unsafe {
        let _ = PeekMessageW(&mut queue_message, None, 0, 0, PM_NOREMOVE);
    }
    THREAD_ID.store(unsafe { GetCurrentThreadId() }, Ordering::Relaxed);

    let mut current = initial;
    let mut registered = register(initial);
    let status = if registered {
        HotkeyStatus::Ready
    } else {
        log(format!(
            "{} could not be registered: {}",
            initial.label(),
            std::io::Error::last_os_error()
        ));
        HotkeyStatus::Unavailable
    };

    STATUS.store(status as u8, Ordering::Relaxed);
    let _ = sender.send(status);
    log(format!("hotkey status: {status:?} ({})", current.label()));

    let mut message = MSG::default();

    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };

        if result.0 <= 0 {
            break;
        }

        if message.message == WM_HOTKEY && message.wParam.0 as i32 == HOTKEY_ID {
            request_toggle();
        } else if message.message == CHANGE_HOTKEY_MESSAGE {
            let request = pending().take();

            if let Some((choice, reply)) = request {
                let status = change(&mut current, &mut registered, choice);
                STATUS.store(status as u8, Ordering::Relaxed);
                let _ = reply.send(status);
            }
        }

        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    if registered {
        unsafe {
            let _ = UnregisterHotKey(None, HOTKEY_ID);
        }
    }

    THREAD_ID.store(0, Ordering::Relaxed);
}

/// Applies a requested hotkey change on the thread that owns the registration.
fn change(current: &mut HotkeyChoice, registered: &mut bool, next: HotkeyChoice) -> HotkeyStatus {
    if *registered && next == *current {
        return HotkeyStatus::Ready;
    }

    if *registered {
        unsafe {
            let _ = UnregisterHotKey(None, HOTKEY_ID);
        }
        *registered = false;
    }

    if register(next) {
        *current = next;
        *registered = true;
        log(format!("hotkey changed to {}", next.label()));
        HotkeyStatus::Ready
    } else {
        log(format!(
            "{} could not be registered: {}",
            next.label(),
            std::io::Error::last_os_error()
        ));

        // Keep the launcher reachable with its previous hotkey.
        *registered = register(*current);

        if *registered {
            log(format!("kept {}", current.label()));
        } else {
            log("no global hotkey is active");
        }

        HotkeyStatus::Unavailable
    }
}

fn register(choice: HotkeyChoice) -> bool {
    let (modifiers, key) = binding(choice);

    unsafe { RegisterHotKey(None, HOTKEY_ID, modifiers | MOD_NOREPEAT, key).is_ok() }
}

fn binding(choice: HotkeyChoice) -> (HOT_KEY_MODIFIERS, u32) {
    use HotkeyChoice::{
        AltBacktick, AltQ, AltShiftSpace, AltSpace, CtrlAltP, CtrlAltQ, CtrlAltSpace, CtrlBacktick,
        CtrlShiftSpace, CtrlSpace,
    };

    match choice {
        AltSpace => (MOD_ALT, VK_SPACE.0 as u32),
        CtrlSpace => (MOD_CONTROL, VK_SPACE.0 as u32),
        CtrlAltSpace => (MOD_CONTROL | MOD_ALT, VK_SPACE.0 as u32),
        AltShiftSpace => (MOD_ALT | MOD_SHIFT, VK_SPACE.0 as u32),
        CtrlShiftSpace => (MOD_CONTROL | MOD_SHIFT, VK_SPACE.0 as u32),
        AltBacktick => (MOD_ALT, VK_OEM_3.0 as u32),
        CtrlBacktick => (MOD_CONTROL, VK_OEM_3.0 as u32),
        AltQ => (MOD_ALT, VK_Q.0 as u32),
        CtrlAltQ => (MOD_CONTROL | MOD_ALT, VK_Q.0 as u32),
        CtrlAltP => (MOD_CONTROL | MOD_ALT, VK_P.0 as u32),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_choice_has_a_distinct_binding() {
        let mut bindings = Vec::new();

        for choice in HotkeyChoice::ALL {
            let binding = binding(*choice);
            assert_ne!(binding.1, 0, "{} has no virtual key", choice.label());
            assert!(
                !bindings.contains(&binding),
                "{} reuses another binding",
                choice.label()
            );
            bindings.push(binding);
        }

        assert_eq!(bindings.len(), HotkeyChoice::ALL.len());
    }

    #[test]
    fn alt_space_is_the_default_binding() {
        assert_eq!(
            binding(HotkeyChoice::AltSpace),
            (MOD_ALT, VK_SPACE.0 as u32)
        );
    }
}
