//! The Windows executable's icon (LLP 1163 D6): the resource file `host/windows/build.mjs` writes from the app's
//! manifest, named by `EXACT_WINDOWS_ICON_RES`. MSVC's linker takes a `.res` file as an input, so the icon is in the
//! executable for Explorer, and the host loads it for its window and the taskbar.

use std::path::Path;

/// Link the icon's resource file into a Windows executable; nothing for another target or without one.
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
}
