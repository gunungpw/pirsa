//! Fonts stored in the user's local font directory.
//!
//! `~/.local/share/fonts` is the XDG user font directory. On native Windows
//! `~` resolves from `USERPROFILE` (with `HOME` as a fallback for environments
//! that set it), so the directory is usually
//! `%USERPROFILE%\.local\share\fonts`.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};
use std::sync::{Arc, OnceLock};

use pirsa_core::settings::FontChoice;
use pirsa_core::util::log;

const MAX_DEPTH: u8 = 3;

/// Font files and the families they contain, loaded once at startup.
#[derive(Debug, Default)]
pub struct LocalFonts {
    /// Raw font files, passed to iced so it can render the families.
    pub files: Vec<Vec<u8>>,
    /// One choice per family found in `files`.
    pub choices: Vec<FontChoice>,
}

/// Returns the fonts found in `~/.local/share/fonts`, scanning on first use.
pub fn local_fonts() -> &'static LocalFonts {
    static FONTS: OnceLock<LocalFonts> = OnceLock::new();

    FONTS.get_or_init(load)
}

/// The directory scanned for local fonts.
#[must_use]
pub fn directory() -> Option<PathBuf> {
    let home = std::env::var_os("USERPROFILE")
        .filter(|value| !value.is_empty())
        .or_else(|| std::env::var_os("HOME").filter(|value| !value.is_empty()))?;

    Some(
        PathBuf::from(home)
            .join(".local")
            .join("share")
            .join("fonts"),
    )
}

fn load() -> LocalFonts {
    directory().map_or_else(LocalFonts::default, |directory| load_from(&directory))
}

fn load_from(directory: &Path) -> LocalFonts {
    if !directory.is_dir() {
        return LocalFonts::default();
    }

    let mut paths = Vec::new();
    collect_font_files(directory, 0, &mut paths);
    paths.sort();

    let mut files = Vec::with_capacity(paths.len());
    let mut families = BTreeSet::new();

    for path in paths {
        let Ok(bytes) = std::fs::read(&path) else {
            log(format!("could not read local font {}", path.display()));
            continue;
        };

        let font_families = families_in(&bytes);

        if font_families.is_empty() {
            log(format!(
                "skipping unsupported local font {}",
                path.display()
            ));
            continue;
        }

        families.extend(font_families);
        files.push(bytes);
    }

    let choices: Vec<FontChoice> = families
        .into_iter()
        .map(|family| FontChoice::custom(&family))
        .collect();

    log(format!(
        "loaded {} local font file(s) with {} families from {}",
        files.len(),
        choices.len(),
        directory.display()
    ));

    LocalFonts { files, choices }
}

fn collect_font_files(directory: &Path, depth: u8, paths: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(directory) else {
        return;
    };

    for entry in entries.flatten() {
        let Ok(file_type) = entry.file_type() else {
            continue;
        };
        let path = entry.path();

        if file_type.is_dir() {
            if depth < MAX_DEPTH {
                collect_font_files(&path, depth + 1, paths);
            }
        } else if file_type.is_file() && is_font_file(&path) {
            paths.push(path);
        }
    }
}

fn is_font_file(path: &Path) -> bool {
    path.extension()
        .and_then(|extension| extension.to_str())
        .is_some_and(|extension| {
            ["ttf", "otf", "ttc", "otc"]
                .iter()
                .any(|known| extension.eq_ignore_ascii_case(known))
        })
}

fn families_in(bytes: &[u8]) -> Vec<String> {
    let mut database = fontdb::Database::new();
    let ids = database.load_font_source(fontdb::Source::Binary(Arc::new(bytes.to_vec())));

    ids.iter()
        .filter_map(|id| database.face(*id))
        .filter_map(|face| face.families.first())
        .map(|(family, _)| family.clone())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn font_files_are_recognized_by_extension() {
        assert!(is_font_file(Path::new("regular.ttf")));
        assert!(is_font_file(Path::new("Regular.OTF")));
        assert!(is_font_file(Path::new("collection.ttc")));
        assert!(!is_font_file(Path::new("readme.txt")));
        assert!(!is_font_file(Path::new("font")));
    }

    #[test]
    fn invalid_font_data_has_no_families() {
        assert!(families_in(b"not a font").is_empty());
    }

    #[test]
    fn a_real_font_yields_its_family() {
        let Some(windows) = std::env::var_os("WINDIR") else {
            return;
        };
        let path = PathBuf::from(windows).join("Fonts").join("segoeui.ttf");
        let Ok(bytes) = std::fs::read(path) else {
            return;
        };

        assert!(
            families_in(&bytes)
                .iter()
                .any(|family| family == "Segoe UI")
        );
    }

    #[test]
    fn nested_font_files_are_discovered() {
        let Some(windows) = std::env::var_os("WINDIR") else {
            return;
        };
        let source = PathBuf::from(windows).join("Fonts").join("segoeui.ttf");
        if !source.is_file() {
            return;
        }

        let directory =
            std::env::temp_dir().join(format!("pirsa-local-fonts-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&directory);
        std::fs::create_dir_all(directory.join("nested")).unwrap();
        std::fs::copy(source, directory.join("nested").join("segoeui.ttf")).unwrap();

        let fonts = load_from(&directory);

        assert_eq!(fonts.files.len(), 1);
        assert!(
            fonts
                .choices
                .iter()
                .any(|choice| choice.name() == "Segoe UI")
        );

        let _ = std::fs::remove_dir_all(directory);
    }
}
