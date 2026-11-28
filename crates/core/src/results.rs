//! The launcher's result list.
//!
//! App queries rank the Start Menu index; file-mode queries build their rows
//! from the Turso file index, in ranked order. Each row indexes the matching
//! list.

/// One row of the launcher's result list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ResultRow {
    /// Index into the indexed app list.
    App(usize),
    /// Index into the current file-index matches.
    File(usize),
}
