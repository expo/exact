//! macOS folder roles and Screenshot's current destination (LLP 1173 D4).
use std::{ffi::c_void, path::PathBuf};
type Ref = *const c_void;
#[link(name = "CoreFoundation", kind = "framework")]
unsafe extern "C" {
    fn CFStringCreateWithBytes(
        allocator: Ref,
        bytes: *const u8,
        length: isize,
        encoding: u32,
        external: u8,
    ) -> Ref;
    fn CFPreferencesAppSynchronize(app: Ref) -> u8;
    fn CFPreferencesCopyAppValue(key: Ref, app: Ref) -> Ref;
    fn CFGetTypeID(value: Ref) -> usize;
    fn CFStringGetTypeID() -> usize;
    fn CFStringGetLength(value: Ref) -> isize;
    fn CFStringGetMaximumSizeForEncoding(length: isize, encoding: u32) -> isize;
    fn CFStringGetCString(value: Ref, buffer: *mut u8, size: isize, encoding: u32) -> u8;
    fn CFRelease(value: Ref);
}
const UTF8: u32 = 0x08000100;
struct Owned(Ref);
impl Drop for Owned {
    fn drop(&mut self) {
        unsafe { CFRelease(self.0) };
    }
}
fn string(text: &str) -> Option<Owned> {
    let value = unsafe {
        CFStringCreateWithBytes(
            std::ptr::null(),
            text.as_ptr(),
            text.len() as isize,
            UTF8,
            0,
        )
    };
    (!value.is_null()).then(|| Owned(value))
}
fn screenshot() -> Option<String> {
    let domain = string("com.apple.screencapture")?;
    // Reload the domain so a change in Screenshot takes effect without relaunching.
    unsafe { CFPreferencesAppSynchronize(domain.0) };
    for key in ["location-screenshot", "location"] {
        let key = string(key)?;
        let value = unsafe { CFPreferencesCopyAppValue(key.0, domain.0) };
        if value.is_null() {
            continue;
        }
        let value = Owned(value);
        unsafe {
            if CFGetTypeID(value.0) != CFStringGetTypeID() {
                continue;
            }
            let size = CFStringGetMaximumSizeForEncoding(CFStringGetLength(value.0), UTF8) + 1;
            let mut buffer = vec![0; usize::try_from(size).ok()?];
            if CFStringGetCString(value.0, buffer.as_mut_ptr(), size, UTF8) == 0 {
                return None;
            }
            buffer.truncate(buffer.iter().position(|b| *b == 0)?);
            return String::from_utf8(buffer).ok();
        }
    }
    None
}

/// The account's folders; missing folders and non-folder screenshot destinations refuse.
pub fn macos(name: &str) -> std::io::Result<PathBuf> {
    let home = account_home().ok_or_else(super::absent)?;
    let path = match name {
        "desktop" => home.join("Desktop"),
        "documents" => home.join("Documents"),
        "downloads" => home.join("Downloads"),
        "music" => home.join("Music"),
        "pictures" => home.join("Pictures"),
        "videos" => home.join("Movies"),
        "screenshots" => match screenshot() {
            Some(path) => match path.strip_prefix("~/") {
                Some(rest) => home.join(rest),
                None => PathBuf::from(path),
            },
            None => home.join("Desktop"),
        },
        _ => return Err(super::absent()),
    };
    if !path.is_absolute() {
        return Err(super::absent());
    }
    super::existing(path)
}
/// The settings file that can redirect the Screenshot destination.
pub fn macos_settings() -> std::io::Result<PathBuf> {
    Ok(account_home()
        .ok_or_else(super::absent)?
        .join("Library/Preferences/com.apple.screencapture.plist"))
}

fn account_home() -> Option<PathBuf> {
    let mut buffer = vec![0u8; 16384];
    let mut user = std::mem::MaybeUninit::<libc::passwd>::uninit();
    let mut found = std::ptr::null_mut();
    // SAFETY: getpwuid_r fills our passwd and buffer; copy pw_dir before either drops.
    unsafe {
        if libc::getpwuid_r(
            libc::getuid(),
            user.as_mut_ptr(),
            buffer.as_mut_ptr().cast(),
            buffer.len(),
            &mut found,
        ) != 0
            || found.is_null()
        {
            return None;
        }
        let user = user.assume_init();
        if user.pw_dir.is_null() {
            return None;
        }
        use std::os::unix::ffi::OsStrExt;
        Some(PathBuf::from(std::ffi::OsStr::from_bytes(
            std::ffi::CStr::from_ptr(user.pw_dir).to_bytes(),
        )))
    }
}
