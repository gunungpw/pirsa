//! Single-instance handling and "open the existing instance" signalling.

use std::ffi::c_void;
use std::sync::atomic::{AtomicIsize, Ordering};
use std::thread;

use windows::Win32::Foundation::{ERROR_ALREADY_EXISTS, HANDLE, WAIT_OBJECT_0};
use windows::Win32::System::Threading::{
    CreateEventW, CreateMutexW, EVENT_MODIFY_STATE, INFINITE, OpenEventW, SetEvent,
    WaitForSingleObject,
};
use windows::core::w;

use crate::platform::request_show;
use crate::util::log;

static MUTEX_HANDLE: AtomicIsize = AtomicIsize::new(0);
static SHOW_EVENT: AtomicIsize = AtomicIsize::new(0);

/// Returns `true` when this process owns the single-instance mutex.
pub fn claim() -> bool {
    let mutex = unsafe { CreateMutexW(None, false, w!("Local\\Pirsa.SingleInstance.v1")) };

    match mutex {
        Ok(handle) => {
            // The handle is intentionally kept open for the process lifetime.
            MUTEX_HANDLE.store(handle.0 as isize, Ordering::Relaxed);

            let already_running = std::io::Error::last_os_error().raw_os_error()
                == Some(ERROR_ALREADY_EXISTS.0 as i32);

            !already_running
        }
        Err(error) => {
            log(format!(
                "could not create the single-instance mutex: {error}; continuing"
            ));
            true
        }
    }
}

/// Creates the named event that later instances use to ask this instance to
/// show its window.
pub fn create_show_event() -> Option<isize> {
    match unsafe { CreateEventW(None, false, false, w!("Local\\Pirsa.Show.v1")) } {
        Ok(handle) => {
            let raw = handle.0 as isize;
            SHOW_EVENT.store(raw, Ordering::Relaxed);
            log("created single-instance show event");
            Some(raw)
        }
        Err(error) => {
            log(format!("could not create the show event: {error}"));
            None
        }
    }
}

/// Asks an already-running launcher to show itself. Used by a second process.
pub fn signal_existing() {
    match unsafe { OpenEventW(EVENT_MODIFY_STATE, false, w!("Local\\Pirsa.Show.v1")) } {
        Ok(handle) => {
            let _ = unsafe { SetEvent(handle) };
            log("signalled the running launcher to show");
        }
        Err(error) => {
            log(format!(
                "could not open the running launcher's show event: {error}"
            ));
        }
    }
}

/// Watches the show event for the lifetime of the process.
pub fn watch_show_event(raw: isize) {
    if raw == 0 {
        return;
    }

    let _ = thread::Builder::new()
        .name("pirsa-single-instance".to_owned())
        .spawn(move || {
            let event = HANDLE(raw as *mut c_void);

            loop {
                let wait = unsafe { WaitForSingleObject(event, INFINITE) };

                if wait != WAIT_OBJECT_0 {
                    log(format!("show-event wait ended unexpectedly: {wait:?}"));
                    break;
                }

                request_show();
            }
        });
}
