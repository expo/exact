//! The icon and common-controls manifest for Windows executables (LLP 1163 D6).

use std::path::Path;

/// Link the app's icon for Explorer and the taskbar. MSVC also embeds the
/// activation manifest that enables themed common controls.
pub fn link_windows_resources() {
    println!("cargo:rerun-if-env-changed=EXACT_WINDOWS_ICON_RES");
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        return;
    }
    if let Some(res) = std::env::var_os("EXACT_WINDOWS_ICON_RES").filter(|path| !path.is_empty()) {
        let res = Path::new(&res);
        println!("cargo:rerun-if-changed={}", res.display());
        println!("cargo:rustc-link-arg-bins={}", res.display());
    }
    // These are link.exe's flags: a GNU target, which only the cross-target check builds, gets no manifest.
    if std::env::var("CARGO_CFG_TARGET_ENV").as_deref() == Ok("msvc") {
        println!("cargo:rustc-link-arg-bins=/MANIFEST:EMBED");
        println!("cargo:rustc-link-arg-bins=/MANIFESTDEPENDENCY:type='win32' name='Microsoft.Windows.Common-Controls' version='6.0.0.0' processorArchitecture='*' publicKeyToken='6595b64144ccf1df' language='*'");
    }
}
