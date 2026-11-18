//! Pure Windows path spellings; no host filesystem access.
//! @ref LLP 1027.001#proposed-windows-native-filesystem-grant-integration — one lexical authority.
use std::io;

fn refuse(message: &str) -> io::Error {
    io::Error::other(message)
}

/// A qualified local-drive spelling, independent of the compilation host.
/// Execution is a separate capability owned by the Windows filesystem adapter.
#[derive(Clone, Debug)]
pub struct WindowsPath {
    pub drive: u8,
    pub parts: Vec<String>,
}
impl WindowsPath {
    /// A shell path as UTF-16, without changing the file a verbatim spelling names.
    /// Refuse paths beyond MAX_PATH and names Win32 would normalize or treat as devices.
    /// https://learn.microsoft.com/windows/win32/fileio/naming-a-file
    pub fn shell_path(path: &[u16]) -> io::Result<Vec<u16>> {
        let wide = |text: &str| text.encode_utf16().collect::<Vec<_>>();
        let ordinary = match path.strip_prefix(wide(r"\\?\").as_slice()) {
            Some(rest) => match rest.strip_prefix(wide(r"UNC\").as_slice()) {
                Some(unc) => [wide(r"\\").as_slice(), unc].concat(),
                None => rest.to_vec(),
            },
            None => path.to_vec(),
        };
        if ordinary.len() >= 260 {
            return Err(refuse("path is too long for the Windows shell (MAX_PATH)"));
        }
        // Validate a decoded copy, but return the original UTF-16, including lone surrogates.
        let text = String::from_utf16_lossy(&ordinary);
        let bytes = text.as_bytes();
        let parts = if bytes.len() >= 3
            && bytes[0].is_ascii_alphabetic()
            && bytes[1] == b':'
            && bytes[2] == b'\\'
        {
            &text[3..]
        } else if let Some(unc) = text.strip_prefix(r"\\") {
            if unc.split('\\').filter(|s| !s.is_empty()).count() < 2 {
                return Err(refuse("a Windows shell UNC path needs a server and share"));
            }
            unc
        } else {
            return Err(refuse(
                "path has no drive or UNC spelling for the Windows shell",
            ));
        };
        if !parts.is_empty() {
            for part in parts.strip_suffix('\\').unwrap_or(parts).split('\\') {
                if part.ends_with(['.', ' ']) {
                    return Err(refuse("a Windows shell name cannot end in a dot or space"));
                }
                Self::validate_component(part).map_err(|error| {
                    refuse(&format!(
                        "path has no equivalent Windows shell name: {error}"
                    ))
                })?;
                let base = part
                    .split('.')
                    .next()
                    .unwrap_or_default()
                    .trim_end_matches(' ');
                if (!base.is_empty() && Self::validate_component(base).is_err())
                    || matches!(base.to_ascii_uppercase().as_str(), "CONIN$" | "CONOUT$")
                {
                    return Err(refuse("path contains a reserved Windows shell name"));
                }
            }
        }
        Ok(ordinary)
    }

    /// Validate a native leaf before either grant admission or an NT open.
    pub fn validate_component(value: &str) -> io::Result<()> {
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
            || value.chars().any(char::is_control)
            || value.ends_with(['.', ' '])
            || device
        {
            return Err(refuse("not a native filesystem name"));
        }
        if value.encode_utf16().count() > 255 {
            return Err(refuse("filesystem name too long"));
        }
        Ok(())
    }

    /// Grants reject traversal; requests may normalize it before admission.
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
                    Self::validate_component(part)?;
                    parts.push(part.to_owned());
                }
            }
        }
        Ok(Self {
            drive: bytes[0].to_ascii_uppercase(),
            parts,
        })
    }
    pub fn spelling(&self) -> String {
        self.prefix(self.parts.len())
    }
    pub fn prefix(&self, length: usize) -> String {
        format!("{}:/{}", self.drive as char, self.parts[..length].join("/"))
    }
    pub(crate) fn grant_components(self) -> Vec<String> {
        let mut result = vec![format!("win:{}", self.drive as char)];
        result.extend(self.parts);
        result
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn shell(path: &str) -> io::Result<String> {
        let path = WindowsPath::shell_path(&path.encode_utf16().collect::<Vec<_>>())?;
        Ok(String::from_utf16(&path).unwrap())
    }

    #[test]
    fn shell_paths_remove_only_the_drive_or_unc_verbatim_prefix() {
        for (input, expected) in [
            (r"\\?\C:\samples", r"C:\samples"),
            (r"\\?\c:\café\图像", r"c:\café\图像"),
            (r"\\?\C:\", r"C:\"),
            (r"\\?\UNC\server\share\samples", r"\\server\share\samples"),
            (r"\\?\UNC\server\share", r"\\server\share"),
            (r"C:\samples", r"C:\samples"),
            (r"\\server\share\samples", r"\\server\share\samples"),
        ] {
            assert_eq!(shell(input).unwrap(), expected);
        }
        let mut path: Vec<_> = r"\\?\C:\".encode_utf16().collect();
        path.push(0xd800);
        assert_eq!(WindowsPath::shell_path(&path).unwrap(), path[4..]);
    }

    #[test]
    fn shell_paths_refuse_names_whose_ordinary_spelling_can_change_the_target() {
        for path in [
            r"\\?\C:\NUL.txt",
            r"\\?\C:\con",
            r"\\?\C:\COM¹.log\child",
            r"\\?\C:\CON .txt",
            r"\\?\C:\CONIN$",
            r"\\?\C:\CONOUT$",
            r"\\?\C:\trailing.\child",
            r"\\?\C:\trailing \child",
            r"\\?\UNC\server\share\LPT9",
            r"\\?\UNC\server\share\name.",
            r"\\?\C:\a\..\b",
            r"\\?\C:\a/child",
            r"\\?\C:\a:stream",
            r"\\?\Volume{1234}\samples",
            r"\\.\C:\samples",
            r"C:relative",
            r"\rooted",
            r"\\server",
            "C:\\a\0b",
        ] {
            assert!(shell(path).is_err(), "{path:?}");
        }
    }

    #[test]
    fn shell_paths_count_the_ordinary_utf16_spelling_and_its_terminator() {
        for prefix in [r"\\?\C:\", r"\\?\UNC\server\share\"] {
            let short = format!("{prefix}{}\\", "a".repeat(120));
            let ordinary = if prefix.contains("UNC") {
                short.len() - 6
            } else {
                short.len() - 4
            };
            let limit = format!("{short}{}", "b".repeat(259 - ordinary));
            assert_eq!(shell(&limit).unwrap().encode_utf16().count(), 259);
            assert!(shell(&format!("{limit}x")).is_err());
        }
        let short = format!(r"\\?\C:\{}\", "é".repeat(120));
        assert!(shell(&format!("{short}{}", "😀".repeat(67))).is_ok());
        assert!(shell(&format!("{short}{}", "😀".repeat(68))).is_err());
    }
}
