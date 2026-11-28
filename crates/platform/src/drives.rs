//! Drive enumeration for the folder search scope.

use windows::Win32::Storage::FileSystem::{
    GetDiskFreeSpaceExW, GetDriveTypeW, GetLogicalDrives, GetVolumeInformationW,
};
use windows::core::PCWSTR;

use pirsa_core::search_scope::{DriveDetails, DriveInfo, DriveKind};

// The Win32 drive-type values returned by GetDriveTypeW.
const DRIVE_REMOVABLE: u32 = 2;
const DRIVE_FIXED: u32 = 3;
const DRIVE_REMOTE: u32 = 4;

/// Lists attached drives, sorted by letter. CD-ROM and unknown device types are
/// skipped because they cannot be searched.
#[must_use]
pub fn list() -> Vec<DriveDetails> {
    let mask = unsafe { GetLogicalDrives() };
    let mut drives = Vec::new();

    for bit in 0..26_u32 {
        if mask & (1 << bit) == 0 {
            continue;
        }

        let letter = char::from(b'A' + bit as u8);
        let root = format!("{letter}:\\");
        let wide_root = wide(&root);

        let kind = match unsafe { GetDriveTypeW(PCWSTR(wide_root.as_ptr())) } {
            DRIVE_FIXED => DriveKind::Fixed,
            DRIVE_REMOVABLE => DriveKind::Removable,
            DRIVE_REMOTE => DriveKind::Remote,
            _ => continue,
        };

        let (free_bytes, total_bytes) = free_space(&wide_root);

        drives.push(DriveDetails {
            info: DriveInfo { letter, kind },
            label: volume_label(&wide_root),
            free_bytes,
            total_bytes,
        });
    }

    drives.sort_by_key(|drive| drive.info.letter);
    drives
}

/// The minimal drive list the scope needs for root expansion.
#[must_use]
pub fn infos() -> Vec<DriveInfo> {
    list().into_iter().map(|drive| drive.info).collect()
}

fn free_space(root: &[u16]) -> (u64, u64) {
    let mut free = 0_u64;
    let mut total = 0_u64;

    let result = unsafe {
        GetDiskFreeSpaceExW(
            PCWSTR(root.as_ptr()),
            Some(&raw mut free),
            Some(&raw mut total),
            None,
        )
    };

    if result.is_ok() {
        (free, total)
    } else {
        (0, 0)
    }
}

fn volume_label(root: &[u16]) -> String {
    let mut label = [0_u16; 64];
    let mut filesystem = [0_u16; 16];

    let result = unsafe {
        GetVolumeInformationW(
            PCWSTR(root.as_ptr()),
            Some(&mut label),
            None,
            None,
            None,
            Some(&mut filesystem),
        )
    };

    if result.is_err() {
        return String::new();
    }

    let length = label
        .iter()
        .position(|character| *character == 0)
        .unwrap_or(label.len());

    String::from_utf16_lossy(&label[..length]).trim().to_owned()
}

fn wide(value: &str) -> Vec<u16> {
    value.encode_utf16().chain(std::iter::once(0)).collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn listing_does_not_panic_without_drives() {
        // The real list on CI depends on the machine; the call must simply work.
        let drives = list();

        assert!(
            drives
                .iter()
                .all(|drive| drive.info.letter.is_ascii_uppercase())
        );
    }

    #[test]
    fn infos_mirror_the_details() {
        let details = list();
        let infos = infos();

        assert_eq!(details.len(), infos.len());
    }
}
