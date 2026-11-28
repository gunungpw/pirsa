//! Background Turso service and the handle used by the UI.

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};

use pirsa_core::FileEntry;
use pirsa_core::search_scope::{IndexStatus, SearchScope};
use tokio::sync::{Mutex, mpsc, oneshot};
use turso::Connection;

use crate::{db, indexer};

/// Everything the indexer needs for one scope.
#[derive(Debug, Clone)]
pub struct IndexConfig {
    pub roots: Vec<PathBuf>,
    pub scope: SearchScope,
    pub fingerprint: u64,
}

enum Command {
    Search {
        term: String,
        limit: usize,
        reply: oneshot::Sender<Vec<FileEntry>>,
    },
    RecordLaunch(String),
    Rebuild,
}

/// Cheap handle used by iced tasks. The actual database lives on a worker
/// thread with its own Tokio runtime.
#[derive(Clone, Debug)]
pub struct IndexHandle {
    tx: mpsc::UnboundedSender<Command>,
    status: Arc<RwLock<IndexStatus>>,
}

impl IndexHandle {
    pub async fn search(&self, term: String, limit: usize) -> Vec<FileEntry> {
        let (reply, receiver) = oneshot::channel();

        if self
            .tx
            .send(Command::Search { term, limit, reply })
            .is_err()
        {
            return Vec::new();
        }

        receiver.await.unwrap_or_default()
    }

    /// Queues a usage update; the writer actor serializes it with indexing.
    pub fn record_launch(&self, usage_key: String) {
        let _ = self.tx.send(Command::RecordLaunch(usage_key));
    }

    pub fn rebuild(&self) {
        {
            let mut status = self
                .status
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            status.building = true;
            status.ready = false;
        }
        let _ = self.tx.send(Command::Rebuild);
    }

    #[must_use]
    pub fn status(&self) -> IndexStatus {
        *self
            .status
            .read()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
    }
}

/// Starts the indexer/search service. Returns immediately; the worker opens the
/// database, migrates it, and starts scanning on its own thread.
#[must_use]
pub fn start(db_path: PathBuf, config: IndexConfig) -> IndexHandle {
    let status = Arc::new(RwLock::new(IndexStatus {
        building: true,
        ..IndexStatus::default()
    }));
    let (tx, rx) = mpsc::unbounded_channel();
    let handle = IndexHandle {
        tx,
        status: status.clone(),
    };

    let worker_status = status.clone();
    let spawn = std::thread::Builder::new()
        .name("pirsa-file-index".to_owned())
        .spawn(move || {
            set_below_normal_priority();

            let runtime = match tokio::runtime::Builder::new_multi_thread()
                .worker_threads(2)
                .enable_all()
                .build()
            {
                Ok(runtime) => runtime,
                Err(error) => {
                    pirsa_core::util::log(format!(
                        "could not start the file index runtime: {error}"
                    ));
                    mark_idle(&worker_status);
                    return;
                }
            };

            runtime.block_on(run(db_path, config, rx, worker_status));
        });

    if let Err(error) = spawn {
        pirsa_core::util::log(format!("could not start the file index thread: {error}"));
        let mut state = status
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        state.building = false;
    }

    handle
}

async fn run(
    db_path: PathBuf,
    config: IndexConfig,
    mut rx: mpsc::UnboundedReceiver<Command>,
    status: Arc<RwLock<IndexStatus>>,
) {
    if let Some(parent) = db_path.parent()
        && let Err(error) = std::fs::create_dir_all(parent)
    {
        pirsa_core::util::log(format!(
            "could not create the file index directory: {error}"
        ));
        mark_idle(&status);
        return;
    }

    let database = match db::open(&db_path).await {
        Ok(database) => database,
        Err(error) => {
            pirsa_core::util::log(format!("could not open the file index: {error}"));
            mark_idle(&status);
            return;
        }
    };

    let writer = match database.connect() {
        Ok(connection) => connection,
        Err(error) => {
            pirsa_core::util::log(format!("could not connect to the file index: {error}"));
            mark_idle(&status);
            return;
        }
    };

    if let Err(error) = db::migrate(&writer).await {
        pirsa_core::util::log(format!("file index migration failed: {error}"));
        mark_idle(&status);
        return;
    }

    let reader = match database.connect() {
        Ok(connection) => connection,
        Err(error) => {
            pirsa_core::util::log(format!("could not open the file index reader: {error}"));
            mark_idle(&status);
            return;
        }
    };

    let writer = Arc::new(Mutex::new(writer));
    let scanning = Arc::new(AtomicBool::new(false));

    let (complete, fingerprint, last_scan) = {
        let connection = writer.lock().await;
        let complete = db::get_meta(&connection, "complete")
            .await
            .ok()
            .flatten()
            .as_deref()
            == Some("1");
        let fingerprint = db::get_meta(&connection, "fingerprint")
            .await
            .ok()
            .flatten();
        let last_scan = db::get_meta(&connection, "last_scan")
            .await
            .ok()
            .flatten()
            .and_then(|value| value.parse::<i64>().ok())
            .unwrap_or(0);
        (complete, fingerprint, last_scan)
    };
    let expected_fingerprint = config.fingerprint.to_string();
    let scope_changed = fingerprint.as_deref() != Some(expected_fingerprint.as_str());
    let stale = crate::now_secs().saturating_sub(last_scan) > 6 * 60 * 60;

    if complete && !scope_changed && !stale {
        let count = {
            let connection = writer.lock().await;
            db::count(&connection).await.unwrap_or(0)
        };
        let mut status = status
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        status.ready = true;
        status.building = false;
        status.entries = count;
    } else {
        spawn_scan(
            config.clone(),
            writer.clone(),
            status.clone(),
            scanning.clone(),
        );
    }

    spawn_watcher(config.clone(), writer.clone(), status.clone());

    while let Some(command) = rx.recv().await {
        match command {
            Command::Search { term, limit, reply } => {
                let hits = db::search(&reader, &term, limit)
                    .await
                    .unwrap_or_else(|error| {
                        pirsa_core::util::log(format!("file search failed: {error}"));
                        Vec::new()
                    });
                let _ = reply.send(hits);
            }
            Command::RecordLaunch(usage_key) => {
                let connection = writer.lock().await;
                if let Err(error) = db::record_launch(&connection, &usage_key).await {
                    pirsa_core::util::log(format!("could not record file usage: {error}"));
                }
            }
            Command::Rebuild => {
                spawn_scan(
                    config.clone(),
                    writer.clone(),
                    status.clone(),
                    scanning.clone(),
                );
            }
        }
    }
}

fn spawn_scan(
    config: IndexConfig,
    writer: Arc<Mutex<Connection>>,
    status: Arc<RwLock<IndexStatus>>,
    scanning: Arc<AtomicBool>,
) {
    if scanning.swap(true, Ordering::SeqCst) {
        return;
    }

    tokio::spawn(async move {
        indexer::scan(config, writer, status).await;
        scanning.store(false, Ordering::SeqCst);
    });
}

fn spawn_watcher(
    config: IndexConfig,
    writer: Arc<Mutex<Connection>>,
    status: Arc<RwLock<IndexStatus>>,
) {
    tokio::spawn(async move {
        indexer::watch(config, writer, status).await;
    });
}

fn mark_idle(status: &RwLock<IndexStatus>) {
    let mut status = status
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    status.building = false;
}

fn set_below_normal_priority() {
    #[cfg(windows)]
    unsafe {
        use windows::Win32::System::Threading::{
            GetCurrentThread, SetThreadPriority, THREAD_PRIORITY_BELOW_NORMAL,
        };

        let _ = SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn service_indexes_and_searches_off_thread() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("root");
        std::fs::create_dir_all(root.join("reports")).unwrap();
        std::fs::write(root.join("reports").join("annual-report.txt"), b"hello").unwrap();

        let handle = start(
            directory.path().join("index.db"),
            IndexConfig {
                roots: vec![root],
                scope: SearchScope::default(),
                fingerprint: 1,
            },
        );

        let deadline = tokio::time::Instant::now() + std::time::Duration::from_secs(8);
        while tokio::time::Instant::now() < deadline && !handle.status().ready {
            tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        }
        assert!(handle.status().ready, "indexer should become ready");

        let hits = handle.search("report".to_owned(), 10).await;
        assert!(hits.iter().any(|hit| hit.name() == "annual-report.txt"));
        assert!(
            hits.iter()
                .any(|hit| hit.name() == "reports" && hit.is_dir())
        );

        // Exercise the writer path; it must not panic or block the search worker.
        handle.record_launch(hits[0].usage_key().to_owned());
    }
}
