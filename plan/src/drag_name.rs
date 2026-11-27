//! @ref LLP 1094.000 D14 — a dragged file's name: the rule a name keeps, the
//! name a destination is offered once the file's suffix is kept, and the
//! decoding of `dataTransferEntries`, the files and names as the compiler
//! binds them. The compiler checks a literal name by [`drag_name_refusal`];
//! the runner, and the JS target's port (`host/web-js/drag.js`), check a
//! bound one, as written and as offered.

/// Separates the names from the files in `dataTransferEntries`.
pub const DRAG_NAMES_MARK: char = '\u{1e}';

/// Why `name` is no name every host can write, as written: Exact's rule,
/// informed by Windows' file-name rules. An empty name is no name at all:
/// the caller keeps the file's own.
pub fn drag_name_refusal(name: &str) -> Option<&'static str> {
    if name.is_empty() || name.len() > 255 {
        return Some("a name is 1 to 255 bytes of UTF-8");
    }
    if name.chars().any(|c| c <= '\u{1f}' || c == '\u{7f}') {
        return Some("a name holds no control character");
    }
    if name.chars().any(|c| r#"<>:"/\|?*"#.contains(c)) {
        return Some(r#"a name holds none of < > : " / \ | ? *"#);
    }
    if name.starts_with(' ') || name.ends_with(' ') {
        return Some("a name neither begins nor ends with a space");
    }
    if name.ends_with('.') {
        return Some("a name does not end with a dot");
    }
    if is_device_name(name) {
        return Some("Windows keeps that name for a device");
    }
    None
}

/// Whether `name`, before its first dot and without case, is one Windows
/// keeps for a device.
fn is_device_name(name: &str) -> bool {
    let stem = name.split('.').next().unwrap_or(name).to_ascii_uppercase();
    if ["CON", "PRN", "AUX", "NUL"].contains(&stem.as_str()) {
        return true;
    }
    let Some(rest) = stem.strip_prefix("COM").or_else(|| stem.strip_prefix("LPT")) else {
        return false;
    };
    matches!(rest, "1" | "2" | "3" | "4" | "5" | "6" | "7" | "8" | "9" | "¹" | "²" | "³")
}

/// The suffixes that spell one type in the extension table `expect drag`
/// reads (`scripts/share-target.mjs`).
const ALIASES: [[&str; 2]; 4] = [["jpg", "jpeg"], ["htm", "html"], ["txt", "text"], ["md", "markdown"]];

/// What follows the last dot of `name`, when that dot is neither its first
/// character nor its last.
fn suffix(name: &str) -> Option<&str> {
    let dot = name.rfind('.')?;
    (dot > 0 && dot + 1 < name.len()).then(|| &name[dot + 1..])
}

/// Whether two suffixes name one type: the same without case, or two
/// spellings in the table.
fn same_type(a: &str, b: &str) -> bool {
    a.eq_ignore_ascii_case(b)
        || ALIASES.iter().any(|pair| {
            pair.iter().any(|s| s.eq_ignore_ascii_case(a)) && pair.iter().any(|s| s.eq_ignore_ascii_case(b))
        })
}

/// The name a destination is offered for `file` named `name`: as written
/// when its suffix types it as the file is typed, else with the file's own
/// suffix added; a file with no suffix takes the name as written.
pub fn offered_drag_name(file: &str, name: &str) -> String {
    let own = file.rsplit('/').next().unwrap_or(file);
    match suffix(own) {
        Some(theirs) if !suffix(name).is_some_and(|s| same_type(s, theirs)) => format!("{name}.{theirs}"),
        _ => name.to_owned(),
    }
}

/// The files and, when the node names them, the names that
/// `dataTransferEntries` holds, or `None` for a value the compiler would not
/// write. Each entry is percent-encoded and followed by a line feed.
pub fn decode_drag_entries(value: &str) -> Option<(Vec<String>, Option<Vec<String>>)> {
    let (files, names) = match value.split_once(DRAG_NAMES_MARK) {
        Some((files, names)) => (files, Some(names)),
        None => (value, None),
    };
    let names = match names {
        Some(names) => Some(entries(names)?),
        None => None,
    };
    Some((entries(files)?, names))
}

/// Each line-feed-terminated, percent-encoded entry of `part`, decoded.
fn entries(part: &str) -> Option<Vec<String>> {
    if part.is_empty() {
        return Some(Vec::new());
    }
    part.strip_suffix('\n')?.split('\n').map(percent_decoded).collect()
}

/// `s` with each `%XX` read as a byte, which must make UTF-8.
fn percent_decoded(s: &str) -> Option<String> {
    let mut bytes = Vec::with_capacity(s.len());
    let mut at = s.bytes();
    while let Some(b) = at.next() {
        if b == b'%' {
            let hex = [at.next()?, at.next()?];
            bytes.push(u8::from_str_radix(std::str::from_utf8(&hex).ok()?, 16).ok()?);
        } else {
            bytes.push(b);
        }
    }
    String::from_utf8(bytes).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The cases `host/web-js/drag.test.mjs` holds its port to.
    #[test]
    fn a_name_is_a_portable_basename_of_up_to_255_bytes() {
        for ok in ["Mount Fuji.jpg", "St. Paul", "Fuji", ".profile", "a".repeat(255).as_str(), "COM0.txt", "CONSOLE"] {
            assert_eq!(drag_name_refusal(ok), None, "{ok}");
        }
        for (bad, why) in [
            ("", "a name is 1 to 255 bytes of UTF-8"),
            (&"é".repeat(128), "a name is 1 to 255 bytes of UTF-8"),
            ("a\nb", "a name holds no control character"),
            ("a\u{7f}", "a name holds no control character"),
            ("photos/1016.jpg", r#"a name holds none of < > : " / \ | ? *"#),
            ("a:b", r#"a name holds none of < > : " / \ | ? *"#),
            (" a", "a name neither begins nor ends with a space"),
            ("a ", "a name neither begins nor ends with a space"),
            ("Fuji.", "a name does not end with a dot"),
            (".", "a name does not end with a dot"),
            ("..", "a name does not end with a dot"),
            ("con", "Windows keeps that name for a device"),
            ("Nul.txt", "Windows keeps that name for a device"),
            ("COM9.tar.gz", "Windows keeps that name for a device"),
            ("lpt³", "Windows keeps that name for a device"),
        ] {
            assert_eq!(drag_name_refusal(bad), Some(why), "{bad:?}");
        }
    }

    #[test]
    fn a_name_keeps_the_files_suffix() {
        for (file, name, offered) in [
            ("assets/apple/1016.jpg", "Mount Fuji.jpg", "Mount Fuji.jpg"),
            ("assets/apple/1016.jpg", "Fuji.JPG", "Fuji.JPG"),
            ("assets/apple/1016.jpg", "Fuji.jpeg", "Fuji.jpeg"),
            ("app:/data/notes.md", "Notes.markdown", "Notes.markdown"),
            ("assets/apple/1016.jpg", "Mount Fuji", "Mount Fuji.jpg"),
            ("assets/apple/1016.jpg", "St. Paul", "St. Paul.jpg"),
            ("assets/apple/1016.jpg", "Fuji.png", "Fuji.png.jpg"),
            ("assets/apple/1016.jpg", ".profile", ".profile.jpg"),
            ("assets/README", "Read me.txt", "Read me.txt"),
            ("assets/.profile", "Profile", "Profile"),
            // A Kelvin sign is no `k`: only ASCII letters fold, here and in the JS port
            ("assets/a.md", "Notes.mar\u{212a}down", "Notes.mar\u{212a}down.md"),
        ] {
            assert_eq!(offered_drag_name(file, name), offered, "{file} {name}");
        }
    }

    #[test]
    fn the_entries_keep_every_boundary() {
        assert_eq!(decode_drag_entries(""), Some((vec![], None)));
        assert_eq!(decode_drag_entries("a%0Ab.jpg\n"), Some((vec!["a\nb.jpg".into()], None)));
        assert_eq!(
            decode_drag_entries("assets%2Fa.jpg\nassets%2Fb.jpg\n\u{1e}\nB%20b\n"),
            Some((vec!["assets/a.jpg".into(), "assets/b.jpg".into()], Some(vec!["".into(), "B b".into()])))
        );
        // No names differs from one empty name
        assert_eq!(decode_drag_entries("a\n\u{1e}"), Some((vec!["a".into()], Some(vec![]))));
        assert_eq!(decode_drag_entries("a\n\u{1e}\n"), Some((vec!["a".into()], Some(vec!["".into()]))));
        for broken in ["a", "a\n\u{1e}b", "%4", "%zz\n", "%FF\n"] {
            assert_eq!(decode_drag_entries(broken), None, "{broken:?}");
        }
    }
}
