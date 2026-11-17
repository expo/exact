//! What an update takes from the client instead of the origin: an embedded
//! asset, or a file an earlier launch fetched, whose bytes are what the head
//! declares, and nothing else.
//!
//! @ref LLP 1026 D11 (assets by digest, embedded assets included)

use crate::support::*;
use exact_update::{sha256_hex, Check, Client, EmbeddedAsset, Outcome};

const CLIENT_ORIGIN: &str = "https://o.example";

/// The `compat.json` of a binary of cohort `id`.
fn compat(id: &str) -> String {
    format!(
        r#"{{"id":"{id}","inputs":{{"app":"com.exact.t","keys":null,"trust":"development","store":{{"L":"A"}}}},"delivery":{{"activate":"next-launch","channel":"prod","origin":"{CLIENT_ORIGIN}"}}}}"#
    )
}

/// Cohort `cohort`'s head at `seq` naming `plan` and an index of `assets`
/// (name, bytes), and every file the origin serves for it by URL.
fn publish(
    cohort: &str,
    seq: u64,
    plan: &[u8],
    assets: &[(&str, &[u8])],
) -> Vec<(String, Vec<u8>)> {
    let base = format!("{CLIENT_ORIGIN}/.exact/prod/{cohort}");
    let index = serde_json::to_vec(&serde_json::json!({
        "assets": assets.iter().map(|(name, bytes)| serde_json::json!({
            "name": name, "url": format!("./{name}"),
            "sha256": sha256_hex(bytes), "bytes": bytes.len(),
        })).collect::<Vec<_>>(),
    }))
    .unwrap();
    let head = serde_json::json!({
        "exact": 1, "app": {"id": "com.exact.t"},
        "plan": {"url": "./app.plan", "sha256": sha256_hex(plan), "bytes": plan.len()},
        "assets": {"url": "./assets.json", "sha256": sha256_hex(&index), "bytes": index.len(), "count": assets.len()},
        "stream": {"channel": "prod", "compatibilityId": cohort, "seq": seq},
    });
    let mut files = vec![
        (
            format!("{base}/exact.json"),
            serde_json::to_vec(&head).unwrap(),
        ),
        (format!("{base}/assets.json"), index),
        (format!("{base}/app.plan"), plan.to_vec()),
    ];
    files.extend(
        assets
            .iter()
            .map(|(name, bytes)| (format!("{base}/{name}"), bytes.to_vec())),
    );
    files
}

/// One check over `files`; returns its outcome and the last segment of each
/// URL fetched, in order.
fn check(client: &mut Client, files: &[(String, Vec<u8>)]) -> (Outcome, Vec<String>) {
    check_losing(client, files, None)
}

/// [`check`], with the connection lost on the URL ending in `lost`.
fn check_losing(
    client: &mut Client,
    files: &[(String, Vec<u8>)],
    lost: Option<&str>,
) -> (Outcome, Vec<String>) {
    let mut asked = Vec::new();
    let outcome = client.check(&mut |url| {
        asked.push(url.rsplit('/').next().unwrap().to_string());
        if lost.is_some_and(|lost| url.ends_with(lost)) {
            return Err(format!("{url}: the connection was lost"));
        }
        files
            .iter()
            .find(|(u, _)| u == url)
            .map(|(_, bytes)| bytes.clone())
            .ok_or_else(|| format!("404 {url}"))
    });
    (outcome, asked)
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Windows durable store is not qualified (LLP 1026 D11a)"
)]
fn the_first_update_takes_unchanged_assets_from_the_bundle() {
    let base = Temp::new("client-embedded-base");
    let bundle = Temp::new("client-embedded-bundle");
    let assets: [(&str, &[u8]); 3] = [
        ("assets/a1.png", b"one"),
        ("assets/a2.png", b"two"),
        ("assets/a3.png", b"three"),
    ];
    for (name, bytes) in assets {
        let path = bundle.path().join(name);
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, bytes).unwrap();
    }
    // The bundle's a3 has the head's length but other bytes: its name is
    // not enough, so it is fetched.
    std::fs::write(bundle.path().join("assets/a3.png"), b"THREE").unwrap();
    let mut client = Client::open_at(
        base.path(),
        bundle.path(),
        &compat("abc"),
        b"plan v1",
        Some(CLIENT_ORIGIN),
    )
    .unwrap();
    let (outcome, asked) = check(&mut client, &publish("abc", 1, b"plan v2", &assets));
    assert_eq!(
        (outcome.to_string(), asked),
        (
            "staged seq 1".to_string(),
            vec![
                "exact.json".into(),
                "assets.json".into(),
                "app.plan".into(),
                "a3.png".into()
            ]
        )
    );
    let candidate = client.prepare_activation().unwrap().unwrap();
    for (name, bytes) in assets {
        assert_eq!(
            candidate.assets.resolve(name).unwrap().as_deref(),
            Some(bytes),
            "{name}"
        );
    }
}

#[test]
#[cfg_attr(
    windows,
    ignore = "Windows durable store is not qualified (LLP 1026 D11a)"
)]
fn an_embedded_file_changed_after_it_was_hashed_is_fetched() {
    let temp = Temp::new("reuse-stale-digest");
    let bundle = Temp::new("reuse-stale-digest-bundle");
    let path = bundle.path().join("mark.png");
    // The host hashed "a mark"; the file now holds other bytes of that length.
    std::fs::write(&path, b"A MARK").unwrap();
    let mut origin = Origin::of(&Bundle::new(4, b"plan four").asset("mark.png", b"a mark"));
    let mut store = open(&temp);
    let files = &origin.files;
    let asked = &mut origin.asked;
    let mut fetch = |url: &str| -> Result<Vec<u8>, String> {
        asked.push(url.rsplit('/').next().unwrap().to_string());
        files.get(url).cloned().ok_or_else(|| format!("404 {url}"))
    };
    let mut embedded = |_: &str| {
        Some(EmbeddedAsset {
            sha256: sha256_hex(b"a mark"),
            path: path.clone(),
        })
    };
    let head = exact_update::head_url(ORIGIN, "release", COHORT);
    let Ok(Check::Staged { .. }) = store.check(&head, &mut fetch, &mut embedded) else {
        panic!("the head should have staged");
    };
    assert_eq!(
        origin.asked,
        ["exact.json", "assets.json", "app.plan", "mark.png"]
    );
    let candidate = store.prepare_activation().unwrap().unwrap();
    assert_eq!(
        candidate.assets.resolve("mark.png").unwrap().as_deref(),
        Some(b"a mark".as_slice())
    );
}

/// One launch of a binary of `cohort` over the store under `base`, with no
/// embedded assets. Drop it before the next launch opens, so that one owns
/// the store.
fn launch(base: &Temp, cohort: &str) -> Client {
    Client::open_at(
        base.path(),
        &base.path().join("no-bundle"),
        &compat(cohort),
        b"plan v1",
        Some(CLIENT_ORIGIN),
    )
    .unwrap()
}

/// The outcome without a refusal's reason, and what was fetched.
fn summary((outcome, asked): (Outcome, Vec<String>)) -> (String, Vec<String>) {
    let outcome = match outcome {
        Outcome::Refused(_) => "refused".to_string(),
        outcome => outcome.to_string(),
    };
    (outcome, asked)
}

fn expect(outcome: &str, asked: &[&str]) -> (String, Vec<String>) {
    (
        outcome.to_string(),
        asked.iter().map(|s| s.to_string()).collect(),
    )
}

/// A launch that ends before its download finishes keeps what it fetched:
/// the next launches fetch only what is missing, and fetch again a kept
/// file whose bytes no longer match the head.
#[test]
#[cfg_attr(
    windows,
    ignore = "Windows durable store is not qualified (LLP 1026 D11a)"
)]
fn an_interrupted_download_resumes_at_the_next_launch() {
    let base = Temp::new("reuse-resume");
    let assets: [(&str, &[u8]); 3] = [("a1", b"one"), ("a2", b"two"), ("a3", b"three")];
    let files = publish("abc", 1, b"plan v2", &assets);

    let mut client = launch(&base, "abc");
    assert_eq!(
        summary(check_losing(&mut client, &files, Some("/a3"))),
        expect(
            "refused",
            &["exact.json", "assets.json", "app.plan", "a1", "a2", "a3"]
        )
    );
    let blobs = client.dir().join("blobs");
    drop(client);

    // Between launches a2's kept file takes other bytes of the same length.
    std::fs::write(blobs.join(sha256_hex(b"two")), b"TWO").unwrap();
    let mut client = launch(&base, "abc");
    assert_eq!(
        summary(check_losing(&mut client, &files, Some("/a3"))),
        expect("refused", &["exact.json", "a2", "a3"])
    );
    drop(client);

    let mut client = launch(&base, "abc");
    assert_eq!(
        summary(check(&mut client, &files)),
        expect("staged seq 1", &["exact.json", "a3"])
    );
    let candidate = client.prepare_activation().unwrap().unwrap();
    for (name, bytes) in assets {
        assert_eq!(
            candidate.assets.resolve(name).unwrap().as_deref(),
            Some(bytes),
            "{name}"
        );
    }
}

/// A new binary over a store an old cohort filled fetches only its head
/// when the head names the files the old cohort fetched.
#[test]
#[cfg_attr(
    windows,
    ignore = "Windows durable store is not qualified (LLP 1026 D11a)"
)]
fn a_new_cohort_reuses_the_files_an_old_one_fetched() {
    let base = Temp::new("reuse-cohort");
    let assets: [(&str, &[u8]); 3] = [("a1", b"one"), ("a2", b"two"), ("a3", b"three")];
    let mut client = launch(&base, "abc");
    assert_eq!(
        summary(check(&mut client, &publish("abc", 1, b"plan v2", &assets))),
        expect(
            "staged seq 1",
            &["exact.json", "assets.json", "app.plan", "a1", "a2", "a3"]
        )
    );
    drop(client);

    let mut client = launch(&base, "def");
    assert_eq!(
        summary(check(&mut client, &publish("def", 1, b"plan v2", &assets))),
        expect("staged seq 1", &["exact.json"])
    );
    let candidate = client.prepare_activation().unwrap().unwrap();
    for (name, bytes) in assets {
        assert_eq!(
            candidate.assets.resolve(name).unwrap().as_deref(),
            Some(bytes),
            "{name}"
        );
    }
}

/// An asset index at the origin's root URL with a query and no path is the
/// index at `/?index=1`: its relative card resolves under `/`.
#[test]
#[cfg_attr(
    windows,
    ignore = "Windows durable store is not qualified (LLP 1026 D11a)"
)]
fn an_index_at_an_authority_and_a_query_resolves_its_cards() {
    let base = Temp::new("reuse-root-index");
    let mut client = launch(&base, "abc");
    let index = serde_json::to_vec(&serde_json::json!({"assets": [
        {"name": "asset.bin", "url": "asset.bin", "sha256": sha256_hex(b"one"), "bytes": 3},
    ]}))
    .unwrap();
    let head = serde_json::json!({
        "exact": 1, "app": {"id": "com.exact.t"},
        "plan": {"url": "./app.plan", "sha256": sha256_hex(b"plan v2"), "bytes": 7},
        "assets": {"url": format!("{CLIENT_ORIGIN}?index=1"), "sha256": sha256_hex(&index), "bytes": index.len(), "count": 1},
        "stream": {"channel": "prod", "compatibilityId": "abc", "seq": 1},
    });
    let files = [
        (
            format!("{CLIENT_ORIGIN}/.exact/prod/abc/exact.json"),
            serde_json::to_vec(&head).unwrap(),
        ),
        (format!("{CLIENT_ORIGIN}/?index=1"), index),
        (
            format!("{CLIENT_ORIGIN}/.exact/prod/abc/app.plan"),
            b"plan v2".to_vec(),
        ),
        (format!("{CLIENT_ORIGIN}/asset.bin"), b"one".to_vec()),
    ];
    assert_eq!(
        summary(check(&mut client, &files)),
        expect(
            "staged seq 1",
            &["exact.json", "?index=1", "app.plan", "asset.bin"]
        )
    );
}
