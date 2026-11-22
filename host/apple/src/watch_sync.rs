//! Selected replicas use the host's stores. Exports send plaintext only to the
//! private sealing callback; decrypted imports return to the owner. Neither
//! path uses a view batch or an agent reply (LLP 1018 D6).
//!
//! This file performs I/O in the order `import` decides; it decides nothing
//! about that order itself.
#![allow(unsafe_code)]
use crate::store::{Platform, Prefs};
use exact_plan::Plan;
use exact_runner::{Store, StoreWrite};
use import::{Decision, Incoming, Journal, Meta, Record, Sent, Step};
use serde_json::{json, Value};
use std::collections::BTreeMap;

pub(crate) mod import;

const LIMIT: usize = 60_000;

fn names(policy: &Value, kind: &str) -> Result<Vec<String>, String> {
    let values = policy[kind].as_array().ok_or("invalid watch selection")?;
    let mut names = Vec::new();
    for value in values {
        let name = value.as_str().ok_or("invalid watch name")?;
        if name.is_empty()
            || name.len() > 64
            || name.starts_with("exact.")
            || !name
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c))
            || names.iter().any(|n| n == name)
        {
            return Err("invalid watch name".into());
        }
        names.push(name.to_owned());
    }
    Ok(names)
}

/// Signed selection from this executable's bundle, also in complication processes.
pub(crate) fn policy() -> Value {
    #[cfg(all(target_vendor = "apple", any(target_os = "watchos", test)))]
    {
        static POLICY: std::sync::OnceLock<Value> = std::sync::OnceLock::new();
        POLICY
            .get_or_init(|| {
                let config = crate::defaults::bundle_value("ExactWatchSync");
                if config.is_object() {
                    config["policy"].clone()
                } else {
                    json!({"state":[],"secrets":[]})
                }
            })
            .clone()
    }
    #[cfg(not(all(target_vendor = "apple", any(target_os = "watchos", test))))]
    json!({"state":[],"secrets":[]})
}

pub(crate) fn read_only_names() -> Vec<String> {
    if !cfg!(any(target_os = "watchos", test)) {
        return Vec::new();
    }
    let policy = policy();
    names(&policy, "state")
        .unwrap_or_default()
        .into_iter()
        .map(|n| format!("{}{n}", Store::STATE))
        .chain(names(&policy, "secrets").unwrap_or_default())
        .collect()
}

/// Phone-owned values cannot be changed by a watch session or extension.
pub fn read_only(name: &str) -> bool {
    read_only_names().iter().any(|n| n == name)
}

/// Retirement precedes store snapshots in every watch process, the widget
/// extension included, even in a no-I/O composition.
pub(crate) fn startup(grants: &str, app: &str) -> Result<(), String> {
    #[cfg(all(target_vendor = "apple", any(target_os = "watchos", test)))]
    return prune(grants, app);
    #[cfg(not(all(target_vendor = "apple", any(target_os = "watchos", test))))]
    {
        let _ = (grants, app);
        Ok(())
    }
}

/// Why a request was not carried out. A reply names the kind and nothing else:
/// no value, and no store's own error text.
#[derive(Debug)]
pub(crate) enum Refusal {
    /// This binary can never accept the request; asking again changes nothing.
    Invalid,
    /// The sealed snapshot cannot fit WatchConnectivity's application context.
    Oversized,
    /// The store could not finish now. The same request may succeed later.
    Unavailable,
}
impl From<String> for Refusal {
    fn from(_: String) -> Self {
        Refusal::Unavailable
    }
}
impl Refusal {
    fn reply(&self) -> Value {
        let mut reply = json!({"error":"watch store unavailable or payload refused"});
        match self {
            Refusal::Invalid => reply["refused"] = json!(true),
            Refusal::Oversized => reply["oversized"] = json!(true),
            Refusal::Unavailable => {}
        }
        reply
    }
}

/// One callback on the owner thread. The callback must copy its bytes before returning.
pub type Reply = extern "C" fn(*const u8, usize, usize);

/// CryptoKit's synchronous sealer; only its ciphertext reply leaves the transport call.
pub type Seal = extern "C" fn(*const u8, usize, Reply, usize);

/// The answer to every request in an archive that links no I/O.
pub fn refuse(reply: Reply, context: usize) {
    let bytes = serde_json::to_vec(&Refusal::Invalid.reply()).expect("watch reply");
    reply(bytes.as_ptr(), bytes.len(), context);
}

/// Private transport entry. The optional Swift target calls it on Exact's owner,
/// which serializes snapshots with all sessions' committed writes.
///
/// # Safety
/// Both inputs must be readable for their lengths, and `reply` must remain valid for the call.
pub unsafe fn call(
    plan: *const u8,
    plan_len: usize,
    input: *const u8,
    len: usize,
    seal: Option<Seal>,
    reply: Reply,
    context: usize,
) {
    let result =
        if len > LIMIT * 4 || plan_len > 16 * 1024 * 1024 || input.is_null() || plan.is_null() {
            Err(Refusal::Invalid)
        } else {
            // SAFETY: the caller owns both input buffers until this callback returns.
            execute(
                unsafe { std::slice::from_raw_parts(plan, plan_len) },
                unsafe { std::slice::from_raw_parts(input, len) },
                seal,
            )
        };
    let out = result.unwrap_or_else(|refusal| refusal.reply());
    let bytes = serde_json::to_vec(&out).expect("watch reply");
    reply(bytes.as_ptr(), bytes.len(), context);
}

fn execute(bytes: &[u8], input: &[u8], seal: Option<Seal>) -> Result<Value, Refusal> {
    let plan = Plan::decode(bytes).map_err(|_| Refusal::Invalid)?;
    let request: Value = serde_json::from_slice(input).map_err(|_| Refusal::Invalid)?;
    let app = request["app"]
        .as_str()
        .filter(|s| *s == plan.app_id)
        .ok_or(Refusal::Invalid)?;
    let grants = request["grants"].as_str().ok_or(Refusal::Invalid)?;
    let bindings = crate::store::endow_for(grants, app)?;
    let platform = Platform::of(&bindings).with_prefs(Prefs::of(app));
    let platform = if matches!(request["op"].as_str(), Some("apply" | "cleanup")) {
        import_store(&platform, grants, app)?
    } else {
        platform
    };
    if request["op"] != "snapshot" {
        return transact(&platform, &plan, grants, &request);
    }
    let seal = seal.ok_or(Refusal::Invalid)?;
    let snapshot = transact(&platform, &plan, grants, &request)?;
    let input = serde_json::to_vec(&json!({"snapshot":snapshot,"recipient":request["recipient"],"publicKey":request["publicKey"]})).expect("watch snapshot");
    /// An envelope longer than the transport's limit is counted, not copied.
    #[derive(Default)]
    struct Sealed {
        envelope: Vec<u8>,
        oversized: bool,
    }
    extern "C" fn sealed(bytes: *const u8, len: usize, context: usize) {
        // SAFETY: execute owns the context until the synchronous sealer returns,
        // and the sealer owns its bytes through this call.
        let out = unsafe { &mut *(context as *mut Sealed) };
        if len > LIMIT {
            out.oversized = true;
        } else if !bytes.is_null() {
            out.envelope = unsafe { std::slice::from_raw_parts(bytes, len) }.to_vec();
        }
    }
    let mut output = Sealed::default();
    seal(
        input.as_ptr(),
        input.len(),
        sealed,
        &mut output as *mut _ as usize,
    );
    if output.oversized {
        return Err(Refusal::Oversized);
    }
    let envelope: Value =
        serde_json::from_slice(&output.envelope).map_err(|_| Refusal::Unavailable)?;
    const FIELDS: [&str; 8] = [
        "format",
        "recipient",
        "sender",
        "revision",
        "state",
        "names",
        "encapsulated",
        "ciphertext",
    ];
    match envelope.as_object() {
        Some(fields)
            if fields.len() == FIELDS.len()
                && fields.keys().all(|k| FIELDS.contains(&k.as_str())) =>
        {
            Ok(envelope)
        }
        _ => Err(Refusal::Invalid),
    }
}

// Imported-name metadata is written only by this adapter. It permits deleting
// a former plain replica even after its grant has been removed from the binary.
fn import_store(store: &Platform, grants: &str, app: &str) -> Result<Platform, String> {
    let mut augmented = grants.to_owned();
    let meta = meta(store)?;
    let recorded = meta.received.iter().flat_map(|r| &r.names);
    for name in recorded.chain(meta.journal.iter().flat_map(|j| &j.names)) {
        if !name.starts_with("exact.")
            && ibex2::secrets::is_valid_name(name)
            && !crate::presence_secrets::lines(&augmented)
                .iter()
                .any(|(n, _)| n == name)
        {
            augmented.push_str(&format!("\nsecret.keep {name}"));
        }
    }
    let bindings = crate::store::endow_for(&augmented, app)?;
    Ok(Platform::of(&bindings).with_prefs(Prefs::of(app)))
}

/// Retire removed replicas without linking the transport or cryptography.
#[cfg(all(target_vendor = "apple", any(target_os = "watchos", test)))]
fn prune(grants: &str, app: &str) -> Result<(), String> {
    let policy = policy();
    let bindings = crate::store::endow_for(grants, app)?;
    let platform = Platform::of(&bindings).with_prefs(Prefs::of(app));
    let store = import_store(&platform, grants, app)?;
    let selected: Vec<_> = names(&policy, "state")?
        .into_iter()
        .map(|n| format!("{}{n}", Store::STATE))
        .chain(names(&policy, "secrets")?)
        .collect();
    // A selected transport retires names only after accepting its next snapshot.
    if !selected.is_empty() {
        return Ok(());
    }
    let meta = meta(&store)?;
    // Imported secrets are in the watch app's Keychain access group. With no
    // secret selected, an extension is not signed for that group, and the
    // Keychain reports an item it cannot see as deleted. An extension therefore
    // retires the state replicas and leaves the secrets recorded for the app.
    let kept: Vec<String> = if crate::defaults::bundle_value("WKApplication") == true {
        Vec::new()
    } else {
        let recorded = meta.received.iter().flat_map(|r| &r.names);
        recorded
            .chain(meta.journal.iter().flat_map(|j| &j.names))
            .filter(|n| !n.starts_with(Store::STATE))
            .cloned()
            .collect()
    };
    match import::retire(&meta, &kept) {
        Decision::Import(steps) => perform(&store, app, &steps, &BTreeMap::new()),
        Decision::Current | Decision::Blocked => Ok(()),
    }
}

fn metadata(store: &Platform, key: &str) -> Result<Value, String> {
    store
        .watch_meta(key)?
        .map(|bytes| serde_json::from_slice(&bytes).map_err(|_| "invalid watch metadata".into()))
        .unwrap_or(Ok(Value::Null))
}

fn meta(store: &Platform) -> Result<Meta, String> {
    Ok(Meta {
        received: Record::read(&metadata(store, "received")?),
        journal: Journal::read(&metadata(store, "pending")?),
    })
}

fn put(store: &Platform, key: &str, value: &Value) -> Result<(), String> {
    store.watch_put_meta(key, &serde_json::to_vec(value).expect("watch metadata"))
}

/// One import's I/O, in `import`'s order, stopping at the first failure.
fn perform(
    store: &Platform,
    app: &str,
    steps: &[Step],
    values: &BTreeMap<String, Option<String>>,
) -> Result<(), String> {
    for step in steps {
        match step {
            Step::Journal(journal) => put(store, "pending", &journal.json())?,
            Step::Invalidate => crate::store::watch_changed(app)?,
            Step::Write(name) | Step::Delete(name) => store.watch_write(&StoreWrite {
                durable: false,
                name: name.clone(),
                value: values
                    .get(name)
                    .and_then(|v| v.as_ref())
                    .map(|v| v.as_bytes().to_vec()),
            })?,
            Step::Record(record) => put(store, "received", &record.json())?,
            Step::Generation => {
                let next = generation(&metadata(store, "generation")?)
                    .and_then(|g| g.checked_add(1))
                    .ok_or("invalid watch generation")?;
                put(store, "generation", &json!(next))?
            }
            Step::Clear => put(store, "pending", &Value::Null)?,
        }
    }
    Ok(())
}

fn generation(stored: &Value) -> Option<u64> {
    if stored.is_null() {
        Some(0)
    } else {
        stored.as_u64()
    }
}

/// A read's mark for `import::coherent`; `None` when the metadata cannot be read.
#[cfg(any(target_os = "watchos", test))]
fn mark(kv: &ibex2::host::Kv) -> Option<import::Mark> {
    let read = |key| match kv.get("exact.watch", key) {
        Ok(Some(bytes)) => serde_json::from_slice::<Value>(&bytes).ok(),
        Ok(None) => Some(Value::Null),
        Err(_) => None,
    };
    Some(import::Mark {
        journal: read("pending")?.is_object(),
        generation: generation(&read("generation")?)?,
    })
}

/// Run a store read that no import may overlap; `None` when one did, or when
/// one is unfinished. The read's values are then not one snapshot's.
pub(crate) fn reading<T>(kv: &ibex2::host::Kv, read: impl FnOnce() -> T) -> Option<T> {
    #[cfg(any(target_os = "watchos", test))]
    {
        let before = mark(kv);
        let value = read();
        import::coherent(before.as_ref(), mark(kv).as_ref()).then_some(value)
    }
    #[cfg(not(any(target_os = "watchos", test)))]
    {
        let _ = kv;
        Some(read())
    }
}

fn transact(
    store: &Platform,
    plan: &Plan,
    grants: &str,
    request: &Value,
) -> Result<Value, Refusal> {
    let policy = &request["policy"];
    let state = names(policy, "state").map_err(|_| Refusal::Invalid)?;
    let secrets = names(policy, "secrets").map_err(|_| Refusal::Invalid)?;
    let lines = crate::presence_secrets::lines(grants);
    for name in &secrets {
        if !lines.iter().any(|(n, p)| n == name && p.is_none()) || store.watch_presence(name)? {
            return Err(Refusal::Invalid);
        }
    }
    for name in &state {
        state_type(plan, name)?;
    }
    let allowed: Vec<_> = state
        .iter()
        .map(|n| format!("{}{n}", Store::STATE))
        .chain(secrets.iter().cloned())
        .collect();
    let app = request["app"].as_str().unwrap_or("");
    let sent = || Ok::<_, String>(Sent::read(&metadata(store, "sender")?));
    match request["op"].as_str() {
        Some("ready") => match meta(store)?.journal {
            Some(_) => Err(Refusal::Unavailable),
            None => Ok(json!({"ready":true})),
        },
        // The candidate revision is spent by `reserve`, once its sealed snapshot
        // is known to fit the transport.
        Some("snapshot") => {
            let mut values = BTreeMap::new();
            for name in &allowed {
                values.insert(name.clone(), store.watch_read(name)?);
            }
            let proposed = request["sender"].as_str().ok_or(Refusal::Invalid)?;
            let next = import::candidate(sent()?.as_ref(), proposed).ok_or(Refusal::Invalid)?;
            Ok(json!({"sender":next.sender,"revision":next.revision,"values":values}))
        }
        Some("reserve") => {
            let next = Sent::read(request).ok_or(Refusal::Invalid)?;
            let spent = import::reserve(sent()?.as_ref(), &next).ok_or(Refusal::Unavailable)?;
            put(store, "sender", &spent.json())?;
            Ok(json!({"reserved":true}))
        }
        Some("release") => {
            let revision = request["revision"].as_u64().ok_or(Refusal::Invalid)?;
            if let Some(back) = sent()?.and_then(|s| import::release(&s, revision)) {
                put(store, "sender", &back.json())?;
            }
            Ok(json!({"released":true}))
        }
        Some("apply") => {
            let (incoming, values) = validated(plan, &allowed, &request["snapshot"])?;
            match import::decide(&meta(store)?, &incoming) {
                Decision::Current => Ok(json!({"applied":false})),
                Decision::Blocked => Err(Refusal::Unavailable),
                Decision::Import(steps) => {
                    perform(store, app, &steps, &values)?;
                    Ok(json!({"applied":true}))
                }
            }
        }
        Some("cleanup") => match import::retire(&meta(store)?, &allowed) {
            Decision::Current => Ok(json!({"applied":false})),
            Decision::Blocked => Err(Refusal::Unavailable),
            Decision::Import(steps) => {
                perform(store, app, &steps, &BTreeMap::new())?;
                Ok(json!({"applied":true}))
            }
        },
        _ => Err(Refusal::Invalid),
    }
}

/// A snapshot's identity and the values of the names this binary selects, each
/// checked before anything is written. Other names are ignored with their values.
#[allow(clippy::type_complexity)]
fn validated(
    plan: &Plan,
    allowed: &[String],
    snapshot: &Value,
) -> Result<(Incoming, BTreeMap<String, Option<String>>), Refusal> {
    let revision = snapshot["revision"]
        .as_u64()
        .filter(|r| *r > 0)
        .ok_or(Refusal::Invalid)?;
    let sender = snapshot["sender"]
        .as_str()
        .filter(|s| !s.is_empty() && s.len() <= 64)
        .ok_or(Refusal::Invalid)?;
    let carried = snapshot["values"].as_object().ok_or(Refusal::Invalid)?;
    let mut values = BTreeMap::new();
    for (name, value) in carried.iter().filter(|(name, _)| allowed.contains(name)) {
        let value = match value {
            Value::Null => None,
            Value::String(v) => Some(v.clone()),
            _ => return Err(Refusal::Invalid),
        };
        if let Some(v) = &value {
            if v.len() > LIMIT {
                return Err(Refusal::Invalid);
            }
            if let Some(n) = name.strip_prefix(Store::STATE) {
                let ty = state_type(plan, n)?;
                if v.len() > exact_runner::runner::persist::PERSIST_LIMIT
                    || !exact_runner::runner::persist::decode(plan, ty, v.as_bytes())
                        .is_some_and(|v| v.conforms(plan, ty))
                {
                    return Err(Refusal::Invalid);
                }
            }
        }
        values.insert(name.clone(), value);
    }
    let incoming = Incoming {
        sender: sender.to_owned(),
        revision,
        names: values.keys().cloned().collect(),
    };
    Ok((incoming, values))
}

fn state_type(plan: &Plan, name: &str) -> Result<exact_plan::TypesId, Refusal> {
    plan.slots
        .iter()
        .find(|s| s.persist == exact_plan::PersistScope::App && plan.str(s.name) == name)
        .map(|s| s.ty)
        .ok_or(Refusal::Invalid)
}

#[cfg(test)]
mod import_tests;
#[cfg(test)]
mod tests;

#[cfg(all(test, target_os = "macos"))]
mod boot_tests;
