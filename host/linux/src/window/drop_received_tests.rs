use super::*;
use std::io::Write;
use std::os::fd::AsRawFd;
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

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
    for junk in [
        "not a uri",
        "1http://example.com/",
        "https://exa mple.com/",
        ":no-scheme",
    ] {
        assert_eq!(web_link(junk), None, "{junk}");
    }
    for uri in [
        "http://[::1]:8080/a",
        "mailto:a@example.com",
        "https://[broken",
        "x-app:open",
    ] {
        assert_eq!(
            web_link(uri),
            Some(uri),
            "a URI's parts are the app's to check"
        );
    }
    assert_eq!(
        file_path(b"///tmp/%+1"),
        None,
        "an escape is two hex digits"
    );
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

/// An escaped `/` or NUL names no file's name, as GLib's
/// `g_filename_from_uri` holds: the entry is refused and the rest kept.
#[test]
fn an_escaped_separator_or_nul_refuses_its_entry_alone() {
    let dropped = items(
        b"file:///tmp/album%2Fcover.jpg
file:///tmp/a%00b
file:///tmp/album%2fx
file:///tmp/kept
",
        b"",
        b"",
    );
    assert_eq!(dropped.paths, [PathBuf::from("/tmp/kept")]);
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
    // A stray last byte would otherwise leave a shorter link that is valid all the same
    let mut cut: Vec<u8> = [0xFF, 0xFE]
        .into_iter()
        .chain(utf16("https://example.com/ab"))
        .collect();
    cut.pop();
    assert_eq!(
        items(b"", &cut, b"").url,
        "",
        "UTF-16 cut partway through a unit gives no link"
    );
    let mut cut: Vec<u8> = [0xFE, 0xFF]
        .into_iter()
        .chain(
            "https://example.com/ab"
                .encode_utf16()
                .flat_map(u16::to_be_bytes),
        )
        .collect();
    cut.pop();
    assert_eq!(items(b"", &cut, b"").url, "");
}

#[test]
fn an_offer_is_read_for_its_types_as_it_spells_them() {
    let wanted = Wanted::of(
        &types(&["text/plain", "UTF8_STRING", "text/uri-list"]),
        false,
    )
    .unwrap();
    assert_eq!(wanted.uris.as_deref(), Some(URI_LIST));
    assert_eq!(wanted.link, None);
    assert_eq!(wanted.text.as_deref(), Some("UTF8_STRING"));
    assert_eq!(wanted.first(), Some(URI_LIST));
    let spelled = Wanted::of(&types(&["TEXT/PLAIN;charset=UTF-8"]), false).unwrap();
    assert_eq!(spelled.text.as_deref(), Some("TEXT/PLAIN;charset=UTF-8"));
    for spaced in [
        "text/plain; charset=utf-8",
        "text/plain ; charset = \"UTF-8\"",
    ] {
        let wanted = Wanted::of(&types(&[spaced, "text/plain"]), false).unwrap();
        assert_eq!(
            wanted.text.as_deref(),
            Some(spaced),
            "RFC 2045's spaces and quotes"
        );
    }
    let other = Wanted::of(
        &types(&[
            "text/plain;charset=utf-16",
            "text/plain;charset=utf-8;format=flowed",
        ]),
        false,
    );
    assert_eq!(
        other, None,
        "another charset, or another parameter, is another type"
    );
    assert_eq!(
        Wanted::of(&types(&["image/png", "application/x-example"]), false),
        None
    );
    let firefox = Wanted::of(
        &types(&["text/x-moz-url", "_NETSCAPE_URL", "text/html", "text/plain"]),
        false,
    )
    .unwrap();
    assert_eq!(
        (firefox.link.as_deref(), firefox.first()),
        (Some(MOZ_URL), Some(MOZ_URL))
    );
}

/// The portal's key is read in a sandbox alone: outside one, the URI list
/// names files the window can open itself.
#[test]
fn a_sandbox_reads_the_portals_key_beside_the_uri_list() {
    let offered = types(&["application/vnd.portal.filetransfer", "text/uri-list"]);
    assert_eq!(Wanted::of(&offered, false).unwrap().key, None);
    let sandboxed = Wanted::of(&offered, true).unwrap();
    assert_eq!(
        (sandboxed.key.as_deref(), sandboxed.uris.as_deref()),
        (Some("application/vnd.portal.filetransfer"), Some(URI_LIST))
    );
    let older = Wanted::of(&types(&["application/vnd.portal.files"]), true).unwrap();
    assert_eq!(older.key.as_deref(), Some("application/vnd.portal.files"));
    assert_eq!(older.first(), Some("application/vnd.portal.files"));
}

/// A reading of four pipes, their write ends handed back.
pub(crate) fn reading() -> (Reading, [std::io::PipeWriter; 4]) {
    let ends = [(); 4].map(|_| std::io::pipe().unwrap());
    let writers = ends.each_ref().map(|(_, w)| w.try_clone().unwrap());
    let pipes = ends.map(|(r, _)| {
        assert!(nonblocking(&r));
        Some(r)
    });
    let reading = Reading {
        id: 7,
        pipes,
        bytes: Default::default(),
        largest: 32,
        retrieved: None,
    };
    (reading, writers)
}

/// Read each pipe once, then each but those in `open`, which the test keeps
/// open, until it ends: a test that spawns a process meanwhile lends it the
/// write ends between its fork and its exec, so an end can come a moment
/// late. One that never comes fails the test.
#[allow(unsafe_code)]
pub(crate) fn read_all(reading: &mut Reading, deadline: Instant, open: &[usize]) {
    for k in 0..4 {
        reading.read(k, deadline);
    }
    let until = Instant::now() + Duration::from_secs(5);
    loop {
        let mut ending: Vec<libc::pollfd> = (0..4)
            .filter(|k| !open.contains(k))
            .filter_map(|k| {
                reading.pipes[k]
                    .as_ref()
                    .map(|p| poll_for(p.as_raw_fd(), libc::POLLIN))
            })
            .collect();
        if ending.is_empty() {
            return;
        }
        assert!(
            Instant::now() < until,
            "a pipe whose writers all closed never ended"
        );
        // SAFETY: the pollfds are live for the call and name pipes this reading holds open.
        unsafe { libc::poll(ending.as_mut_ptr(), ending.len() as libc::nfds_t, 50) };
        for k in (0..4).filter(|k| !open.contains(k)) {
            reading.read(k, deadline);
        }
    }
}

#[test]
fn a_drop_lands_each_type_it_finished_sending_by_the_deadline() {
    let later = Instant::now() + Duration::from_secs(60);
    let (mut whole, [mut uris, link, mut text, key]) = reading();
    uris.write_all(b"file:///tmp/a\r\n").unwrap();
    text.write_all(b"Mount Fuji").unwrap();
    drop((uris, link, text, key));
    read_all(&mut whole, later, &[]);
    assert!(whole.complete(), "every type arrived");
    let (id, dropped, portal) = whole.items();
    assert!(!portal);
    assert_eq!(
        (id, dropped.paths, dropped.text.as_str()),
        (7, vec![PathBuf::from("/tmp/a")], "Mount Fuji")
    );

    let (mut stalled, [mut uris, link, mut text, key]) = reading();
    uris.write_all(b"file:///tmp/a\r\n").unwrap();
    drop((uris, link, key));
    text.write_all(b"partial").unwrap();
    read_all(&mut stalled, later, &[TEXT]);
    assert!(!stalled.complete(), "the text is still arriving");
    let (_, dropped, _) = stalled.items();
    assert_eq!(dropped.paths, [PathBuf::from("/tmp/a")]);
    assert_eq!(
        dropped.text, "",
        "a type its source never finished is not delivered"
    );
    drop(text);

    let (mut late, [mut uris, link, mut text, key]) = reading();
    drop((link, key));
    uris.write_all(b"file:///tmp/a\r\n").unwrap();
    text.write_all(b"Mount Fuji").unwrap();
    drop((uris, text));
    read_all(&mut late, Instant::now(), &[]);
    assert!(late.complete());
    let dropped = late.items().1;
    assert_eq!(
        dropped.text, "",
        "a text whose end the listener observed at the deadline is not delivered"
    );
    assert_eq!(
        dropped.paths,
        [PathBuf::from("/tmp/a")],
        "a URI list has no deadline: its source may first copy its files"
    );
}

/// Past the deadline the text and the titled link are given up, while the
/// URI list is still waited for; another drop then cuts the wait, and the
/// drop lands with what came.
#[test]
fn past_its_deadline_a_drop_waits_for_its_files_alone_until_another_drop() {
    let (mut drop_, [mut uris, link, mut text, key]) = reading();
    uris.write_all(b"file:///tmp/half").unwrap();
    text.write_all(b"partial").unwrap();
    drop((link, key));
    read_all(
        &mut drop_,
        Instant::now() + Duration::from_secs(60),
        &[URIS, TEXT],
    );
    assert!(drop_.timed());
    drop_.time_out();
    assert!(!drop_.complete(), "the URI list is still arriving");
    assert!(
        !drop_.timed(),
        "no deadline wakes the loop for the URI list alone"
    );
    assert!(drop_.pipes[TEXT].is_none() && drop_.bytes[TEXT].is_empty());
    drop_.cut();
    assert!(drop_.complete());
    let dropped = drop_.items().1;
    assert!(
        dropped.paths.is_empty(),
        "a URI list cut short is not delivered"
    );
    drop((uris, text));
}

/// The key a sandbox redeems: GTK ends it with a NUL, KDE does not, and one
/// with a NUL inside, or none at all, is no key.
#[test]
fn a_key_is_read_as_gtk_and_kde_write_it() {
    let key_of = |bytes: &[u8]| {
        let (mut reading, [uris, link, text, mut key]) = reading();
        key.write_all(bytes).unwrap();
        drop((uris, link, text, key));
        read_all(&mut reading, Instant::now() + Duration::from_secs(60), &[]);
        reading.key().map(str::to_owned)
    };
    assert_eq!(key_of(b"8f3a\0").as_deref(), Some("8f3a"));
    assert_eq!(key_of(b"8f3a").as_deref(), Some("8f3a"));
    assert_eq!(key_of(b"8f\0a"), None);
    assert_eq!(key_of(b""), None);
    assert_eq!(key_of(b"\0"), None);

    let (mut cut, [uris, link, text, mut key]) = reading();
    key.write_all(b"8f3a\0").unwrap();
    drop((link, text, key));
    read_all(&mut cut, Instant::now() + Duration::from_secs(60), &[URIS]);
    cut.retrieved = Some(Arc::new(Mutex::new(None)));
    cut.cut();
    assert_eq!(cut.key(), None, "a cut drop redeems no key");
    assert!(cut.complete(), "nor waits for the portal");
    drop(uris);
}

/// A drop cut while only its URI list was still coming keeps the files the
/// portal already gave for its key.
#[test]
fn a_cut_drop_keeps_the_portals_answer_that_came() {
    let (mut reading, [uris, link, text, key]) = reading();
    drop((link, text, key));
    read_all(
        &mut reading,
        Instant::now() + Duration::from_secs(60),
        &[URIS],
    );
    let paths = vec![PathBuf::from("/run/user/1000/doc/1a2b/notes.txt")];
    reading.retrieved = Some(Arc::new(Mutex::new(Some(paths.clone()))));
    reading.cut();
    assert!(reading.complete());
    let (_, dropped, portal) = reading.items();
    assert!(portal);
    assert_eq!(dropped.paths, paths);
    drop(uris);
}

/// The paths the portal gave for the key are the files, in place of the
/// URI list's, and a drop waits for them.
#[test]
fn the_portals_paths_for_the_key_are_the_files() {
    let (mut reading, [mut uris, link, text, key]) = reading();
    uris.write_all(b"file:///a\r\nhttps://e.example/\r\n")
        .unwrap();
    drop((uris, link, text, key));
    read_all(&mut reading, Instant::now() + Duration::from_secs(60), &[]);
    let answer = Arc::new(Mutex::new(None));
    reading.retrieved = Some(answer.clone());
    assert!(!reading.complete(), "the portal has not answered");
    *answer.lock().unwrap() = Some(vec![PathBuf::from("/run/user/1000/doc/1a2b/notes.txt")]);
    assert!(reading.complete());
    let (_, dropped, portal) = reading.items();
    assert!(portal, "the files are the portal's, which need no check");
    assert_eq!(
        dropped.paths,
        [PathBuf::from("/run/user/1000/doc/1a2b/notes.txt")]
    );
    assert_eq!(
        dropped.url, "https://e.example/",
        "the URI list still gives the link"
    );
}

#[test]
fn a_type_larger_than_the_limit_is_not_delivered() {
    let (mut reading, [mut uris, link, mut text, key]) = reading();
    uris.write_all(&[b'x'; 32]).unwrap();
    text.write_all(&[b'x'; 33]).unwrap();
    drop((uris, link, text, key));
    read_all(&mut reading, Instant::now() + Duration::from_secs(60), &[]);
    assert!(reading.complete());
    assert_eq!(reading.bytes[0].len(), 32, "the limit itself is delivered");
    assert!(reading.bytes[2].is_empty(), "one byte past it is not");
}
