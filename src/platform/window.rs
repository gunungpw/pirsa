//! Native Win32 window positioning, focus, and visibility helpers.
//!
//! iced's `window::set_mode` updates its own visibility state, while these
//! helpers handle the native side: centering on the active monitor, reliable
//! foreground activation, and returning focus to the previous window.

use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::time::Duration;

use windows::Win32::Foundation::{HWND, POINT, RECT};
use windows::Win32::Graphics::Gdi::{
    GetMonitorInfoW, MONITOR_DEFAULTTONEAREST, MONITORINFO, MonitorFromWindow,
};
use windows::Win32::System::Threading::{AttachThreadInput, GetCurrentThreadId};
use windows::Win32::UI::Input::KeyboardAndMouse::{
    KEYBD_EVENT_FLAGS, KEYEVENTF_KEYUP, MOUSEEVENTF_LEFTDOWN, MOUSEEVENTF_LEFTUP, VK_MENU,
    keybd_event, mouse_event,
};
use windows::Win32::UI::WindowsAndMessaging::{
    BringWindowToTop, GetCursorPos, GetForegroundWindow, GetWindowRect, GetWindowThreadProcessId,
    HWND_NOTOPMOST, HWND_TOPMOST, IsWindowVisible, SW_HIDE, SW_SHOWNORMAL, SWP_HIDEWINDOW,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SWP_NOZORDER, SWP_SHOWWINDOW, SetCursorPos,
    SetForegroundWindow, SetWindowPos, ShowWindow, SwitchToThisWindow,
};

use crate::util::log;

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

        // Some shell surfaces (notably Windows Search) deny programmatic
        // foreground activation even through the fallbacks above. A synthetic
        // click inside our own search field is treated as real user input and
        // reliably activates the launcher.
        if GetForegroundWindow() != hwnd {
            click_focus(hwnd);
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
fn alt_focus(hwnd: HWND) {
    unsafe {
        keybd_event(VK_MENU.0 as u8, 0, KEYBD_EVENT_FLAGS(0), 0);
        let _ = SetForegroundWindow(hwnd);
        keybd_event(VK_MENU.0 as u8, 0, KEYEVENTF_KEYUP, 0);
        log("focused via synthetic Alt key");
    }
}

fn click_focus(hwnd: HWND) {
    unsafe {
        let mut rect = RECT::default();
        if GetWindowRect(hwnd, &mut rect).is_err() {
            return;
        }

        let x = rect.left + (rect.right - rect.left) / 2;
        // The borderless search row occupies roughly the top 3–13% of the
        // window. Aim at 8% so the click lands in the input, not on a row.
        let y = rect.top + (rect.bottom - rect.top) * 8 / 100;

        let mut previous_cursor = POINT::default();
        if GetCursorPos(&mut previous_cursor).is_err() {
            return;
        }

        // Only raise the window if another surface still owns the foreground.
        let needs_raise = GetForegroundWindow() != hwnd;

        if needs_raise {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_TOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE | SWP_SHOWWINDOW,
            );
        }

        if SetCursorPos(x, y).is_ok() {
            // Give the queued motion a moment to land before dispatching the
            // button events; otherwise the click can be resolved at the old
            // cursor position.
            std::thread::sleep(Duration::from_millis(15));
            mouse_event(MOUSEEVENTF_LEFTDOWN, 0, 0, 0, 0);
            std::thread::sleep(Duration::from_millis(10));
            mouse_event(MOUSEEVENTF_LEFTUP, 0, 0, 0, 0);
            std::thread::sleep(Duration::from_millis(20));
            let _ = SetCursorPos(previous_cursor.x, previous_cursor.y);
            log("focused via synthetic click in the search field");
        }

        if needs_raise {
            let _ = SetWindowPos(
                hwnd,
                Some(HWND_NOTOPMOST),
                0,
                0,
                0,
                0,
                SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
            );
        }
    }
}
