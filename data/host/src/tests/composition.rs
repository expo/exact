use super::*;

#[test]
fn mixed_writes_use_app_reads_for_notifications_and_source_grants_for_execution() {
    for (source, op, args, succeeds, changed) in [
        (
            "rust",
            "fs.writeFile",
            json!({"path":"app:/data/note","text":"hello"}),
            true,
            true,
        ),
        (
            "rust",
            "fs.writeFile",
            json!({"path":"app:/cache/hidden","text":"hidden"}),
            true,
            false,
        ),
        (
            "rust",
            "fs.readFile",
            json!({"path":"app:/data/note"}),
            false,
            false,
        ),
        (
            "javascript",
            "fs.writeFile",
            json!({"path":"app:/data/note","text":"hello"}),
            false,
            false,
        ),
    ] {
        let paths = Paths::new();
        std::fs::create_dir_all(paths.0.join("data")).unwrap();
        std::fs::write(paths.0.join("data/note"), "original").unwrap();
        let request = storage::request(op, args);
        let javascript = Fixture {
            grants: "fs.read app:/data",
            request: request.clone(),
            ..Fixture::new()
        };
        let rust = Fixture {
            grants: "fs.write app:/data\nfs.write app:/cache",
            request,
            ..Fixture::new()
        };
        let mut host = Storage::new(
            exact_data::Mixed::new(javascript, rust, &["javascript"], &["rust"]).unwrap(),
        );
        let directories = paths.directories();
        host.configure_storage(directories.data, directories.cache, directories.temporary)
            .unwrap();
        host.activate().unwrap();
        let mut store = Store::default();
        let Answer::Later(request) = host.answer(&mut store, source, &[]).unwrap() else {
            panic!("expected storage request")
        };
        let Dispatch::Run(exact_runner::Work::Now(job)) =
            host.dispatch(request.continuation.unwrap(), &store)
        else {
            panic!("expected storage worker")
        };
        let outcome = std::thread::spawn(job).join().unwrap();
        let result = storage::response(outcome);
        assert_eq!(result.is_ok(), succeeds, "{source} {op}: {result:?}");
        let expected = if changed { "hello" } else { "original" };
        assert_eq!(
            std::fs::read_to_string(paths.0.join("data/note")).unwrap(),
            expected,
        );
        assert_eq!(host.take_storage_changed(), changed, "{source} {op}");
        assert!(!host.take_storage_changed());
    }
}

#[test]
fn native_storage_changes_reach_both_mixed_children_through_storage() {
    use std::{cell::Cell, rc::Rc};
    struct Changed(Rc<Cell<bool>>);
    impl DataSource for Changed {
        fn app_id(&self) -> &str {
            "test.storage.changed"
        }
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            Ok(Value::Unit)
        }
        fn note_storage_changed(&mut self) {
            self.0.set(true);
        }
        fn take_storage_changed(&mut self) -> bool {
            self.0.replace(false)
        }
    }
    let javascript = Rc::new(Cell::new(false));
    let rust = Rc::new(Cell::new(false));
    let mut source = Storage::new(
        exact_data::Mixed::new(Changed(javascript.clone()), Changed(rust.clone()), &[], &[])
            .unwrap(),
    );
    assert!(!source.take_storage_changed());
    for _ in 0..2 {
        source.note_storage_changed();
        assert!(javascript.get(), "the JavaScript child is notified");
        assert!(rust.get(), "the Rust child is notified");
        assert!(source.take_storage_changed());
        assert!(!javascript.get() && !rust.get(), "both flags are consumed");
        assert!(!source.take_storage_changed());
    }
}

#[test]
fn mixed_storage_waits_for_both_images_without_activating_sources() {
    use std::{cell::Cell, rc::Rc};
    struct Preparing {
        ready: Rc<Cell<bool>>,
        polls: Rc<Cell<usize>>,
    }
    impl DataSource for Preparing {
        fn app_id(&self) -> &str {
            "test.preload"
        }
        fn preload(&self) -> Result<bool, DataError> {
            self.polls.set(self.polls.get() + 1);
            Ok(self.ready.get())
        }
        fn query(
            &mut self,
            _: &str,
            _: &[exact_plan::Value],
        ) -> Result<exact_plan::Value, DataError> {
            panic!("image preparation must not run app queries")
        }
        fn activate(&mut self) -> Result<(), DataError> {
            panic!("image preparation must not activate app instances")
        }
    }
    let first = Rc::new(Cell::new(false));
    let second = Rc::new(Cell::new(false));
    let polls = Rc::new(Cell::new(0));
    let source = Storage::new(
        exact_data::Mixed::new(
            Preparing {
                ready: first.clone(),
                polls: polls.clone(),
            },
            Preparing {
                ready: second.clone(),
                polls: polls.clone(),
            },
            &["first"],
            &["second"],
        )
        .unwrap(),
    );
    assert!(!source.preload().unwrap());
    assert_eq!(
        polls.get(),
        2,
        "both images start even while the first is pending"
    );
    first.set(true);
    assert!(!source.preload().unwrap());
    second.set(true);
    assert!(source.preload().unwrap());
    assert!(!source.ready(), "preloading does not activate storage");
}

#[test]
fn source_scope_refuses_writes_and_cannot_expand_the_app_ceiling() {
    let paths = Paths::new();
    let mut host = Storage::new(Fixture::new());
    paths.configure(&mut host);
    host.activate().unwrap();
    host.source.request.grants = Some("fs.write app:/elsewhere".into());
    assert!(host
        .answer(&mut Store::default(), "operation", &[])
        .is_err());
    assert!(!paths.0.exists());
    for scope in ["", "fs.read app:/data"] {
        let mut request = Fixture::new().request;
        request.grants = Some(scope.into());
        assert!(run(&mut host, request).is_err());
        assert!(!paths.0.join("data/note").exists());
        assert!(!host.take_storage_changed());
    }
    run(&mut host, Fixture::new().request).unwrap();
    assert_eq!(std::fs::read(paths.0.join("data/note")).unwrap(), b"hello");
}
