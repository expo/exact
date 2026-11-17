//! The adapter over real stores: what the search in `import_tests` cannot see.
use super::*;
use ibex2::{host::Host, kv::MemoryStore, secrets};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

fn plan() -> Plan {
    contract::compile("component A\n  state liked = false persist\n  view\n    text \"watch\"\n")
        .unwrap()
}
fn policy() -> Value {
    json!({"state":["liked"],"secrets":["seed"]})
}
fn request(op: &str) -> Value {
    json!({"op":op,"policy":policy(),"sender":"phone"})
}
fn apply(revision: u64, seed: Value, state: Value) -> Value {
    let mut r = request("apply");
    r["snapshot"] = json!({"sender":"phone","revision":revision,"values":{"seed":seed,"exact.state.liked":state}});
    r
}
struct Refusing {
    values: secrets::MemoryStore,
    fail: Arc<AtomicBool>,
}
impl secrets::SecretStore for Refusing {
    fn get(&self, name: &str) -> Result<Option<String>, ibex2::boundary::HostError> {
        self.values.get(name)
    }
    fn set(&self, name: &str, value: &str) -> Result<(), ibex2::boundary::HostError> {
        if self.fail.load(Ordering::SeqCst) {
            return Err(ibex2::boundary::HostError::Failed("injected".into()));
        }
        self.values.set(name, value)
    }
    fn forget(&self, name: &str) -> Result<(), ibex2::boundary::HostError> {
        self.values.forget(name)
    }
}
fn store() -> (Platform, Arc<AtomicBool>) {
    let fail = Arc::new(AtomicBool::new(false));
    let bindings = crate::store::endow_in(
        Host::new()
            .with_kv_store(Box::new(MemoryStore::new()))
            .with_secret_store(Box::new(Refusing {
                values: secrets::MemoryStore::new(),
                fail: fail.clone(),
            })),
        "secret.keep seed",
    )
    .unwrap();
    (Platform::of(&bindings), fail)
}
#[test]
fn transport_reply_never_returns_a_plaintext_snapshot() {
    use ibex2::secrets::SecretStore;
    let secrets = Arc::new(secrets::MemoryStore::new());
    secrets.set("seed", "synthetic-private-seed").unwrap();
    crate::store::PLATFORM.with(|p| *p.borrow_mut() = Some(secrets));
    let plan = plan().encode();
    let mut request = request("snapshot");
    request["app"] = json!("");
    request["grants"] = json!("secret.keep seed");
    let input = serde_json::to_vec(&request).unwrap();
    extern "C" fn reply(bytes: *const u8, len: usize, context: usize) {
        // SAFETY: call owns its bytes, and this test owns the reply until it returns.
        unsafe {
            *(context as *mut Vec<u8>) = std::slice::from_raw_parts(bytes, len).to_vec();
        }
    }
    extern "C" fn seal(bytes: *const u8, len: usize, reply: Reply, context: usize) {
        // SAFETY: call lends this input for the synchronous callback.
        let input: Value =
            serde_json::from_slice(unsafe { std::slice::from_raw_parts(bytes, len) }).unwrap();
        assert_eq!(
            input["snapshot"]["values"]["seed"],
            "synthetic-private-seed"
        );
        let envelope = json!({"format":1,"recipient":"watch","sender":"phone","revision":1,
            "state":{},"names":["seed"],"encapsulated":"fixture","ciphertext":"sealed-fixture"});
        let output = serde_json::to_vec(&envelope).unwrap();
        reply(output.as_ptr(), output.len(), context);
    }
    let mut output = Vec::<u8>::new();
    // SAFETY: the buffers and callback live through this synchronous call.
    unsafe {
        call(
            plan.as_ptr(),
            plan.len(),
            input.as_ptr(),
            input.len(),
            Some(seal),
            reply,
            &mut output as *mut _ as usize,
        );
    }
    crate::store::PLATFORM.with(|p| *p.borrow_mut() = None);
    assert!(!String::from_utf8_lossy(&output).contains("synthetic-private-seed"));
    let response: Value = serde_json::from_slice(&output).unwrap();
    assert!(response.get("values").is_none());
    assert_eq!(
        response,
        json!({"format":1,"recipient":"watch","sender":"phone","revision":1,
        "state":{},"names":["seed"],"encapsulated":"fixture","ciphertext":"sealed-fixture"})
    );
}

#[test]
fn retained_context_ignores_removed_names_before_retiring_replicas() {
    let plan = plan();
    let (watch, _) = store();
    transact(
        &watch,
        &plan,
        "secret.keep seed",
        &apply(1, json!("old"), json!("true")),
    )
    .unwrap();
    let mut incoming = apply(1, json!("old"), json!("invalid"));
    incoming["policy"]["secrets"] = json!([]);
    assert!(transact(&watch, &plan, "", &incoming).is_err());
    assert_eq!(watch.watch_read("seed").unwrap().as_deref(), Some("old"));
    incoming["snapshot"]["values"]["exact.state.liked"] = json!("false");
    assert_eq!(
        transact(&watch, &plan, "", &incoming).unwrap()["applied"],
        true
    );
    assert_eq!(watch.watch_read("seed").unwrap(), None);
    assert_eq!(
        watch.watch_read("exact.state.liked").unwrap().as_deref(),
        Some("false")
    );
}

#[test]
fn sender_replacement_keeps_a_bounded_recent_history() {
    let plan = plan();
    let (watch, _) = store();
    for n in 0..40 {
        let mut incoming = apply(1, json!("new"), json!("true"));
        incoming["snapshot"]["sender"] = json!(format!("phone-{n}"));
        assert_eq!(
            transact(&watch, &plan, "secret.keep seed", &incoming).unwrap()["applied"],
            true
        );
    }
    let meta: Value =
        serde_json::from_slice(&watch.watch_meta("received").unwrap().unwrap()).unwrap();
    assert_eq!(meta["retired"].as_array().unwrap().len(), 32);
    assert_eq!(meta["retired"][0], "phone-7");
    let mut stale = apply(2, json!("stale"), json!("false"));
    stale["snapshot"]["sender"] = json!("phone-38");
    assert_eq!(
        transact(&watch, &plan, "secret.keep seed", &stale).unwrap()["applied"],
        false
    );
}

#[test]
fn initial_snapshot_change_deletion_and_revision_order() {
    let plan = plan();
    let (phone, _) = store();
    let (watch, _) = store();
    let initial = transact(&phone, &plan, "secret.keep seed", &request("snapshot")).unwrap();
    assert!(initial["values"]["seed"].is_null());
    assert_eq!(initial["revision"], 1);
    phone
        .watch_write(&StoreWrite {
            durable: false,
            name: "seed".into(),
            value: Some(b"fixture".to_vec()),
        })
        .unwrap();
    let changed = transact(&phone, &plan, "secret.keep seed", &request("snapshot")).unwrap();
    assert_eq!(changed["values"]["seed"], "fixture");
    assert_eq!(
        changed["revision"], 1,
        "a snapshot that was never handed over spends no revision"
    );
    // A repeated or older revision changes nothing, whatever it carries.
    for (revision, seed, applied) in [
        (2, "fixture", true),
        (2, "repeated", false),
        (1, "older", false),
    ] {
        let reply = transact(
            &watch,
            &plan,
            "secret.keep seed",
            &apply(revision, json!(seed), json!("true")),
        )
        .unwrap();
        assert_eq!(
            reply["applied"], applied,
            "revision {revision} carrying {seed}"
        );
        assert_eq!(
            watch.watch_read("seed").unwrap().as_deref(),
            Some("fixture")
        );
    }
    transact(
        &watch,
        &plan,
        "secret.keep seed",
        &apply(3, Value::Null, json!("false")),
    )
    .unwrap();
    assert_eq!(watch.watch_read("seed").unwrap(), None);
    let mut replaced = apply(1, json!("new"), json!("true"));
    replaced["snapshot"]["sender"] = json!("new-phone");
    transact(&watch, &plan, "secret.keep seed", &replaced).unwrap();
    transact(
        &watch,
        &plan,
        "secret.keep seed",
        &apply(100, json!("old"), json!("false")),
    )
    .unwrap();
    assert_eq!(watch.watch_read("seed").unwrap().as_deref(), Some("new"));
}
#[test]
fn whole_validation_precedes_writes_and_policy_removal_deletes_only_imports() {
    let plan = plan();
    let (watch, _) = store();
    let mut bad = apply(1, json!("new"), json!("wrong type"));
    assert!(transact(&watch, &plan, "secret.keep seed", &bad).is_err());
    assert_eq!(watch.watch_read("seed").unwrap(), None);
    bad = apply(1, json!("new"), json!("true"));
    bad["snapshot"]["values"]["unknown"] = json!("x");
    assert_eq!(
        transact(&watch, &plan, "secret.keep seed", &bad).unwrap()["applied"],
        true
    );
    for grants in ["", "secret.keep seed user-presence optional"] {
        assert!(transact(
            &watch,
            &plan,
            grants,
            &apply(1, json!("new"), json!("true"))
        )
        .is_err());
    }
    transact(
        &watch,
        &plan,
        "secret.keep seed",
        &apply(1, json!("new"), json!("true")),
    )
    .unwrap();
    let mut removed = request("cleanup");
    removed["policy"]["secrets"] = json!([]);
    transact(&watch, &plan, "secret.keep seed", &removed).unwrap();
    assert_eq!(watch.watch_read("seed").unwrap(), None);
    assert_eq!(
        watch.watch_read("exact.state.liked").unwrap().as_deref(),
        Some("true")
    );
}
#[test]
fn interrupted_import_is_not_completed_or_exposed_and_retries_idempotently() {
    use crate::store::KeptStore;
    let plan = plan();
    let (watch, fail) = store();
    fail.store(true, Ordering::SeqCst);
    let incoming = apply(4, json!("new"), json!("true"));
    assert!(transact(&watch, &plan, "secret.keep seed", &incoming).is_err());
    assert!(watch.watch_meta("received").unwrap().is_none());
    assert!(transact(&watch, &plan, "secret.keep seed", &request("ready")).is_err());
    let read = watch.snapshot(Vec::new());
    assert!(!read.complete);
    assert!(read.values.is_empty());
    fail.store(false, Ordering::SeqCst);
    transact(
        &watch,
        &plan,
        "secret.keep seed",
        &apply(3, json!("old"), json!("false")),
    )
    .unwrap_err();
    assert_eq!(watch.watch_read("seed").unwrap(), None);
    transact(&watch, &plan, "secret.keep seed", &incoming).unwrap();
    let read = watch.snapshot(Vec::new());
    assert!(read.complete);
    assert_eq!(read.values.len(), 2);
    assert_eq!(watch.watch_read("seed").unwrap().as_deref(), Some("new"));
    assert_eq!(
        transact(&watch, &plan, "secret.keep seed", &request("ready")).unwrap()["ready"],
        true
    );
}

#[test]
fn a_revision_is_spent_at_reserve_and_given_back_when_the_transport_refuses() {
    let plan = plan();
    let (phone, _) = store();
    let grants = "secret.keep seed";
    let revision = |phone: &Platform| {
        transact(phone, &plan, grants, &request("snapshot")).unwrap()["revision"]
            .as_u64()
            .unwrap()
    };
    let reserve = |phone: &Platform, revision: u64| {
        let mut r = request("reserve");
        r["revision"] = json!(revision);
        transact(phone, &plan, grants, &r)
    };
    let release = |phone: &Platform, revision: u64| {
        let mut r = request("release");
        r["revision"] = json!(revision);
        transact(phone, &plan, grants, &r).unwrap();
    };
    assert_eq!(revision(&phone), 1);
    assert!(
        reserve(&phone, 2).is_err(),
        "only the candidate can be spent"
    );
    reserve(&phone, 1).unwrap();
    assert_eq!(revision(&phone), 2);
    assert!(reserve(&phone, 1).is_err(), "a revision is spent once");
    // WatchConnectivity refused revision 2: no watch saw it.
    reserve(&phone, 2).unwrap();
    release(&phone, 2);
    assert_eq!(revision(&phone), 2);
    // Only the last revision spent can come back.
    reserve(&phone, 2).unwrap();
    release(&phone, 1);
    assert_eq!(revision(&phone), 3);
    // Another process's sender name does not replace the stored identity.
    let mut other = request("snapshot");
    other["sender"] = json!("another-process");
    assert_eq!(
        transact(&phone, &plan, grants, &other).unwrap()["sender"],
        "phone"
    );
}

#[test]
fn replies_tell_a_payload_this_binary_refuses_from_a_store_that_may_recover() {
    let plan = plan();
    let (watch, fail) = store();
    let reply = |result: Result<Value, Refusal>| result.unwrap_err().reply();
    let invalid = reply(transact(
        &watch,
        &plan,
        "secret.keep seed",
        &apply(1, json!("new"), json!("wrong type")),
    ));
    assert_eq!(invalid["refused"], true);
    fail.store(true, Ordering::SeqCst);
    let failed = reply(transact(
        &watch,
        &plan,
        "secret.keep seed",
        &apply(1, json!("new"), json!("true")),
    ));
    assert!(failed["refused"].is_null() && failed["oversized"].is_null());
    let waiting = reply(transact(
        &watch,
        &plan,
        "secret.keep seed",
        &request("ready"),
    ));
    assert!(waiting["refused"].is_null());
    for refusal in [invalid, failed, waiting] {
        assert!(!refusal.to_string().contains("new"), "{refusal}");
    }
}

#[test]
fn a_sealed_snapshot_over_the_transport_limit_is_refused_as_oversized() {
    use ibex2::secrets::SecretStore;
    let secrets = Arc::new(secrets::MemoryStore::new());
    secrets.set("seed", "synthetic-private-seed").unwrap();
    crate::store::PLATFORM.with(|p| *p.borrow_mut() = Some(secrets));
    let plan = plan().encode();
    let mut request = request("snapshot");
    request["app"] = json!("");
    request["grants"] = json!("secret.keep seed");
    let input = serde_json::to_vec(&request).unwrap();
    extern "C" fn reply(bytes: *const u8, len: usize, context: usize) {
        // SAFETY: call owns its bytes, and this test owns the reply until it returns.
        unsafe {
            *(context as *mut Vec<u8>) = std::slice::from_raw_parts(bytes, len).to_vec();
        }
    }
    extern "C" fn seal(_: *const u8, _: usize, reply: Reply, context: usize) {
        let output = vec![b'x'; LIMIT + 1];
        reply(output.as_ptr(), output.len(), context);
    }
    let mut output = Vec::<u8>::new();
    // SAFETY: the buffers and callback live through this synchronous call.
    unsafe {
        call(
            plan.as_ptr(),
            plan.len(),
            input.as_ptr(),
            input.len(),
            Some(seal),
            reply,
            &mut output as *mut _ as usize,
        );
    }
    crate::store::PLATFORM.with(|p| *p.borrow_mut() = None);
    let response: Value = serde_json::from_slice(&output).unwrap();
    assert_eq!(response["oversized"], true, "{response}");
}
