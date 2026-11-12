//! The server's own unit tests (`serve.rs`).
use super::*;

#[test]
fn a_socket_grant_is_a_connect_source() {
    let policy = csp(
        "net.fetch https://api.test\nnet.websocket wss://jetstream.test\nfs.read /x",
        Path::new("/nonexistent"),
    );
    assert!(
        policy.contains("connect-src 'self' https://api.test wss://jetstream.test;"),
        "{policy}"
    );
}

#[test]
fn a_scheme_line_is_a_scheme_source() {
    let policy = csp(
        "net.fetch HTTP:\nnet.websocket ws:\nnet.websocket wss://feed.test/path",
        Path::new("/nonexistent"),
    );
    assert!(
        policy.contains("connect-src 'self' http: ws: wss://feed.test;"),
        "{policy}"
    );
}

#[test]
fn a_local_network_line_admits_local_names_and_no_address_range() {
    // LLP 1133.036 D5: no CSP source names a private range, so a served page
    // reaches `.local` names by the line and an address only by a line naming it.
    let policy = csp(
        "net.fetch local-network\nnet.websocket local-network development",
        Path::new("/nonexistent"),
    );
    assert!(
        policy.contains(
            "connect-src 'self' http://*.local:* https://*.local:* ws://*.local:* wss://*.local:*;"
        ),
        "{policy}"
    );
}

#[test]
fn connect_sources_use_app_grants_even_when_a_request_has_a_narrower_source_scope() {
    let policy = csp(
        "net.fetch https://a.test\nnet.fetch https://b.test\nnet.fetch https://c.test",
        Path::new("/nonexistent"),
    );
    assert!(
        policy.contains("connect-src 'self' https://a.test https://b.test https://c.test;"),
        "{policy}"
    );
}

#[test]
fn the_boot_swap_script_is_admitted_by_its_hash() {
    // @ref LLP 1048.005 — the one inline script a boot document's page adds.
    let policy = csp("", Path::new("/nonexistent"));
    use sha2::{Digest, Sha256};
    let digest = Sha256::digest(crate::direct::boot_swap_js().as_bytes());
    let admitted = format!("'sha256-{}'", exact_data::envelope::base64(&digest));
    assert!(policy.contains(&admitted), "{policy}");
}

/// A shell as the web build writes it: the sheet, then `host.web.head`'s
/// entries, then the root.
const SHELL: &str = "<!doctype html>\n<title>t</title>\n<style>.a{}</style>\n\
<script>gtag('js')</script>\n\
<script type=\"application/ld+json\">{\"@type\":\"Person\"}</script>\n\
<script async src=\"https://cdn.test/lib/x.js?id=1\"></script>\n\
<script src=\"/own.js\"></script>\n\
<div id=\"exact-root\"></div>\n<script type=\"module\" src=\"./app.js\"></script>\n";

/// A dist holding `.gen/csp.json` as `json`, removed when dropped.
struct Dist(std::path::PathBuf);
impl Dist {
    fn with(json: Option<&str>) -> Dist {
        use std::sync::atomic::{AtomicUsize, Ordering};
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let dir = std::env::temp_dir().join(format!(
            "exact-render-csp-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        std::fs::create_dir_all(dir.join(".gen")).unwrap();
        if let Some(json) = json {
            std::fs::write(dir.join(".gen/csp.json"), json).unwrap();
        }
        Dist(dir)
    }
    fn headers(&self) -> Result<String, String> {
        csp::headers("", &self.0, SHELL, Some("https://site.test"))
    }
}
impl Drop for Dist {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn directive<'h>(headers: &'h str, header: &str, name: &str) -> Option<&'h str> {
    let line = headers
        .lines()
        .find_map(|line| line.strip_prefix(header)?.strip_prefix(": "))?;
    line.split("; ")
        .find_map(|d| d.strip_prefix(name)?.strip_prefix(' '))
}

#[test]
fn the_apps_head_scripts_are_admitted_and_its_data_blocks_are_not() {
    // @ref LLP 1048.000 D3 — the app's head scripts run as on a static host.
    let headers = Dist::with(None).headers().unwrap();
    let scripts = directive(&headers, "Content-Security-Policy", "script-src").unwrap();
    use sha2::{Digest, Sha256};
    let hash = |text: &str| {
        format!(
            "'sha256-{}'",
            exact_data::envelope::base64(&Sha256::digest(text.as_bytes()))
        )
    };
    assert!(scripts.contains(&hash("gtag('js')")), "{scripts}");
    assert!(
        !scripts.contains(&hash("{\"@type\":\"Person\"}")),
        "{scripts}"
    );
    assert!(scripts.ends_with(" https://cdn.test"), "{scripts}");
    assert!(!headers.contains("Report-Only"), "{headers}");
}

#[test]
fn a_list_adds_a_string_replaces_and_exact_stands_for_exacts_sources() {
    // @ref LLP 1048.000 D11 — host.web.csp, the app's part of the policy.
    let headers = Dist::with(Some(
        r#"{"csp": {"connect-src": ["https://analytics.test"], "manifest-src": ["https://m.test"],
            "script-src-elem": ["https://more.test"], "img-src": "'self'", "style-src": "exact https://fonts.test",
            "report-uri": ["https://reports.test/csp"]}}"#,
    ))
    .headers()
    .unwrap();
    let get = |name| directive(&headers, "Content-Security-Policy", name).unwrap();
    assert_eq!(get("connect-src"), "'self' https://analytics.test");
    // Not written by Exact: what it fell back to comes first.
    assert_eq!(get("manifest-src"), "'self' https://m.test");
    assert!(
        get("script-src-elem").starts_with("'self' 'wasm-unsafe-eval' 'sha256-"),
        "{headers}"
    );
    assert!(
        get("script-src-elem").ends_with(" https://cdn.test https://more.test"),
        "{headers}"
    );
    assert_eq!(get("img-src"), "'self'");
    assert_eq!(
        get("style-src"),
        "'self' 'unsafe-inline' https://fonts.test"
    );
    // No fallback: written as the app gives it.
    assert_eq!(get("report-uri"), "https://reports.test/csp");
}

#[test]
fn false_sends_no_policy_and_report_only_starts_from_exacts() {
    let headers = Dist::with(Some(
        r#"{"csp": false, "cspReportOnly": {"img-src": ["https://x.test"]}}"#,
    ))
    .headers()
    .unwrap();
    assert!(!headers.contains("Content-Security-Policy:"), "{headers}");
    let img = directive(&headers, "Content-Security-Policy-Report-Only", "img-src").unwrap();
    assert_eq!(img, "'self' data: blob: https: https://x.test");
    assert!(directive(
        &headers,
        "Content-Security-Policy-Report-Only",
        "script-src"
    )
    .is_some());
}

#[test]
fn a_value_that_would_end_the_directive_or_the_policy_is_refused() {
    for json in [
        r#"{"csp": {"img-src": ["a.test, b.test"]}}"#,
        r#"{"csp": {"img-src": "'self'; script-src *"}}"#,
        r#"{"csp": {"Img-Src": ["a.test"]}}"#,
        r#"{"csp": {"img-src": [1]}}"#,
    ] {
        assert!(Dist::with(Some(json)).headers().is_err(), "{json}");
    }
}

fn sha(text: &str) -> String {
    use sha2::{Digest, Sha256};
    format!(
        "'sha256-{}'",
        exact_data::envelope::base64(&Sha256::digest(text.as_bytes()))
    )
}

#[test]
fn a_script_src_is_its_origin_and_an_inline_script_is_hashed_as_the_browser_reads_it() {
    let shell = "<style>.a{}</style>\n\
        <script src=\"//cdn.test:8443/a.js\"></script>\n\
        <script src=\"https://site.test/b.js\"></script>\n\
        <script type=\"module\">m()</script>\n\
        <script type=\"text/javascript; charset=utf-8\">d()</script>\n\
        <script data-x=\"a>b\">x()</script>\n\
        <script>one();\r\ntwo();\rthree();</script>\n\
        <div id=\"exact-root\"></div>";
    let dist = Dist::with(None);
    let scripts = |origin| {
        let headers = csp::headers("", &dist.0, shell, origin).unwrap();
        directive(&headers, "Content-Security-Policy", "script-src")
            .unwrap()
            .to_string()
    };
    let at_site = scripts(Some("https://site.test"));
    // Protocol-relative: no scheme, so it matches the page's.
    assert!(at_site.contains(" cdn.test:8443 "), "{at_site}");
    assert!(!at_site.contains("site.test"), "{at_site}");
    assert!(at_site.contains(&sha("m()")), "{at_site}");
    // Not a JavaScript MIME type essence: a data block.
    assert!(!at_site.contains(&sha("d()")), "{at_site}");
    assert!(at_site.contains(&sha("x()")), "{at_site}");
    assert!(
        at_site.contains(&sha("one();\ntwo();\nthree();")),
        "{at_site}"
    );
    // With no --origin, another origin's script is still that origin's.
    assert!(scripts(None).contains(" https://site.test"));
}

#[test]
fn the_web_shells_head_entries_are_found_between_its_sheet_and_its_root() {
    // host/web/build.mjs writes `host.web.head` just before the root.
    let shell = include_str!("../../web/index.html").replace(
        "<div id=\"exact-root\"></div>",
        "<script>gtag()</script>\n<div id=\"exact-root\"></div>",
    );
    let found = crate::page::head_scripts(&shell);
    assert_eq!(
        found.iter().map(|s| s.text.as_str()).collect::<Vec<_>>(),
        ["gtag()"]
    );
}

#[test]
fn a_string_that_says_exact_keeps_exacts_sources_and_the_head_scripts() {
    let headers = Dist::with(Some(r#"{"csp": {"script-src": "exact 'unsafe-eval'"}}"#))
        .headers()
        .unwrap();
    let scripts = directive(&headers, "Content-Security-Policy", "script-src").unwrap();
    assert!(
        scripts.starts_with("'self' 'wasm-unsafe-eval' 'sha256-"),
        "{scripts}"
    );
    assert!(scripts.contains(&sha("gtag('js')")), "{scripts}");
    assert!(
        scripts.ends_with(" https://cdn.test 'unsafe-eval'"),
        "{scripts}"
    );
}

#[test]
fn a_policy_file_that_is_not_an_object_or_cannot_be_read_fails_the_start() {
    assert!(Dist::with(Some("[]")).headers().is_err());
    assert!(Dist::with(Some("{\"csp\": {\"img-src\": \"caf\u{e9}\"}}"))
        .headers()
        .is_err());
    let dist = Dist::with(None);
    std::fs::create_dir(dist.0.join(".gen/csp.json")).unwrap();
    assert!(dist.headers().is_err());
}

#[test]
fn the_cache_budget_counts_each_page_s_variants() {
    let page = |body: usize| {
        CachedPage {
            target: "/p".into(),
            class: String::new(),
            created: Instant::now(),
            response: Response::text(200, "").header("ETag", "\"t\""),
            best: Arc::new(OnceLock::new()),
            hash: String::new(),
            pairs: HashMap::new(),
        }
        .with_body(body)
    };
    let mut pages = VecDeque::from([page(MAX_CACHE_BYTES - 100)]);
    assert!(!over(&pages, 0));
    // A pair against a dictionary counts, and a pair that didn't shrink doesn't.
    pages[0].pairs.insert("d".into(), Some(vec![0; 101]));
    pages[0].pairs.insert("e".into(), None);
    assert_eq!(pages[0].bytes(), MAX_CACHE_BYTES + 1);
    assert!(over(&pages, 0));
    // So do the best variants, once made.
    pages[0].pairs.clear();
    let body: Vec<u8> = (0..4096u32)
        .flat_map(|i| (i * 7919 % 251).to_le_bytes())
        .collect();
    let _ = pages[0].best.set(encode::Best::of(&body));
    let best = pages[0].best.get().unwrap().bytes();
    assert!(best > 0);
    assert_eq!(pages[0].bytes(), MAX_CACHE_BYTES - 100 + best);
    assert!(over(&pages, 0));
}

#[test]
fn invalid_response_headers_fail_before_any_bytes_are_written() {
    for status in [200, 304] {
        for value in [
            "noindex\r\n\r\ninjected",
            "noindex\nX-Fake: yes",
            "noindex\0",
        ] {
            let listener = TcpListener::bind("127.0.0.1:0").unwrap();
            let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
            let (mut server, _) = listener.accept().unwrap();
            Response::text(status, "original")
                .header("X-Robots-Tag", value)
                .write(
                    &mut server,
                    false,
                    "Content-Security-Policy: default-src 'self'\r\n",
                    "",
                    false,
                );
            server.shutdown(std::net::Shutdown::Write).unwrap();
            let mut received = String::new();
            client.read_to_string(&mut received).unwrap();
            assert!(received.starts_with("HTTP/1.1 500 "), "{received}");
            assert!(!received.contains("X-Robots-Tag"));
            assert!(!received.contains("injected"));
        }
    }
}

#[test]
fn a_page_carries_the_permissions_policy_its_grants_derive() {
    let policy = exact_runner::device::permissions_policy(
        "net.fetch https://x/\ndevice.microphone purpose.mic",
    );
    assert_eq!(
        policy,
        "microphone=(self), camera=(), geolocation=(), midi=(), accelerometer=(), gyroscope=(), magnetometer=(), serial=(), hid=(), usb=()"
    );
    for (content_type, expected) in [("text/html; charset=utf-8", true), ("text/plain", false)] {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
        let (mut server, _) = listener.accept().unwrap();
        let mut page = Response::text(200, "<p>");
        page.headers[0].1 = content_type.into();
        let lines = format!("Permissions-Policy: {policy}\r\n");
        page.write(
            &mut server,
            false,
            "Content-Security-Policy: default-src 'self'\r\n",
            &lines,
            false,
        );
        server.shutdown(std::net::Shutdown::Write).unwrap();
        let mut received = String::new();
        client.read_to_string(&mut received).unwrap();
        assert_eq!(
            received.contains(
                "\r\nPermissions-Policy: microphone=(self), camera=(), geolocation=(), midi=(), accelerometer=(), gyroscope=(), magnetometer=(), serial=(), hid=(), usb=()\r\n"
            ),
            expected,
            "{received}"
        );
    }
}

#[test]
fn a_request_head_that_drips_is_refused_at_its_bound() {
    // A hang bound, never a deadline.
    const BOUND: Duration = Duration::from_secs(60);
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (mut server, _) = listener.accept().unwrap();
    // The start of a head, then a byte of it every 20 ms: each read gets
    // one long before any idle timeout, and the head never ends.
    let stop = Arc::new(AtomicBool::new(false));
    let stopped = stop.clone();
    client.write_all(b"GET / HTTP/1.1\r\nX-Slow: ").unwrap();
    let dripping = std::thread::spawn(move || {
        let mut sent = 0;
        while !stopped.load(Ordering::SeqCst) && client.write_all(b"a").is_ok() {
            sent += 1;
            std::thread::sleep(Duration::from_millis(20));
        }
        sent
    });
    let (done, read) = std::sync::mpsc::channel();
    let reading = std::thread::spawn(move || {
        let _ = done.send(read_request(&mut server, Duration::from_millis(300)).is_err());
        server
    });
    let refused = read.recv_timeout(BOUND);
    // The peer was still sending when the read gave up on it.
    let still_sending = !dripping.is_finished();
    stop.store(true, Ordering::SeqCst);
    assert!(dripping.join().unwrap() > 0);
    assert_eq!(refused, Ok(true), "the head was still being read");
    assert!(still_sending);
    // A head that arrives whole inside the bound is read as before.
    drop(reading.join().unwrap());
    let mut client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    client.write_all(b"GET /a HTTP/1.1\r\n\r\n").unwrap();
    let (mut server, _) = listener.accept().unwrap();
    let request = read_request(&mut server, BOUND).unwrap();
    assert_eq!(
        (request.method.as_str(), request.target.as_str()),
        ("GET", "/a")
    );
}
