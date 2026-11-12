use super::*;

#[test]
fn storage_and_source_changes_are_consumed_together() {
    let paths = Paths::new();
    let mut host = Storage::new(Fixture::new());
    paths.configure(&mut host);
    host.activate().unwrap();
    host.source.changed = true;
    assert!(host.take_storage_changed());
    assert!(!host.take_storage_changed());
    for _ in 0..2 {
        let mut store = Store::default();
        let Answer::Later(request) = host.answer(&mut store, "write", &[]).unwrap() else {
            panic!("expected storage request")
        };
        let Dispatch::Run(exact_runner::Work::Now(job)) =
            host.dispatch(request.continuation.unwrap(), &store)
        else {
            panic!("expected storage worker")
        };
        assert!(!host.take_storage_changed());
        let outcome = std::thread::spawn(job).join().unwrap();
        assert_eq!(std::fs::read(paths.0.join("data/note")).unwrap(), b"hello");
        host.source.changed = true;
        // A committed write is visible even before the source parses its reply.
        assert!(host.take_storage_changed());
        assert!(!host.source.changed);
        assert!(!host.take_storage_changed());
        host.parse(&mut store, "write", &[], outcome).unwrap();
        assert!(!host.take_storage_changed());
    }
}

#[test]
fn sqlite_changes_do_not_invalidate_file_snapshots() {
    let paths = Paths::new();
    let mut host = Storage::new(Fixture::new());
    paths.configure(&mut host);
    host.activate().unwrap();
    sql(
        &mut host,
        "sqlite",
        json!([{"kind":"execute","sql":"CREATE TABLE items(id INTEGER PRIMARY KEY)","params":[]}]),
    )
    .unwrap();
    assert!(!host.take_storage_changed(), "a refresh cannot read SQLite");
    sql(
        &mut host,
        "sqlite.transaction",
        json!([{"kind":"execute","sql":"INSERT INTO items VALUES (1)","params":[]}]),
    )
    .unwrap();
    assert!(!host.take_storage_changed());
    assert!(sql(
        &mut host,
        "sqlite",
        json!([
            {"kind":"execute","sql":"INSERT INTO items VALUES (2)","params":[]},
            {"kind":"execute","sql":"INSERT INTO items VALUES (1)","params":[]}
        ]),
    )
    .is_err());
    let rows = sql(
        &mut host,
        "sqlite",
        json!([{"kind":"query","sql":"SELECT id FROM items ORDER BY id","params":[]}]),
    )
    .unwrap();
    assert_eq!(
        rows[0]["rows"],
        json!([[{"integer":"1"}], [{"integer":"2"}]])
    );
    assert!(
        !host.take_storage_changed(),
        "partial commits stay SQLite-only"
    );
}

#[test]
fn only_readable_app_file_changes_invalidate_snapshots() {
    let paths = Paths::new();
    let mut host = Storage::new(Fixture {
        grants: "fs.read app:/data/note\nfs.read app:/data/tree/leaf\nfs.write app:/data\nfs.write app:/cache",
        ..Fixture::new()
    });
    paths.configure(&mut host);
    host.activate().unwrap();
    std::fs::create_dir_all(paths.0.join("data/tree")).unwrap();
    std::fs::write(paths.0.join("data/tree/leaf"), "leaf").unwrap();
    for (op, args, changed) in [
        (
            "fs.writeFile",
            json!({"path":"app:/data/note","text":"hello"}),
            true,
        ),
        (
            "fs.atomicWriteFile",
            json!({"path":"app:/cache/hidden","text":"hidden"}),
            false,
        ),
        (
            "fs.appendFile",
            json!({"path":"app:/data/note","text":"!"}),
            true,
        ),
        ("fs.mkdir", json!({"path":"app:/cache/folder"}), false),
        ("fs.mkdir", json!({"path":"app:/data/tree"}), true),
        (
            "fs.writeFile",
            json!({"path":"app:/data/noteworthy","text":"sibling"}),
            false,
        ),
        (
            "fs.chmod",
            json!({"path":"app:/data/note","mode":384}),
            false,
        ),
        (
            "fs.copyFile",
            json!({"path":"app:/data/note","destination":"app:/cache/copy"}),
            false,
        ),
        (
            "fs.copyFile",
            json!({"path":"app:/data/note","destination":"app:/data/tree/leaf"}),
            true,
        ),
        (
            "fs.rename",
            json!({"path":"app:/data/note","destination":"app:/cache/moved"}),
            true,
        ),
        ("fs.rm", json!({"path":"app:/data/tree"}), true),
        ("fs.rm", json!({"path":"app:/data/noteworthy"}), false),
    ] {
        run(&mut host, storage::request(op, args.clone())).unwrap();
        assert_eq!(host.take_storage_changed(), changed, "{op} {args}");
        assert!(!host.take_storage_changed());
    }
    host.source.grants = "fs.write app:/data";
    run(&mut host, Fixture::new().request).unwrap();
    assert!(
        !host.take_storage_changed(),
        "write authority alone is insufficient"
    );
}
