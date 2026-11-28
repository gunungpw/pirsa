//! Platform-independent logic for Pirsa.
//!
//! This crate contains the calculator, search ranking, filesystem search,
//! usage history, shared models, and small utilities. It intentionally has no
//! Windows or iced dependencies so changes here compile quickly.

pub mod calc;
pub mod folders;
pub mod model;
pub mod results;
pub mod search;
pub mod search_scope;
pub mod settings;
pub mod usage;
pub mod util;

pub use calc::{CalcKind, CalcResult};
pub use model::{AppEntry, FileEntry, HotkeyStatus, Icon, LaunchTarget, ScanResult};
pub use results::ResultRow;
pub use search_scope::{DriveInfo, DriveKind, DriveMode, SearchScope};
pub use settings::{FontChoice, HotkeyChoice, Settings, ThemeChoice};
pub use usage::Usage;
