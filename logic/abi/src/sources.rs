//! The sources a data crate answers beside app.ts (its `RUST_SOURCES`, LLP 1153.003 D2), carried in a wasm module
//! as the custom section [`SECTION`]: the names as a JSON array. The web build reads them from the module the page
//! runs, so a mixed app's web build compiles no host bake for them.

/// The custom section's name.
pub const SECTION: &str = "exact.rust_sources";

/// The section's length for `names`.
pub const fn section_len(names: &[&str]) -> usize {
    let mut len = 2 + names.len().saturating_sub(1);
    let mut i = 0;
    while i < names.len() {
        len += names[i].len() + 2;
        i += 1;
    }
    len
}

/// The section's bytes for `names`: `["a","b"]`. A source's name is a Contract identifier, so it needs no escape;
/// one that would is refused as the module compiles.
pub const fn section<const N: usize>(names: &[&str]) -> [u8; N] {
    let mut out = [0u8; N];
    out[0] = b'[';
    let (mut at, mut i) = (1, 0);
    while i < names.len() {
        if i > 0 {
            out[at] = b',';
            at += 1;
        }
        out[at] = b'"';
        at += 1;
        let name = names[i].as_bytes();
        let mut k = 0;
        while k < name.len() {
            assert!(
                name[k] >= 0x20 && name[k] != b'"' && name[k] != b'\\',
                "a RUST_SOURCES name is a Contract identifier"
            );
            out[at] = name[k];
            at += 1;
            k += 1;
        }
        out[at] = b'"';
        at += 1;
        i += 1;
    }
    out[at] = b']';
    out
}

/// Carry `$names`, the data crate's `RUST_SOURCES`, in this wasm module as [`SECTION`], beside
/// [`crate::export!`]'s exports. A native build carries nothing: its host composes the crate's list itself.
#[macro_export]
macro_rules! export_sources {
    ($names:expr) => {
        #[cfg(target_family = "wasm")]
        const _: () = {
            const NAMES: &[&str] = $names;
            #[link_section = "exact.rust_sources"]
            #[used]
            static SOURCES: [u8; $crate::sources::section_len(NAMES)] =
                $crate::sources::section(NAMES);
        };
    };
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_section_is_the_names_as_json() {
        const ONE: &[&str] = &["greeting"];
        const TWO: &[&str] = &["greeting", "note"];
        assert_eq!(&section::<{ section_len(&[]) }>(&[]), b"[]");
        assert_eq!(&section::<{ section_len(ONE) }>(ONE), br#"["greeting"]"#);
        assert_eq!(
            &section::<{ section_len(TWO) }>(TWO),
            br#"["greeting","note"]"#
        );
    }
}
