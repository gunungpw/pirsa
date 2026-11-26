//! Dedicated shell workers.
//!
//! Start Menu enumeration and icon extraction are blocking COM/GDI work. Each
//! runs on its own long-lived thread that initializes COM as STA, so executor
//! threads stay free and a slow icon batch cannot delay a rescan. Results are
//! returned through one-shot channels; the UI applies generation checks so a
//! slow result cannot overwrite a newer one.

use std::sync::OnceLock;
use std::sync::mpsc::{self, Receiver, Sender};
use std::thread;
use std::time::Instant;

use tokio::sync::oneshot;

use pirsa_core::util::log;
use pirsa_core::{AppEntry, Icon, ScanResult};

use super::apps;
use super::com::ComApartment;
use super::icons;

struct IconRequest {
    apps: Vec<AppEntry>,
    reply: oneshot::Sender<Vec<(String, Icon)>>,
}

static SCAN_JOBS: OnceLock<Sender<oneshot::Sender<ScanResult>>> = OnceLock::new();
static ICON_JOBS: OnceLock<Sender<IconRequest>> = OnceLock::new();

/// Starts both workers eagerly. Safe to call more than once.
pub fn start() {
    let _ = scan_sender();
    let _ = icon_sender();
}

/// Scans the Start Menu on the scan worker.
pub async fn scan() -> ScanResult {
    let (reply, receiver) = oneshot::channel();

    if scan_sender().send(reply).is_err() {
        return failed_scan("the scan worker is not running");
    }

    match receiver.await {
        Ok(result) => result,
        Err(error) => {
            log(format!("scan reply failed: {error}"));
            failed_scan("the scan worker stopped before finishing")
        }
    }
}

/// Extracts icons on the icon worker.
pub async fn extract_icons(apps: Vec<AppEntry>) -> Vec<(String, Icon)> {
    if apps.is_empty() {
        return Vec::new();
    }

    let (reply, receiver) = oneshot::channel();

    if icon_sender().send(IconRequest { apps, reply }).is_err() {
        log("could not send the icon request to the icon worker");
        return Vec::new();
    }

    match receiver.await {
        Ok(icons) => icons,
        Err(error) => {
            log(format!("icon reply failed: {error}"));
            Vec::new()
        }
    }
}

fn scan_sender() -> &'static Sender<oneshot::Sender<ScanResult>> {
    SCAN_JOBS.get_or_init(|| start_worker("pirsa-scan", run_scans))
}

fn icon_sender() -> &'static Sender<IconRequest> {
    ICON_JOBS.get_or_init(|| start_worker("pirsa-icons", run_icons))
}

fn start_worker<T: Send + 'static>(
    thread_name: &'static str,
    run: impl FnOnce(Receiver<T>) + Send + 'static,
) -> Sender<T> {
    let (sender, receiver) = mpsc::channel();
    let spawn = thread::Builder::new()
        .name(thread_name.to_owned())
        .spawn(move || run(receiver));

    if let Err(error) = spawn {
        log(format!("could not start the {thread_name} worker: {error}"));
    }

    sender
}

fn run_scans(receiver: Receiver<oneshot::Sender<ScanResult>>) {
    let com = ComApartment::new();
    log_worker_start("scan", &com);

    while let Ok(reply) = receiver.recv() {
        let started = Instant::now();
        // A panic while talking to the shell must not kill the worker: report
        // an empty index so the UI leaves its "indexing" state.
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(apps::scan))
            .unwrap_or_else(|_| {
                log("shell scan panicked; returning an empty index");
                failed_scan("the shell worker panicked")
            });
        let count = result.apps.len();

        if reply.send(result).is_err() {
            log("scan result dropped (request already cancelled)");
        }

        log(format!(
            "shell scan finished in {:?} ({count} apps)",
            started.elapsed()
        ));
    }

    log("scan worker stopped");
}

fn run_icons(receiver: Receiver<IconRequest>) {
    let com = ComApartment::new();
    log_worker_start("icon", &com);

    while let Ok(mut request) = receiver.recv() {
        // If a rescan queued another icon batch while this one was waiting,
        // only the newest batch matters; dropping the older request cancels
        // its receiver in the UI.
        while let Ok(newer) = receiver.try_recv() {
            request = newer;
        }

        let started = Instant::now();
        let icons = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            icons::extract(&request.apps)
        }))
        .unwrap_or_else(|_| {
            log("icon extraction panicked; returning no icons");
            Vec::new()
        });
        let count = icons.len();

        if request.reply.send(icons).is_err() {
            log("icon result dropped (request already cancelled)");
        }

        log(format!(
            "shell icon extraction finished in {:?} ({count} icons)",
            started.elapsed()
        ));
    }

    log("icon worker stopped");
}

fn log_worker_start(kind: &str, com: &ComApartment) {
    if com.is_initialized() {
        log(format!("{kind} worker started (STA)"));
    } else {
        log(format!(
            "{kind} worker COM initialization failed: HRESULT {:#x}",
            com.status()
        ));
    }
}

fn failed_scan(message: &str) -> ScanResult {
    ScanResult {
        apps: Vec::new(),
        warning: Some(format!("Applications could not be indexed: {message}.")),
    }
}
