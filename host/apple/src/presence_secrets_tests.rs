//! The keychain vault over the hook ExactPresence installs (LLP 1133.010 D6): a
//! keychain in memory stands for it, as `Presence.swift` answers each op.

use super::*;
use exact_runner::presence::{Checked, Done, Env, Event, Faults, Occupancy, Op};
use std::collections::BTreeMap;
use std::sync::Mutex;

/// (service, account) → (the line's words, the value).
type Items = BTreeMap<(String, String), (String, Vec<u8>)>;
static SERIAL: Mutex<()> = Mutex::new(());
static ITEMS: Mutex<Items> = Mutex::new(BTreeMap::new());
static FAIL_PROBE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static FAIL_DELETE: std::sync::atomic::AtomicBool = std::sync::atomic::AtomicBool::new(false);
static FAIL_CANDIDATE: Mutex<usize> = Mutex::new(0);

#[allow(unsafe_code)] // The hook's C shape: the request and value as pointers.
extern "C" fn keychain(
    op: u32,
    json: *const u8,
    len: usize,
    value: *const u8,
    value_len: usize,
    reply: *mut std::ffi::c_void,
    done: extern "C" fn(*mut std::ffi::c_void, i32, *const u8, usize),
) {
    // SAFETY: `call` passes its request's and value's bytes for the call.
    let (json, value) = unsafe {
        (
            std::slice::from_raw_parts(json, len),
            std::slice::from_raw_parts(value, value_len),
        )
    };
    let r: serde_json::Value = serde_json::from_slice(json).unwrap();
    let text = |k: &str| r[k].as_str().unwrap_or_default().to_owned();
    let mut items = ITEMS.lock().unwrap();
    let (status, out): (i32, Vec<u8>) = match op {
        1 => {
            let key = (text("service"), text("account"));
            match items.contains_key(&key) {
                true => (-1, b"keychain -25299: duplicate".to_vec()),
                false => {
                    items.insert(key, (text("policy"), value.to_vec()));
                    (0, Vec::new())
                }
            }
        }
        2 if FAIL_DELETE.load(std::sync::atomic::Ordering::Relaxed) => {
            (-1, b"keychain delete unavailable".to_vec())
        }
        2 => match items.remove(&(text("service"), text("account"))) {
            Some(_) => (0, Vec::new()),
            None => (1, Vec::new()),
        },
        3 => {
            assert!(
                !text("account").is_empty(),
                "a probe must name one indexed item"
            );
            let mut failures = FAIL_CANDIDATE.lock().unwrap();
            let failed_candidate = text("account") == "key#3" && *failures > 0;
            if failed_candidate {
                *failures -= 1;
            }
            if failed_candidate || FAIL_PROBE.load(std::sync::atomic::Ordering::Relaxed) {
                (-1, b"keychain unavailable".to_vec())
            } else {
                let present = items.contains_key(&(text("service"), text("account")));
                (if present { 0 } else { 1 }, Vec::new())
            }
        }
        4 if FAIL_DELETE.load(std::sync::atomic::Ordering::Relaxed) => {
            (-1, b"keychain delete unavailable".to_vec())
        }
        4 => {
            items.retain(|(s, _), _| *s != text("service"));
            (0, Vec::new())
        }
        5 => (0, Vec::new()),
        _ => panic!("unexpected keychain operation {op}"),
    };
    done(reply, status, out.as_ptr(), out.len());
}

const GRANTS: &str = "secret.keep token user-presence optional\nsecret.keep key biometry-any apple.orWatch\nsecret.keep plain";

fn bindings() -> Bindings {
    let spec = format!("{GRANTS}\n{}", scopes());
    let host = ibex2::host::Host::new()
        .with_secret_store(Box::new(ibex2::secrets::MemoryStore::new()))
        .with_kv_store(Box::new(ibex2::kv::MemoryStore::new()));
    host.endow(ibex2::grant::GrantSet::parse(&spec).unwrap())
}

fn pump(backend: &mut Backend) -> Vec<Event> {
    pump_at(backend, 0.0)
}

fn pump_at(backend: &mut Backend, now_ms: f64) -> Vec<Event> {
    let env = Env {
        can_lock: true,
        moves: true,
        now_ms,
        faults: Faults::NONE,
    };
    backend.pump(&env)
}

fn item(account: &str) -> Option<(String, Vec<u8>)> {
    ITEMS
        .lock()
        .unwrap()
        .get(&("com.example.lock.presence".into(), account.into()))
        .cloned()
}

#[test]
fn locked_items_are_the_keychain_s_by_name_and_generation_and_never_plain() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    ITEMS.lock().unwrap().clear();
    FAIL_PROBE.store(false, std::sync::atomic::Ordering::Relaxed);
    FAIL_DELETE.store(false, std::sync::atomic::Ordering::Relaxed);
    *FAIL_CANDIDATE.lock().unwrap() = 0;
    on_keychain(Some(keychain));
    let b = bindings();
    let (mut backend, service) = keychain_backend(Some(&b), GRANTS, "com.example.lock").unwrap();
    assert_eq!(service, "com.example.lock.presence");
    assert!(keychain_capable());

    // A mandatory name's write: one item under its first generation, its
    // access control the line's words, and no plain copy.
    backend.enqueue("key", Op::Write(Some(b"secret-1".to_vec())), None);
    pump(&mut backend);
    assert_eq!(
        item("key#1"),
        Some(("biometry-any apple.orWatch".into(), b"secret-1".to_vec()))
    );
    assert_eq!(b.secrets.get("key").unwrap(), None);
    assert_eq!(backend.occupancy("key"), Some(Occupancy::Locked));

    // An overwrite adds the next generation before deleting the last (D6).
    backend.enqueue("key", Op::Write(Some(b"secret-2".to_vec())), None);
    pump(&mut backend);
    assert_eq!(item("key#1"), None);
    assert_eq!(item("key#2").map(|i| i.1), Some(b"secret-2".to_vec()));

    // A relaunch probes indexed generations without reading their values.
    let (mut again, _) = keychain_backend(Some(&b), GRANTS, "com.example.lock").unwrap();
    assert_eq!(again.occupancy("key"), Some(Occupancy::Locked));
    assert_eq!(again.generation("key"), 2);

    // An unlock waits on the person; the host's read after the check answers it.
    again.enqueue("key", Op::Unlock, Some(7));
    let events = pump(&mut again);
    assert!(events
        .iter()
        .any(|e| matches!(e, Event::Check { id: 7, .. })));
    again.checked(7, Checked::Read(Some(b"secret-2".to_vec())));
    let events = pump(&mut again);
    assert!(events.iter().any(
        |e| matches!(e, Event::Done { id: 7, done: Done::Value(Some(v)) } if v == b"secret-2")
    ));

    // An optional name's plain write is the app's Keychain item, and
    // `protect` moves it into a locked one, deleting the plain copy.
    again.enqueue("token", Op::Write(Some(b"open".to_vec())), None);
    pump(&mut again);
    assert_eq!(b.secrets.get("token").unwrap().as_deref(), Some("open"));
    again.enqueue("token", Op::Protect, Some(8));
    pump(&mut again);
    assert_eq!(item("token#1").map(|i| i.1), Some(b"open".to_vec()));
    assert_eq!(b.secrets.get("token").unwrap(), None);

    // A crash between indexing and adding leaves no durable item. A crash
    // after deleting leaves a stale index entry. Both are removed by probing.
    b.kv.set(INDEX, "key#3", &[]).unwrap();
    b.kv.set(INDEX, "key#1", &[]).unwrap();
    let (again, _) = keychain_backend(Some(&b), GRANTS, "com.example.lock").unwrap();
    assert_eq!(again.generation("key"), 2);
    assert_eq!(b.kv.keys(INDEX).unwrap(), vec!["key#2", "token#1"]);
    for account in b.kv.keys(INDEX).unwrap() {
        assert_eq!(b.kv.get(INDEX, &account).unwrap(), Some(Vec::new()));
    }

    // A crash before the add leaves an unconfirmed candidate. Neither a
    // transient probe failure nor repeated failures may select it. Recovery
    // and the queued unlock use this backend, without another launch.
    for failures in [1, 2] {
        b.kv.set(INDEX, "key#3", &[]).unwrap();
        *FAIL_CANDIDATE.lock().unwrap() = failures;
        let (mut recovering, _) = keychain_backend(Some(&b), GRANTS, "com.example.lock").unwrap();
        assert_eq!(item("key#2").map(|i| i.1), Some(b"secret-2".to_vec()));
        assert_eq!(recovering.generation("key"), 0, "no candidate selected");
        assert_eq!(recovering.occupancy("key"), Some(Occupancy::Pending));
        assert_eq!(b.kv.get(INDEX, "key#3").unwrap(), Some(Vec::new()));
        recovering.enqueue("key", Op::Unlock, Some(9));
        // A reply before any check must not invalidate the real credential.
        recovering.checked(9, Checked::Read(None));
        let mut events = pump(&mut recovering);
        if failures == 2 {
            assert!(events.is_empty(), "no authentication during a failed probe");
            assert_eq!(recovering.generation("key"), 0);
            assert!(pump_at(&mut recovering, 999.0).is_empty());
            events = pump_at(&mut recovering, 1000.0);
        }
        assert!(events
            .iter()
            .any(|e| matches!(e, Event::Check { id: 9, .. })));
        assert_eq!(recovering.generation("key"), 2);
        let value = item(&format!("key#{}", recovering.generation("key"))).map(|i| i.1);
        assert_eq!(value.as_deref(), Some(b"secret-2".as_slice()));
        recovering.checked(9, Checked::Read(value));
        assert!(pump_at(&mut recovering, 1000.0).iter().any(
            |e| matches!(e, Event::Done { id: 9, done: Done::Value(Some(v)) } if v == b"secret-2")
        ));
        assert_eq!(recovering.occupancy("key"), Some(Occupancy::Locked));
        assert_eq!(item("key#2").map(|i| i.1), Some(b"secret-2".to_vec()));
        assert_eq!(b.kv.get(INDEX, "key#3").unwrap(), None);
    }

    // A crash after adding the new item but before deleting the old one
    // keeps both indexed. Relaunch selects the newest and retires the old.
    let mut vault = KeychainVault {
        secrets: b.secrets.clone(),
        kv: b.kv.clone(),
        service: service.clone(),
        prune_index: true,
    };
    let policy = lines(GRANTS)
        .into_iter()
        .find(|(name, _)| name == "key")
        .unwrap()
        .1
        .unwrap();
    vault.add_locked("key", 3, &policy, b"secret-3").unwrap();
    *FAIL_CANDIDATE.lock().unwrap() = 1;
    let (mut again, _) = keychain_backend(Some(&b), GRANTS, "com.example.lock").unwrap();
    assert_eq!(again.occupancy("key"), Some(Occupancy::Pending));
    assert!(item("key#2").is_some() && item("key#3").is_some());
    assert!(
        pump(&mut again).is_empty(),
        "recovery must not authenticate"
    );
    assert_eq!(again.generation("key"), 3);
    assert_eq!(item("key#2"), None);
    assert_eq!(item("key#3").map(|i| i.1), Some(b"secret-3".to_vec()));

    // Platform invalidation removes the item. The next launch removes its
    // candidate and reports absence, with no authentication to discover it.
    ITEMS
        .lock()
        .unwrap()
        .remove(&(service.clone(), "key#3".into()));
    let (again, _) = keychain_backend(Some(&b), GRANTS, "com.example.lock").unwrap();
    assert_eq!(again.occupancy("key"), Some(Occupancy::Absent));
    assert_eq!(b.kv.get(INDEX, "key#3").unwrap(), None);

    // New calls retry immediately, even inside a failed pump's retry interval.
    FAIL_PROBE.store(true, std::sync::atomic::Ordering::Relaxed);
    let (mut again, _) = keychain_backend(Some(&b), GRANTS, "com.example.lock").unwrap();
    assert_eq!(again.occupancy("token"), Some(Occupancy::Pending));
    assert_eq!(again.plain_value("token"), None);
    assert!(again
        .snapshot_entries(true)
        .contains(&("exact.presence.token".into(), b"pending".to_vec())));
    assert!(pump(&mut again).is_empty());
    FAIL_PROBE.store(false, std::sync::atomic::Ordering::Relaxed);
    again.enqueue("token", Op::Unlock, Some(10));
    assert!(pump_at(&mut again, 1.0)
        .iter()
        .any(|e| matches!(e, Event::Check { id: 10, .. })));
    again.cancel(10);
    again.checked(10, Checked::Read(None));
    assert!(pump_at(&mut again, 1.0).is_empty());
    assert!(item("token#1").is_some());

    // Writes and an intentional forget stay ordered behind reconciliation.
    // A failed probe must neither overwrite the old item nor lose the forget.
    FAIL_PROBE.store(true, std::sync::atomic::Ordering::Relaxed);
    let (mut again, _) = keychain_backend(Some(&b), GRANTS, "com.example.lock").unwrap();
    again.enqueue("token", Op::Write(Some(b"replacement".to_vec())), None);
    again.enqueue("token", Op::Write(None), None);
    again.enqueue("token", Op::Unlock, Some(11));
    assert!(pump(&mut again).is_empty());
    assert_eq!(item("token#1").map(|i| i.1), Some(b"open".to_vec()));
    assert!(b.kv.get(RECORDS, "token").unwrap().is_some());
    FAIL_PROBE.store(false, std::sync::atomic::Ordering::Relaxed);
    assert_eq!(
        pump_at(&mut again, 1000.0),
        vec![Event::Done {
            id: 11,
            done: Done::Value(None)
        }]
    );
    assert_eq!(again.occupancy("token"), Some(Occupancy::Absent));
    assert_eq!(b.secrets.get("token").unwrap(), None);
    assert_eq!(b.kv.get(RECORDS, "token").unwrap(), None);
    assert!(b.kv.keys(INDEX).unwrap().is_empty());
    assert!(ITEMS.lock().unwrap().is_empty());

    // Forget and authenticated invalidation delete candidates unread, even
    // when an existence probe would fail. A failed deletion keeps its intent.
    for invalidate in [false, true] {
        let (mut again, _) = keychain_backend(Some(&b), GRANTS, "com.example.lock").unwrap();
        again.enqueue("key", Op::Write(Some(b"last".to_vec())), None);
        pump(&mut again);
        b.kv.set(INDEX, "key#2", &[]).unwrap();
        FAIL_PROBE.store(true, std::sync::atomic::Ordering::Relaxed);
        FAIL_DELETE.store(true, std::sync::atomic::Ordering::Relaxed);
        if invalidate {
            again.enqueue("key", Op::Unlock, Some(12));
            assert!(pump(&mut again)
                .iter()
                .any(|e| matches!(e, Event::Check { id: 12, .. })));
            again.checked(12, Checked::Invalidated);
            assert!(pump(&mut again).iter().any(|e| matches!(
                e, Event::Done { id: 12, done: Done::Failed(code, _) } if code == "UnknownError"
            )));
        } else {
            again.enqueue("key", Op::Write(None), None);
            assert!(pump(&mut again).is_empty());
        }
        assert_eq!(again.occupancy("key"), Some(Occupancy::Locked));
        assert!(item("key#1").is_some());
        assert!(b.kv.get(RECORDS, "key").unwrap().is_some());
        FAIL_DELETE.store(false, std::sync::atomic::Ordering::Relaxed);
        if invalidate {
            again.enqueue("key", Op::Unlock, Some(13));
            pump(&mut again);
            again.checked(13, Checked::Invalidated);
            assert_eq!(
                pump(&mut again),
                vec![Event::Done {
                    id: 13,
                    done: Done::Value(None)
                }]
            );
        } else {
            assert!(pump_at(&mut again, 1000.0).is_empty());
        }
        assert_eq!(again.occupancy("key"), Some(Occupancy::Absent));
        assert!(ITEMS.lock().unwrap().is_empty());
        assert!(b.kv.keys(INDEX).unwrap().is_empty());
        assert_eq!(b.kv.get(RECORDS, "key").unwrap(), None);
        FAIL_PROBE.store(false, std::sync::atomic::Ordering::Relaxed);
    }
    // A reinstall is recorded only after both services have been wiped.
    let root = crate::TempDir(
        std::env::temp_dir().join(format!("exact-presence-reinstall-{}", std::process::id())),
    );
    let marker = root.join("installed");
    let clean = ibex2::host::Host::new()
        .with_kv_store(Box::new(ibex2::kv::MemoryStore::new()))
        .endow(
            ibex2::grant::GrantSet::parse(&format!(
                "{}\nstorage.kv {}",
                scopes(),
                crate::store::KEPT
            ))
            .unwrap(),
        );
    ITEMS.lock().unwrap().insert(
        (service.clone(), "old#1".into()),
        ("".into(), b"old".to_vec()),
    );
    FAIL_DELETE.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(first_launch_at("com.example.lock", &service, &clean.kv, &marker).is_err());
    assert!(!marker.exists());
    assert!(item("old#1").is_some());
    FAIL_DELETE.store(false, std::sync::atomic::Ordering::Relaxed);
    assert!(first_launch_at("com.example.lock", &service, &clean.kv, &marker).is_ok());
    assert!(marker.exists());
    assert!(item("old#1").is_none());
    std::fs::remove_file(&marker).unwrap();
    assert!(
        first_launch_at("com.example.lock", &service, &b.kv, &marker).is_err(),
        "a refused record listing must retry"
    );
    assert!(!marker.exists());
    clean.kv.set(RECORDS, "kept", b"record").unwrap();
    ITEMS.lock().unwrap().insert(
        (service.clone(), "old#1".into()),
        ("".into(), b"old".to_vec()),
    );
    assert!(first_launch_at("com.example.lock", &service, &clean.kv, &marker).is_ok());
    assert!(marker.exists());
    assert!(item("old#1").is_some(), "an update keeps its keychain");
    ITEMS.lock().unwrap().clear();
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn removing_last_presence_policy_tears_down_keychain_agent_and_stripped_vaults() {
    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    ITEMS.lock().unwrap().clear();
    FAIL_PROBE.store(false, std::sync::atomic::Ordering::Relaxed);
    FAIL_DELETE.store(false, std::sync::atomic::Ordering::Relaxed);
    *FAIL_CANDIDATE.lock().unwrap() = 0;
    on_keychain(Some(keychain));

    // 1. Keychain vault with Capability::Presence linked.
    let app_id = "com.example.teardown";
    let service = locked_service(app_id);
    let b = bindings();
    let (mut backend, _) =
        keychain_backend(Some(&b), "secret.keep token biometry-any", app_id).unwrap();
    backend.enqueue("token", Op::Write(Some(b"secret-1".to_vec())), None);
    pump(&mut backend);
    assert!(ITEMS
        .lock()
        .unwrap()
        .contains_key(&(service.clone(), "token#1".into())));
    assert_eq!(b.kv.keys(INDEX).unwrap(), vec!["token#1"]);
    assert!(b.kv.get(RECORDS, "token").unwrap().is_some());

    // Downgrading the last presence policy to plain tears down Keychain item, index, and record.
    assert!(keychain_backend(Some(&b), "secret.keep token", app_id).is_none());
    assert!(ITEMS.lock().unwrap().is_empty());
    assert!(b.kv.keys(INDEX).unwrap().is_empty());
    assert_eq!(b.kv.get(RECORDS, "token").unwrap(), None);

    // Removing the secret from grants altogether also tears down orphaned items without reading plain.
    let (mut backend, _) =
        keychain_backend(Some(&b), "secret.keep token biometry-any", app_id).unwrap();
    backend.enqueue("token", Op::Write(Some(b"secret-2".to_vec())), None);
    pump(&mut backend);
    assert!(keychain_backend(Some(&b), "", app_id).is_none());
    assert!(ITEMS.lock().unwrap().is_empty());
    assert!(b.kv.keys(INDEX).unwrap().is_empty());
    assert_eq!(b.kv.get(RECORDS, "token").unwrap(), None);

    // 2. Agent vault with Capability::Presence linked.
    let b_agent = bindings();
    let mut agent = agent_backend(Some(&b_agent), "secret.keep token biometry-any", true).unwrap();
    agent.enqueue("token", Op::Write(Some(b"agent-secret".to_vec())), None);
    pump(&mut agent);
    assert_eq!(b_agent.kv.keys(LOCKED).unwrap(), vec!["token#1"]);
    assert!(b_agent.kv.get(RECORDS, "token").unwrap().is_some());
    assert!(agent_backend(Some(&b_agent), "secret.keep token", true).is_none());
    assert!(b_agent.kv.keys(LOCKED).unwrap().is_empty());
    assert_eq!(b_agent.kv.get(RECORDS, "token").unwrap(), None);

    // 3. Stripped release build (reconcile_presence::<false>): deletes locked item, index, and record,
    //    and retains index + record for retry when Keychain deletion fails transiently.
    fn delete_hook(service: &str, account: &str) -> bool {
        if FAIL_DELETE.load(std::sync::atomic::Ordering::Relaxed) {
            return false;
        }
        ITEMS
            .lock()
            .unwrap()
            .remove(&(service.to_owned(), account.to_owned()));
        true
    }
    let b_stripped = bindings();
    ITEMS.lock().unwrap().insert(
        (service.clone(), "token#1".into()),
        ("biometry-any".into(), b"stripped-secret".to_vec()),
    );
    b_stripped.kv.set(INDEX, "token#1", &[]).unwrap();
    b_stripped
        .kv
        .set(RECORDS, "token", b"locked biometry-any")
        .unwrap();
    crate::store::DELETE_LOCKED_ITEM.with(|h| *h.borrow_mut() = Some(delete_hook));
    FAIL_DELETE.store(true, std::sync::atomic::Ordering::Relaxed);
    assert!(crate::store::reconcile_presence::<false>(
        Some(&b_stripped),
        "secret.keep token",
        app_id
    )
    .is_none());
    assert!(b_stripped.kv.get(INDEX, "token#1").unwrap().is_some());
    assert!(b_stripped.kv.get(RECORDS, "token").unwrap().is_some());
    assert!(ITEMS
        .lock()
        .unwrap()
        .contains_key(&(service.clone(), "token#1".into())));

    FAIL_DELETE.store(false, std::sync::atomic::Ordering::Relaxed);
    assert!(crate::store::reconcile_presence::<false>(
        Some(&b_stripped),
        "secret.keep token",
        app_id
    )
    .is_none());
    crate::store::DELETE_LOCKED_ITEM.with(|h| *h.borrow_mut() = None);
    assert!(ITEMS.lock().unwrap().is_empty());
    assert!(b_stripped.kv.keys(INDEX).unwrap().is_empty());
    assert_eq!(b_stripped.kv.get(RECORDS, "token").unwrap(), None);
}

#[test]
fn prepare_plan_and_prepare_module_do_not_mutate_live_presence_vault_before_commit() {
    use crate::abi::{Bridge, Hooks};
    use crate::link::{IoLinks, Links};
    use ibex2::{kv::KvStore, secrets::SecretStore};
    use std::cell::RefCell;
    use std::sync::Arc;

    struct SharedSecrets(Arc<ibex2::secrets::MemoryStore>);
    impl SecretStore for SharedSecrets {
        fn get(&self, name: &str) -> Result<Option<String>, ibex2::boundary::HostError> {
            self.0.get(name)
        }
        fn set(&self, name: &str, value: &str) -> Result<(), ibex2::boundary::HostError> {
            self.0.set(name, value)
        }
        fn forget(&self, name: &str) -> Result<(), ibex2::boundary::HostError> {
            self.0.forget(name)
        }
    }
    struct SharedKv(Arc<ibex2::kv::MemoryStore>);
    impl KvStore for SharedKv {
        fn get(
            &self,
            scope: &str,
            key: &str,
        ) -> Result<Option<Vec<u8>>, ibex2::boundary::HostError> {
            self.0.get(scope, key)
        }
        fn set(
            &self,
            scope: &str,
            key: &str,
            value: &[u8],
        ) -> Result<(), ibex2::boundary::HostError> {
            self.0.set(scope, key, value)
        }
        fn delete(&self, scope: &str, key: &str) -> Result<(), ibex2::boundary::HostError> {
            self.0.delete(scope, key)
        }
        fn keys(&self, scope: &str) -> Result<Vec<String>, ibex2::boundary::HostError> {
            self.0.keys(scope)
        }
    }

    thread_local! {
        static TEST_STORES: RefCell<Option<(Arc<ibex2::secrets::MemoryStore>, Arc<ibex2::kv::MemoryStore>)>> =
            const { RefCell::new(None) };
    }

    fn test_endow(grants: &str, app_id: &str, fresh: bool) -> crate::store::Endowed {
        let (secrets, kv) = TEST_STORES.with(|s| s.borrow().clone().unwrap());
        let host = ibex2::host::Host::new()
            .with_secret_store(Box::new(SharedSecrets(secrets)))
            .with_kv_store(Box::new(SharedKv(kv)));
        let bindings = crate::store::endow_in(host, grants).unwrap();
        let mut snapshot = Vec::new();
        if fresh {
            snapshot = crate::store::snapshot_of(Some(&bindings));
            snapshot.extend(keychain_snapshot(Some(&bindings), grants, app_id));
        }
        crate::store::Endowed {
            snapshot,
            unread_kept: Vec::new(),
            presence: None,
            secrets: Some(Box::new(crate::store::Platform::of(&bindings))),
            bindings: Some(bindings),
            unbound: None,
        }
    }

    fn test_presence(
        bindings: Option<&Bindings>,
        grants: &str,
        app_id: &str,
    ) -> Option<(Backend, Option<String>)> {
        keychain_backend(bindings, grants, app_id).map(|(b, s)| (b, Some(s)))
    }

    const APP_ID: &str = "com.example.prepare";
    #[derive(Clone)]
    struct PresenceSource;
    impl exact_runner::DataSource for PresenceSource {
        fn app_id(&self) -> &str {
            APP_ID
        }
        fn grants(&self) -> &str {
            GRANTS
        }
        fn query(
            &mut self,
            name: &str,
            _: &[exact_runner::Value],
        ) -> Result<exact_runner::Value, exact_runner::DataError> {
            Err(exact_runner::DataError::UnknownSource(name.into()))
        }
        fn replacement(
            &self,
            _: &[u8],
            _: &str,
            _: Vec<u8>,
        ) -> Result<Self, exact_runner::DataError> {
            Ok(self.clone())
        }
    }

    let _serial = SERIAL.lock().unwrap_or_else(|e| e.into_inner());
    ITEMS.lock().unwrap().clear();
    FAIL_PROBE.store(false, std::sync::atomic::Ordering::Relaxed);
    FAIL_DELETE.store(false, std::sync::atomic::Ordering::Relaxed);
    *FAIL_CANDIDATE.lock().unwrap() = 0;
    on_keychain(Some(keychain));

    let secrets = Arc::new(ibex2::secrets::MemoryStore::new());
    let kv = Arc::new(ibex2::kv::MemoryStore::new());
    TEST_STORES.with(|s| *s.borrow_mut() = Some((secrets.clone(), kv.clone())));
    let service = locked_service(APP_ID);

    let seed = || {
        ITEMS.lock().unwrap().clear();
        // (a) pending Op::Move: plain copy + locked record
        secrets.set("key", "pending-secret").unwrap();
        kv.set(RECORDS, "key", b"locked biometry-any apple.orWatch")
            .unwrap();
        // (b) in-flight unprotect step 2: plain copy + locked item + plain record
        secrets.set("token", "unprotect-plain").unwrap();
        ITEMS.lock().unwrap().insert(
            (service.clone(), "token#1".into()),
            ("user-presence optional".into(), b"locked-copy".to_vec()),
        );
        kv.set(INDEX, "token#1", &[]).unwrap();
        kv.set(RECORDS, "token", b"plain").unwrap();
        // Missing Keychain candidate in index (must not be pruned during read-only prepare).
        kv.set(INDEX, "key#9", &[]).unwrap();
    };

    let assert_untouched = |stage: &str| {
        assert_eq!(
            secrets.get("key").unwrap().as_deref(),
            Some("pending-secret"),
            "{stage}: pending plain copy mutated"
        );
        assert!(
            !ITEMS
                .lock()
                .unwrap()
                .contains_key(&(service.clone(), "key#1".into())),
            "{stage}: Op::Move wrote locked Keychain item before commit"
        );
        assert_eq!(
            secrets.get("token").unwrap().as_deref(),
            Some("unprotect-plain"),
            "{stage}: in-flight unprotect plain copy deleted"
        );
        assert_eq!(
            ITEMS
                .lock()
                .unwrap()
                .get(&(service.clone(), "token#1".into()))
                .map(|i| i.1.as_slice()),
            Some(&b"locked-copy"[..]),
            "{stage}: locked item mutated"
        );
        assert_eq!(
            kv.get(RECORDS, "token").unwrap().as_deref(),
            Some(&b"plain"[..]),
            "{stage}: record mutated"
        );
        assert!(
            kv.get(INDEX, "key#9").unwrap().is_some(),
            "{stage}: index entry pruned before commit"
        );
    };

    let links = Links {
        io: Some(IoLinks {
            endow: test_endow,
            presence: test_presence,
            ..IoLinks::LINKED
        }),
        ..Links::ALL
    };
    let ok_plan = contract::compile("component App\n  view\n    text \"ok\"\n")
        .unwrap()
        .encode();
    let bad_plan =
        contract::compile("component App\n  derive boom = 1 / 0\n  view\n    text `${boom}`\n")
            .unwrap()
            .encode();

    // 1. Cold prepare_plan + discard_plan, and cold refused prepare_plan: zero vault mutations.
    seed();
    let mut cold = Bridge::with_links(links.clone());
    cold.input_write(&ok_plan);
    cold.prepare_plan(ok_plan.len(), PresenceSource, Hooks::none(), 400., 800.);
    assert_untouched("cold prepare_plan");
    cold.discard_plan();
    assert_untouched("cold discard_plan");
    cold.input_write(&bad_plan);
    cold.prepare_plan(bad_plan.len(), PresenceSource, Hooks::none(), 400., 800.);
    assert_untouched("cold refused prepare_plan");

    // 2. Live host with painted = true: prepare_plan, prepare_module (runs disposable validation),
    //    refused prepare_module, and discard_plan all leave the live vault untouched.
    let mut live = Bridge::with_links(links);
    live.boot(&ok_plan, PresenceSource, Hooks::none(), 400., 800.);
    live.data_ready();
    seed();

    live.input_write(&ok_plan);
    live.prepare_plan(ok_plan.len(), PresenceSource, Hooks::none(), 400., 800.);
    assert_untouched("live prepare_plan");
    live.discard_plan();
    assert_untouched("live discard_plan after prepare_plan");

    live.input_write(&ok_plan);
    live.prepare_module(
        [ok_plan.len(), 0, 0],
        PresenceSource,
        Hooks::none(),
        400.,
        800.,
    );
    assert_untouched("live prepare_module (painted validation + candidate)");
    live.discard_plan();
    assert_untouched("live discard_plan after prepare_module");

    live.input_write(&bad_plan);
    live.prepare_module(
        [bad_plan.len(), 0, 0],
        PresenceSource,
        Hooks::none(),
        400.,
        800.,
    );
    assert_untouched("live refused prepare_module");

    // 3. Committing the prepared module reconciles the vault and pumps Op::Move.
    live.input_write(&ok_plan);
    live.prepare_module(
        [ok_plan.len(), 0, 0],
        PresenceSource,
        Hooks::none(),
        400.,
        800.,
    );
    assert_untouched("before commit_plan");
    live.commit_plan();
    assert_eq!(secrets.get("key").unwrap(), None);
    assert_eq!(
        ITEMS
            .lock()
            .unwrap()
            .get(&(service.clone(), "key#1".into()))
            .map(|i| i.1.as_slice()),
        Some(&b"pending-secret"[..])
    );
    assert_eq!(secrets.get("token").unwrap(), None);
    assert_eq!(
        kv.get(RECORDS, "token").unwrap().as_deref(),
        Some(&b"locked user-presence optional"[..])
    );
    assert_eq!(kv.get(INDEX, "key#9").unwrap(), None);

    TEST_STORES.with(|s| *s.borrow_mut() = None);
    ITEMS.lock().unwrap().clear();
}
