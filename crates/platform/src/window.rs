//! Native Win32 window positioning, focus, and visibility helpers.
//!
//! iced's `window::set_mode` updates its own visibility state, while these
//! helpers handle the native side: centering on the active monitor, reliable
//! foreground activation, and returning focus to the previous window.

use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};

use windows::Win32::Foundation::{HWND, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, VK_MENU, keybd_event,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId, HWND_NOTOPMOST,
    HWND_TOPMOST, IsWindowVisible, SW_HIDE, SW_SHOWNORMAL, SWP_HIDEWINDOW, SWP_NOACTIVATE,
    SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SetForegroundWindow, SetWindowPos,
    ShowWindow, SwitchToThisWindow,
};

use pirsa_core::util::log;

static PREVIOUS_FOREGROUND: AtomicIsize = AtomicIsize::new(0);

fn hwnd(raw: isize) -> HWND {
    HWND(raw as *mut c_void)
}

/// Shows, centers, and foregrounds the launcher window.
pub fn show(raw: isize) -> bool {
    let hwnd = hwnd(raw);

    if hwnd.0.is_null() {
        return false;
    }

    unsafe {
        let previous = GetForegroundWindow();
        if !previous.0.is_null() && previous != hwnd {
            PREVIOUS_FOREGROUND.store(previous.0 as isize, Ordering::Relaxed);
        }

        center(hwnd);

        // Raise above other windows without permanently becoming topmost.
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_TOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );

        let _ = ShowWindow(hwnd, SW_SHOWNORMAL);

        let _ = SetWindowPos(
            hwnd,
            Some(HWND_NOTOPMOST),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
        );

        let _ = BringWindowToTop(hwnd);
        activate(hwnd);
    }

    true
}

/// Reasserts foreground focus for an already visible launcher.
pub fn focus(raw: isize) -> bool {
    let hwnd = hwnd(raw);

    if hwnd.0.is_null() {
        return false;
    }

    activate(hwnd)
}

fn activate(hwnd: HWND) -> bool {
    unsafe {
        let first_try = SetForegroundWindow(hwnd).as_bool();

        if !first_try || GetForegroundWindow() != hwnd {
            force_foreground(hwnd);
        }

        if GetForegroundWindow() != hwnd {
            // SwitchToThisWindow is the most forceful user32 activation
            // fallback; it works when AttachThreadInput is denied by an
            // AppContainer/UWP window owning the foreground.
            SwitchToThisWindow(hwnd, true);
            log("focused via SwitchToThisWindow");
        }

        if GetForegroundWindow() != hwnd {
            alt_focus(hwnd);
        }

        let focused = GetForegroundWindow() == hwnd;

        log(format!(
            "activate: SetForegroundWindow={first_try}, final focus={focused}"
        ));

        focused
    }
}

/// Hides the launcher without destroying it.
pub fn hide(raw: isize) -> bool {
    let hwnd = hwnd(raw);

    if hwnd.0.is_null() {
        return false;
    }

    unsafe {
        let before = IsWindowVisible(hwnd).as_bool();
        let was_foreground = GetForegroundWindow() == hwnd;

        // SetWindowPos is significantly more reliable than ShowWindow here:
        // winit owns the window and ShowWindow can be ignored while the
        // runtime is processing window state.
        let _ = SetWindowPos(
            hwnd,
            None,
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE | SWP_HIDEWINDOW,
        );

        if IsWindowVisible(hwnd).as_bool() {
            let _ = ShowWindow(hwnd, SW_HIDE);
        }

        let after = IsWindowVisible(hwnd).as_bool();
        log(format!("hide: visible {before} -> {after}"));

        // Only return focus if we still owned it. If the user clicked another
        // application (the usual auto-hide path), that application must keep
        // focus.
        let previous = PREVIOUS_FOREGROUND.swap(0, Ordering::Relaxed);
        if was_foreground && previous != 0 {
            let previous = HWND(previous as *mut c_void);

            if !previous.0.is_null() && IsWindowVisible(previous).as_bool() {
                let _ = SetForegroundWindow(previous);
            }
        }

        !after
    }
}

pub fn is_visible(raw: isize) -> bool {
    let hwnd = hwnd(raw);
    !hwnd.0.is_null() && unsafe { IsWindowVisible(hwnd).as_bool() }
}

pub fn is_foreground(raw: isize) -> bool {
    let hwnd = hwnd(raw);
    !hwnd.0.is_null() && unsafe { GetForegroundWindow() == hwnd }
}

fn center(hwnd: HWND) {
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return;
        }

        let width = rect.right - rect.left;
        let height = rect.bottom - rect.top;

        if width <= 0 || height <= 0 {
            return;
        }

        let monitor = MonitorFromWindow(hwnd, MONITOR_DEFAULTTONEAREST);
        let mut info = MONITORINFO {
            cbSize: std::mem::size_of::<MONITORINFO>() as u32,
            ..MONITORINFO::default()
        };

        if !GetMonitorInfoW(monitor, &mut info).as_bool() {
            return;
        }

        let x = info.rcWork.left + ((info.rcWork.right - info.rcWork.left) - width) / 2;
        let y = info.rcWork.top + ((info.rcWork.bottom - info.rcWork.top) - height) / 2;

        let _ = SetWindowPos(
            hwnd,
            None,
            x,
            y,
            0,
            0,
            SWP_NOSIZE | SWP_NOZORDER | SWP_NOACTIVATE,
        );
    }
}

fn force_foreground(hwnd: HWND) {
    unsafe {
        let foreground = GetForegroundWindow();

        if foreground.0.is_null() {
            return;
        }

        let foreground_thread = GetWindowThreadProcessId(foreground, None);
        let current_thread = GetCurrentThreadId();

        if foreground_thread == 0 || foreground_thread == current_thread {
            return;
        }

        if AttachThreadInput(current_thread, foreground_thread, true).as_bool() {
            let _ = BringWindowToTop(hwnd);
            let _ = SetForegroundWindow(hwnd);
            let _ = AttachThreadInput(current_thread, foreground_thread, false);
            log("focused via AttachThreadInput");
        } else {
            log("could not attach to the foreground thread for focus");
        }
    }
}

/// Windows' foreground lock can reject activation from a background process.
/// A synthetic Alt press is the standard way to satisfy the foreground lock
/// without affecting the focused application.
///
/// This used to be followed by a synthetic click inside the window, which moved
/// the user's cursor and could click whatever ended up under that point. The
/// Alt press is enough to satisfy the lock; if even that fails the window stays
/// visible and the periodic re-focus retry takes over.
fn alt_focus(hwnd: HWND) {
    unsafe {
        keybd_event(VK_MENU.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
        let _ = SetForegroundWindow(hwnd);
        keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_KEYUP, 0);
        log("focused via synthetic Alt key");
    }
}
