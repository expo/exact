//! Windows common item dialogs (LLP 1173 D1–D2).
use exact_runner::file_pickers::Kind;

#[cfg(windows)]
mod os;
#[cfg(windows)]
pub(super) use os::Dialogs;

// FILEOPENDIALOGOPTIONS values, kept portable so the mapping runs on every host.
// https://learn.microsoft.com/windows/win32/api/shobjidl_core/ne-shobjidl_core-_fileopendialogoptions
fn options(kind: Kind, multiple: bool, defaults: u32) -> u32 {
    const FORCE_FILESYSTEM: u32 = 0x40;
    const NO_CHANGE_DIR: u32 = 0x8;
    const DONT_ADD_TO_RECENT: u32 = 0x0200_0000;
    const ALLOW_MULTISELECT: u32 = 0x200;
    const PICK_FOLDERS: u32 = 0x20;
    defaults
        | FORCE_FILESYSTEM
        | NO_CHANGE_DIR
        | DONT_ADD_TO_RECENT
        | match kind {
            Kind::Open if multiple => ALLOW_MULTISELECT,
            Kind::Directory => PICK_FOLDERS,
            _ => 0,
        }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn requests_preserve_shell_defaults_and_add_only_their_selection_options() {
        let base = 0x0200_0048;
        for (kind, multiple, defaults, expected) in [
            (Kind::Open, false, 0x1800, base | 0x1800),
            (Kind::Open, true, 0x1800, base | 0x1a00),
            (Kind::Directory, false, 0x1800, base | 0x1820),
            (Kind::Save, false, 0x8802, base | 0x8802),
            (Kind::Save, true, 0x8802, base | 0x8802),
        ] {
            assert_eq!(options(kind, multiple, defaults), expected, "{kind:?}");
        }
    }
}
