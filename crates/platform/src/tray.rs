//! Notification-area (system tray) icon and menu.

use std::mem::size_of;
use std::sync::atomic::{AtomicU32, Ordering};
use std::thread;

use windows::Win32::Foundation::{HINSTANCE, HWND, LPARAM, LRESULT, POINT, WPARAM};
use windows::Win32::Graphics::Gdi::HBRUSH;
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::Shell::{
    NIF_ICON, NIF_MESSAGE, NIF_TIP, NIM_ADD, NIM_DELETE, NOTIFYICONDATAW, Shell_NotifyIconW,
};
use windows::Win32::UI::WindowsAndMessaging::{
    AppendMenuW, CreatePopupMenu, CreateWindowExW, DefWindowProcW, DestroyMenu, DispatchMessageW,
    GetCursorPos, GetMessageW, HCURSOR, HICON, IDI_APPLICATION, IMAGE_ICON, LR_DEFAULTSIZE,
    LR_SHARED, LoadIconW, LoadImageW, MF_SEPARATOR, MF_STRING, MSG, PostMessageW, PostQuitMessage,
    RegisterClassW, RegisterWindowMessageW, SetForegroundWindow, TPM_RETURNCMD, TPM_RIGHTBUTTON,
    TRACK_POPUP_MENU_FLAGS, TrackPopupMenu, TranslateMessage, WINDOW_EX_STYLE, WINDOW_STYLE,
    WM_APP, WM_DESTROY, WM_LBUTTONDBLCLK, WM_LBUTTONUP, WM_NULL, WM_RBUTTONUP, WNDCLASS_STYLES,
    WNDCLASSW,
};
use windows::core::{PCWSTR, w};

use pirsa_core::util::log;

use crate::{request_quit, request_settings, request_show};

const TRAY_ID: u32 = 1;
const CALLBACK_MESSAGE: u32 = WM_APP + 1;
const CLASS_NAME: PCWSTR = w!("PirsaTrayWindow");
const MENU_OPEN: usize = 1;
const MENU_SETTINGS: usize = 2;
const MENU_QUIT: usize = 3;
const TOOLTIP: &str = "Pirsa — Alt+Space";

static TASKBAR_CREATED: AtomicU32 = AtomicU32::new(0);

/// Starts the tray thread. Safe to call once during startup.
pub fn start() {
    let spawn = thread::Builder::new()
        .name("pirsa-tray".to_owned())
        .spawn(run);

    if let Err(error) = spawn {
        log(format!("could not start the tray thread: {error}"));
    }
}

fn run() {
    let instance = match unsafe { GetModuleHandleW(PCWSTR::null()) } {
        Ok(module) => HINSTANCE(module.0),
        Err(error) => {
            log(format!("tray could not get the module handle: {error}"));
            return;
        }
    };

    let window_class = WNDCLASSW {
        style: WNDCLASS_STYLES(0),
        lpfnWndProc: Some(window_proc),
        cbClsExtra: 0,
        cbWndExtra: 0,
        hInstance: instance,
        hIcon: HICON::default(),
        hCursor: HCURSOR::default(),
        hbrBackground: HBRUSH::default(),
        lpszMenuName: PCWSTR::null(),
        lpszClassName: CLASS_NAME,
    };

    if unsafe { RegisterClassW(&window_class) } == 0 {
        log("tray could not register its window class");
        return;
    }

    let hwnd = match unsafe {
        CreateWindowExW(
            WINDOW_EX_STYLE::default(),
            CLASS_NAME,
            w!("Pirsa"),
            WINDOW_STYLE::default(),
            0,
            0,
            0,
            0,
            None,
            None,
            Some(instance),
            None,
        )
    } {
        Ok(hwnd) => hwnd,
        Err(error) => {
            log(format!("tray could not create its window: {error}"));
            return;
        }
    };

    let taskbar_created = unsafe { RegisterWindowMessageW(w!("TaskbarCreated")) };
    TASKBAR_CREATED.store(taskbar_created, Ordering::Relaxed);

    add_icon(hwnd, instance);
    log("tray window created");

    let mut message = MSG::default();

    loop {
        let result = unsafe { GetMessageW(&mut message, None, 0, 0) };

        if result.0 <= 0 {
            break;
        }

        unsafe {
            let _ = TranslateMessage(&message);
            DispatchMessageW(&message);
        }
    }

    remove_icon(hwnd);
    log("tray icon stopped");
}

unsafe extern "system" fn window_proc(
    hwnd: HWND,
    message: u32,
    wparam: WPARAM,
    lparam: LPARAM,
) -> LRESULT {
    if message != 0 && message == TASKBAR_CREATED.load(Ordering::Relaxed) {
        if let Ok(module) = unsafe { GetModuleHandleW(PCWSTR::null()) } {
            add_icon(hwnd, HINSTANCE(module.0));
            log("tray icon restored after Explorer restart");
        }

        return LRESULT(0);
    }

    match message {
        CALLBACK_MESSAGE => {
            match (lparam.0 as u32) & 0xffff {
                WM_LBUTTONUP | WM_LBUTTONDBLCLK => request_show(),
                WM_RBUTTONUP => unsafe { show_menu(hwnd) },
                _ => {}
            }

            LRESULT(0)
        }
        WM_DESTROY => {
            unsafe { PostQuitMessage(0) };
            LRESULT(0)
        }
        _ => unsafe { DefWindowProcW(hwnd, message, wparam, lparam) },
    }
}

unsafe fn show_menu(hwnd: HWND) {
    let Ok(menu) = (unsafe { CreatePopupMenu() }) else {
        return;
    };

    let _ = unsafe { AppendMenuW(menu, MF_STRING, MENU_OPEN, w!("Open Pirsa")) };
    let _ = unsafe { AppendMenuW(menu, MF_STRING, MENU_SETTINGS, w!("Settings…")) };
    let _ = unsafe { AppendMenuW(menu, MF_SEPARATOR, 0, PCWSTR::null()) };
    let _ = unsafe { AppendMenuW(menu, MF_STRING, MENU_QUIT, w!("Quit Pirsa")) };

    let mut point = POINT::default();

    if unsafe { GetCursorPos(&mut point) }.is_err() {
        let _ = unsafe { DestroyMenu(menu) };
        return;
    }

    let _ = unsafe { SetForegroundWindow(hwnd) };
    let command = unsafe {
        TrackPopupMenu(
            menu,
            TRACK_POPUP_MENU_FLAGS(TPM_RETURNCMD.0 | TPM_RIGHTBUTTON.0),
            point.x,
            point.y,
            None,
            hwnd,
            None,
        )
    };
    let _ = unsafe { DestroyMenu(menu) };
    let _ = unsafe { PostMessageW(Some(hwnd), WM_NULL, WPARAM(0), LPARAM(0)) };

    match command.0 as usize {
        MENU_OPEN => request_show(),
        MENU_SETTINGS => request_settings(),
        MENU_QUIT => request_quit(),
        _ => {}
    }
}

fn add_icon(hwnd: HWND, instance: HINSTANCE) {
    let mut data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        uFlags: NIF_MESSAGE | NIF_ICON | NIF_TIP,
        uCallbackMessage: CALLBACK_MESSAGE,
        hIcon: load_icon(instance),
        ..Default::default()
    };

    copy_text(&mut data.szTip, TOOLTIP);

    if !unsafe { Shell_NotifyIconW(NIM_ADD, &data) }.as_bool() {
        log("tray could not add its notification icon");
    } else {
        log("tray icon added");
    }
}

fn remove_icon(hwnd: HWND) {
    let data = NOTIFYICONDATAW {
        cbSize: size_of::<NOTIFYICONDATAW>() as u32,
        hWnd: hwnd,
        uID: TRAY_ID,
        ..Default::default()
    };

    let _ = unsafe { Shell_NotifyIconW(NIM_DELETE, &data) };
}

fn load_icon(instance: HINSTANCE) -> HICON {
    match unsafe {
        LoadImageW(
            Some(instance),
            PCWSTR(1 as _),
            IMAGE_ICON,
            0,
            0,
            LR_DEFAULTSIZE | LR_SHARED,
        )
    } {
        Ok(handle) => HICON(handle.0),
        Err(error) => {
            log(format!(
                "tray could not load the app icon: {error}; using the default icon"
            ));
            unsafe { LoadIconW(None, IDI_APPLICATION) }.unwrap_or_default()
        }
    }
}

fn copy_text(destination: &mut [u16], text: &str) {
    let encoded: Vec<u16> = text
        .encode_utf16()
        .take(destination.len().saturating_sub(1))
        .collect();

    destination[..encoded.len()].copy_from_slice(&encoded);

    if let Some(last) = destination.get_mut(encoded.len()) {
        *last = 0;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tray_tooltip_is_null_terminated() {
        let mut buffer = [1_u16; 8];

        copy_text(&mut buffer, "Pirsa");

        assert_eq!(buffer[0], 'P' as u16);
        assert_eq!(buffer[5], 0);
    }

    #[test]
    fn tray_tooltip_is_truncated_without_overflow() {
        let mut buffer = [1_u16; 4];

        copy_text(&mut buffer, "Pirsa");

        assert_eq!(&buffer[..3], &[0x50, 0x69, 0x72]);
        assert_eq!(buffer[3], 0);
    }
}
