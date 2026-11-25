//! Platform-independent logic for Pirsa.
//!
//! This crate contains the calculator, search ranking, usage history, shared
//! models, and small utilities. It intentionally has no Windows or iced
//! dependencies so changes here compile quickly.

pub mod calc;
pub mod model;
pub mod search;
pub mod settings;
pub mod usage;
pub mod util;

pub use calc::{CalcKind, CalcResult};
pub use model::{AppEntry, HotkeyStatus, Icon, LaunchTarget, ScanResult};
pub use settings::{FontChoice, Settings, ThemeChoice};
pub use usage::Usage;
