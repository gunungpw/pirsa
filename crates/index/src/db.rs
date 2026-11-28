//! Turso schema, writes, usage, and ranked FTS search.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use turso::{Builder, Connection, Database, params};

use pirsa_core::FileEntry;

use crate::rank;

pub type Result<T> = std::result::Result<T, turso::Error>;

const SCHEMA_VERSION: i64 = 1;

const SCHEMA: &[&str] = &[
    "CREATE TABLE IF NOT EXISTS schema_migrations (
        version INTEGER PRIMARY KEY,
        applied_at INTEGER NOT NULL
    )",
    "CREATE TABLE IF NOT EXISTS files (
        id INTEGER PRIMARY KEY,
        path TEXT NOT NULL,
        path_key TEXT NOT NULL UNIQUE,
        name TEXT NOT NULL,
        name_key TEXT NOT NULL,
        is_dir INTEGER NOT NULL DEFAULT 0,
        size INTEGER NOT NULL DEFAULT 0,
        modified INTEGER NOT NULL DEFAULT 0,
        usage_key TEXT NOT NULL,
        indexed_at INTEGER NOT NULL
    )",
    "CREATE UNIQUE INDEX IF NOT EXISTS files_usage_key ON files(usage_key)",
    "CREATE INDEX IF NOT EXISTS files_name_key ON files(name_key)",
    "CREATE INDEX IF NOT EXISTS files_path_key ON files(path_key)",
    "CREATE INDEX IF NOT EXISTS files_fts ON files USING fts (name, path)
        WITH (tokenizer = 'ngram', weights = 'name=5.0,path=1.0')",
    "CREATE TABLE IF NOT EXISTS usage (
        item_key TEXT PRIMARY KEY,
        launch_count INTEGER NOT NULL DEFAULT 0,
        last_launched INTEGER NOT NULL DEFAULT 0,
        frecency REAL NOT NULL DEFAULT 0
    )",
    "CREATE TABLE IF NOT EXISTS meta (
        key TEXT PRIMARY KEY,
        value TEXT NOT NULL
    )",
];

#[derive(Debug, Clone)]
pub(crate) struct FileRow {
    pub path: String,
    pub path_key: String,
    pub name: String,
    pub name_key: String,
    pub is_dir: bool,
    pub size: i64,
    pub modified: i64,
    pub usage_key: String,
}

impl FileRow {
    pub(crate) fn from_path(path: &Path, metadata: &std::fs::Metadata) -> Self {
        let path_string = path.to_string_lossy().into_owned();
        let path_key = path_key(path);
        let name = path.file_name().map_or_else(
            || path_string.clone(),
            |name| name.to_string_lossy().into_owned(),
        );
        let name_key = name.to_lowercase();
        let modified = metadata
            .modified()
            .ok()
            .and_then(|time| time.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |duration| duration.as_secs() as i64);

        Self {
            path: path_string,
            path_key: path_key.clone(),
            name,
            name_key,
            is_dir: metadata.is_dir(),
            size: metadata.len() as i64,
            modified,
            usage_key: format!("path:{path_key}"),
        }
    }
}

pub(crate) fn path_key(path: &Path) -> String {
    path.to_string_lossy()
        .replace('/', "\\")
        .trim_end_matches('\\')
        .to_lowercase()
}

pub(crate) async fn open(path: &Path) -> Result<Database> {
    Builder::new_local(path.to_string_lossy().as_ref())
        .experimental_index_method(true)
        .build()
        .await
}

pub(crate) async fn migrate(conn: &Connection) -> Result<()> {
    conn.execute("PRAGMA journal_mode = wal", ()).await?;
    conn.execute(
        "CREATE TABLE IF NOT EXISTS schema_migrations (
            version INTEGER PRIMARY KEY,
            applied_at INTEGER NOT NULL
        )",
        (),
    )
    .await?;

    let current: i64 = {
        let mut rows = conn
            .query(
                "SELECT COALESCE(MAX(version), 0) FROM schema_migrations",
                (),
            )
            .await?;
        let row = rows.next().await?.expect("aggregate row");
        row.get(0)?
    };

    if current < SCHEMA_VERSION {
        for statement in SCHEMA {
            conn.execute(statement, ()).await?;
        }

        conn.execute(
            "INSERT INTO schema_migrations (version, applied_at)
             VALUES (?1, ?2)",
            params![SCHEMA_VERSION, crate::now_secs()],
        )
        .await?;
    }

    Ok(())
}

pub(crate) async fn insert_batch(conn: &mut Connection, rows: &[FileRow]) -> Result<()> {
    if rows.is_empty() {
        return Ok(());
    }

    let tx = conn.transaction().await?;
    for row in rows {
        tx.execute(
            "INSERT INTO files
                (path, path_key, name, name_key, is_dir, size, modified, usage_key, indexed_at)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(path_key) DO UPDATE SET
                path = excluded.path,
                name = excluded.name,
                name_key = excluded.name_key,
                is_dir = excluded.is_dir,
                size = excluded.size,
                modified = excluded.modified,
                usage_key = excluded.usage_key,
                indexed_at = excluded.indexed_at",
            params![
                row.path.clone(),
                row.path_key.clone(),
                row.name.clone(),
                row.name_key.clone(),
                row.is_dir as i64,
                row.size,
                row.modified,
                row.usage_key.clone(),
                crate::now_secs(),
            ],
        )
        .await?;
    }
    tx.commit().await
}

pub(crate) async fn clear_files(conn: &Connection) -> Result<()> {
    conn.execute("DELETE FROM files", ()).await?;
    Ok(())
}

pub(crate) async fn delete_path(conn: &Connection, path_key: &str) -> Result<()> {
    let path_key = path_key.trim_end_matches('\\');
    let prefix = format!("{path_key}\\");
    let upper = format!("{prefix}\u{10FFFF}");

    conn.execute(
        "DELETE FROM files
         WHERE path_key = ?1 OR (path_key >= ?2 AND path_key < ?3)",
        params![path_key, prefix, upper],
    )
    .await?;
    Ok(())
}

pub(crate) async fn set_meta(conn: &Connection, key: &str, value: &str) -> Result<()> {
    conn.execute(
        "INSERT INTO meta (key, value) VALUES (?1, ?2)
         ON CONFLICT(key) DO UPDATE SET value = excluded.value",
        params![key, value],
    )
    .await?;
    Ok(())
}

pub(crate) async fn get_meta(conn: &Connection, key: &str) -> Result<Option<String>> {
    let mut rows = conn
        .query("SELECT value FROM meta WHERE key = ?1", params![key])
        .await?;
    rows.next().await?.map(|row| row.get(0)).transpose()
}

pub(crate) async fn count(conn: &Connection) -> Result<u64> {
    let mut rows = conn.query("SELECT COUNT(*) FROM files", ()).await?;
    let row = rows.next().await?.expect("aggregate row");
    let count: i64 = row.get(0)?;
    Ok(count as u64)
}

pub(crate) async fn optimize(conn: &Connection) -> Result<()> {
    conn.execute("OPTIMIZE INDEX files_fts", ()).await?;
    Ok(())
}

#[derive(Debug)]
struct Candidate {
    entry: FileEntry,
    frecency: f64,
}

pub(crate) async fn search(conn: &Connection, term: &str, limit: usize) -> Result<Vec<FileEntry>> {
    let term = term.trim().to_lowercase();
    if term.is_empty() {
        return Ok(Vec::new());
    }

    let limit = limit.clamp(1, 5_000);
    // Rank in Rust, so fetch a wider pool from each lookup than the caller
    // needs; a handful of high-usage weak matches must not crowd out an exact
    // name that the SQL planner returned later.
    let prefix_limit = limit;
    let fts_limit = (limit * 4).min(20_000);
    let mut candidates = Vec::new();
    let upper = format!("{term}\u{10FFFF}");

    let mut rows = conn
        .query(
            "SELECT f.path, f.name, f.is_dir, f.size, f.modified, f.usage_key,
                    COALESCE(u.frecency, 0.0)
             FROM files f
             LEFT JOIN usage u ON u.item_key = f.usage_key
             WHERE f.name_key >= ?1 AND f.name_key < ?2
             LIMIT ?3",
            params![term.clone(), upper, prefix_limit as i64],
        )
        .await?;
    collect_rows(&mut rows, &mut candidates).await?;

    if term.chars().count() >= 2 {
        let mut rows = conn
            .query(
                "SELECT f.path, f.name, f.is_dir, f.size, f.modified, f.usage_key,
                        COALESCE(u.frecency, 0.0)
                 FROM files f
                 LEFT JOIN usage u ON u.item_key = f.usage_key
                 WHERE fts_match(f.name, f.path, ?1)
                 LIMIT ?2",
                params![fts_query(&term), fts_limit as i64],
            )
            .await?;
        collect_rows(&mut rows, &mut candidates).await?;
    }

    let mut unique: HashMap<String, Candidate> = HashMap::new();
    for candidate in candidates {
        unique
            .entry(candidate.entry.usage_key().to_owned())
            .or_insert(candidate);
    }

    let mut hits: Vec<Candidate> = unique.into_values().collect();
    hits.sort_by(|left, right| {
        let left_score = rank::final_score(
            rank::match_quality(
                left.entry.name(),
                left.entry.path().to_str().unwrap_or(""),
                &term,
            )
            .unwrap_or(0),
            left.frecency as f32,
        );
        let right_score = rank::final_score(
            rank::match_quality(
                right.entry.name(),
                right.entry.path().to_str().unwrap_or(""),
                &term,
            )
            .unwrap_or(0),
            right.frecency as f32,
        );

        right_score
            .cmp(&left_score)
            .then_with(|| left.entry.name().cmp(right.entry.name()))
    });

    hits.truncate(limit);
    Ok(hits.into_iter().map(|hit| hit.entry).collect())
}

async fn collect_rows(rows: &mut turso::Rows, out: &mut Vec<Candidate>) -> Result<()> {
    while let Some(row) = rows.next().await? {
        let path: String = row.get(0)?;
        let name: String = row.get(1)?;
        let is_dir: i64 = row.get(2)?;
        let size: i64 = row.get(3)?;
        let modified: i64 = row.get(4)?;
        let usage_key: String = row.get(5)?;
        let frecency: f64 = row.get(6)?;

        out.push(Candidate {
            entry: FileEntry::new(
                PathBuf::from(path),
                name,
                is_dir != 0,
                size as u64,
                modified as u64,
                usage_key,
            ),
            frecency,
        });
    }

    Ok(())
}

fn fts_query(term: &str) -> String {
    // Quoted phrase; Tantivy escaping for quotes and backslashes.
    format!("\"{}\"", term.replace('\\', "\\\\").replace('"', "\\\""))
}

pub(crate) async fn record_launch(conn: &Connection, usage_key: &str) -> Result<()> {
    let now = crate::now_secs();

    let mut rows = conn
        .query(
            "SELECT launch_count, last_launched FROM usage WHERE item_key = ?1",
            params![usage_key],
        )
        .await?;

    let (launch_count, last_launched) = match rows.next().await? {
        Some(row) => (row.get::<i64>(0)? as u32, row.get::<i64>(1)?),
        None => (0, now),
    };

    let launch_count = launch_count.saturating_add(1);
    let frecency = rank::frecency(launch_count, last_launched, now);

    conn.execute(
        "INSERT INTO usage (item_key, launch_count, last_launched, frecency)
         VALUES (?1, ?2, ?3, ?4)
         ON CONFLICT(item_key) DO UPDATE SET
            launch_count = excluded.launch_count,
            last_launched = excluded.last_launched,
            frecency = excluded.frecency",
        params![usage_key, launch_count as i64, now, frecency as f64],
    )
    .await?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn memory_db() -> (Database, Connection) {
        let db = Builder::new_local(":memory:")
            .experimental_index_method(true)
            .build()
            .await
            .unwrap();
        let conn = db.connect().unwrap();
        migrate(&conn).await.unwrap();
        (db, conn)
    }

    fn row(name: &str) -> FileRow {
        let path = PathBuf::from(format!("C:\\work\\{name}"));
        let key = path_key(&path);

        FileRow {
            path: path.to_string_lossy().into_owned(),
            path_key: key.clone(),
            name: name.to_owned(),
            name_key: name.to_lowercase(),
            is_dir: false,
            size: 0,
            modified: 0,
            usage_key: format!("path:{key}"),
        }
    }

    #[tokio::test]
    async fn fts_and_prefix_find_files() {
        let (_db, mut conn) = memory_db().await;
        insert_batch(
            &mut conn,
            &[
                row("reports"),
                row("annual-reports"),
                row("notes"),
                row("report-2025"),
            ],
        )
        .await
        .unwrap();

        let hits = search(&conn, "report", 20).await.unwrap();
        let names: Vec<&str> = hits.iter().map(FileEntry::name).collect();

        assert!(names.contains(&"reports"));
        assert!(names.contains(&"annual-reports"));
        assert!(names.contains(&"report-2025"));
        assert!(!names.contains(&"notes"));
    }

    #[tokio::test]
    async fn exact_match_ranks_before_substring() {
        let (_db, mut conn) = memory_db().await;
        insert_batch(&mut conn, &[row("reports"), row("annual-reports")])
            .await
            .unwrap();

        let hits = search(&conn, "reports", 10).await.unwrap();
        assert_eq!(hits[0].name(), "reports");
    }

    #[tokio::test]
    async fn usage_cannot_override_a_better_match() {
        let (_db, mut conn) = memory_db().await;
        insert_batch(&mut conn, &[row("reports"), row("reports-2025")])
            .await
            .unwrap();

        // Give the weaker prefix match a large launch history.
        for _ in 0..1_000 {
            record_launch(&conn, "path:c:\\work\\reports-2025")
                .await
                .unwrap();
        }

        let hits = search(&conn, "reports", 10).await.unwrap();
        assert_eq!(hits[0].name(), "reports", "an exact match wins");
    }

    #[tokio::test]
    async fn usage_is_persisted() {
        let (_db, conn) = memory_db().await;
        record_launch(&conn, "path:c:\\work\\reports")
            .await
            .unwrap();

        let mut rows = conn
            .query(
                "SELECT launch_count FROM usage WHERE item_key = 'path:c:\\work\\reports'",
                (),
            )
            .await
            .unwrap();
        let row = rows.next().await.unwrap().unwrap();
        let count: i64 = row.get(0).unwrap();
        assert_eq!(count, 1);
    }
}
