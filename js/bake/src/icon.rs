//! The Windows executable's resources. Its icon (LLP 1163 D6): the resource file `host/windows/build.mjs` writes from
//! the app's manifest, named by `EXACT_WINDOWS_ICON_RES`. MSVC's linker takes a `.res` file as an input, so the icon is
//! in the executable for Explorer, and the host loads it for its window and the taskbar. Its activation manifest names
//! version 6 of the common controls, as a hand-built Windows app's does, so a control the host makes (the tooltip)
//! takes the system's current look ([Enabling visual styles](https://learn.microsoft.com/windows/win32/controls/cookbook-overview)).

use std::path::Path;

/// Link the icon's resource file and the manifest into a Windows executable; nothing for another target.
pub(crate) fn link() {
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
