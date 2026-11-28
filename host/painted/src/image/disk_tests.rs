//! The disk cache over a directory of its own, and its freshness decision.
use super::*;

fn dir() -> PathBuf {
    std::env::temp_dir().join(format!(
        "exact-disk-test-{}",
        super::super::spool_name().unwrap()
    ))
}

fn headers(pairs: &[(&str, &str)]) -> Vec<(String, String)> {
    pairs
        .iter()
        .map(|(k, v)| (k.to_string(), v.to_string()))
        .collect()
}

/// A body file of `len` bytes of `byte`.
fn body(dir: &Path, byte: u8, len: usize) -> PathBuf {
    std::fs::create_dir_all(dir).unwrap();
    let path = dir.join(format!("body-{byte}"));
    std::fs::write(&path, vec![byte; len]).unwrap();
    path
}

fn read_body(entry: &mut Entry, dir: &Path) -> Vec<u8> {
    let path = dir.join("copy");
    let mut out = File::create(&path).unwrap();
    entry.copy_to(&mut out).unwrap();
    std::fs::read(path).unwrap()
}

#[test]
fn freshness_decides_what_a_load_does() {
    let now = SystemTime::now();
    let at = |ago: u64, pairs: &[(&str, &str)]| {
        Head::of(
            &headers(pairs),
            &[],
            now - Duration::from_secs(ago),
            Duration::ZERO,
        )
        .usable(now)
    };
    assert_eq!(at(10, &[("Cache-Control", "max-age=60")]), Use::Fresh);
    assert_eq!(at(100, &[("cache-control", "max-age=60")]), Use::Refetch);
    assert_eq!(
        at(100, &[("cache-control", "max-age=60"), ("etag", "\"a\"")]),
        Use::Validate
    );
    let window = ("cache-control", "max-age=60, stale-while-revalidate=60");
    assert_eq!(at(100, &[window]), Use::StaleWhileRevalidate);
    assert_eq!(
        at(
            200,
            &[window, ("last-modified", "Sun, 06 Nov 1994 08:49:37 GMT")]
        ),
        Use::Validate
    );
    let must = (
        "cache-control",
        "max-age=60, stale-while-revalidate=60, must-revalidate",
    );
    assert_eq!(at(100, &[must, ("etag", "\"a\"")]), Use::Validate);
    assert_eq!(
        at(0, &[("cache-control", "no-cache"), ("etag", "\"a\"")]),
        Use::Validate
    );
    // Nothing to go by: never stored, and never used if it were.
    assert_eq!(at(0, &[]), Use::Refetch);
    // An unreadable `Age` is as old as can be; a huge lifetime does not overflow.
    assert_eq!(
        at(
            10,
            &[
                ("cache-control", "max-age=60"),
                ("age", "x"),
                ("etag", "\"a\"")
            ]
        ),
        Use::Validate
    );
    let huge = (
        "cache-control",
        "max-age=99999999999999999999, stale-while-revalidate=60",
    );
    assert_eq!(at(10, &[huge]), Use::Fresh);
    // Only `Last-Modified`, 1,000 s before its `Date`: fresh for 100 s.
    let received = UNIX_EPOCH + Duration::from_secs(784_111_777);
    let modified = Head::of(
        &headers(&[
            ("date", "Sun, 06 Nov 1994 08:49:37 GMT"),
            ("last-modified", "Sun, 06 Nov 1994 08:32:57 GMT"),
        ]),
        &[],
        received,
        Duration::ZERO,
    );
    assert_eq!(
        modified.usable(received + Duration::from_secs(99)),
        Use::Fresh
    );
    assert_eq!(
        modified.usable(received + Duration::from_secs(101)),
        Use::Validate
    );
}

#[test]
fn an_entry_outlives_its_process_under_its_own_url_and_a_304_freshens_it() {
    let dir = dir();
    let url = "https://cdn.example/a.jpg?v=1";
    let stored = Head::of(
        &headers(&[
            ("Cache-Control", "max-age=60"),
            ("ETag", "\"v1\""),
            ("Set-Cookie", "secret"),
            ("Date", "Sun, 06 Nov 1994 08:49:37 GMT"),
        ]),
        &[],
        SystemTime::now(),
        Duration::from_millis(40),
    );
    // Only what freshness and validation read is kept.
    assert!(stored.headers.iter().all(|(name, _)| name != "set-cookie"));
    Disk::new(dir.clone(), DISK_BYTES).put(url, &stored, &body(&dir, 7, 100), 100);

    // Another process over the same directory.
    let disk = Disk::new(dir.clone(), DISK_BYTES);
    assert!(disk.get("https://cdn.example/a.jpg?v=2").is_none());
    let mut entry = disk.get(url).expect("stored");
    assert_eq!(entry.head.headers, stored.headers);
    assert_eq!(entry.head.delay, Duration::from_millis(40));
    assert_eq!(read_body(&mut entry, &dir), vec![7; 100]);
    assert_eq!(entry.head.validators(), [("if-none-match", "\"v1\"")]);
    drop(entry);

    // An answer it may not keep takes the stored one with it.
    let no_store = Head::of(
        &headers(&[("cache-control", "no-store")]),
        &[],
        SystemTime::now(),
        Duration::ZERO,
    );
    disk.put(
        "https://cdn.example/b.jpg",
        &stored,
        &body(&dir, 8, 100),
        100,
    );
    disk.put(
        "https://cdn.example/b.jpg",
        &no_store,
        &body(&dir, 8, 100),
        100,
    );
    assert!(disk.get("https://cdn.example/b.jpg").is_none());
    let mut entry = disk.get(url).expect("stored");

    // A 304's headers replace the stored ones; the old exchange's `Date` goes.
    let refreshed = |pairs: &[(&str, &str)]| {
        (entry.head).refreshed(&headers(pairs), &[], SystemTime::now(), Duration::ZERO)
    };
    let freshened = refreshed(&[("cache-control", "max-age=3600")]).unwrap();
    assert_eq!(
        freshened.headers,
        headers(&[("etag", "\"v1\""), ("cache-control", "max-age=3600")])
    );
    // A 304 that names another representation freshens nothing.
    assert!(refreshed(&[("etag", "\"v1\"")]).is_some());
    assert!(refreshed(&[("etag", "\"v2\"")]).is_none());

    // A torn entry reads as none, and is gone.
    let file = std::fs::read_dir(&dir)
        .unwrap()
        .flatten()
        .find(|e| e.file_name().len() == 64)
        .unwrap()
        .path();
    let len = std::fs::metadata(&file).unwrap().len();
    File::options()
        .write(true)
        .open(&file)
        .unwrap()
        .set_len(len - 1)
        .unwrap();
    assert!(disk.get(url).is_none());
    assert!(!file.exists());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn the_least_recently_used_go_first_to_the_cap() {
    let dir = dir();
    let head = Head::of(
        &headers(&[("cache-control", "max-age=60")]),
        &[],
        SystemTime::now(),
        Duration::ZERO,
    );
    let url = |n: u8| format!("https://cdn.example/{n}.jpg");
    // Bodies at the largest a 2,000-byte cap keeps, each with its head.
    let disk = Disk::new(dir.clone(), 2000);
    disk.put(&url(0), &head, &body(&dir, 0, 100), 100);
    let fit = (2000 / std::fs::metadata(dir.join(key(&url(0)))).unwrap().len()) as u8;
    for n in 1..fit {
        disk.put(&url(n), &head, &body(&dir, n, 100), 100);
    }
    // Used: 0 is now the most recent.
    assert!(disk.get(&url(0)).is_some());
    for n in fit..fit + 2 {
        disk.put(&url(n), &head, &body(&dir, n, 100), 100);
    }
    // Two over: 1 and 2 have gone, and a new process sees the same.
    let disk = Disk::new(dir.clone(), 2000);
    let held: Vec<u8> = (0..fit + 2)
        .filter(|&n| disk.get(&url(n)).is_some())
        .collect();
    let expected: Vec<u8> = [0].into_iter().chain(3..fit + 2).collect();
    assert_eq!(held, expected);
    // Over 1/20 of the cap is never kept.
    disk.put(&url(99), &head, &body(&dir, 99, 101), 101);
    assert!(disk.get(&url(99)).is_none());
    std::fs::remove_dir_all(dir).unwrap();
}

#[test]
fn an_entry_answers_only_the_request_its_vary_names() {
    let head = |vary: &str, accept: &str| {
        Head::of(
            &headers(&[("cache-control", "max-age=60"), ("vary", vary)]),
            &[("accept", accept)],
            SystemTime::now(),
            Duration::ZERO,
        )
    };
    let png = [("accept", "image/png")];
    assert!(head("Accept", "image/png").selects(&png));
    assert!(!head("Accept", "image/webp").selects(&png));
    // The transport decodes a body, and adds a `User-Agent` this cache never sees.
    assert!(head("accept-encoding, accept", "image/png").selects(&png));
    assert!(!head("User-Agent", "image/png").selects(&png));
    assert!(!head("*", "image/png").selects(&png));
    // What it was selected by outlives the process.
    let dir = dir();
    let url = "https://cdn.example/v.jpg";
    let disk = Disk::new(dir.clone(), DISK_BYTES);
    disk.put(url, &head("Accept", "image/webp"), &body(&dir, 1, 10), 10);
    assert!(!disk.get(url).unwrap().head.selects(&png));
    std::fs::remove_dir_all(dir).unwrap();
}
