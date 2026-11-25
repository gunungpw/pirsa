//! COM apartment management.
//!
//! Shell APIs behave most reliably on an STA (single-threaded apartment). The
//! shell worker initializes one apartment for its lifetime; direct callers
//! (mostly tests) get a balanced guard of their own.

use windows::Win32::System::Com::{COINIT_APARTMENTTHREADED, CoInitializeEx, CoUninitialize};

/// Keeps COM initialized for as long as the guard lives.
#[must_use = "COM is uninitialized when the guard is dropped"]
pub struct ComApartment {
    status: i32,
}

impl ComApartment {
    pub fn new() -> Self {
        let status = unsafe { CoInitializeEx(None, COINIT_APARTMENTTHREADED) }.0;
        Self { status }
    }

    /// Whether this call initialized (or joined) a COM apartment successfully.
    pub fn is_initialized(&self) -> bool {
        self.status >= 0
    }

    /// The raw `HRESULT` from `CoInitializeEx`.
    pub fn status(&self) -> i32 {
        self.status
    }
}

impl Drop for ComApartment {
    fn drop(&mut self) {
        if self.is_initialized() {
            unsafe { CoUninitialize() };
        }
    }
}
