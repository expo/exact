use super::*;
use std::io::Write;

fn types(list: &[&str]) -> Vec<String> {
    list.iter().map(|t| t.to_string()).collect()
}

fn utf16(text: &str) -> Vec<u8> {
    text.encode_utf16().flat_map(u16::to_le_bytes).collect()
}

#[test]
fn a_uri_list_gives_local_files_and_its_first_link() {
    let list = b"# from a file manager\r\nfile:///home/a/Photo%20one.jpg\r\nfile://localhost/home/a/b.pdf\r\nhttps://example.com/fuji\r\nfile://other.example/x\r\nhttps://example.com/second\r\n";
    let dropped = items(list, b"", b"");
    assert_eq!(
        dropped.paths,
        [
            PathBuf::from("/home/a/Photo one.jpg"),
            PathBuf::from("/home/a/b.pdf")
        ]
    );
    assert_eq!(dropped.url, "https://example.com/fuji");
    assert_eq!(dropped.text, "");
}

#[test]
fn a_malformed_entry_is_neither_the_link_nor_a_path() {
    let list = b"not a uri\r\n1http://example.com/\r\nfile:///tmp/a?b\r\nfile:///tmp/c#d\r\nhttps://example.com/valid\r\n";
    let dropped = items(list, b"", b"");
    assert_eq!(dropped.url, "https://example.com/valid");
    assert!(dropped.paths.is_empty(), "{:?}", dropped.paths);
    let junk = items(
        b"no scheme here\r\n",
        &utf16("https://example.com/titled\nTitle"),
        b"",
    );
    assert_eq!(
        junk.url, "https://example.com/titled",
        "junk leaves the titled link its place"
    );
}

#[test]
fn a_path_keeps_its_bytes_raw_or_percent_encoded() {
    let encoded = items(b"file:///tmp/caf%E9\r\n", b"", b"");
    assert_eq!(
        encoded.paths[0].as_os_str().as_encoded_bytes(),
        b"/tmp/caf\xe9"
    );
    let raw = items(b"file:///tmp/caf\xe9\r\n", b"", b"");
    assert_eq!(raw.paths[0].as_os_str().as_encoded_bytes(), b"/tmp/caf\xe9");
    assert_eq!(file_path(b"//host.example/tmp/a"), None);
    assert_eq!(file_path(b"relative"), None);
    assert_eq!(file_path(b"///tmp/%4"), None);
}

#[test]
fn text_arrives_as_it_came_and_a_link_is_not_made_from_it() {
    let dropped = items(b"", b"", "Mount Fuji\nhttps://example.com/fuji".as_bytes());
    assert_eq!(dropped.text, "Mount Fuji\nhttps://example.com/fuji");
    assert_eq!(dropped.url, "");
    assert!(dropped.paths.is_empty());
}

#[test]
fn a_browsers_titled_link_gives_the_link_the_uri_list_lacks() {
    let titled = utf16("https://example.com/fuji\nA link to Fuji");
    let dropped = items(b"", &titled, b"https://example.com/fuji");
    assert_eq!(dropped.url, "https://example.com/fuji");
    assert_eq!(
        dropped.text, "https://example.com/fuji",
        "the text is the source's own"
    );
    let listed = items(b"https://example.com/listed\r\n", &titled, b"");
    assert_eq!(
        listed.url, "https://example.com/listed",
        "a URI list's link comes first"
    );
    let file = items(
        b"file:///tmp/a.jpg\r\n",
        &utf16("file:///tmp/a.jpg\nA photo"),
        b"",
    );
    assert_eq!(
        (file.url.as_str(), file.paths.len()),
        ("", 1),
        "a file: link is a file"
    );
}

#[test]
fn a_titled_links_encoding_is_read_from_its_bytes() {
    let big: Vec<u8> = [0xFE, 0xFF]
        .into_iter()
        .chain(
            "https://example.com/b\n"
                .encode_utf16()
                .flat_map(u16::to_be_bytes),
        )
        .collect();
    assert_eq!(items(b"", &big, b"").url, "https://example.com/b");
    let unmarked: Vec<u8> = "https://example.com/be"
        .encode_utf16()
        .flat_map(u16::to_be_bytes)
        .collect();
    assert_eq!(items(b"", &unmarked, b"").url, "https://example.com/be");
    assert_eq!(
        items(b"", b"https://example.com/u8\ntitle", b"").url,
        "https://example.com/u8"
    );
    assert_eq!(
        items(b"", b"https://example.com/c-string\0", b"").url,
        "https://example.com/c-string",
        "UTF-8 ending in a NUL is not UTF-16"
    );
}

#[test]
fn an_offer_is_read_for_its_types_as_it_spells_them() {
    let wanted = Wanted::of(&types(&["text/plain", "UTF8_STRING", "text/uri-list"])).unwrap();
    assert_eq!(wanted.uris.as_deref(), Some(URI_LIST));
    assert_eq!(wanted.link, None);
    assert_eq!(wanted.text.as_deref(), Some("UTF8_STRING"));
    assert_eq!(wanted.first(), Some(URI_LIST));
    let spelled = Wanted::of(&types(&["TEXT/PLAIN;charset=UTF-8"])).unwrap();
    assert_eq!(spelled.text.as_deref(), Some("TEXT/PLAIN;charset=UTF-8"));
    assert_eq!(
        Wanted::of(&types(&["image/png", "application/x-example"])),
        None
    );
    let firefox = Wanted::of(&types(&[
        "text/x-moz-url",
        "_NETSCAPE_URL",
        "text/html",
        "text/plain",
    ]))
    .unwrap();
    assert_eq!(
        (firefox.link.as_deref(), firefox.first()),
        (Some(MOZ_URL), Some(MOZ_URL))
    );
}

#[test]
fn a_drop_is_read_whole_and_a_stalled_or_oversized_type_is_dropped() {
    let quiet = Duration::from_millis(200);
    let (whole, mut source) = std::io::pipe().unwrap();
    let (stalled, mut silent) = std::io::pipe().unwrap();
    let (large, mut flood) = std::io::pipe().unwrap();
    source.write_all(b"file:///tmp/a\r\n").unwrap();
    drop(source);
    silent.write_all(b"partial").unwrap();
    flood.write_all(&[b'x'; 64]).unwrap();
    drop(flood);
    let started = std::time::Instant::now();
    let [whole, stalled, large] = drain([Some(whole), Some(stalled), Some(large)], quiet, 32);
    assert_eq!(whole, b"file:///tmp/a\r\n");
    assert!(
        stalled.is_empty(),
        "a type its source never finished is not delivered"
    );
    assert!(
        large.is_empty(),
        "a type larger than the limit is not delivered"
    );
    assert!(
        started.elapsed() < quiet * 10,
        "the read ends once its sources are quiet"
    );
    drop(silent);
}
