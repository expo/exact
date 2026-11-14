use super::*;
use std::fs;
#[cfg(unix)]
use std::os::unix::fs::symlink;
#[cfg(unix)]
use std::path::Path;
use std::path::PathBuf;
#[cfg(unix)]
use std::sync::atomic::AtomicBool;
use std::sync::atomic::{AtomicU64, Ordering};
#[cfg(unix)]
use std::sync::Arc;

pub(super) const TOKEN: &str = "0123456789abcdef0123456789abcdef0123456789abcdef";
pub(super) struct Fixture(PathBuf);
impl Fixture {
    pub(super) fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let path = std::env::temp_dir().join(format!(
            "exact-fs-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, Ordering::Relaxed)
        ));
        fs::create_dir(&path).unwrap();
        Self(path)
    }
    pub(super) fn path(&self, name: &str) -> PathBuf {
        self.0.join(name)
    }
    pub(super) fn dir(&self, name: &str) -> Directory {
        Directory::root(self.path(name).to_str().unwrap(), true).unwrap()
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn static_trees_omit_finder_metadata_and_keep_other_dotfiles() {
    let fixture = Fixture::new();
    let root = fixture.dir("assets");
    fixture.dir("assets/icons");
    fixture.dir("assets/.well-known");
    for name in [
        ".DS_Store",
        "icons/.DS_Store",
        ".well-known/.DS_Store",
        ".config",
        ".well-known/data.json",
        "icons/a.png",
        "mark.png",
        "mark.DS_Store",
    ] {
        fs::write(fixture.path(&format!("assets/{name}")), name).unwrap();
    }
    let names = [
        ".config",
        ".well-known/data.json",
        "icons/a.png",
        "mark.DS_Store",
        "mark.png",
    ];
    for op in ["names", "tree", "inventory"] {
        let entries = operate(&root, &json!({"op":op}), None).unwrap();
        assert_eq!(
            entries.as_object().unwrap().keys().collect::<Vec<_>>(),
            names,
            "{op}"
        );
        for name in names {
            let expected = if op == "tree" {
                json!(STANDARD.encode(name))
            } else if op == "inventory" {
                json!({"sha256": format!("{:x}", Sha256::digest(name.as_bytes())), "bytes": name.len()})
            } else {
                Value::Null
            };
            assert_eq!(entries[name], expected);
        }
    }
    assert!(root.names().unwrap().contains(&".DS_Store".to_owned()));
    let copy = fixture.path("copy");
    operate(
        &root,
        &json!({"op":"copy", "target":copy, "token":TOKEN}),
        None,
    )
    .unwrap();
    for name in names {
        assert_eq!(fs::read(copy.join(name)).unwrap(), name.as_bytes());
    }
    for name in [".DS_Store", "icons/.DS_Store", ".well-known/.DS_Store"] {
        assert!(!copy.join(name).exists(), "{name} was copied");
    }
}

#[test]
fn a_copy_into_its_own_source_copies_what_the_source_held() {
    let fixture = Fixture::new();
    let root = fixture.dir("src");
    fixture.dir("src/deck");
    fs::write(fixture.path("src/a.txt"), "a").unwrap();
    fs::write(fixture.path("src/deck/b.txt"), "b").unwrap();
    let target = fixture.path("src/output");
    operate(
        &root,
        &json!({"op":"copy", "target":target, "token":TOKEN}),
        None,
    )
    .unwrap();
    assert_eq!(fs::read(target.join("a.txt")).unwrap(), b"a");
    assert_eq!(fs::read(target.join("deck/b.txt")).unwrap(), b"b");
    let copied: Vec<_> = fs::read_dir(&target)
        .unwrap()
        .map(|e| e.unwrap().file_name())
        .collect();
    assert_eq!(
        copied.len(),
        2,
        "{copied:?}: the target holds no copy of itself"
    );
}

#[test]
#[cfg(unix)]
fn a_static_inventory_refuses_a_fifo() {
    let fixture = Fixture::new();
    let root = fixture.dir("assets");
    let path = std::ffi::CString::new(fixture.path("assets/pipe").to_str().unwrap()).unwrap();
    // SAFETY: path is a live, NUL-terminated name in this test's private directory.
    assert_eq!(unsafe { libc::mkfifo(path.as_ptr(), 0o600) }, 0);
    for op in ["tree", "inventory"] {
        let error = operate(&root, &json!({"op":op}), None).unwrap_err();
        assert!(error.to_string().contains("regular files"), "{error}");
    }
}

#[test]
fn retained_generation_quota_is_atomic_and_never_prunes() {
    let fixture = Fixture::new();
    let root = fixture.dir("cache");
    let request = |seq, quota| {
        json!({"op":"retain", "path":format!("{}/{seq}", "a".repeat(32)),
        "quota":quota, "token":TOKEN,
        "files":{"app.plan":STANDARD.encode("plan"), "deck/nested/page.html":STANDARD.encode("old guest"), "exact.json":STANDARD.encode("envelope")}})
    };
    let first = request(1, 1024);
    let used = operate(&root, &first, None).unwrap()["bytes"]
        .as_u64()
        .unwrap();
    assert_eq!(used, 68 + 4 + 9 + 8);
    assert_eq!(
        operate(&root, &request(1, used), None).unwrap()["bytes"],
        used
    );
    assert!(operate(&root, &request(2, used + 1), None)
        .unwrap_err()
        .to_string()
        .contains("quota exceeded"));
    assert!(!fixture
        .path(&format!("cache/{}/2", "a".repeat(32)))
        .exists());
    assert_eq!(
        fs::read(fixture.path(&format!("cache/{}/1/deck/nested/page.html", "a".repeat(32))))
            .unwrap(),
        b"old guest"
    );
    let mut changed = first.clone();
    changed["files"]["deck/nested/page.html"] = json!(STANDARD.encode("other"));
    assert!(operate(&root, &changed, None).is_err());
    let lock = Lock::acquire(&root, ".retained/.lock", TOKEN).unwrap();
    assert!(operate(&root, &request(2, 1024), None).is_err());
    drop(lock);
    assert!(operate(&root, &request(2, 1024), None).is_ok());
}

#[test]
fn retained_blobs_are_stored_once_and_named_by_their_digest() {
    let fixture = Fixture::new();
    let root = fixture.dir("cache");
    let digest = format!("{:x}", Sha256::digest("payload"));
    let request = |seq, files: Value, blobs: Value| {
        let mut input = json!({"op":"retain", "path":format!("{}/{seq}", "e".repeat(32)),
            "quota":1024, "token":TOKEN, "blobs":blobs});
        if !files.is_null() {
            input["files"] = files;
        }
        operate(&root, &input, None)
    };
    let envelope = json!({"exact.json":STANDARD.encode("envelope")});
    let blob = json!({ digest.as_str(): STANDARD.encode("payload") });
    // Blobs may precede the envelope in a request of their own.
    let part = request(1, Value::Null, blob.clone()).unwrap()["bytes"]
        .as_u64()
        .unwrap();
    assert_eq!(part, 68 + 7);
    assert!(!fixture
        .path(&format!("cache/{}/1", "e".repeat(32)))
        .exists());
    let first = request(1, envelope.clone(), blob.clone()).unwrap()["bytes"]
        .as_u64()
        .unwrap();
    let second = request(2, envelope, blob).unwrap()["bytes"]
        .as_u64()
        .unwrap();
    assert_eq!((first, second), (part + 8, part + 16));
    assert_eq!(
        fs::read(fixture.path(&format!("cache/blobs/{digest}"))).unwrap(),
        b"payload"
    );
    let wrong = json!({ digest.as_str(): STANDARD.encode("other") });
    assert!(request(3, Value::Null, wrong)
        .unwrap_err()
        .to_string()
        .contains("digest"));
    assert!(request(3, Value::Null, json!({}))
        .unwrap_err()
        .to_string()
        .contains("missing files"));
    assert!(
        request(3, json!({"app.plan":STANDARD.encode("plan")}), json!({}))
            .unwrap_err()
            .to_string()
            .contains("envelope")
    );
}

#[test]
fn a_publication_is_admitted_whole_and_releases_the_blobs_it_abandons() {
    let fixture = Fixture::new();
    let root = fixture.dir("cache");
    let retain = |seq, extra: Value| {
        let mut input = json!({"op":"retain", "path":format!("{}/{seq}", "f".repeat(32)),
            "quota":400, "token":TOKEN});
        for (key, value) in extra.as_object().unwrap() {
            input[key] = value.clone();
        }
        operate(&root, &input, None)
    };
    let (a, b) = (
        format!("{:x}", Sha256::digest("aaaa")),
        format!("{:x}", Sha256::digest("bbbb")),
    );
    let blob = |digest: &str| fixture.path(&format!("cache/blobs/{digest}"));
    let generation = |digest: &str| {
        let index = json!({"assets":[{"sha256":digest}]}).to_string();
        json!({"assets.json":STANDARD.encode(index), "exact.json":STANDARD.encode("envelope")})
    };
    // The first batch fits alone, not with the bytes still to come: nothing is written.
    let first = json!({"blobs":{ a.as_str(): STANDARD.encode("aaaa") }, "reserve":400});
    assert!(retain(1, first)
        .unwrap_err()
        .to_string()
        .contains("quota exceeded"));
    assert!(!blob(&a).exists());
    let published =
        json!({"blobs":{ a.as_str(): STANDARD.encode("aaaa") }, "files":generation(&a)});
    retain(1, published).unwrap();
    retain(2, json!({"blobs":{ b.as_str(): STANDARD.encode("bbbb") }})).unwrap();
    // Only the blob no published generation names is released.
    let release = json!({"op":"release", "token":TOKEN, "blobs":[a, b]});
    assert_eq!(operate(&root, &release, None).unwrap(), json!(1));
    assert!(blob(&a).exists() && !blob(&b).exists());
    // An envelope whose index names a released blob is refused.
    assert!(retain(2, json!({"files":generation(&b)}))
        .unwrap_err()
        .to_string()
        .contains("missing blob"));
    assert!(!fixture
        .path(&format!("cache/{}/2/exact.json", "f".repeat(32)))
        .exists());
}

#[test]
fn a_tree_page_reads_to_its_limit_and_names_the_rest() {
    let fixture = Fixture::new();
    let root = fixture.dir("tree");
    fixture.dir("tree/nested");
    for (name, body) in [("a", "12345"), ("nested/b", "678"), ("nested/c", "9")] {
        fs::write(fixture.path(&format!("tree/{name}")), body).unwrap();
    }
    let first = operate(&root, &json!({"op":"tree", "limit":6}), None).unwrap();
    assert_eq!(
        first,
        json!({"a":STANDARD.encode("12345"), "nested/b":STANDARD.encode("678"), "nested/c":null})
    );
    let rest = operate(
        &root,
        &json!({"op":"tree", "limit":6, "paths":["nested/c"]}),
        None,
    )
    .unwrap();
    assert_eq!(rest, json!({"nested/c":STANDARD.encode("9")}));
    assert!(operate(&root, &json!({"op":"tree", "paths":["../tree/a"]}), None).is_err());
    assert!(operate(&root, &json!({"op":"tree", "paths":["absent"]}), None).is_err());
}

#[test]
fn retained_payload_failure_never_publishes_the_envelope() {
    let fixture = Fixture::new();
    let root = fixture.dir("cache");
    let epoch = "c".repeat(32);
    let request = |sequence, files| {
        json!({"op":"retain", "path":format!("{epoch}/{sequence}"),
        "quota":4096, "token":TOKEN, "files":files})
    };
    let good = json!({"a":STANDARD.encode("first"), "b":STANDARD.encode("second"),
        "c":STANDARD.encode("third"), "d":STANDARD.encode("fourth"),
        "exact.json":STANDARD.encode("complete")});
    operate(&root, &request(0, good.clone()), None).unwrap();
    // Both absent names pass quota admission. Publication must fail because
    // one payload needs a directory where the other needs a regular file.
    let bad = json!({"collision":STANDARD.encode("file"),
        "collision/child":STANDARD.encode("child"), "independent":STANDARD.encode("partial"),
        "exact.json":STANDARD.encode("must not publish")});
    assert!(operate(&root, &request(1, bad), None).is_err());
    assert!(!fixture
        .path(&format!("cache/{epoch}/1/exact.json"))
        .exists());
    assert_eq!(
        fs::read(fixture.path(&format!("cache/{epoch}/0/exact.json"))).unwrap(),
        b"complete"
    );
    assert_eq!(
        fs::read(fixture.path(&format!("cache/{epoch}/0/b"))).unwrap(),
        b"second"
    );
    // Failure releases the single publisher lock, retains the partial quota
    // usage, and leaves every temporary name cleaned up before returning.
    let used = operate(&root, &request(2, good.clone()), None).unwrap();
    assert_eq!(used["bytes"], retained_bytes(&root).unwrap());
    assert_eq!(operate(&root, &request(2, good), None).unwrap(), used);
    // Reuse an already-linked payload from the failed batch. The retry must
    // include its held directory in durability completion before its envelope.
    let recovered = json!({"independent":STANDARD.encode("partial"),
        "exact.json":STANDARD.encode("recovered")});
    operate(&root, &request(1, recovered), None).unwrap();
    assert_eq!(
        fs::read(fixture.path(&format!("cache/{epoch}/1/exact.json"))).unwrap(),
        b"recovered"
    );
    let mut names = serde_json::Map::new();
    tree(&root, "", &mut names, false).unwrap();
    assert!(names
        .keys()
        .all(|path| !path.split('/').any(|part| part.starts_with(".tmp-"))));
}

#[test]
#[cfg(unix)]
fn retained_completion_counts_require_the_last_successful_holder() {
    let fixture = Fixture::new();
    let root = fixture.dir("cache");
    let epoch = "d".repeat(32);
    let other = "abcdef0123456789abcdef0123456789abcdef0123456789";
    let request = |seq, token, previous| {
        json!({"op":"retain", "path":format!("{epoch}/{seq}"),
        "quota":4096, "token":token, "previousToken":previous,
        "files":{"payload":STANDARD.encode("payload"), "exact.json":STANDARD.encode("complete")}})
    };
    let first = operate(&root, &request(0, TOKEN, Value::Null), None).unwrap();
    assert_eq!(first["bytes"], retained_bytes(&root).unwrap());
    assert_eq!(first["token"], TOKEN);
    // A different publisher completes under the same lock. Its extra files
    // must count even when the original publisher supplies its stale token.
    operate(&root, &request(1, other, Value::Null), None).unwrap();
    let next = operate(&root, &request(2, TOKEN, json!(TOKEN)), None).unwrap();
    assert_eq!(next["bytes"], retained_bytes(&root).unwrap());
    let held = Lock::acquire(&root, ".retained/.lock", other).unwrap();
    assert!(operate(&root, &request(3, TOKEN, json!(TOKEN)), None).is_err());
    drop(held);
    let next = operate(&root, &request(3, TOKEN, json!(TOKEN)), None).unwrap();
    assert_eq!(next["bytes"], retained_bytes(&root).unwrap());
    // Failure after a payload write invalidates the count, even if a caller
    // reuses the same token. A fresh scan includes the abandoned partial bytes.
    let mut failed = request(4, TOKEN, json!(TOKEN));
    failed["files"] = json!({"a-partial":STANDARD.encode("partial"),
        "collision":STANDARD.encode("file"), "collision/child":STANDARD.encode("child"),
        "exact.json":STANDARD.encode("absent")});
    assert!(operate(&root, &failed, None).is_err());
    assert_eq!(
        fs::metadata(fixture.path("cache/.retained/.lock"))
            .unwrap()
            .len(),
        48
    );
    let next = operate(&root, &request(5, TOKEN, json!(TOKEN)), None).unwrap();
    assert_eq!(next["bytes"], retained_bytes(&root).unwrap());
    let mut quota_refusal = request(6, TOKEN, json!(TOKEN));
    quota_refusal["quota"] = json!(next["bytes"].as_u64().unwrap());
    assert!(operate(&root, &quota_refusal, None).is_err());
    assert_eq!(
        fs::metadata(fixture.path("cache/.retained/.lock"))
            .unwrap()
            .len(),
        48
    );
    // A malformed fixed-width completion cannot be used as accounting.
    fs::write(
        fixture.path("cache/.retained/.lock"),
        format!("{TOKEN}{}", "x".repeat(20)),
    )
    .unwrap();
    let next = operate(&root, &request(6, TOKEN, json!(TOKEN)), None).unwrap();
    assert_eq!(next["bytes"], retained_bytes(&root).unwrap());
    // Untouched historical corruption is discovered on reads or the next full
    // scan, not by a matching completion. New writes still use nofollow handles.
    let linked = fixture.path(&format!("cache/{epoch}/0/corrupt"));
    symlink(fixture.path("outside"), &linked).unwrap();
    operate(&root, &request(7, TOKEN, json!(TOKEN)), None).unwrap();
    assert!(operate(&root, &request(8, TOKEN, Value::Null), None).is_err());
    assert!(root
        .parent(&format!("{epoch}/0/corrupt"), false)
        .unwrap()
        .0
        .read("corrupt")
        .is_err());
    fs::remove_file(linked).unwrap();
    let next = operate(&root, &request(8, TOKEN, json!(TOKEN)), None).unwrap();
    assert_eq!(next["bytes"], retained_bytes(&root).unwrap());
}

#[test]
#[cfg(unix)]
fn retained_generations_cannot_bypass_heads_or_symlink_gate() {
    let fixture = Fixture::new();
    let root = fixture.dir("cache");
    fixture.dir("outside");
    let epoch = "a".repeat(32);
    symlink(
        fixture.path("outside"),
        fixture.path(&format!("cache/{epoch}")),
    )
    .unwrap();
    for path in [
        format!("{epoch}/1"),
        ".exact/prod/cohort".into(),
        "../escape".into(),
    ] {
        assert!(operate(
            &root,
            &json!({"op":"retain", "path":path, "quota":4096, "token":TOKEN,
            "files":{"exact.json":STANDARD.encode("head")}}),
            None
        )
        .is_err());
    }
    assert!(fs::read_dir(fixture.path("outside"))
        .unwrap()
        .next()
        .is_none());
}

#[test]
#[cfg(unix)]
fn held_parent_cannot_be_redirected_after_validation() {
    let fixture = Fixture::new();
    let root = fixture.dir("origin");
    let outside = fixture.dir("outside");
    outside.write("secret", b"private", false, TOKEN).unwrap();
    let (parent, leaf) = root.parent("nested/value", true).unwrap();
    fs::rename(fixture.path("origin/nested"), fixture.path("origin/held")).unwrap();
    symlink(fixture.path("outside"), fixture.path("origin/nested")).unwrap();
    parent.write(&leaf, b"public", false, TOKEN).unwrap();
    assert_eq!(
        fs::read(fixture.path("origin/held/value")).unwrap(),
        b"public"
    );
    assert!(!fixture.path("outside/value").exists());
    assert!(root.parent("nested/secret", false).is_err());
    assert_eq!(outside.read("secret").unwrap(), b"private");
}

#[test]
#[cfg(unix)]
fn replaced_lock_and_head_are_refused_before_commit() {
    let fixture = Fixture::new();
    let root = fixture.dir("origin");
    let lock = Lock::acquire(&root, ".exact/prod/cohort/.lock", TOKEN).unwrap();
    assert!(Lock::acquire(&root, ".exact/prod/cohort/.lock", TOKEN).is_err());
    let (dir, leaf) = root.parent(&lock.head, false).unwrap();
    dir.write(&leaf, b"old", false, TOKEN).unwrap();
    let result = dir.write_checked(&leaf, b"candidate", false, TOKEN, || {
        fs::write(fixture.path("origin/.exact/prod/cohort/exact.json"), b"other publisher").unwrap();
        let request = json!({"op":"head", "path":lock.head, "bytes":STANDARD.encode("candidate"),
            "previousDigest":format!("{:x}", Sha256::digest(b"old")), "token":"abcdef0123456789abcdef0123456789abcdef0123456789"});
        operate(&root, &request, Some(&lock)).map(|_| ())
    });
    assert!(result.is_err());
    assert_eq!(dir.read(&leaf).unwrap(), b"other publisher");
    let lock_path = fixture.path("origin/.exact/prod/cohort/.lock");
    fs::rename(&lock_path, fixture.path("old-lock")).unwrap();
    let successor = Lock::acquire(&root, ".exact/prod/cohort/.lock", "successor").unwrap();
    assert!(lock.verify(&root).is_err());
    drop(lock);
    assert!(successor.verify(&root).is_ok());
    assert!(Lock::acquire(&root, ".exact/prod/cohort/.lock", TOKEN).is_err());
    drop(successor);
    assert!(Lock::acquire(&root, ".exact/prod/cohort/.lock", TOKEN).is_ok());
}

// Atomic exchange gives the adversary a continuously present path: no test
// can pass merely because every operation caught a missing-name gap.
#[cfg(unix)]
fn exchange(left: &Path, right: &Path) {
    use std::ffi::CString;
    let left = CString::new(left.to_str().unwrap()).unwrap();
    let right = CString::new(right.to_str().unwrap()).unwrap();
    #[cfg(target_os = "macos")]
    let result = unsafe {
        libc::renameatx_np(
            libc::AT_FDCWD,
            left.as_ptr(),
            libc::AT_FDCWD,
            right.as_ptr(),
            libc::RENAME_SWAP,
        )
    };
    #[cfg(target_os = "linux")]
    let result = unsafe {
        libc::renameat2(
            libc::AT_FDCWD,
            left.as_ptr(),
            libc::AT_FDCWD,
            right.as_ptr(),
            libc::RENAME_EXCHANGE,
        )
    };
    assert_eq!(result, 0, "{}", io::Error::last_os_error());
}

#[test]
#[cfg(unix)]
fn concurrent_intermediate_symlinks_never_supply_or_receive_bytes() {
    let fixture = Fixture::new();
    let root = fixture.dir("origin");
    let legitimate = fixture.dir("origin/.exact");
    legitimate.write("value", b"public", false, TOKEN).unwrap();
    let outside = fixture.dir("outside");
    outside.write("value", b"secret", false, TOKEN).unwrap();
    outside.child("private-only", true).unwrap();
    symlink(fixture.path("outside"), fixture.path("swap")).unwrap();
    let stop = Arc::new(AtomicBool::new(false));
    let ready = Arc::new(AtomicBool::new(false));
    let adversary = {
        let stop = stop.clone();
        let ready = ready.clone();
        let left = fixture.path("origin/.exact");
        let right = fixture.path("swap");
        std::thread::spawn(move || {
            let mut exchanges = 0;
            while !stop.load(Ordering::Relaxed) {
                exchange(&left, &right);
                exchange(&left, &right);
                exchanges += 2;
                ready.store(true, Ordering::Release);
            }
            exchanges
        })
    };
    while !ready.load(Ordering::Acquire) {
        std::thread::yield_now();
    }
    let mut reads = 0;
    for _ in 0..2000 {
        if let Ok(value) = operate(&root, &json!({"op":"get","path":".exact/value"}), None) {
            assert_eq!(value, STANDARD.encode("public"));
            reads += 1;
        }
        if let Ok(names) = operate(&root, &json!({"op":"list","path":".exact"}), None) {
            assert!(!names.as_array().unwrap().contains(&json!("private-only")));
        }
        let _ = operate(
            &root,
            &json!({"op":"put","path":".exact/new/leaf", "bytes":STANDARD.encode("safe"), "token":TOKEN}),
            None,
        );
        if let Ok(source) = Directory::root(fixture.path("origin/.exact").to_str().unwrap(), false)
        {
            let captured = operate(&source, &json!({"op":"tree"}), None).unwrap();
            assert_eq!(captured["value"], STANDARD.encode("public"));
            assert!(captured.get("private-only").is_none());
        }
        if let Ok(lock) = Lock::acquire(&root, ".exact/prod/cohort/.lock", TOKEN) {
            let _ = operate(
                &root,
                &json!({"op":"head","path":lock.head,"bytes":STANDARD.encode("head"),"previousDigest":null,"token":TOKEN}),
                Some(&lock),
            );
        }
    }
    stop.store(true, Ordering::Relaxed);
    assert!(adversary.join().unwrap() > 0);
    assert!(
        reads > 0,
        "the race must also exercise successfully opened owned handles"
    );
    assert_eq!(outside.names().unwrap(), vec!["private-only", "value"]);
    assert_eq!(outside.read("value").unwrap(), b"secret");
}

#[test]
#[cfg(unix)]
fn resident_reader_reopens_roots_and_refuses_mutations() {
    let fixture = Fixture::new();
    let path = fixture.path("root");
    fs::create_dir(&path).unwrap();
    fs::write(path.join("value"), "before").unwrap();
    let input = json!({"op":"get","root":path,"path":"value"});
    assert_eq!(read_request(&input).unwrap(), STANDARD.encode("before"));
    fs::rename(&path, fixture.path("old")).unwrap();
    fs::create_dir(&path).unwrap();
    fs::write(path.join("value"), "after").unwrap();
    assert_eq!(read_request(&input).unwrap(), STANDARD.encode("after"));
    assert!(read_request(&json!({"op":"put","root":path,"path":"value"})).is_err());
    fs::remove_file(path.join("value")).unwrap();
    symlink(fixture.path("old/value"), path.join("value")).unwrap();
    assert!(read_request(&input).is_err());
}

#[test]
fn puts_cannot_replace_case_variants_of_a_stream_head_or_lock() {
    let fixture = Fixture::new();
    let root = fixture.dir("origin");
    for prefix in [".exact", ".EXACT"] {
        for name in [
            "exact.json",
            "Exact.json",
            "EXACT.JSON",
            ".lock",
            ".Lock",
            ".LOCK",
        ] {
            let error = operate(&root, &json!({"op":"put", "path":format!("{prefix}/prod/cohort/{name}"), "bytes":STANDARD.encode("forged"), "token":TOKEN}), None).unwrap_err();
            assert!(
                error.to_string().contains("stream heads require putHead"),
                "{error}"
            );
        }
    }
    operate(&root, &json!({"op":"put", "path":".exact/blobs/abc", "bytes":STANDARD.encode("blob"), "immutable":true, "token":TOKEN}), None).unwrap();
}

/// A spelling the volume's lookup folds onto the head or lock (APFS folds
/// `ſ` to `s` and the Kelvin sign to `k`; NTFS matches 8.3 short names) is
/// the same file, and a put of it is refused. On a volume that does not fold,
/// the put is a file of its own.
#[test]
fn puts_cannot_replace_a_stream_head_or_lock_by_a_folded_spelling() {
    let fixture = Fixture::new();
    let root = fixture.dir("origin");
    let cohort = fixture.path("origin/.exact/prod/cohort");
    fs::create_dir_all(&cohort).unwrap();
    fs::write(cohort.join("exact.json"), "head").unwrap();
    fs::write(cohort.join(".lock"), "lock").unwrap();
    for name in ["exact.j\u{17f}on", ".loc\u{212a}"] {
        let folds = cohort.join(name).exists();
        let put = operate(
            &root,
            &json!({"op":"put", "path":format!(".exact/prod/cohort/{name}"), "bytes":STANDARD.encode("forged"), "token":TOKEN}),
            None,
        );
        assert_eq!(put.is_err(), folds, "{name}: {put:?}");
        assert_eq!(
            fs::read_to_string(cohort.join("exact.json")).unwrap(),
            "head"
        );
        assert_eq!(fs::read_to_string(cohort.join(".lock")).unwrap(), "lock");
    }
}
