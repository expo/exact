//! Drags out of the window (LLP 1094.000 D13): what a lifted node's drag
//! offers, by HTML's names for its data (D1), as Wayland's data source
//! offers it.

use std::os::unix::ffi::OsStrExt;
use std::path::Path;

/// The plain text types a lift offers its text as, in the order a reader
/// asks: UTF-8 by name, then text in no named charset, then X11's name.
const TEXTS: [&str; 3] = ["text/plain;charset=utf-8", "text/plain", "UTF8_STRING"];
/// The URI list a lift offers its files, then its link, as (RFC 2483).
const URI_LIST: &str = "text/uri-list";

/// The MIME types a drag of `text`, `url` and files offers: its text by the
/// plain text types, and its files and link as one URI list. Nothing is
/// made from another value: a link alone offers no text.
pub(super) fn types(text: &str, url: &str, files: usize) -> Vec<&'static str> {
    let mut types = Vec::new();
    if !text.is_empty() {
        types.extend(TEXTS);
    }
    if files > 0 || !url.is_empty() {
        types.push(URI_LIST);
    }
    types
}

/// The URI list of `files`, then `url`: each file a `file:` URI with no
/// host (RFC 8089), its path's bytes percent-encoded but for RFC 3986's
/// unreserved characters and `/`, each line ended by CRLF.
pub(super) fn uri_list(files: &[impl AsRef<Path>], url: &str) -> String {
    let mut list = String::new();
    for file in files {
        list.push_str("file://");
        for &b in file.as_ref().as_os_str().as_bytes() {
            if b.is_ascii_alphanumeric() || b"-._~/".contains(&b) {
                list.push(b as char);
            } else {
                list.push_str(&format!("%{b:02X}"));
            }
        }
        list.push_str("\r\n");
    }
    if !url.is_empty() {
        list.push_str(url);
        list.push_str("\r\n");
    }
    list
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::ffi::OsStr;
    use std::path::PathBuf;

    #[test]
    fn a_lift_offers_its_text_and_its_files_and_link_by_their_own_types() {
        assert_eq!(types("Fuji", "", 0), TEXTS);
        assert_eq!(
            types("", "https://example.com/fuji", 0),
            [URI_LIST],
            "a link alone offers no text"
        );
        assert_eq!(types("", "", 2), [URI_LIST]);
        assert_eq!(types("Fuji", "https://example.com/fuji", 1).len(), 4);
        assert!(types("", "", 0).is_empty());
    }

    #[test]
    fn a_uri_list_names_each_file_then_the_link_and_a_drop_reads_it_back() {
        let raw = PathBuf::from(OsStr::from_bytes(b"/tmp/caf\xe9 #1.jpg"));
        let list = uri_list(
            &[PathBuf::from("/srv/app/assets/caltrain.png"), raw.clone()],
            "https://example.com/fuji",
        );
        assert_eq!(
            list,
            "file:///srv/app/assets/caltrain.png\r\nfile:///tmp/caf%E9%20%231.jpg\r\nhttps://example.com/fuji\r\n"
        );
        let dropped = super::super::drop::items(list.as_bytes(), b"", b"");
        assert_eq!(
            dropped.paths,
            [PathBuf::from("/srv/app/assets/caltrain.png"), raw]
        );
        assert_eq!(dropped.url, "https://example.com/fuji");
    }
}
