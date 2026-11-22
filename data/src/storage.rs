//! Portable storage requests; handles stay inside their host (LLP 1027.001 D2).
use exact_runner::{Outcome, Request};
use serde_json::{json, Value};

/// Bounded serialized request/result, including byte representation overhead.
pub const MAX_BYTES: usize = exact_runner::MAX_HOST_WORK_BYTES;
/// Encode a storage operation. Validation occurs before the host performs effects.
pub fn request(op: &str, args: Value) -> Request {
    Request::storage(serde_json::to_vec(&json!({"version":1,"op":op,"args":args})).unwrap())
}
/// Decode the dedicated outcome, preserving the host's failure diagnostic.
pub fn response(outcome: Outcome) -> Result<Value, String> {
    match outcome {
        Outcome::Storage(bytes) if bytes.len() <= MAX_BYTES => {
            let value: Value = serde_json::from_slice(&bytes).map_err(|e| e.to_string())?;
            if let Some(error) = value.get("error").and_then(Value::as_str) {
                Err(error.into())
            } else {
                Ok(value)
            }
        }
        Outcome::Failed { message, .. } => Err(message),
        _ => Err("expected a bounded storage result".into()),
    }
}
/// Source scopes can only select exact grant declarations already admitted by
/// their containing app. URI/path matching itself remains the host's authority.
pub fn scope<'a>(admitted: &'a str, requested: Option<&'a str>) -> Result<&'a str, String> {
    let Some(requested) = requested else {
        return Ok(admitted);
    };
    let lines: Vec<_> = admitted
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .collect();
    if requested
        .lines()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .any(|line| !lines.contains(&line))
    {
        return Err("source scope exceeds the app's admitted grants".into());
    }
    Ok(requested)
}

/// Native file stores use this scope for persisted root state (LLP 1116 D5).
pub const PERSISTED_SCOPE: &str = "exact.state";

type WindowSeed = Vec<(String, Vec<u8>)>;
static FIRST_WINDOW: std::sync::Mutex<std::collections::BTreeMap<std::path::PathBuf, WindowSeed>> =
    std::sync::Mutex::new(std::collections::BTreeMap::new());

/// Claim the named drive's authored window state for its first boot.
pub fn take_window_seed(tree: &std::path::Path) -> Vec<(String, Vec<u8>)> {
    let fixture = FIRST_WINDOW
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .remove(tree);
    if let Some(fixture) = fixture {
        return fixture;
    }
    let Some(id) = std::env::var("EXACT_AGENT_RESTORE_WINDOW").ok() else {
        return Vec::new();
    };
    if std::env::var_os("EXACT_AGENT").is_none()
        || std::env::var_os("EXACT_AGENT_STORAGE").is_none()
    {
        return Vec::new();
    }
    let restored = (|| -> std::io::Result<WindowSeed> {
        let path = reload_archive(tree, &id)?;
        let claimed = path.with_extension("claimed");
        match std::fs::rename(path, &claimed) {
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(Vec::new()),
            result => result?,
        }
        let archive = std::fs::read(&claimed);
        std::fs::remove_file(claimed)?;
        let entries: std::collections::BTreeMap<String, String> =
            serde_json::from_slice(&archive?)?;
        Ok(entries
            .into_iter()
            .map(|(name, json)| {
                (
                    format!("{}{name}", exact_runner::Store::WINDOW_STATE),
                    json.into_bytes(),
                )
            })
            .collect())
    })();
    restored.unwrap_or_else(|e| {
        eprintln!("exact: could not restore the test window: {e}");
        Vec::new()
    })
}

fn reload_archive(tree: &std::path::Path, id: &str) -> std::io::Result<std::path::PathBuf> {
    if id.len() != 32 || !id.bytes().all(|b| b.is_ascii_hexdigit()) {
        return Err(std::io::Error::other("invalid test window identifier"));
    }
    Ok(tree.join(format!("window-{id}.json")))
}

/// A test relaunches its native process. Hand its window archive to the next boot in
/// the named scratch tree, where it can exceed the platform's environment size limit.
pub fn save_test_window<D: exact_runner::DataSource>(
    runner: &exact_runner::Runner<D>,
    tree: Option<std::path::PathBuf>,
    request: &str,
) -> String {
    let save = || -> std::io::Result<()> {
        let tree = tree
            .filter(|_| {
                std::env::var_os("EXACT_AGENT").is_some()
                    && std::env::var_os("EXACT_AGENT_STORAGE").is_some()
            })
            .ok_or_else(|| std::io::Error::other("window reload needs a named test store"))?;
        let id = exact_runner::json::field_str(request, "id").unwrap_or_default();
        let path = reload_archive(&tree, &id)?;
        std::fs::create_dir_all(&tree)?;
        let file = std::fs::File::create_new(path)?;
        serde_json::to_writer(file, &runner.window_state())?;
        Ok(())
    };
    match save() {
        Ok(()) => r#"{"saved":true}"#.into(),
        Err(e) => exact_runner::json::error(&format!("reload: {e}")),
    }
}

/// Populate a fresh drive's preferences and reserve its per-window fixtures for the first boot.
pub fn seed_persisted<E: std::fmt::Display, S: std::fmt::Display>(
    tree: &std::path::Path,
    mut write: impl FnMut(&str, &[u8]) -> Result<(), E>,
    mut session: impl FnMut(&str, &str) -> Result<(), S>,
) -> Result<(), exact_runner::DataError> {
    let Some(raw) = std::env::var_os("EXACT_AGENT_STORED") else {
        return Ok(());
    };
    let invalid = |why: String| exact_runner::DataError::Unavailable(format!("stored: {why}"));
    let entries: Vec<(String, String)> = serde_json::from_str(
        raw.to_str()
            .ok_or_else(|| invalid("expected UTF-8".into()))?,
    )
    .map_err(|e| invalid(e.to_string()))?;
    let mut window = Vec::new();
    for (key, json) in entries {
        if let Some(name) = key.strip_prefix("#oauth.") {
            let valid = exact_runner::grants::parse(&format!("secret.keep {name}"))
                .is_ok_and(|set| set.secrets().eq(std::iter::once(name)));
            if !valid || std::env::var("EXACT_STORE").as_deref() != Ok("memory") {
                return Err(invalid(
                    "signed-in needs a plain secret in the test store".into(),
                ));
            }
            session(name, &json).map_err(|e| invalid(format!("{name}: {e}")))?;
            continue;
        }
        let scoped = key.strip_prefix("#window.");
        let name = scoped.unwrap_or(&key);
        if name.is_empty()
            || !name
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b"_-".contains(&b))
        {
            return Err(invalid(format!("invalid state name {name:?}")));
        }
        if scoped.is_some() {
            window.push((
                format!("{}{name}", exact_runner::Store::WINDOW_STATE),
                json.into_bytes(),
            ));
        } else {
            write(name, json.as_bytes()).map_err(|e| invalid(format!("{name}: {e}")))?;
        }
    }
    if !window.is_empty() {
        FIRST_WINDOW
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .insert(tree.to_owned(), window);
    }
    Ok(())
}
