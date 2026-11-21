//! Single-instance handling and "open the existing instance" signalling.

use std::ffi::c_void;
use std::thread;

use windows::Win32::Foundation::{
    CloseHandle, ERROR_ALREADY_EXISTS, ERROR_SUCCESS, GetLastError, HANDLE, SetLastError,
    WAIT_OBJECT_0,
};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent,
    WaitForSingleObject,
};
use windows::core::{PCWSTR, w};

use crate::util::log;

/// Returns `true` when this process owns the single-instance mutex.
pub fn claim() -> bool {
    // Clear the thread's last error so ERROR_ALREADY_EXISTS can only come from
    // this call.
    unsafe { SetLastError(ERROR_SUCCESS) };
    let mutex = unsafe { CreateMutexW(None, false, w!("Local\\Pirsa.SingleInstance.v1")) };
    let last_error = unsafe { GetLastError() };

    match mutex {
        Ok(_handle) => {
            // The mutex handle is intentionally never closed: Windows releases
            // it when the process exits, which is exactly what we want.
            last_error != ERROR_ALREADY_EXISTS
        }
        Err(error) => {
            log(format!(
                "could not create the single-instance mutex: {error}; continuing"
            ));
            true
        }
    }
}

/// Creates the events a second process (or the installer) uses to ask this
/// instance to show its window or quit.
pub fn create_events() -> (Option<isize>, Option<isize>) {
    (
        create_event(w!("Local\\Pirsa.Show.v1"), "show event"),
        create_event(w!("Local\\Pirsa.Quit.v1"), "quit event"),
    )
}

/// Asks an already-running launcher to show itself. Used by a second process.
pub fn signal_existing() {
    signal_event(w!("Local\\Pirsa.Show.v1"), "show event");
}

/// Watches one of the created events for the lifetime of the process.
pub fn watch(raw: Option<isize>, label: &'static str, on_signal: fn()) {
    let Some(raw) = raw.filter(|raw| *raw != 0) else {
        return;
    };

    let spawn = thread::Builder::new()
        .name(format!("pirsa-{label}"))
        .spawn(move || {
            let event = HANDLE(raw as *mut c_void);

            loop {
                let wait = unsafe { WaitForSingleObject(event, INFINITE) };

                if wait != WAIT_OBJECT_0 {
                    log(format!("{label} wait ended unexpectedly: {wait:?}"));
                    break;
                }

                on_signal();
            }
        });

    if let Err(error) = spawn {
        log(format!("could not start the {label} watcher: {error}"));
    }
}

fn create_event(name: PCWSTR, label: &str) -> Option<isize> {
    match unsafe { CreateEventW(None, false, false, name) } {
        Ok(handle) => {
            log(format!("created single-instance {label}"));
            Some(handle.0 as isize)
        }
        Err(error) => {
            log(format!("could not create the {label}: {error}"));
            None
        }
    }
}

fn signal_event(name: PCWSTR, label: &str) {
    match unsafe { OpenEventW(EVENT_MODIFY_STATE, false, name) } {
        Ok(handle) => {
            let _ = unsafe { SetEvent(handle) };
            let _ = unsafe { CloseHandle(handle) };
            log(format!("signalled the running launcher's {label}"));
        }
        Err(error) => {
            log(format!(
                "could not open the running launcher's {label}: {error}"
            ));
        }
    }
}
