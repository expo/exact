//! Namespace identity follows the selected backend, including widget groups.
use exact_data::secrets::{Backend, Namespace};
use ibex2::host::Secrets;

pub(super) struct SecretBackend(pub Secrets, pub Option<std::path::PathBuf>);
impl Backend for SecretBackend {
    fn read(&self, name: &str) -> Result<Option<String>, String> {
        self.0.get(name).map_err(|error| error.to_string())
    }
    fn write(&self, name: &str, value: Option<&str>) -> Result<(), String> {
        if crate::watch_sync::read_only(name) {
            return Err("watch replica is phone-owned".into());
        }
        crate::launch_preloads::with_secret_write(name, || self.persist(name, value))
    }
}
impl SecretBackend {
    pub(super) fn persist(&self, name: &str, value: Option<&str>) -> Result<(), String> {
        match value {
            Some(value) => self.0.set(name, value),
            None => self.0.forget(name),
        }
        .map_err(|error| error.to_string())?;
        if let Some(directory) = &self.1 {
            exact_data::secrets::sync_directory_chain(directory)?;
        }
        Ok(())
    }
}

pub(super) fn secret_namespace(app: &str) -> Namespace {
    let agent = std::env::var_os("EXACT_AGENT").is_some();
    let store = std::env::var("EXACT_STORE").unwrap_or_default();
    if (agent && store != "real") || store == "memory" {
        return crate::picker::agent_secret_root(app)
            .map(|root| Namespace::disk(root.join("secret-leases")))
            .unwrap_or_else(Namespace::memory);
    }
    let service = if super::group_root(app).is_some() {
        app.to_owned()
    } else {
        ibex2::secrets::app_identity()
    };
    if let Some(root) = super::group_root(&service) {
        return Namespace::named(root.join("secret-leases"), &service);
    }
    // The ordinary Keychain backend uses the bundle's identity, which can
    // differ from an embedded session's app id.
    let home = std::env::var_os("HOME")
        .map(std::path::PathBuf::from)
        .filter(|path| path.is_absolute());
    let root = home
        .map(|home| home.join("Library/Application Support/exact/secret-leases"))
        .unwrap_or_else(|| std::path::PathBuf::from("/dev/null/exact-secret-leases"));
    Namespace::named(root, &service)
}

pub(super) fn secret_directory(app: &str) -> Option<std::path::PathBuf> {
    let agent = std::env::var_os("EXACT_AGENT").is_some();
    let store = std::env::var("EXACT_STORE").unwrap_or_default();
    ((agent && store != "real") || store == "memory")
        .then(|| crate::picker::agent_secret_root(app))
        .flatten()
        .map(|root| root.join("secrets"))
}

/// Read only plain secrets named by launch definitions, once each (LLP 1162.000 D2).
pub(crate) fn launch_secrets(
    grants: &str,
    app: &str,
    definitions: &[exact_runner::request::Preload],
) -> std::collections::BTreeMap<String, String> {
    let Ok(bindings) = super::endow_for(grants, app) else {
        return Default::default();
    };
    read_launch_secrets(&bindings, grants, definitions)
}

pub(crate) fn read_launch_secrets(
    bindings: &ibex2::host::Bindings,
    grants: &str,
    definitions: &[exact_runner::request::Preload],
) -> std::collections::BTreeMap<String, String> {
    let guarded: std::collections::BTreeSet<_> = crate::presence_secrets::lines(grants)
        .into_iter()
        .filter_map(|(name, policy)| policy.map(|_| name))
        .collect();
    let names: std::collections::BTreeSet<_> = definitions
        .iter()
        .flat_map(|d| d.secrets.iter().copied())
        .collect();
    let backend = SecretBackend(bindings.secrets.clone(), None);
    names
        .into_iter()
        .filter(|name| !guarded.contains(*name))
        .filter_map(|name| {
            backend
                .read(name)
                .ok()
                .flatten()
                .map(|value| (name.into(), value))
        })
        .collect()
}

struct LaunchBackend(SecretBackend);
impl Backend for LaunchBackend {
    fn read(&self, name: &str) -> Result<Option<String>, String> {
        self.0.read(name)
    }
    fn write(&self, name: &str, value: Option<&str>) -> Result<(), String> {
        self.0.persist(name, value)
    }
}
pub(crate) fn launch_backend(grants: &str, app: &str) -> Result<impl Backend, String> {
    let bindings = super::endow_for(grants, app)?;
    Ok(launch_backend_in(bindings.secrets, secret_directory(app)))
}

pub(crate) fn launch_backend_in(
    secrets: Secrets,
    directory: Option<std::path::PathBuf>,
) -> impl Backend {
    LaunchBackend(SecretBackend(secrets, directory))
}

/// Whether the process runs in agent mode, and whether locked secrets belong in
/// the platform Keychain (production, or an agent drive with `EXACT_STORE=real`).
pub(super) fn presence_mode() -> (bool, bool) {
    let agent = std::env::var_os("EXACT_AGENT").is_some();
    let platform = !agent || std::env::var("EXACT_STORE").as_deref() == Ok("real");
    (agent, platform)
}

/// Reconcile or tear down the presence vault when a boot or reload commits.
/// `PRESENCE` links the vault only when the composition names it.
pub fn reconcile_presence<const PRESENCE: bool>(
    bindings: Option<&ibex2::host::Bindings>,
    grants: &str,
    app_id: &str,
) -> Option<(exact_runner::presence::Backend, Option<String>)> {
    let (agent, platform) = presence_mode();
    if !PRESENCE {
        if let Some(b) = bindings {
            clear_retired_locked_store(b, app_id, platform);
        }
        return None;
    }
    if !platform {
        crate::presence_secrets::agent_backend(bindings, grants, agent).map(|b| (b, None))
    } else {
        crate::presence_secrets::keychain_backend(bindings, grants, app_id)
            .map(|(b, service)| (b, Some(service)))
    }
}

#[cfg(test)]
thread_local! {
    pub(crate) static DELETE_LOCKED_ITEM: std::cell::RefCell<Option<fn(&str, &str) -> bool>> =
        const { std::cell::RefCell::new(None) };
}

fn delete_locked_item(service: &str, account: &str) -> bool {
    #[cfg(test)]
    if let Some(f) = DELETE_LOCKED_ITEM.with(|hook| *hook.borrow()) {
        return f(service, account);
    }
    #[cfg(target_vendor = "apple")]
    {
        crate::defaults::delete_data_protection_item(service, account)
    }
    #[cfg(not(target_vendor = "apple"))]
    {
        let _ = (service, account);
        false
    }
}

/// Tear down stale locked items and KV records after an app removes its last
/// presence policy and stops linking `Capability::Presence` (LLP 1133.010 D4).
fn clear_retired_locked_store(bindings: &ibex2::host::Bindings, app_id: &str, platform: bool) {
    const RECORDS: &str = "exact.presence";
    const LOCKED: &str = "exact.presence.agent";
    const INDEX: &str = "exact.presence.items";

    if !platform {
        for scope in [LOCKED, INDEX, RECORDS] {
            if let Ok(keys) = bindings.kv.keys(scope) {
                for key in keys {
                    let _ = bindings.kv.delete(scope, &key);
                }
            }
        }
        return;
    }

    let mut blocked = std::collections::BTreeSet::new();
    if !app_id.is_empty() {
        if let Ok(accounts) = bindings.kv.keys(INDEX) {
            let service = format!("{app_id}.presence");
            for account in accounts {
                if delete_locked_item(&service, &account)
                    && bindings.kv.delete(INDEX, &account).is_ok()
                {
                    continue;
                }
                if let Some((name, _)) = account.split_once('#') {
                    blocked.insert(name.to_owned());
                }
            }
        }
    }
    if let Ok(names) = bindings.kv.keys(RECORDS) {
        for name in names {
            if !blocked.contains(&name) {
                let _ = bindings.kv.delete(RECORDS, &name);
            }
        }
    }
}
