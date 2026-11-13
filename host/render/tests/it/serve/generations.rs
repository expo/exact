use super::*;
use sha2::{Digest, Sha256};

fn hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn launch(dist: &Path, kept: Option<&Path>, build: &[u8]) -> Served {
    super::super::warm_transport();
    std::fs::write(dist.join("app.wasm"), build).unwrap();
    run(Serve {
        dist: dist.into(),
        port: 0,
        name: "Blog".into(),
        origin: None,
        deadline: Duration::from_millis(300),
        renders: 1,
        queue: 8,
        viewport: Default::default(),
        lifetime: Duration::from_secs(120),
        generations: kept.map(Path::to_path_buf),
    })
}

#[test]
fn a_page_gets_its_retained_wasm_after_a_deploy() {
    let dist = dist("retained-wasm");
    let kept = dist.join("generations");
    let earlier = b"the first build";
    let current = b"the second build";
    let old_url = format!("/app.wasm?v={}", &hex(earlier)[..16]);
    let new_url = format!("/app.wasm?v={}", &hex(current)[..16]);
    let first = launch(&dist, Some(&kept), earlier);
    let (status, _, body) = fetch_bytes(first.addr, &format!("GET {old_url} HTTP/1.1\r\n\r\n"));
    assert_eq!(status, 200);
    assert_eq!(body, earlier);
    drop(first);
    let second = launch(&dist, Some(&kept), current);
    let mut etags = Vec::new();
    for (url, expected) in [
        (&old_url, earlier.as_slice()),
        (&new_url, current.as_slice()),
    ] {
        let (status, headers, body) =
            fetch_bytes(second.addr, &format!("GET {url} HTTP/1.1\r\n\r\n"));
        assert_eq!(status, 200);
        assert_eq!(body, expected, "a versioned request must get its own build");
        assert_eq!(
            header(&headers, "cache-control"),
            Some("public, max-age=31536000, immutable")
        );
        let tag = header(&headers, "etag").unwrap().to_string();
        let (status, headers, body) = fetch_bytes(
            second.addr,
            &format!("GET {url} HTTP/1.1\r\nIf-None-Match: W/{tag}\r\n\r\n"),
        );
        assert_eq!((status, body.len()), (304, 0));
        assert_eq!(header(&headers, "etag"), Some(tag.as_str()));
        assert_eq!(
            header(&headers, "cache-control"),
            Some("public, max-age=31536000, immutable")
        );
        let (status, headers, body) =
            fetch_bytes(second.addr, &format!("HEAD {url} HTTP/1.1\r\n\r\n"));
        assert_eq!((status, body.len()), (200, 0));
        assert_eq!(
            header(&headers, "content-length"),
            Some(expected.len().to_string().as_str())
        );
        etags.push(tag);
    }
    assert_ne!(etags[0], etags[1]);
    let (status, _, body) = fetch_bytes(
        second.addr,
        &format!(
            "GET {old_url} HTTP/1.1\r\nIf-None-Match: {}\r\n\r\n",
            etags[1]
        ),
    );
    assert_eq!(status, 200);
    assert_eq!(body, earlier);
    for (query, expected) in [
        ("v=0000000000000000".into(), 404),
        ("v=".into(), 400),
        ("v=not-a-version".into(), 400),
        ("v=0123456789abcdeg".into(), 400),
        (format!("v={0}&v={0}", &hex(current)[..16]), 400),
        (
            format!("v={}&%76={}", &hex(current)[..16], &hex(earlier)[..16]),
            400,
        ),
    ] {
        let (status, headers, _) = fetch_bytes(
            second.addr,
            &format!("GET /app.wasm?{query} HTTP/1.1\r\nIf-None-Match: *\r\n\r\n"),
        );
        assert_eq!(status, expected, "{query}");
        assert_eq!(header(&headers, "cache-control"), Some("no-store"));
        assert_eq!(header(&headers, "use-as-dictionary"), None);
        assert_eq!(header(&headers, "etag"), None);
    }
    drop(second);
    // Retention follows build order, independently of filesystem timestamp precision.
    let at = std::time::SystemTime::now() - Duration::from_secs(120);
    for (index, bytes) in [earlier.as_slice(), current.as_slice()].iter().enumerate() {
        std::fs::File::options()
            .write(true)
            .open(kept.join(format!("{}.wasm", hex(bytes))))
            .unwrap()
            .set_modified(at + Duration::from_secs(index as u64))
            .unwrap();
    }
    for build in [b"third".as_slice(), b"fourth", b"fifth"] {
        drop(launch(&dist, Some(&kept), build));
    }
    let fifth = launch(&dist, Some(&kept), b"fifth");
    let (status, headers, _) = get(fifth.addr, &old_url);
    assert_eq!(status, 404);
    assert_eq!(header(&headers, "cache-control"), Some("no-store"));
    assert!(!kept.join(format!("{}.wasm", hex(earlier))).exists());
    assert_eq!(get(fifth.addr, &new_url).0, 200);
    drop(fifth);
    let without_retention = launch(&dist, None, current);
    assert_eq!(get(without_retention.addr, &new_url).0, 200);
    assert_eq!(get(without_retention.addr, &old_url).0, 404);
    drop(without_retention);
    std::fs::remove_dir_all(dist).unwrap();
}

#[test]
fn only_verified_content_addressed_stages_are_immutable() {
    let dist = dist("immutable-stages");
    let stages = dist.join("stages");
    std::fs::create_dir_all(stages.join("nested")).unwrap();
    let bytes = b"a capability stage\n".repeat(200);
    let digest = hex(&bytes);
    let named = format!("inspection.{}.wasm", &digest[..16]);
    for file in [
        named.as_str(),
        "inspection.wasm",
        "inspection.0000000000000000.wasm",
        "inspection.0123456789abcdef0.wasm",
        &format!("nested/{named}"),
    ] {
        std::fs::write(stages.join(file), &bytes).unwrap();
    }
    std::fs::write(dist.join(&named), &bytes).unwrap();
    let served = launch(&dist, None, b"core");
    for (url, cache) in [
        (
            format!("/stages/{named}"),
            "public, max-age=31536000, immutable",
        ),
        ("/stages/inspection.wasm".into(), "no-cache"),
        (
            "/stages/inspection.0000000000000000.wasm".into(),
            "no-cache",
        ),
        (
            "/stages/inspection.0123456789abcdef0.wasm".into(),
            "no-cache",
        ),
        (format!("/stages/nested/{named}"), "no-cache"),
        (format!("/{named}"), "no-cache"),
    ] {
        let (status, headers, body) =
            fetch_bytes(served.addr, &format!("GET {url} HTTP/1.1\r\n\r\n"));
        assert_eq!(status, 200);
        assert_eq!(body, bytes);
        assert_eq!(header(&headers, "cache-control"), Some(cache), "{url}");
        assert_eq!(header(&headers, "use-as-dictionary"), None);
        let tag = header(&headers, "etag").unwrap();
        let (status, headers, body) = fetch_bytes(
            served.addr,
            &format!("GET {url} HTTP/1.1\r\nIf-None-Match: {tag}\r\n\r\n"),
        );
        assert_eq!((status, body.len()), (304, 0));
        assert_eq!(header(&headers, "cache-control"), Some(cache));
    }
    // The immutable decision uses the decoded bytes' digest for compressed variants too.
    let waited = std::time::Instant::now();
    loop {
        let (status, headers, body) = fetch_bytes(
            served.addr,
            &format!("GET /stages/{named} HTTP/1.1\r\nAccept-Encoding: gzip\r\n\r\n"),
        );
        assert_eq!(status, 200);
        if header(&headers, "content-encoding") == Some("gzip") {
            assert_eq!(
                header(&headers, "cache-control"),
                Some("public, max-age=31536000, immutable")
            );
            let mut decoded = Vec::new();
            flate2::read::GzDecoder::new(&body[..])
                .read_to_end(&mut decoded)
                .unwrap();
            assert_eq!(decoded, bytes);
            break;
        }
        assert!(
            waited.elapsed() < BOUND,
            "no compressed stage within {BOUND:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
    std::fs::write(stages.join(&named), b"different stage bytes").unwrap();
    let (_, headers, body) = fetch_bytes(
        served.addr,
        &format!("GET /stages/{named} HTTP/1.1\r\n\r\n"),
    );
    assert_eq!(body, b"different stage bytes");
    assert_eq!(header(&headers, "cache-control"), Some("no-cache"));
    drop(served);
    std::fs::remove_dir_all(dist).unwrap();
}

#[test]
fn a_named_build_goes_against_the_earlier_one_the_browser_holds() {
    // Two builds that differ in a few places, each incompressible alone.
    let mut seed = 7u32;
    let earlier: Vec<u8> = (0..200_000)
        .map(|_| {
            seed = seed.wrapping_mul(1_664_525).wrapping_add(1_013_904_223);
            (seed >> 24) as u8
        })
        .collect();
    let mut build = earlier.clone();
    for at in [10, 50_000, 120_000] {
        build.splice(at..at + 8, *b"changed!");
    }
    let dist = dist("generations");
    let kept = dist.join("generations");
    drop(launch(&dist, Some(&kept), &earlier));
    let served = launch(&dist, Some(&kept), &build);
    let addr = served.addr;
    // The server keeps the build it serves beside the earlier one.
    assert!(kept.join(format!("{}.wasm", hex(&build))).is_file());
    let named = format!("/app.wasm?v={}", &hex(&build)[..16]);
    let hash = exact_data::envelope::base64(&Sha256::digest(&earlier));
    let ask = |target: &str, extra: &str| {
        fetch_bytes(
            addr,
            &format!("GET {target} HTTP/1.1\r\nAccept-Encoding: gzip, br, dcb\r\nAvailable-Dictionary: :{hash}:\r\n{extra}\r\n"),
        )
    };
    // The URL names the build, so it caches for good, and a browser may keep
    // it as the next build's dictionary. The delta is made off the request's
    // path; until then the build goes as it otherwise would.
    let waited = std::time::Instant::now();
    let (headers, body) = loop {
        let (status, headers, body) = ask(&named, "");
        assert_eq!(status, 200);
        assert_eq!(
            header(&headers, "cache-control"),
            Some("public, max-age=31536000, immutable")
        );
        assert_eq!(
            header(&headers, "use-as-dictionary"),
            Some("match=\"/app.wasm\"")
        );
        assert_eq!(
            header(&headers, "vary"),
            Some("Accept-Encoding, Available-Dictionary")
        );
        if header(&headers, "content-encoding") == Some("dcb") {
            break (headers, body);
        }
        assert!(waited.elapsed() < BOUND, "no delta within {BOUND:?}");
        std::thread::sleep(Duration::from_millis(50));
    };
    assert_eq!(
        header(&headers, "vary"),
        Some("Accept-Encoding, Available-Dictionary")
    );
    let etag = header(&headers, "etag").unwrap().to_string();
    assert!(etag.ends_with("-dcb\""), "{etag}");
    assert_eq!(body[..4], [0xff, 0x44, 0x43, 0x42]);
    assert_eq!(body[4..36], Sha256::digest(&earlier)[..]);
    let mut decoded = Vec::new();
    brotli::Decompressor::new_with_custom_dict(&body[36..], 4096, earlier.clone().into())
        .read_to_end(&mut decoded)
        .unwrap();
    assert!(decoded == build, "the delta decodes to the build");
    assert!(body.len() < 1_000, "{} bytes", body.len());
    // Its ETag gets a 304.
    let (status, _, body) = ask(&named, &format!("If-None-Match: {etag}\r\n"));
    assert_eq!((status, body.len()), (304, 0));
    // An unversioned request revalidates and holds no dictionary.
    let (status, headers, _) = ask("/app.wasm", "");
    assert_eq!(
        (status, header(&headers, "cache-control")),
        (200, Some("no-cache"))
    );
    assert_eq!(header(&headers, "use-as-dictionary"), None);
    assert_ne!(header(&headers, "content-encoding"), Some("dcb"));
    let (_, headers, _) = ask(&named, "CDN-Loop: cloudflare\r\n");
    assert_eq!(header(&headers, "use-as-dictionary"), None);
    assert_ne!(header(&headers, "content-encoding"), Some("dcb"));
    let unknown = exact_data::envelope::base64(&[0u8; 32]);
    let (_, headers, _) = fetch_bytes(
        addr,
        &format!("GET {named} HTTP/1.1\r\nAccept-Encoding: dcb\r\nAvailable-Dictionary: :{unknown}:\r\n\r\n"),
    );
    assert_eq!(header(&headers, "content-encoding"), None);
    let old_url = format!("/app.wasm?v={}", &hex(&earlier)[..16]);
    let current_hash = exact_data::envelope::base64(&Sha256::digest(&build));
    let waited = std::time::Instant::now();
    let (headers, body) = loop {
        let (status, headers, body) = fetch_bytes(
            addr,
            &format!("GET {old_url} HTTP/1.1\r\nAccept-Encoding: br, dcb\r\nAvailable-Dictionary: :{current_hash}:\r\n\r\n"),
        );
        assert_eq!(status, 200);
        if header(&headers, "content-encoding") == Some("dcb") {
            break (headers, body);
        }
        assert!(
            waited.elapsed() < BOUND,
            "no retained delta within {BOUND:?}"
        );
        std::thread::sleep(Duration::from_millis(50));
    };
    let old_etag = header(&headers, "etag").unwrap();
    assert_ne!(old_etag, etag);
    assert_eq!(
        header(&headers, "cache-control"),
        Some("public, max-age=31536000, immutable")
    );
    let (status, headers, conditional) = fetch_bytes(
        addr,
        &format!("GET {old_url} HTTP/1.1\r\nAccept-Encoding: br, dcb\r\nAvailable-Dictionary: :{current_hash}:\r\nIf-None-Match: {old_etag}\r\n\r\n"),
    );
    assert_eq!((status, conditional.len()), (304, 0));
    assert_eq!(header(&headers, "etag"), Some(old_etag));
    let mut decoded = Vec::new();
    brotli::Decompressor::new_with_custom_dict(&body[36..], 4096, build.into())
        .read_to_end(&mut decoded)
        .unwrap();
    assert!(
        decoded == earlier,
        "a retained build's delta decodes to that build"
    );
    drop(served);
    std::fs::remove_dir_all(dist).unwrap();
}
