//! Windows folder roles (LLP 1173 D4). Keep the IDs portable for table tests.

/// The shell's KNOWNFOLDERID for a picker or a storage namespace.
pub fn windows_id(name: &str) -> Option<u128> {
    Some(match name {
        "desktop" => 0xb4bfcc3a_db2c_424c_b029_7fe99a87c641,
        "documents" => 0xfdd39ad0_238f_46af_adb4_6c85480369c7,
        "downloads" => 0x374de290_123f_4565_9164_39c4925e467b,
        "music" => 0x4bd8d571_6d19_48d3_be97_422220080e43,
        "pictures" => 0x33e28130_4e1e_4676_835a_98395c3bc3bb,
        "videos" => 0x18989b1d_99b5_455b_841c_ab7c74e4ddfc,
        "screenshots" => 0xb7bede81_df94_4682_a7d8_57a52620b86f,
        _ => return None,
    })
}

#[cfg(any(windows, test))]
fn flags(name: &str) -> u32 {
    // D4 requests the current folder; only Screenshots may be created.
    if name == "screenshots" {
        0x8000
    } else {
        0
    }
}

/// Resolve on every operation, following the person's folder redirection.
#[cfg(windows)]
pub fn windows_storage(name: &str) -> std::io::Result<std::path::PathBuf> {
    use std::{ffi::OsString, os::windows::ffi::OsStringExt, path::PathBuf};
    use windows_sys::Win32::{System::Com::CoTaskMemFree, UI::Shell::SHGetKnownFolderPath};
    let id = windows_sys::core::GUID::from_u128(
        windows_id(name).ok_or_else(|| std::io::Error::from_raw_os_error(2))?,
    );
    if name == "screenshots" {
        windows_storage("pictures")?;
    }
    let mut path = std::ptr::null_mut();
    // SAFETY: the ID and output pointer live through the call; the shell's
    // allocation must be freed even when the call fails.
    let result = unsafe { SHGetKnownFolderPath(&id, flags(name), std::ptr::null_mut(), &mut path) };
    let folder = if path.is_null() {
        None
    } else {
        // SAFETY: the shell returned a terminated UTF-16 allocation.
        let folder = unsafe {
            let mut length = 0;
            while *path.add(length) != 0 {
                length += 1;
            }
            PathBuf::from(OsString::from_wide(std::slice::from_raw_parts(
                path, length,
            )))
        };
        unsafe { CoTaskMemFree(path.cast()) };
        Some(folder)
    };
    if result < 0 {
        return Err(if result as u32 & 0xffff0000 == 0x80070000 {
            std::io::Error::from_raw_os_error(result & 0xffff)
        } else {
            std::io::Error::other(format!("SHGetKnownFolderPath: HRESULT {result:#x}"))
        });
    }
    let folder = folder.ok_or_else(|| std::io::Error::from_raw_os_error(2))?;
    super::existing(folder)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roles_match_the_shell_ids_and_only_screenshots_may_be_created() {
        // https://learn.microsoft.com/windows/win32/shell/knownfolderid
        for (name, id) in [
            ("desktop", 0xb4bfcc3a_db2c_424c_b029_7fe99a87c641),
            ("documents", 0xfdd39ad0_238f_46af_adb4_6c85480369c7),
            ("downloads", 0x374de290_123f_4565_9164_39c4925e467b),
            ("music", 0x4bd8d571_6d19_48d3_be97_422220080e43),
            ("pictures", 0x33e28130_4e1e_4676_835a_98395c3bc3bb),
            ("videos", 0x18989b1d_99b5_455b_841c_ab7c74e4ddfc),
            ("screenshots", 0xb7bede81_df94_4682_a7d8_57a52620b86f),
        ] {
            assert_eq!(windows_id(name), Some(id), "{name}");
            assert_eq!(flags(name), if name == "screenshots" { 0x8000 } else { 0 });
        }
        assert_eq!(windows_id("Pictures"), None);
        assert_eq!(windows_id("home"), None);
    }
}
