use super::*;

#[test]
fn entry_refresh_answers_without_mutating_the_preview() {
    // Picker roots are process-global; isolate this check from parallel picker tests.
    if std::env::var_os("EXACT_ENTRY_ROOTS_TEST").is_none() {
        let result = std::process::Command::new(std::env::current_exe().unwrap())
            .args([
                "entry_refresh_answers_without_mutating_the_preview",
                "--test-threads=1",
            ])
            .env("EXACT_ENTRY_ROOTS_TEST", "1")
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}{}",
            String::from_utf8_lossy(&result.stdout),
            String::from_utf8_lossy(&result.stderr)
        );
        return;
    }
    #[derive(Default)]
    struct Timeline(u8, String);
    impl exact_runner::DataSource for Timeline {
        fn app_id(&self) -> &str {
            &self.1
        }
        fn answer(
            &mut self,
            _: &mut Store,
            source: &str,
            args: &[Value],
        ) -> Result<Answer, DataError> {
            if self.0 == 4 {
                return Ok(Answer::Later(exact_runner::Request::get(
                    "https://example.invalid/timeline",
                )));
            }
            self.query(source, args).map(Answer::Now)
        }
        fn query(&mut self, _: &str, args: &[Value]) -> Result<Value, DataError> {
            assert!(matches!(args, [Value::Number(1000.0)]));
            match self.0 {
                1 => Err(DataError::Unavailable("offline".into())),
                2 => Ok(Value::list(vec![])),
                3 => Ok(Value::str("wrong type")),
                _ => Ok(Value::list(vec![Value::record(vec![
                    Value::Number(1000.0),
                    Value::record(vec![Value::str("Refreshed train")]),
                ])])),
            }
        }
    }
    let app_id = format!("org.exact.wp6-refresh-{}", std::process::id());
    let (roots, _) = crate::picker::app_dirs(&app_id).unwrap().unwrap();
    let picked = roots[2].join("picked/survives");
    std::fs::create_dir_all(picked.parent().unwrap()).unwrap();
    std::fs::write(&picked, b"running app pick").unwrap();
    let widget = contract::Widget {
        requires_secrets: false,
        kind: "test-entry".into(),
        title: None,
        description: None,
        families: vec!["systemSmall".into()],
        watch_families: vec![],
        component: "Card".into(),
        props: "Props".into(),
        refresh: "refreshCard".into(),
        preview: serde_json::json!({"title": "Preview train"}),
        configuration: None,
    };
    let plan = contract::snapshot::compile("shape Props\n  title: string\ncomponent App\n  view\n    text \"App\"\ncomponent Card\n  props\n    title: string\n  view\n    text title\n", &widget).unwrap();
    for response in 0..5 {
        let mut bridge = Bridge::<Timeline>::new();
        let bytes = plan.encode();
        bridge.input_write(&[bytes.as_slice(), b"test-entry"].concat());
        bridge.boot_entry(
            bytes.len(),
            10,
            0,
            0,
            Timeline(response, app_id.clone()),
            Hooks::none(),
            158.0,
            158.0,
        );
        let before = bridge
            .host
            .as_ref()
            .unwrap()
            .runner()
            .slot("snapshot_props")
            .cloned();
        let picker_roots = crate::picker::roots();
        let n = bridge.refresh_entry(Timeline(response, app_id.clone()), 1000.0);
        let result: serde_json::Value =
            serde_json::from_slice(bridge.output_bytes(n as usize)).unwrap();
        assert_eq!(crate::picker::roots(), picker_roots);
        assert_eq!(std::fs::read(&picked).unwrap(), b"running app pick");
        if response == 0 {
            assert_eq!(result["entryRefresh"], "timeline");
            assert_eq!(result["timeline"][0]["props"]["title"], "Refreshed train");
        } else {
            assert_eq!(
                bridge
                    .host
                    .as_ref()
                    .unwrap()
                    .runner()
                    .slot("snapshot_props")
                    .cloned(),
                before
            );
            match response {
                1 => {
                    assert_eq!(result["entryRefresh"], "unavailable");
                    assert_eq!(result["reason"], "offline");
                }
                2 => assert_eq!(result["timeline"], serde_json::json!([])),
                4 => assert_eq!(result["entryRefresh"], "pending"),
                _ => assert!(result["error"].as_str().unwrap().contains("conform")),
            }
        }
    }
    std::fs::remove_dir_all(roots[2].parent().unwrap()).unwrap();
}

/// A complication's refresh reads the store the watch app imports into. While an
/// import is unfinished the read is refused and the source is not asked: without
/// its secret it would answer "no account", and WidgetKit would keep that timeline.
#[test]
fn an_entry_refresh_waits_for_a_watch_import_and_never_asks_its_source_inside_one() {
    use crate::link::{IoLinks, Links};
    use std::cell::Cell;
    use std::sync::atomic::{AtomicU32, Ordering};
    thread_local! {
        /// The store read at which the import finishes; none when it never does.
        static FINISHES: Cell<Option<u32>> = const { Cell::new(None) };
        static READS: Cell<u32> = const { Cell::new(0) };
    }
    fn endow(_: &str, _: &str, _: bool) -> crate::store::Endowed {
        let grants = "storage.kv exact.watch\nstorage.kv exact.state\nstorage.kv exact.kept";
        let bindings = ibex2::host::Host::new()
            .with_secret_store(Box::new(ibex2::secrets::MemoryStore::new()))
            .with_kv_store(Box::new(ibex2::kv::MemoryStore::new()))
            .endow(ibex2::grant::GrantSet::parse(grants).unwrap());
        // The watch app's import began: its journal is recorded.
        let journal = br#"{"sender":"phone","revision":2,"names":["seed"]}"#;
        bindings.kv.set("exact.watch", "pending", journal).unwrap();
        crate::store::Endowed {
            bindings: Some(bindings),
            unbound: None,
            ..crate::store::Endowed::none()
        }
    }
    fn snapshot(
        bindings: Option<&ibex2::host::Bindings>,
        app: &str,
    ) -> Result<Vec<(String, Vec<u8>)>, String> {
        let read = READS.with(|reads| {
            reads.set(reads.get() + 1);
            reads.get()
        });
        if FINISHES.with(Cell::get) == Some(read) {
            let kv = &bindings.unwrap().kv;
            kv.set("exact.watch", "pending", b"null").unwrap();
        }
        crate::store::snapshot_for(bindings, app)
    }
    struct Timeline(std::sync::Arc<AtomicU32>);
    impl exact_runner::DataSource for Timeline {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            self.0.fetch_add(1, Ordering::SeqCst);
            Ok(Value::list(vec![Value::record(vec![
                Value::Number(1000.0),
                Value::record(vec![Value::str("Signed in")]),
            ])]))
        }
    }
    let widget = contract::Widget {
        requires_secrets: false,
        kind: "account".into(),
        title: None,
        description: None,
        families: vec!["systemSmall".into()],
        watch_families: vec![],
        component: "Card".into(),
        props: "Props".into(),
        refresh: "refreshCard".into(),
        preview: serde_json::json!({"title": "Preview"}),
        configuration: None,
    };
    let plan = contract::snapshot::compile("shape Props\n  title: string\ncomponent App\n  view\n    text \"App\"\ncomponent Card\n  props\n    title: string\n  view\n    text title\n", &widget).unwrap().encode();
    for (finishes, asked) in [(None, 0), (Some(3), 1)] {
        FINISHES.with(|f| f.set(finishes));
        READS.with(|r| r.set(0));
        let asks = std::sync::Arc::new(AtomicU32::new(0));
        let mut entry = Bridge::with_links(Links {
            io: Some(IoLinks {
                endow,
                snapshot,
                ..IoLinks::LINKED
            }),
            ..Links::ALL
        });
        entry.input_write(&[plan.as_slice(), b"account"].concat());
        entry.boot_entry(
            plan.len(),
            "account".len(),
            0,
            0,
            Timeline(asks.clone()),
            Hooks::none(),
            158.0,
            158.0,
        );
        let size = entry.entry_timeline(1000.0);
        let result = String::from_utf8(entry.output_bytes(size as usize).to_vec()).unwrap();
        assert_eq!(asks.load(Ordering::SeqCst), asked, "{result}");
        assert_eq!(
            result.contains("a watch import is unfinished"),
            finishes.is_none(),
            "{result}"
        );
        if let Some(read) = finishes {
            assert!(result.contains("Signed in"), "{result}");
            assert_eq!(READS.with(Cell::get), read);
        }
    }
}
