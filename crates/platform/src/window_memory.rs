//! Remembered launcher placement: one offset per monitor, plus the monitor that
//! was used last.
//!
//! Positions are stored as an offset from the monitor's work area, so a
//! remembered spot stays valid when the taskbar, resolution, or DPI changes,
//! and each monitor keeps its own slot.

use std::collections::HashMap;
use std::path::PathBuf;

use pirsa_core::util::{data_directory, log};

const FILE_NAME: &str = "window.txt";

/// A monitor's usable area in physical pixels.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorkArea {
    pub name: String,
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl WorkArea {
    pub fn width(&self) -> i32 {
        self.right - self.left
    }

    pub fn height(&self) -> i32 {
        self.bottom - self.top
    }
}

/// Saved offsets keyed by monitor device name, with the most recent monitor.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub(crate) struct Memory {
    last: Option<String>,
    offsets: HashMap<String, (i32, i32)>,
}

impl Memory {
    pub fn last(&self) -> Option<&str> {
        self.last.as_deref()
    }

    pub fn offset(&self, monitor: &str) -> Option<(i32, i32)> {
        self.offsets.get(monitor).copied()
    }

    /// Records the offset for `monitor` and makes it the most recent one.
    pub fn remember(&mut self, monitor: &str, offset: (i32, i32)) {
        self.offsets.insert(monitor.to_owned(), offset);
        self.last = Some(monitor.to_owned());
    }

    /// Parses the saved file. Unknown and malformed lines are ignored, so a
    /// damaged file simply behaves like an empty memory.
    pub fn parse(text: &str) -> Self {
        let mut memory = Self::default();

        for line in text.lines() {
            let line = line.trim();

            if line.is_empty() || line.starts_with('#') {
                continue;
            }

            let Some((key, value)) = line.split_once('=') else {
                continue;
            };

            let key = key.trim();

            if key == "last" {
                let value = value.trim();

                if !value.is_empty() {
                    memory.last = Some(value.to_owned());
                }

                continue;
            }

            let Some((x, y)) = value.trim().split_once(',') else {
                continue;
            };

            let (Ok(x), Ok(y)) = (x.trim().parse::<i32>(), y.trim().parse::<i32>()) else {
                continue;
            };

            memory.offsets.insert(key.to_owned(), (x, y));
        }

        memory
    }

    /// Serializes to a stable, line-based format.
    pub fn render(&self) -> String {
        let mut body = String::new();

        if let Some(last) = &self.last {
            body.push_str(&format!("last={last}\n"));
        }

        let mut names: Vec<&String> = self.offsets.keys().collect();
        names.sort();

        for name in names {
            if let Some((x, y)) = self.offsets.get(name) {
                body.push_str(&format!("{name}={x},{y}\n"));
            }
        }

        body
    }
}

/// Where to place a window of `size` on the remembered monitor.
///
/// Returns `None` when nothing is remembered or the remembered monitor is no
/// longer attached, letting the caller fall back to centering.
pub(crate) fn resolve(memory: &Memory, areas: &[WorkArea], size: (i32, i32)) -> Option<(i32, i32)> {
    let name = memory.last()?;
    let area = areas
        .iter()
        .find(|area| area.name.eq_ignore_ascii_case(name))?;
    let (offset_x, offset_y) = memory.offset(&area.name)?;

    Some((
        clamp(area.left + offset_x, area.left, area.right - size.0),
        clamp(area.top + offset_y, area.top, area.bottom - size.1),
    ))
}

/// Centers a window of `size` inside `area`.
pub(crate) fn centered(area: &WorkArea, size: (i32, i32)) -> (i32, i32) {
    (
        area.left + (area.width() - size.0) / 2,
        area.top + (area.height() - size.1) / 2,
    )
}

fn clamp(value: i32, min: i32, max: i32) -> i32 {
    // A window larger than the work area still starts at the top-left corner
    // instead of panicking on an inverted range.
    value.clamp(min, max.max(min))
}

pub(crate) fn load() -> Memory {
    let Some(path) = memory_path() else {
        return Memory::default();
    };

    match std::fs::read_to_string(path) {
        Ok(text) => Memory::parse(&text),
        Err(_) => Memory::default(),
    }
}

pub(crate) fn save(memory: &Memory) {
    let Some(path) = memory_path() else {
        return;
    };

    if let Some(parent) = path.parent() {
        let _ = std::fs::create_dir_all(parent);
    }

    if let Err(error) = std::fs::write(&path, memory.render()) {
        log(format!("could not save the window position: {error}"));
    }
}

fn memory_path() -> Option<PathBuf> {
    data_directory().map(|directory| directory.join(FILE_NAME))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn area(name: &str, left: i32, top: i32, right: i32, bottom: i32) -> WorkArea {
        WorkArea {
            name: name.to_owned(),
            left,
            top,
            right,
            bottom,
        }
    }

    fn monitors() -> Vec<WorkArea> {
        vec![
            area("\\\\.\\DISPLAY1", 0, 0, 1920, 1040),
            area("\\\\.\\DISPLAY2", 1920, 0, 3840, 1040),
        ]
    }

    #[test]
    fn memory_round_trips() {
        let mut memory = Memory::default();
        memory.remember("\\\\.\\DISPLAY1", (120, 80));
        memory.remember("\\\\.\\DISPLAY2", (-30, 15));

        let parsed = Memory::parse(&memory.render());

        assert_eq!(parsed, memory);
        assert_eq!(parsed.last(), Some("\\\\.\\DISPLAY2"));
        assert_eq!(parsed.offset("\\\\.\\DISPLAY1"), Some((120, 80)));
    }

    #[test]
    fn damaged_lines_are_ignored() {
        let memory = Memory::parse("garbage\nlast=\n\\\\.\\DISPLAY1=oops\n\\\\.\\DISPLAY2=-4,9\n");

        assert_eq!(memory.offset("\\\\.\\DISPLAY2"), Some((-4, 9)));
        assert!(memory.last().is_none());
    }

    #[test]
    fn position_restores_on_the_remembered_monitor() {
        let mut memory = Memory::default();
        memory.remember("\\\\.\\DISPLAY2", (100, 50));

        assert_eq!(resolve(&memory, &monitors(), (820, 540)), Some((2020, 50)));
    }

    #[test]
    fn offsets_are_clamped_into_the_work_area() {
        let mut memory = Memory::default();
        memory.remember("\\\\.\\DISPLAY1", (-500, 900));

        assert_eq!(resolve(&memory, &monitors(), (820, 540)), Some((0, 500)));
    }

    #[test]
    fn a_missing_monitor_falls_back_to_centering() {
        let mut memory = Memory::default();
        memory.remember("\\\\.\\DISPLAY9", (10, 10));

        assert_eq!(resolve(&memory, &monitors(), (820, 540)), None);
        assert_eq!(resolve(&Memory::default(), &monitors(), (820, 540)), None);
    }

    #[test]
    fn centering_uses_the_work_area_origin() {
        let second = &monitors()[1];

        assert_eq!(centered(second, (820, 540)), (2470, 250));
    }
}
