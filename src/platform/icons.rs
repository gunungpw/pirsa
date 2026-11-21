//! Application icon extraction.
//!
//! Icons are requested from the shell through `IShellItemImageFactory`, which
//! resolves both Start Menu shortcuts and packaged AppUserModelIDs to their
//! real icon. Extraction is best-effort: an app whose icon cannot be read
//! simply keeps the generated initial tile.

use std::ffi::c_void;
use std::mem::size_of;

use windows::Win32::Foundation::SIZE;
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAP, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleDC, DIB_RGB_COLORS, DeleteDC,
    DeleteObject, GetDIBits, GetObjectW, HBITMAP, HDC, HGDIOBJ, SelectObject,
};
use windows::Win32::UI::Shell::{
    IShellItemImageFactory, SHCreateItemFromParsingName, SIIGBF_ICONONLY,
};
use windows::core::PCWSTR;

use super::com::ComApartment;
use crate::platform::apps::{AppEntry, LaunchTarget};
use crate::util::log;

/// Size requested from the shell. Larger than the 30 px row tile so the icon
/// stays crisp on scaled displays.
const ICON_SIZE: i32 = 64;
/// Sanity bound for a bitmap the shell reports back.
const MAX_ICON_SIDE: i32 = 512;

/// A decoded RGBA icon bitmap.
#[derive(Debug, Clone)]
pub struct Icon {
    /// Pixel width.
    pub width: u32,
    /// Pixel height.
    pub height: u32,
    /// Straight (non-premultiplied) RGBA pixels, rows from the top.
    pub rgba: Vec<u8>,
}

/// Extracts an icon for every app the shell can provide one for.
///
/// Normally called by the icon worker; the COM guard also supports direct
/// callers such as tests.
pub(crate) fn extract(apps: &[AppEntry]) -> Vec<(String, Icon)> {
    let _com = ComApartment::new();

    let icons: Vec<(String, Icon)> = apps
        .iter()
        .filter_map(|app| icon_for(app.target()).map(|icon| (app.name().to_owned(), icon)))
        .collect();

    log(format!(
        "loaded {} app icons for {} apps",
        icons.len(),
        apps.len()
    ));

    icons
}

fn icon_for(target: &LaunchTarget) -> Option<Icon> {
    let path = match target {
        LaunchTarget::Shortcut(path) | LaunchTarget::Executable(path) => {
            path.to_string_lossy().into_owned()
        }
        LaunchTarget::Shell(target) => target.clone(),
    };
    let path = wide(&path);

    let factory: IShellItemImageFactory =
        unsafe { SHCreateItemFromParsingName(PCWSTR(path.as_ptr()), None) }.ok()?;

    let bitmap = unsafe {
        factory.GetImage(
            SIZE {
                cx: ICON_SIZE,
                cy: ICON_SIZE,
            },
            SIIGBF_ICONONLY,
        )
    }
    .ok()?;

    let icon = unsafe { bitmap_to_icon(bitmap) };
    unsafe {
        let _ = DeleteObject(HGDIOBJ(bitmap.0));
    };

    icon
}

/// Reads a shell icon bitmap into straight RGBA pixels.
///
/// # Safety
///
/// `bitmap` must be a valid bitmap handle owned by the caller.
unsafe fn bitmap_to_icon(bitmap: HBITMAP) -> Option<Icon> {
    let hdc = unsafe { CreateCompatibleDC(None) };

    if hdc.is_invalid() {
        return None;
    }

    let previous = unsafe { SelectObject(hdc, HGDIOBJ(bitmap.0)) };
    let icon = unsafe { read_bitmap(hdc, bitmap) };

    unsafe {
        SelectObject(hdc, previous);
        let _ = DeleteDC(hdc);
    }

    icon
}

unsafe fn read_bitmap(hdc: HDC, bitmap: HBITMAP) -> Option<Icon> {
    let mut description = BITMAP::default();
    let copied = unsafe {
        GetObjectW(
            HGDIOBJ(bitmap.0),
            size_of::<BITMAP>() as i32,
            Some(&mut description as *mut BITMAP as *mut c_void),
        )
    };

    if copied == 0 {
        return None;
    }

    let width = description.bmWidth;
    let height = description.bmHeight.unsigned_abs() as i32;

    if width <= 0 || height <= 0 || width > MAX_ICON_SIDE || height > MAX_ICON_SIDE {
        return None;
    }

    let mut info = BITMAPINFO::default();
    info.bmiHeader.biSize = size_of::<BITMAPINFOHEADER>() as u32;
    info.bmiHeader.biWidth = width;
    // Negative height asks for top-down rows, matching iced's RGBA layout.
    info.bmiHeader.biHeight = -height;
    info.bmiHeader.biPlanes = 1;
    info.bmiHeader.biBitCount = 32;
    info.bmiHeader.biCompression = BI_RGB.0;

    let mut pixels = vec![0_u8; width as usize * height as usize * 4];

    let lines = unsafe {
        GetDIBits(
            hdc,
            bitmap,
            0,
            height as u32,
            Some(pixels.as_mut_ptr().cast()),
            &mut info,
            DIB_RGB_COLORS,
        )
    };

    if lines == 0 {
        return None;
    }

    // A 32-bit DIB stores pixels as B, G, R, A; iced expects R, G, B, A.
    // This only reorders the channels, it never adjusts their values.
    bgra_to_rgba(&mut pixels);

    ensure_alpha(&mut pixels);

    Some(Icon {
        width: width as u32,
        height: height as u32,
        rgba: pixels,
    })
}

/// Reorders 32-bit DIB pixels from BGRA to RGBA, preserving every value.
fn bgra_to_rgba(pixels: &mut [u8]) {
    for pixel in pixels.as_chunks_mut::<4>().0 {
        pixel.swap(0, 2);
    }
}

/// Makes sure the bitmap has a usable alpha channel.
///
/// Some shell icons come back without alpha at all. In that case the mask is
/// reconstructed from the luminance, so an all-transparent icon does not turn
/// into an invisible (or solid black) square.
fn ensure_alpha(pixels: &mut [u8]) {
    let (chunks, _) = pixels.as_chunks_mut::<4>();

    if chunks.iter().any(|pixel| pixel[3] != 0) {
        return;
    }

    for pixel in chunks {
        let opaque = pixel[0] | pixel[1] | pixel[2] != 0;
        pixel[3] = if opaque { 255 } else { 0 };
    }
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn system_executable(name: &str) -> Option<LaunchTarget> {
        let root = std::env::var_os("SystemRoot")?;
        let path = std::path::PathBuf::from(root).join("System32").join(name);

        path.is_file().then_some(LaunchTarget::Executable(path))
    }

    #[test]
    fn extracts_a_system_icon() {
        let Some(target) = system_executable("notepad.exe") else {
            return;
        };

        let apps = vec![AppEntry::new(
            "Notepad".to_owned(),
            "Test".to_owned(),
            target,
        )];
        let icons = extract(&apps);

        assert_eq!(icons.len(), 1, "notepad.exe should have an icon");

        let (name, icon) = &icons[0];
        assert_eq!(name, "Notepad");
        assert!(icon.width > 0 && icon.height > 0);
        assert_eq!(
            icon.rgba.len(),
            icon.width as usize * icon.height as usize * 4
        );
        assert!(
            icon.rgba
                .as_chunks::<4>()
                .0
                .iter()
                .any(|pixel| pixel[3] != 0)
        );
    }

    #[test]
    fn channels_are_reordered_without_changing_values() {
        let mut pixels = vec![10, 20, 30, 40, 50, 60, 70, 80];

        bgra_to_rgba(&mut pixels);

        assert_eq!(pixels, vec![30, 20, 10, 40, 70, 60, 50, 80]);
    }

    #[test]
    fn missing_alpha_is_reconstructed() {
        let mut pixels = vec![0_u8, 0, 0, 0, 255, 128, 0, 0];

        ensure_alpha(&mut pixels);

        assert_eq!(pixels[3], 0);
        assert_eq!(pixels[7], 255);
    }

    #[test]
    fn existing_alpha_is_kept() {
        let mut pixels = vec![10_u8, 20, 30, 40, 50, 60, 70, 80];

        ensure_alpha(&mut pixels);

        assert_eq!(pixels[3], 40);
        assert_eq!(pixels[7], 80);
    }
}
