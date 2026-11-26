//! Persistent folder index for full-drive folder search.
//!
//! A bounded live walk (see [`super::folders`]) cannot cover a whole drive, so
//! the index is built once in the background, cached on disk, and queried in
//! memory afterwards. Folders are stored as `(parent, name)` pairs in one name
//! arena, which keeps a million folders around ~40 MB and lets the path be
//! reconstructed only for the rows that are actually shown.

use std::path::PathBuf;
use std::sync::atomic::{AtomicU8, AtomicU64, Ordering};
use std::sync::{Arc, Mutex, OnceLock};
use std::time::Instant;

use pirsa_core::FolderEntry;
use pirsa_core::search_scope::{IndexStatus, SearchScope};
use pirsa_core::util::{data_directory, log};

use super::drives;
use super::folders;

const MAGIC: &[u8; 8] = b"PIRSAFI1";
const VERSION: u32 = 1;
const FILE_NAME: &str = "folders.index";

/// Upper bound on indexed folders. A full `C:` has a few million; capping keeps
/// memory and the cache file predictable.
const MAX_ENTRIES: usize = 1_500_000;
/// How often the walker reports progress.
const PROGRESS_INTERVAL: usize = 2_048;

const STATE_IDLE: u8 = 0;
const STATE_BUILDING: u8 = 1;
const STATE_READY: u8 = 2;

/// Header size: magic + version + fingerprint + count + names length.
const HEADER_LEN: usize = MAGIC.len() + 4 + 8 + 4 + 4;
/// `parent: u32, start: u32, len: u16, pad: u16`.
const ENTRY_LEN: usize = 12;
/// Marks an entry whose parent is a root.
const NO_PARENT: u32 = u32::MAX;

#[derive(Debug, Clone)]
struct Entry {
    parent: u32,
    start: u32,
    len: u16,
}

/// In-memory folder index.
#[derive(Debug, Default, Clone)]
pub(crate) struct FolderIndex {
    entries: Vec<Entry>,
    names: String,
}

impl FolderIndex {
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    fn name(&self, index: usize) -> &str {
        let entry = &self.entries[index];
        &self.names[entry.start as usize..entry.start as usize + entry.len as usize]
    }

    fn push(&mut self, parent: u32, name: &str) -> u32 {
        let start = self.names.len() as u32;
        self.names.push_str(name);
        let len = (self.names.len() - start as usize) as u16;
        self.entries.push(Entry { parent, start, len });

        (self.entries.len() - 1) as u32
    }

    /// Rebuilds the path by walking up the parent chain.
    fn path(&self, index: usize) -> PathBuf {
        let mut parts: Vec<&str> = Vec::new();
        let mut current = index;

        loop {
            let entry = &self.entries[current];

            if entry.parent == NO_PARENT {
                let mut path = PathBuf::from(self.name(current));

                for part in parts.iter().rev() {
                    path.push(part);
                }

                return path;
            }

            parts.push(self.name(current));
            current = entry.parent as usize;
        }
    }

    /// Returns up to `limit` folders whose name matches `term`, best first.
    pub fn search(&self, term: &str, limit: usize) -> Vec<FolderEntry> {
        let needle = term.trim().to_lowercase();

        if needle.is_empty() || self.entries.is_empty() {
            return Vec::new();
        }

        let mut matches: Vec<(i32, u32)> = Vec::new();

        for index in 0..self.entries.len() {
            // Roots carry a full path, not a name, and never match a query.
            if self.entries[index].parent == NO_PARENT {
                continue;
            }

            if let Some(score) = score(self.name(index), &needle) {
                matches.push((score, index as u32));
            }
        }

        matches.sort_by(|left, right| right.0.cmp(&left.0).then_with(|| left.1.cmp(&right.1)));
        matches.truncate(limit);

        matches
            .into_iter()
            .map(|(_, index)| {
                let index = index as usize;
                FolderEntry::new(self.path(index), self.name(index).to_owned())
            })
            .collect()
    }

    fn encode(&self, fingerprint: u64) -> Vec<u8> {
        let mut bytes =
            Vec::with_capacity(HEADER_LEN + self.entries.len() * ENTRY_LEN + self.names.len());

        bytes.extend_from_slice(MAGIC);
        bytes.extend_from_slice(&VERSION.to_le_bytes());
        bytes.extend_from_slice(&fingerprint.to_le_bytes());
        bytes.extend_from_slice(&(self.entries.len() as u32).to_le_bytes());
        bytes.extend_from_slice(&(self.names.len() as u32).to_le_bytes());

        for entry in &self.entries {
            bytes.extend_from_slice(&entry.parent.to_le_bytes());
            bytes.extend_from_slice(&entry.start.to_le_bytes());
            bytes.extend_from_slice(&entry.len.to_le_bytes());
            bytes.extend_from_slice(&0_u16.to_le_bytes());
        }

        bytes.extend_from_slice(self.names.as_bytes());
        bytes
    }

    fn decode(bytes: &[u8], fingerprint: u64) -> Option<Self> {
        if bytes.len() < HEADER_LEN || &bytes[..MAGIC.len()] != MAGIC {
            return None;
        }

        let version = u32::from_le_bytes(bytes[8..12].try_into().ok()?);
        let stored_fingerprint = u64::from_le_bytes(bytes[12..20].try_into().ok()?);
        let count = u32::from_le_bytes(bytes[20..24].try_into().ok()?) as usize;
        let names_len = u32::from_le_bytes(bytes[24..28].try_into().ok()?) as usize;

        if version != VERSION || stored_fingerprint != fingerprint || count > MAX_ENTRIES {
            return None;
        }

        let entries_end = HEADER_LEN.checked_add(count.checked_mul(ENTRY_LEN)?)?;
        let total = entries_end.checked_add(names_len)?;

        if bytes.len() != total {
            return None;
        }

        let names = std::str::from_utf8(&bytes[entries_end..]).ok()?.to_owned();
        let mut entries = Vec::with_capacity(count);

        for chunk in bytes[HEADER_LEN..entries_end].as_chunks::<ENTRY_LEN>().0 {
            let parent = u32::from_le_bytes(chunk[0..4].try_into().ok()?);
            let start = u32::from_le_bytes(chunk[4..8].try_into().ok()?);
            let len = u16::from_le_bytes(chunk[8..10].try_into().ok()?);
            let end = start.checked_add(u32::from(len))?;

            if parent != NO_PARENT && parent as usize >= count {
                return None;
            }

            if end as usize > names.len() {
                return None;
            }

            entries.push(Entry { parent, start, len });
        }

        Some(Self { entries, names })
    }
}

/// Scoring for indexed names: case-insensitive, no allocation on the common
/// ASCII path.
fn score(name: &str, needle: &str) -> Option<i32> {
    if name.eq_ignore_ascii_case(needle) {
        return Some(1_200);
    }

    if let Some(prefix) = name.get(..needle.len())
        && prefix.eq_ignore_ascii_case(needle)
    {
        let extra = name.chars().count().saturating_sub(needle.chars().count()) as i32;
        return Some(1_000 - extra);
    }

    let position = find_ignore_case(name, needle)?;

    Some(600 - position as i32 * 4)
}

fn find_ignore_case(haystack: &str, needle: &str) -> Option<usize> {
    if needle.is_empty() || needle.len() > haystack.len() {
        return None;
    }

    if haystack.is_ascii() {
        let needle = needle.as_bytes();
        let bytes = haystack.as_bytes();

        return bytes
            .windows(needle.len())
            .position(|window| window.eq_ignore_ascii_case(needle));
    }

    haystack.to_lowercase().find(needle)
}

static INDEX: OnceLock<Mutex<Option<Arc<FolderIndex>>>> = OnceLock::new();
static STATE: AtomicU8 = AtomicU8::new(STATE_IDLE);
static STATE_FINGERPRINT: AtomicU64 = AtomicU64::new(0);
static GENERATION: AtomicU64 = AtomicU64::new(0);
static SCANNED: AtomicU64 = AtomicU64::new(0);

fn index_slot() -> &'static Mutex<Option<Arc<FolderIndex>>> {
    INDEX.get_or_init(|| Mutex::new(None))
}

/// Current index state for the UI.
pub fn status() -> IndexStatus {
    let state = STATE.load(Ordering::Relaxed);
    let folders = if state == STATE_READY {
        index_slot()
            .lock()
            .ok()
            .and_then(|slot| slot.as_ref().map(|index| index.len() as u64))
            .unwrap_or(0)
    } else {
        SCANNED.load(Ordering::Relaxed)
    };

    IndexStatus {
        ready: state == STATE_READY,
        building: state == STATE_BUILDING,
        folders,
    }
}

/// Queries the index. `None` means it is not ready and the caller should fall
/// back to the bounded live walk.
pub fn search(term: &str, limit: usize) -> Option<Vec<FolderEntry>> {
    if STATE.load(Ordering::Relaxed) != STATE_READY {
        return None;
    }

    let index = index_slot().lock().ok().and_then(|slot| slot.clone())?;

    Some(index.search(term, limit))
}

/// Ensures an index for `scope` exists: loads the cache when it matches, or
/// starts a background build.
pub fn ensure(scope: &SearchScope) {
    let fingerprint = scope.fingerprint();
    let state = STATE.load(Ordering::Relaxed);

    if state == STATE_READY && STATE_FINGERPRINT.load(Ordering::Relaxed) == fingerprint {
        return;
    }

    if state == STATE_BUILDING && STATE_FINGERPRINT.load(Ordering::Relaxed) == fingerprint {
        return;
    }

    start(scope.clone(), fingerprint);
}

/// Drops the index and cancels any running build.
pub fn invalidate() {
    GENERATION.fetch_add(1, Ordering::Relaxed);
    STATE.store(STATE_IDLE, Ordering::Relaxed);
    SCANNED.store(0, Ordering::Relaxed);

    if let Ok(mut slot) = index_slot().lock() {
        *slot = None;
    }
}

/// Rebuilds from scratch, ignoring the cache.
pub fn rebuild(scope: &SearchScope) {
    invalidate();
    start(scope.clone(), scope.fingerprint());
}

fn start(scope: SearchScope, fingerprint: u64) {
    let generation = GENERATION.fetch_add(1, Ordering::Relaxed) + 1;
    STATE.store(STATE_BUILDING, Ordering::Relaxed);
    STATE_FINGERPRINT.store(fingerprint, Ordering::Relaxed);
    SCANNED.store(0, Ordering::Relaxed);

    let spawn = std::thread::Builder::new()
        .name("pirsa-folder-index".to_owned())
        .spawn(move || run(scope, fingerprint, generation));

    if let Err(error) = spawn {
        log(format!("could not start the folder indexer: {error}"));
        STATE.store(STATE_IDLE, Ordering::Relaxed);
    }
}

fn run(scope: SearchScope, fingerprint: u64, generation: u64) {
    let started = Instant::now();

    if let Some(index) = load_cache(fingerprint) {
        log(format!(
            "folder index loaded from cache ({} folders)",
            index.len()
        ));
        publish(index, generation);
        return;
    }

    let Some(profile) = std::env::var_os("USERPROFILE").map(PathBuf::from) else {
        STATE.store(STATE_IDLE, Ordering::Relaxed);
        return;
    };

    let roots = scope.roots(&profile, &drives::infos());
    let index = build(&roots, &scope, generation);

    if GENERATION.load(Ordering::Relaxed) != generation {
        log("folder index build cancelled");
        return;
    }

    log(format!(
        "folder index built: {} folders in {:?}",
        index.len(),
        started.elapsed()
    ));

    save_cache(&index, fingerprint);
    publish(index, generation);
}

fn publish(index: FolderIndex, generation: u64) {
    if GENERATION.load(Ordering::Relaxed) != generation {
        return;
    }

    let folders = index.len() as u64;

    if let Ok(mut slot) = index_slot().lock() {
        *slot = Some(Arc::new(index));
    }

    SCANNED.store(folders, Ordering::Relaxed);
    STATE.store(STATE_READY, Ordering::Relaxed);
}

/// Walks the roots and indexes every folder that is not excluded.
fn build(roots: &[PathBuf], scope: &SearchScope, generation: u64) -> FolderIndex {
    let mut index = FolderIndex::default();

    for root in roots {
        let root_name = root.to_string_lossy().into_owned();
        let root_index = index.push(NO_PARENT, &root_name);
        let mut stack = vec![(root.clone(), root_index, 0_usize)];

        while let Some((directory, parent, depth)) = stack.pop() {
            if GENERATION.load(Ordering::Relaxed) != generation {
                return index;
            }

            if index.len() >= MAX_ENTRIES {
                log(format!(
                    "folder index hit the {MAX_ENTRIES}-entry cap; the remaining folders are not indexed"
                ));
                return index;
            }

            if depth >= scope.depth {
                continue;
            }

            let Ok(entries) = std::fs::read_dir(&directory) else {
                continue;
            };

            for entry in entries.flatten() {
                if index.len() >= MAX_ENTRIES {
                    log(format!(
                        "folder index hit the {MAX_ENTRIES}-entry cap; the remaining folders are not indexed"
                    ));
                    return index;
                }

                let Ok(file_type) = entry.file_type() else {
                    continue;
                };

                if !file_type.is_dir() {
                    continue;
                }

                let name = entry.file_name().to_string_lossy().into_owned();
                let path = entry.path();

                if scope.is_hidden_skipped(&name)
                    || folders::is_system_directory(&name)
                    || scope.is_excluded(&path)
                {
                    continue;
                }

                // `depth` is the parent's level, so its children sit one
                // level deeper and only exist while that still fits.
                if depth >= scope.depth {
                    continue;
                }

                let child = index.push(parent, &name);
                stack.push((path, child, depth + 1));

                if index.len() % PROGRESS_INTERVAL == 0 {
                    SCANNED.store(index.len() as u64, Ordering::Relaxed);
                }
            }
        }
    }

    index
}

fn cache_path() -> Option<PathBuf> {
    data_directory().map(|directory| directory.join(FILE_NAME))
}

fn load_cache(fingerprint: u64) -> Option<FolderIndex> {
    let path = cache_path()?;
    let bytes = std::fs::read(path).ok()?;

    FolderIndex::decode(&bytes, fingerprint)
}

fn save_cache(index: &FolderIndex, fingerprint: u64) {
    let Some(path) = cache_path() else {
        return;
    };

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    let temporary = path.with_extension("tmp");

    if let Err(error) = std::fs::write(&temporary, index.encode(fingerprint)) {
        log(format!("could not write the folder index cache: {error}"));
        return;
    }

    if let Err(error) = std::fs::rename(&temporary, &path) {
        log(format!("could not replace the folder index cache: {error}"));
        let _ = std::fs::remove_file(&temporary);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;

    fn scratch(name: &str) -> PathBuf {
        let path = std::env::temp_dir().join(format!("pirsa-index-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&path);
        fs::create_dir_all(&path).expect("scratch dir");
        path
    }

    #[test]
    fn an_empty_index_matches_nothing() {
        let index = FolderIndex::default();

        assert_eq!(index.search("report", 8), [] as [FolderEntry; 0]);
    }

    #[test]
    fn index_round_trips_through_the_cache_format() {
        let mut index = FolderIndex::default();
        let root = index.push(NO_PARENT, "C:\\Users\\me");
        let projects = index.push(root, "Projects");
        index.push(projects, "pirsa");

        let decoded = FolderIndex::decode(&index.encode(7), 7).expect("decodes");

        assert_eq!(decoded.len(), 3);
        assert_eq!(decoded.search("pirsa", 8).len(), 1);
    }

    #[test]
    fn a_mismatched_fingerprint_is_rejected() {
        let mut index = FolderIndex::default();
        index.push(NO_PARENT, "C:\\");

        assert!(FolderIndex::decode(&index.encode(7), 8).is_none());
    }

    #[test]
    fn truncated_and_damaged_caches_are_rejected() {
        let mut index = FolderIndex::default();
        index.push(NO_PARENT, "C:\\");
        let bytes = index.encode(1);

        assert!(FolderIndex::decode(&bytes[..bytes.len() / 2], 1).is_none());
        assert!(FolderIndex::decode(b"not an index at all", 1).is_none());
    }

    #[test]
    fn paths_are_reconstructed_from_parents() {
        let mut index = FolderIndex::default();
        let root = index.push(NO_PARENT, "C:\\Users\\me");
        let documents = index.push(root, "Documents");
        let projects = index.push(documents, "Projects");

        assert_eq!(
            index.path(projects as usize),
            PathBuf::from("C:\\Users\\me\\Documents\\Projects")
        );
    }

    #[test]
    fn search_ranks_exact_then_prefix_then_substring() {
        let mut index = FolderIndex::default();
        let root = index.push(NO_PARENT, "C:\\");
        let _projects = index.push(root, "Projects");
        let _old = index.push(root, "old-projects");
        let _archive = index.push(root, "projects-archive");

        let results = index.search("projects", 8);
        let names: Vec<&str> = results.iter().map(FolderEntry::name).collect();

        assert_eq!(names, vec!["Projects", "projects-archive", "old-projects"]);
    }

    #[test]
    fn search_is_case_insensitive() {
        let mut index = FolderIndex::default();
        let root = index.push(NO_PARENT, "C:\\");
        index.push(root, "Documents");

        assert_eq!(index.search("DOCUMENTS", 8).len(), 1);
        assert_eq!(index.search("docu", 8).len(), 1);
    }

    #[test]
    fn build_walks_only_folders_and_respects_excludes() {
        let root = scratch("build");

        fs::create_dir_all(root.join("Documents").join("Projects")).expect("tree");
        fs::create_dir_all(root.join("node_modules").join("left-pad")).expect("tree");
        fs::write(root.join("notes.txt"), b"x").expect("file");

        let scope = SearchScope {
            include: vec![root.clone()],
            ..SearchScope::default()
        };

        let index = build(std::slice::from_ref(&root), &scope, 0);
        let results = index.search("project", 8);

        assert_eq!(results.len(), 1);
        assert_eq!(results[0].name(), "Projects");
        assert!(index.search("notes", 8).is_empty(), "files are not indexed");
        assert!(index.search("left", 8).is_empty(), "excluded is skipped");

        let _ = fs::remove_dir_all(root);
    }

    #[test]
    fn build_respects_the_depth_limit() {
        let root = scratch("depth");

        fs::create_dir_all(root.join("one").join("two").join("three")).expect("tree");

        let mut scope = SearchScope {
            include: vec![root.clone()],
            ..SearchScope::default()
        };
        scope.set_depth(1);

        let index = build(std::slice::from_ref(&root), &scope, 0);

        assert_eq!(index.search("one", 8).len(), 1);
        assert_eq!(index.search("two", 8), [] as [FolderEntry; 0]);
        assert_eq!(index.search("three", 8), [] as [FolderEntry; 0]);

        let _ = fs::remove_dir_all(root);
    }
}
