//! Reading a document the person opens.
use std::io::Read;

/// The largest body Windows Desk opens, in bytes: its edit control's limit.
pub const MOST: u64 = 1_000_000;

/// The UTF-8 text of the file at `path`, or `None` when it is longer than
/// `most` bytes, found by reading at most one byte more: a large file is
/// never read whole to be refused.
pub fn read(path: &std::path::Path, most: u64) -> std::io::Result<Option<String>> {
    let mut bytes = Vec::new();
    std::fs::File::open(path)?
        .take(most + 1)
        .read_to_end(&mut bytes)?;
    if bytes.len() as u64 > most {
        return Ok(None);
    }
    String::from_utf8(bytes).map(Some).map_err(|_| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "stream did not contain valid UTF-8",
        )
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_past_the_limit_is_refused_after_reading_one_byte_more() {
        let path = std::env::temp_dir().join(format!("windows-desk-{}.txt", std::process::id()));
        std::fs::write(&path, "café").unwrap();
        assert_eq!(read(&path, 5).unwrap().as_deref(), Some("café"));
        assert_eq!(read(&path, 4).unwrap(), None);
        std::fs::write(&path, [0x66, 0xff]).unwrap();
        let error = read(&path, 5).unwrap_err();
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
        // A file far past the limit costs one byte more than the limit.
        let file = std::fs::File::create(&path).unwrap();
        file.set_len(1 << 26).unwrap();
        assert_eq!(read(&path, 16).unwrap(), None);
        std::fs::remove_file(&path).unwrap();
    }
}
