use super::*;
use crate::abi::{Bridge, Hooks};
use exact_runner::{DataError, DataSource, Value as RunnerValue};

const APP: &str = "test.exact.watch-sync";
struct Data;
impl DataSource for Data {
    fn app_id(&self) -> &str {
        APP
    }
    fn grants(&self) -> &str {
        let phase = std::env::var("EXACT_WATCH_TEST_PHASE");
        if matches!(phase.as_deref(), Ok("removed" | "complication")) {
            ""
        } else {
            "secret.keep seed"
        }
    }
    fn query(&mut self, _: &str, _: &[RunnerValue]) -> Result<RunnerValue, DataError> {
        Ok(RunnerValue::Bool(false))
    }
}

// Each launch reads its own bundle, as the live app and its extension do.
fn process(test: &str, mode: &str, live: bool, selected: bool, root: &std::path::Path) {
    let bundle = root.join(format!("{mode}.app/Contents"));
    std::fs::create_dir_all(bundle.join("MacOS")).unwrap();
    let binary = bundle.join("MacOS/probe");
    std::fs::hard_link(std::env::current_exe().unwrap(), &binary).unwrap();
    let secret = if mode == "shrunk" || mode == "narrow" {
        ""
    } else {
        "<string>seed</string>"
    };
    let config = if selected {
        format!(
            r#"<key>ExactWatchSync</key><dict>
      <key>app</key><string>{APP}</string><key>grants</key><string>secret.keep seed</string>
      <key>policy</key><dict><key>secrets</key><array>{secret}</array>
      <key>state</key><array><string>liked</string></array></dict></dict>"#
        )
    } else {
        String::new()
    };
    std::fs::write(
        bundle.join("Info.plist"),
        format!(
            r#"<?xml version="1.0"?><plist version="1.0"><dict>
      <key>CFBundleIdentifier</key><string>{APP}.{mode}</string>
      <key>CFBundleExecutable</key><string>probe</string>
      <key>CFBundlePackageType</key><string>{}</string>
      <key>WKApplication</key><{}/>{config}</dict></plist>"#,
            if live { "APPL" } else { "XPC!" },
            if live { "true" } else { "false" }
        ),
    )
    .unwrap();
    let output = std::process::Command::new(binary)
        .args([
            "--exact",
            &format!("watch_sync::boot_tests::{test}"),
            "--nocapture",
        ])
        .env("EXACT_WATCH_TEST_PHASE", mode)
        .env("EXACT_AGENT", "1")
        .env("EXACT_AGENT_STORAGE", "watch")
        .env("EXACT_APP_GROUP_APP", APP)
        .env("EXACT_APP_GROUP_ROOT", root)
        .env_remove("EXACT_STORE")
        .env_remove("EXACT_WATCH_SYNC")
        .env_remove("EXACT_AGENT_STORAGE_FRESH")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{mode}: {}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
}

fn platform() -> Platform {
    Platform::of(&crate::store::endow_for("secret.keep seed", APP).unwrap()).with_prefs(Prefs::Kv)
}
fn seed() {
    let plan = contract::compile(
        "component A\n  state liked = false persist\n  view\n    text \"watch\"\n",
    )
    .unwrap();
    transact(&platform(), &plan, "secret.keep seed", &json!({"op":"apply","policy":{"state":["liked"],"secrets":["seed"]},"snapshot":{"sender":"phone","revision":1,"values":{"seed":"synthetic-seed","exact.state.liked":"true"}}})).unwrap();
}
fn boot(source: &str, io: bool) -> Bridge<Data> {
    let plan = contract::compile(source).unwrap().encode();
    let mut links = crate::link::Links::ALL;
    if !io {
        links.io = None;
    }
    let mut bridge = Bridge::with_links(links);
    let input = bridge.input_write(&plan);
    let len = bridge.boot_plan(input, Data, Hooks::none(), 200., 200.);
    let reply = String::from_utf8_lossy(bridge.output_bytes(len as usize));
    assert!(
        serde_json::from_str::<Value>(&reply).unwrap()["error"].is_null(),
        "{reply}"
    );
    bridge
}

#[test]
fn removing_all_io_retires_imports_at_session_boot() {
    let test = "removing_all_io_retires_imports_at_session_boot";
    if let Ok(phase) = std::env::var("EXACT_WATCH_TEST_PHASE") {
        if phase == "seed" {
            seed();
            return;
        }
        assert_eq!(
            platform().watch_read("seed").unwrap().as_deref(),
            Some("synthetic-seed")
        );
        boot("component A\n  view\n    text \"no storage\"\n", false);
        assert_eq!(platform().watch_read("exact.state.liked").unwrap(), None);
        let recorded = meta(&platform()).unwrap().received.unwrap().names;
        if phase == "complication" {
            // The extension leaves the secret, recorded, for the app.
            assert_eq!(
                platform().watch_read("seed").unwrap().as_deref(),
                Some("synthetic-seed")
            );
            assert_eq!(recorded, ["seed"]);
        } else {
            assert_eq!(platform().watch_read("seed").unwrap(), None);
            assert!(recorded.is_empty());
        }
        return;
    }
    // WidgetKit may refresh a complication before the app is next opened.
    for complication in [false, true] {
        let root = crate::TempDir(std::env::temp_dir().join(format!(
            "watch-retire-{complication}-{}",
            std::process::id()
        )));
        process(test, "seed", true, true, &root);
        if complication {
            process(test, "complication", false, false, &root);
        }
        process(test, "removed", true, false, &root);
    }
}

#[test]
fn an_extension_process_cannot_overwrite_phone_owned_names() {
    let test = "an_extension_process_cannot_overwrite_phone_owned_names";
    if let Ok(phase) = std::env::var("EXACT_WATCH_TEST_PHASE") {
        if phase == "seed" {
            seed();
            return;
        }
        let mut bridge = boot("component A\n  state liked = false persist\n  action change\n    liked = false\n  view\n    button \"change\" press=change testId=\"change\"\n", true);
        let batch: Value = serde_json::from_slice(bridge.output_bytes(usize::MAX)).unwrap();
        let id = batch["ops"]
            .as_array()
            .unwrap()
            .iter()
            .find(|op| op["props"]["testId"] == "change")
            .unwrap()["id"]
            .as_u64()
            .unwrap();
        bridge.dispatch(id as u32, 0, 0, 0.);

        assert_eq!(
            platform()
                .watch_read("exact.state.liked")
                .unwrap()
                .as_deref(),
            Some("true")
        );
        assert!(platform()
            .write(&StoreWrite {
                durable: false,
                name: "seed".into(),
                value: Some(b"extension".to_vec())
            })
            .is_err());
        assert_eq!(
            platform().watch_read("seed").unwrap().as_deref(),
            Some("synthetic-seed")
        );
        return;
    }
    let root = crate::TempDir(
        std::env::temp_dir().join(format!("watch-extension-{}", std::process::id())),
    );
    process(test, "seed", true, true, &root);
    process(test, "extension", false, true, &root);
}

#[test]
fn a_new_process_accepts_a_retained_context_under_a_smaller_policy() {
    let test = "a_new_process_accepts_a_retained_context_under_a_smaller_policy";
    if let Ok(phase) = std::env::var("EXACT_WATCH_TEST_PHASE") {
        if phase == "seed" {
            seed();
            return;
        }
        let source = "component A\n  state liked = false persist\n  view\n    text \"watch\"\n";
        boot(source, true);
        assert_eq!(
            platform().watch_read("seed").unwrap().as_deref(),
            Some("synthetic-seed")
        );
        let plan = contract::compile(source).unwrap();
        let incoming = json!({"op":"apply","policy":super::policy(),"snapshot":{
            "sender":"phone","revision":1,"values":{"seed":"synthetic-seed","exact.state.liked":"true"}}});
        assert_eq!(
            transact(&platform(), &plan, "", &incoming).unwrap()["applied"],
            true
        );
        boot(source, true);
        assert_eq!(platform().watch_read("seed").unwrap(), None);
        assert_eq!(
            platform()
                .watch_read("exact.state.liked")
                .unwrap()
                .as_deref(),
            Some("true")
        );
        return;
    }
    let root =
        crate::TempDir(std::env::temp_dir().join(format!("watch-shrunk-{}", std::process::id())));
    process(test, "seed", true, true, &root);
    process(test, "shrunk", true, true, &root);
}

#[test]
fn a_new_process_imports_added_names_from_the_same_revision() {
    let test = "a_new_process_imports_added_names_from_the_same_revision";
    if let Ok(phase) = std::env::var("EXACT_WATCH_TEST_PHASE") {
        let source = "component A\n  state liked = false persist\n  view\n    text \"watch\"\n";
        let plan = contract::compile(source).unwrap();
        let incoming = json!({"op":"apply","policy":super::policy(),"snapshot":{
            "sender":"phone","revision":10,"values":{"seed":"synthetic-seed","exact.state.liked":"true"}}});
        assert_eq!(
            transact(&platform(), &plan, "secret.keep seed", &incoming).unwrap()["applied"],
            true
        );
        boot(source, true);
        assert_eq!(
            platform().watch_read("seed").unwrap().as_deref(),
            if phase == "narrow" {
                None
            } else {
                Some("synthetic-seed")
            }
        );
        return;
    }
    let root =
        crate::TempDir(std::env::temp_dir().join(format!("watch-expanded-{}", std::process::id())));
    process(test, "narrow", true, true, &root);
    process(test, "expanded", true, true, &root);
}

#[test]
fn a_snapshot_rejects_an_import_completed_between_secret_and_state_reads() {
    use crate::store::KeptStore;
    use ibex2::{host::Host, secrets::SecretStore};
    const TEST: &str = "a_snapshot_rejects_an_import_completed_between_secret_and_state_reads";
    if let Ok(phase) = std::env::var("EXACT_WATCH_TEST_PHASE") {
        if phase == "seed" {
            seed();
            return;
        }
        if phase == "writer" {
            let plan = contract::compile(
                "component A\n  state liked = false persist\n  view\n    text \"watch\"\n",
            )
            .unwrap();
            transact(&platform(), &plan, "secret.keep seed", &json!({"op":"apply","policy":super::policy(),"snapshot":{
                "sender":"phone","revision":2,"values":{"seed":"account-B","exact.state.liked":"false"}}})).unwrap();
            return;
        }
        struct Interleaved {
            secrets: ibex2::secrets::FileStore,
            root: std::path::PathBuf,
            fired: std::sync::atomic::AtomicBool,
        }
        impl SecretStore for Interleaved {
            fn get(&self, name: &str) -> Result<Option<String>, ibex2::boundary::HostError> {
                let value = self.secrets.get(name)?;
                if !self.fired.swap(true, std::sync::atomic::Ordering::SeqCst) {
                    process(TEST, "writer", true, true, &self.root);
                }
                Ok(value)
            }
            fn set(&self, name: &str, value: &str) -> Result<(), ibex2::boundary::HostError> {
                self.secrets.set(name, value)
            }
            fn forget(&self, name: &str) -> Result<(), ibex2::boundary::HostError> {
                self.secrets.forget(name)
            }
        }
        let root = crate::store::group_root(APP).unwrap();
        let scratch = crate::picker::agent_secret_root(APP).unwrap();
        let bindings = crate::store::endow_in(
            Host::new()
                .with_kv_store(Box::new(ibex2::kv::FileStore::new(scratch.join("kv"))))
                .with_secret_store(Box::new(Interleaved {
                    secrets: ibex2::secrets::FileStore::new(scratch.join("secrets")),
                    root,
                    fired: Default::default(),
                })),
            "secret.keep seed",
        )
        .unwrap();
        let reader = Platform::of(&bindings);
        let previous = vec![
            ("exact.state.liked".into(), b"true".to_vec()),
            ("seed".into(), b"synthetic-seed".to_vec()),
        ];
        let read = reader.snapshot(previous.clone());
        assert!(
            !read.complete,
            "a read of secret A followed by state B must retry"
        );
        assert_eq!(
            read.values, previous,
            "a retry retains only the caller's coherent snapshot"
        );
        let retry = reader.snapshot(Vec::new());
        assert!(retry.complete);
        assert_eq!(
            retry.values,
            vec![
                ("exact.state.liked".into(), b"false".to_vec()),
                ("seed".into(), b"account-B".to_vec())
            ]
        );
        return;
    }
    let root = crate::TempDir(
        std::env::temp_dir().join(format!("watch-interleaved-{}", std::process::id())),
    );
    process(TEST, "seed", true, true, &root);
    process(TEST, "reader", false, true, &root);
}

#[test]
fn the_accepted_phone_recovers_a_failed_replacement_before_boot() {
    let test = "the_accepted_phone_recovers_a_failed_replacement_before_boot";
    if std::env::var("EXACT_WATCH_TEST_PHASE").is_ok() {
        use ibex2::{host::Host, secrets};
        use std::sync::{atomic::Ordering, Arc};
        seed();
        let scratch = crate::picker::agent_secret_root(APP).unwrap();
        let fail = Arc::new(std::sync::atomic::AtomicBool::new(true));
        struct Refusing {
            values: secrets::FileStore,
            fail: Arc<std::sync::atomic::AtomicBool>,
        }
        impl secrets::SecretStore for Refusing {
            fn get(&self, n: &str) -> Result<Option<String>, ibex2::boundary::HostError> {
                self.values.get(n)
            }
            fn set(&self, n: &str, v: &str) -> Result<(), ibex2::boundary::HostError> {
                if self.fail.load(Ordering::SeqCst) {
                    return Err(ibex2::boundary::HostError::Failed("locked".into()));
                }
                self.values.set(n, v)
            }
            fn forget(&self, n: &str) -> Result<(), ibex2::boundary::HostError> {
                self.values.forget(n)
            }
        }
        let bindings = crate::store::endow_in(
            Host::new()
                .with_kv_store(Box::new(ibex2::kv::FileStore::new(scratch.join("kv"))))
                .with_secret_store(Box::new(Refusing {
                    values: secrets::FileStore::new(scratch.join("secrets")),
                    fail: fail.clone(),
                })),
            "secret.keep seed",
        )
        .unwrap();
        let watch = Platform::of(&bindings);
        let source = "component A\n  state liked = false persist\n  view\n    text \"watch\"\n";
        let plan = contract::compile(source).unwrap();
        let mut incoming = json!({"op":"apply","policy":super::policy(),"snapshot":{
            "sender":"replacement","revision":2,"values":{"seed":"replacement-seed","exact.state.liked":"false"}}});
        assert!(transact(&watch, &plan, "secret.keep seed", &incoming).is_err());
        fail.store(false, Ordering::SeqCst);
        incoming["snapshot"] = json!({"sender":"phone","revision":1,"values":{"seed":"synthetic-seed","exact.state.liked":"true"}});
        assert_eq!(
            transact(&watch, &plan, "secret.keep seed", &incoming).unwrap()["applied"],
            true
        );
        assert_eq!(
            transact(
                &watch,
                &plan,
                "secret.keep seed",
                &json!({"op":"ready","policy":super::policy()})
            )
            .unwrap()["ready"],
            true
        );
        let mut bridge = boot(source, true);
        let input = bridge.input_write(b"{\"op\":\"state\"}");
        let count = bridge.agent(input);
        let state: Value = serde_json::from_slice(bridge.output_bytes(count as usize)).unwrap();
        assert_eq!(state["slots"]["liked"], true);
        assert_eq!(
            platform().watch_read("seed").unwrap().as_deref(),
            Some("synthetic-seed")
        );
        return;
    }
    let root =
        crate::TempDir(std::env::temp_dir().join(format!("watch-recovery-{}", std::process::id())));
    process(test, "recover", true, true, &root);
}
