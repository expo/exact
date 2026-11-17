//! @ref LLP 0068#proposed-windows-native-filesystem-grants — one disjoint drive namespace.
fn refuse(detail: &str) -> std::io::Error {
    std::io::Error::other(detail)
}
pub(crate) fn validate_name(value: &str) -> io::Result<()> {
    let base = value
        .split('.')
        .next()
        .unwrap_or_default()
        .to_ascii_uppercase();
    let device = matches!(base.as_str(), "CON" | "PRN" | "AUX" | "NUL" | "CLOCK$")
        || ["COM", "LPT"].iter().any(|prefix| {
            base.strip_prefix(prefix).is_some_and(|suffix| {
                matches!(
                    suffix,
                    "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³"
                )
            })
        });
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.contains(['/', '\\', ':', '\0', '*', '?', '"', '<', '>', '|'])
        || value.chars().any(|c| c < ' ')
        || value.ends_with(['.', ' '])
        || device
    {
        return Err(refuse("not a relative filesystem name"));
    }
    let encoded: Vec<_> = value.encode_utf16().collect();
    if encoded.len() > 255 {
        return Err(refuse("filesystem name too long"));
    }
    Ok(())
}
use std::io;

// IsReparseTagNameSurrogate, tested independently of a Windows syscall.
#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn known_entry(attributes: u32, tag: u32) -> bool {
    attributes & 0x400 == 0 || tag & 0x20000000 == 0
}

#[cfg_attr(not(windows), allow(dead_code))]
pub(crate) fn open_known<T>(
    file: T,
    attributes: u32,
    tag: u32,
    reopen: impl FnOnce(&T) -> io::Result<T>,
) -> io::Result<T> {
    if !known_entry(attributes, tag) {
        return Err(refuse("known-folder entries cannot be name surrogates"));
    }
    // Windows can hide cloud tags, so even an apparently ordinary directory
    // needs a normal open before it can be the parent of a create.
    if attributes & 0x410 != 0 {
        reopen(&file)
    } else {
        Ok(file)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn known_folders_admit_cloud_placeholders_and_refuse_name_surrogates() {
        for attributes in [0, 0x10, 0x1000, 0x40000, 0x400000] {
            assert!(known_entry(attributes, 0));
            for tag in [0xa0000003, 0xa000000c, 0x20000000] {
                assert!(!known_entry(attributes | 0x400, tag), "{tag:x}");
            }
            for cloud in 0..=15 {
                assert!(known_entry(attributes | 0x400, 0x9000001a | cloud << 12));
            }
            assert!(known_entry(attributes | 0x400, 0x80000017)); // WOF
            assert!(known_entry(attributes, 0xa000000c)); // tag absent without attribute
        }
    }

    #[test]
    fn known_placeholders_use_the_reopened_handle_and_propagate_recall_failure() {
        for tag in (0..=15)
            .map(|cloud| 0x9000001a | cloud << 12)
            .chain([0x80000017])
        {
            assert_eq!(open_known(1, 0x410, tag, |_| Ok(2)).unwrap(), 2);
            let error = open_known(1, 0x410, tag, |_| {
                Err(io::Error::new(io::ErrorKind::PermissionDenied, "recall"))
            })
            .unwrap_err();
            assert_eq!(error.kind(), io::ErrorKind::PermissionDenied);
        }
        assert_eq!(open_known(1, 0x10, 0, |_| Ok(2)).unwrap(), 2);
        assert_eq!(
            open_known(1, 0x80, 0, |_| panic!("plain file reopened")).unwrap(),
            1
        );
        for tag in [0xa0000003, 0xa000000c, 0x20000000] {
            assert!(open_known(1, 0x410, tag, |_| panic!("surrogate followed")).is_err());
        }
    }

    #[test]
    fn windows_leaf_names_cannot_address_devices_or_alternate_streams() {
        for name in [
            "",
            ".",
            "..",
            "CON",
            "con.png",
            "PRN.txt",
            "AUX",
            "NUL",
            "CLOCK$",
            "COM1",
            "LPT9.png",
            "COM¹.png",
            "LPT²",
            "COM³",
            "a:b",
            "a/b",
            "a\\b",
            "a.",
            "a ",
            "a?",
            "a*",
            "a\0",
            "a\n",
        ] {
            assert!(validate_name(name).is_err(), "{name:?}");
        }
        for name in ["Screenshot 2026-11-17 142307.png", "COM10.png", "写真.png"] {
            assert!(validate_name(name).is_ok(), "{name}");
        }
        assert!(validate_name(&"x".repeat(256)).is_err());
        assert!(validate_name(&"📸".repeat(128)).is_err());
    }
}

#[derive(Clone, Debug)]
pub(crate) struct NativePath {
    pub drive: u8,
    pub parts: Vec<String>,
}
impl NativePath {
    pub fn parse(path: &str, normalize: bool) -> io::Result<Self> {
        let path = path.strip_prefix(r"\\?\").unwrap_or(path);
        let bytes = path.as_bytes();
        if bytes.len() < 3
            || !bytes[0].is_ascii_alphabetic()
            || bytes[1] != b':'
            || !matches!(bytes[2], b'/' | b'\\')
        {
            return Err(refuse("path must be a fully qualified local drive path"));
        }
        let mut parts = Vec::new();
        for part in path[3..].split(['/', '\\']) {
            match part {
                "" => continue,
                "." if normalize => continue,
                ".." if normalize => {
                    if parts.pop().is_none() {
                        return Err(refuse("path escapes its drive root"));
                    }
                }
                _ => {
                    if part.chars().any(char::is_control) {
                        return Err(refuse("native filesystem names cannot contain controls"));
                    }
                    validate_name(part)?;
                    parts.push(part.to_owned());
                }
            }
        }
        Ok(Self {
            drive: bytes[0].to_ascii_uppercase(),
            parts,
        })
    }
    #[cfg(windows)]
    pub fn spelling(&self) -> String {
        self.prefix(self.parts.len())
    }
    #[cfg(windows)]
    pub fn prefix(&self, length: usize) -> String {
        format!("{}:/{}", self.drive as char, self.parts[..length].join("/"))
    }
    pub fn grant_components(self) -> Vec<String> {
        let mut result = vec![format!("win:{}", self.drive as char)];
        result.extend(self.parts);
        result
    }
}
