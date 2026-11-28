//! Recursive scanning and filesystem watching.

use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, RwLock};
use std::time::Duration;

use notify::{RecursiveMode, Watcher};
use tokio::sync::Mutex;
use turso::Connection;

use pirsa_core::search_scope::IndexStatus;

use crate::{IndexConfig, db};

/// Upper bound on a full scan, so a pathological volume cannot fill the disk.
const MAX_ENTRIES: u64 = 2_000_000;

const SYSTEM_DIRECTORIES: &[&str] = &[
    "windows",
    "program files",
    "program files (x86)",
    "programdata",
    "$recycle.bin",
    "system volume information",
    "recovery",
    "perflogs",
];

pub(crate) async fn scan(
    config: IndexConfig,
    writer: Arc<Mutex<Connection>>,
    status: Arc<RwLock<IndexStatus>>,
) {
    {
        let mut status = status
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        status.building = true;
        status.ready = false;
        status.entries = 0;
    }

    if !config.roots.iter().any(|root| root.is_dir()) {
        // A disconnected drive must not wipe a good index; keep what is left.
        pirsa_core::util::log("file index scan skipped: no configured roots are available");
        let count = {
            let conn = writer.lock().await;
            db::count(&conn).await.unwrap_or(0)
        };
        let mut status = status
            .write()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        status.entries = count;
        status.building = false;
        status.ready = count > 0;
        return;
    }

    {
        let conn = writer.lock().await;
        if let Err(error) = db::clear_files(&conn).await {
            pirsa_core::util::log(format!("could not clear the file index: {error}"));
        }
    }

    let (sender, mut receiver) = tokio::sync::mpsc::channel(8_192);
    let walk_config = config.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let walker_stop = stop.clone();
    let walker = tokio::task::spawn_blocking(move || {
        for root in &walk_config.roots {
            if walk_root(root, &walk_config, &walker_stop, &mut |row| {
                let _ = sender.blocking_send(row);
            }) {
                break;
            }
        }
    });

    let mut batch = Vec::with_capacity(2_048);
    let mut indexed = 0_u64;

    while let Some(row) = receiver.recv().await {
        batch.push(row);
        indexed += 1;

        if indexed >= MAX_ENTRIES {
            stop.store(true, Ordering::Relaxed);
            receiver.close();
            break;
        }

        if batch.len() >= 2_048 {
            let mut conn = writer.lock().await;
            if let Err(error) = db::insert_batch(&mut conn, &batch).await {
                pirsa_core::util::log(format!("file index batch failed: {error}"));
            }
            drop(conn);
            batch.clear();

            let mut status = status
                .write()
                .unwrap_or_else(std::sync::PoisonError::into_inner);
            status.entries = indexed;
            status.building = true;
            status.ready = false;
        }
    }

    let _ = walker.await;

    if indexed >= MAX_ENTRIES {
        pirsa_core::util::log(format!(
            "file index reached its {MAX_ENTRIES} entry limit; narrow the scope to index the rest"
        ));
    }

    if !batch.is_empty() {
        let mut conn = writer.lock().await;
        if let Err(error) = db::insert_batch(&mut conn, &batch).await {
            pirsa_core::util::log(format!("final file index batch failed: {error}"));
        }
    }

    {
        let conn = writer.lock().await;
        if let Err(error) = db::optimize(&conn).await {
            pirsa_core::util::log(format!("could not optimize the file FTS index: {error}"));
        }
        let _ = db::set_meta(&conn, "complete", "1").await;
        let _ = db::set_meta(&conn, "fingerprint", &config.fingerprint.to_string()).await;
        let _ = db::set_meta(&conn, "last_scan", &crate::now_secs().to_string()).await;
    }

    let count = {
        let conn = writer.lock().await;
        db::count(&conn).await.unwrap_or(indexed)
    };

    let mut status = status
        .write()
        .unwrap_or_else(std::sync::PoisonError::into_inner);
    status.entries = count;
    status.building = false;
    status.ready = true;

    pirsa_core::util::log(format!("file index ready: {count} entries"));
}

fn walk_root(
    root: &Path,
    config: &IndexConfig,
    stop: &AtomicBool,
    sink: &mut impl FnMut(db::FileRow),
) -> bool {
    walk_dir(root, config, 0, stop, sink);
    stop.load(Ordering::Relaxed)
}

fn walk_dir(
    directory: &Path,
    config: &IndexConfig,
    depth: usize,
    stop: &AtomicBool,
    sink: &mut impl FnMut(db::FileRow),
) {
    if depth > config.scope.depth || stop.load(Ordering::Relaxed) {
        return;
    }

    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };

    for entry in entries.flatten() {
        if stop.load(Ordering::Relaxed) {
            return;
        }

        let path = entry.path();
        let Ok(file_type) = entry.file_type() else {
            continue;
        };

        if file_type.is_symlink() {
            continue;
        }

        let name = entry.file_name().to_string_lossy().into_owned();
        let name_lower = name.to_lowercase();

        if config.scope.is_hidden_skipped(&name_lower)
            || SYSTEM_DIRECTORIES.contains(&name_lower.as_str())
            || config.scope.is_excluded(&path)
        {
            continue;
        }

        let Ok(metadata) = entry.metadata() else {
            continue;
        };

        sink(db::FileRow::from_path(&path, &metadata));

        if metadata.is_dir() {
            walk_dir(&path, config, depth + 1, stop, sink);
        }
    }
}

pub(crate) async fn watch(
    config: IndexConfig,
    writer: Arc<Mutex<Connection>>,
    status: Arc<RwLock<IndexStatus>>,
) {
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel();
    let mut watcher = match notify::recommended_watcher(move |event| {
        let _ = sender.send(event);
    }) {
        Ok(watcher) => watcher,
        Err(error) => {
            pirsa_core::util::log(format!("could not start the file watcher: {error}"));
            return;
        }
    };

    let mut has_watch = false;
    for root in &config.roots {
        if root.is_dir() && watcher.watch(root, RecursiveMode::Recursive).is_ok() {
            has_watch = true;
        }
    }

    if !has_watch {
        return;
    }

    let mut pending = std::collections::BTreeSet::new();

    loop {
        let event = tokio::time::timeout(Duration::from_millis(750), receiver.recv()).await;

        match event {
            Ok(Some(Ok(event))) => {
                for path in event.paths {
                    pending.insert(path);
                }
            }
            Ok(Some(Err(error))) => {
                pirsa_core::util::log(format!("file watcher error: {error}"));
                pending.clear();
                scan(config.clone(), writer.clone(), status.clone()).await;
            }
            Ok(None) => break,
            Err(_) if pending.is_empty() => {}
            Err(_) => {
                let paths: Vec<_> = pending.iter().cloned().collect();
                pending.clear();

                let mut conn = writer.lock().await;
                for path in paths {
                    apply_path(&mut conn, &path, &config).await;
                }
                drop(conn);

                let count = {
                    let conn = writer.lock().await;
                    db::count(&conn).await.unwrap_or(0)
                };
                let mut status = status
                    .write()
                    .unwrap_or_else(std::sync::PoisonError::into_inner);
                status.entries = count;
            }
        }
    }
}

async fn apply_path(conn: &mut Connection, path: &Path, config: &IndexConfig) {
    let key = db::path_key(path);
    let name = path
        .file_name()
        .map(|name| name.to_string_lossy().to_lowercase())
        .unwrap_or_default();

    // A newly created file or folder must obey the same scope rules as the
    // full walk, including when it was indexed before the rules changed.
    if config.scope.is_hidden_skipped(&name)
        || SYSTEM_DIRECTORIES.contains(&name.as_str())
        || config.scope.is_excluded(path)
    {
        let _ = db::delete_path(conn, &key).await;
        return;
    }

    match std::fs::metadata(path) {
        Ok(metadata) if metadata.is_dir() => {
            // The directory itself may be new (a copied folder, for example),
            // so record it before its contents.
            let mut rows = vec![db::FileRow::from_path(path, &metadata)];
            let stop = AtomicBool::new(false);
            walk_dir(path, config, 0, &stop, &mut |row| rows.push(row));
            let _ = db::insert_batch(conn, &rows).await;
        }
        Ok(metadata) => {
            let row = db::FileRow::from_path(path, &metadata);
            let _ = db::insert_batch(conn, &[row]).await;
        }
        Err(_) => {
            let _ = db::delete_path(conn, &key).await;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::db;
    use std::path::PathBuf;

    #[tokio::test]
    async fn indexes_a_temp_tree_and_persists() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("root");
        std::fs::create_dir_all(root.join("reports")).unwrap();
        std::fs::write(root.join("reports").join("annual-report.txt"), b"hello").unwrap();
        std::fs::write(root.join("notes.md"), b"notes").unwrap();

        let db_path = directory.path().join("index.db");
        let database = db::open(&db_path).await.unwrap();
        let writer = database.connect().unwrap();
        db::migrate(&writer).await.unwrap();
        let writer = Arc::new(Mutex::new(writer));

        let status = Arc::new(RwLock::new(IndexStatus::default()));
        let config = IndexConfig {
            roots: vec![root.clone()],
            scope: pirsa_core::search_scope::SearchScope::default(),
            fingerprint: 1,
        };

        scan(config.clone(), writer.clone(), status.clone()).await;

        let reader = database.connect().unwrap();
        let hits = db::search(&reader, "report", 10).await.unwrap();
        assert!(hits.iter().any(|hit| hit.name() == "annual-report.txt"));
        assert!(
            hits.iter()
                .any(|hit| hit.name() == "reports" && hit.is_dir())
        );
        assert!(status.read().unwrap().ready);

        drop(reader);
        drop(database);

        let reopened = db::open(&db_path).await.unwrap();
        let reader = reopened.connect().unwrap();
        let hits = db::search(&reader, "notes", 10).await.unwrap();
        assert!(hits.iter().any(|hit| hit.name() == "notes.md"));
    }

    #[tokio::test]
    async fn applying_a_directory_records_the_directory_itself() {
        let directory = tempfile::tempdir().unwrap();
        let root = directory.path().join("root");
        let copied = root.join("copied");
        std::fs::create_dir_all(copied.join("nested")).unwrap();
        std::fs::write(copied.join("nested").join("notes.txt"), b"notes").unwrap();

        let db_path = directory.path().join("index.db");
        let database = db::open(&db_path).await.unwrap();
        let mut writer = database.connect().unwrap();
        db::migrate(&writer).await.unwrap();

        let config = IndexConfig {
            roots: vec![root],
            scope: pirsa_core::search_scope::SearchScope::default(),
            fingerprint: 1,
        };
        apply_path(&mut writer, &copied, &config).await;

        let hits = db::search(&writer, "copied", 10).await.unwrap();
        assert!(
            hits.iter()
                .any(|hit| hit.name() == "copied" && hit.is_dir()),
            "the new directory is searchable, not just its contents"
        );
    }

    #[test]
    fn path_keys_are_normalized() {
        assert_eq!(
            db::path_key(&PathBuf::from("C:\\Work\\Reports\\")),
            "c:\\work\\reports"
        );
    }
}
