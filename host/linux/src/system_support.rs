//! What the host reads from and writes to the machine outside the tree: the
//! app's store files (a named agent drive's, or the app's own) and physical
//! memory.

use std::path::PathBuf;
use std::sync::Arc;

/// The kv scope the runner's kept answers live in, beside secrets
/// (LLP 1027 D4). The same scope Apple's store writes.
const KEPT: &str = "exact.kept";

/// The kv scope persisted states live in (LLP 1116 D5): a preference, never
/// a secret. The same scope Apple's agent stores write.
const STATE: &str = "exact.state";

/// What the app has kept — in a named agent drive's scratch tree, or outside
/// the agent in its own data root ([`crate::picker::store_root`], LLP
/// 1027.007 D13) — for the runner's boot snapshot. A carried launch and a
/// nameless drive contribute nothing. A fresh drive's
/// tree is emptied first, once a process and before anything is written there
/// (`picker::empty_fresh_tree`); one that cannot be emptied gives nothing,
/// said at boot and again by the activation. Beside it, the kept answers
/// past the read budget, by name, for the runner to forget what it would
/// had it read them (`Runner::forget_unread_kept`).
pub(super) fn store_read(app_id: &str, carried: bool) -> (Vec<(String, Vec<u8>)>, Vec<String>) {
    // What an earlier host in this process committed is on disk before it is read back.
    super::store_writer::settle();
    if carried {
        return Default::default();
    }
    // A tree that could not be emptied is not read: a fresh drive starts
    // with nothing, never with what the last one left (b6 review C1).
    if let Err(e) = crate::picker::empty_fresh_tree(app_id) {
        eprintln!("exact: {e:?}");
        return Default::default();
    }
    let Ok(Some(root)) = crate::picker::store_root(app_id) else {
        return Default::default();
    };
    let kv = ibex2::kv::FileStore::new(root.join("kv"));
    let mut out = Vec::new();
    if let Ok(Some(secret_root)) = crate::picker::secret_root(app_id) {
        let secrets = ibex2::secrets::FileStore::new(secret_root.join("secrets"));
        // A leading dot is the store's temporary file, not a secret.
        let mut names = Vec::new();
        if let Ok(entries) = std::fs::read_dir(secret_root.join("secrets")) {
            for entry in entries.flatten() {
                let Ok(name) = entry.file_name().into_string() else {
                    continue;
                };
                if !name.starts_with('.') && ibex2::secrets::is_valid_name(&name) {
                    names.push(name);
                }
            }
        }
        names.sort();
        for name in names {
            if let Ok(Some(value)) = ibex2::secrets::SecretStore::get(&secrets, &name) {
                out.push((name, value.into_bytes()));
            }
        }
    }
    // The runner's own: kept answers and persisted states (LLP 1116 D5),
    // each a kv scope of its own beside the secrets. No more kept answers
    // are read than the runner keeps together (LLP 1027 D4).
    let mut left = exact_runner::Store::KEPT_TOTAL_BYTES;
    let mut unread = Vec::new();
    for (scope, prefix) in [
        (KEPT, exact_runner::Store::KEPT),
        (STATE, exact_runner::Store::STATE),
    ] {
        for key in ibex2::kv::KvStore::keys(&kv, scope).unwrap_or_default() {
            if let Ok(Some(value)) = ibex2::kv::KvStore::get(&kv, scope, &key) {
                if scope == KEPT {
                    let Some(rest) = left.checked_sub(value.len()) else {
                        unread.push(format!("{prefix}{key}"));
                        continue;
                    };
                    left = rest;
                }
                out.push((format!("{prefix}{key}"), value));
            }
        }
    }
    (out, unread)
}

/// [`store_read`]'s snapshot alone.
#[cfg(test)]
pub(super) fn store_snapshot(app_id: &str, carried: bool) -> Vec<(String, Vec<u8>)> {
    store_read(app_id, carried).0
}

pub(super) fn secret_namespace(roots: &StoreRoots) -> Option<exact_data::secrets::Namespace> {
    roots
        .as_ref()
        .ok()?
        .as_ref()?
        .1
        .as_ref()
        .map(|root| exact_data::secrets::Namespace::disk(root.join("secret-leases")))
}

pub(super) fn write_stamps(
    roots: &StoreRoots,
    writes: &[exact_runner::StoreWrite],
) -> Vec<Option<exact_data::secrets::Stamp>> {
    let namespace = secret_namespace(roots);
    writes
        .iter()
        .map(|write| {
            if write.durable
                || write.name.starts_with(exact_runner::Store::KEPT)
                || write.name.starts_with(exact_runner::Store::STATE)
            {
                return None;
            }
            namespace
                .as_ref()
                .map(|namespace| namespace.stamp(&write.name))
        })
        .collect()
}

struct SecretFiles(ibex2::secrets::FileStore, PathBuf);
impl exact_data::secrets::Backend for SecretFiles {
    fn read(&self, name: &str) -> Result<Option<String>, String> {
        ibex2::secrets::SecretStore::get(&self.0, name).map_err(|error| error.to_string())
    }
    fn write(&self, name: &str, value: Option<&str>) -> Result<(), String> {
        match value {
            Some(value) => ibex2::secrets::SecretStore::set(&self.0, name, value),
            None => ibex2::secrets::SecretStore::forget(&self.0, name),
        }
        .map_err(|error| error.to_string())?;
        exact_data::secrets::sync_directory_chain(&self.1)
    }
}

struct GrantedSecrets {
    coordinator: exact_data::secrets::Coordinator,
    grants: ibex2::grant::GrantSet,
}
impl GrantedSecrets {
    fn new(
        namespace: exact_data::secrets::Namespace,
        backend: Arc<dyn exact_data::secrets::Backend>,
        grants: &str,
    ) -> Arc<Self> {
        Arc::new(Self {
            coordinator: exact_data::secrets::Coordinator::new(namespace, backend),
            grants: ibex2::grant::GrantSet::parse(&exact_runner::io_grants(grants))
                .unwrap_or_default(),
        })
    }
    fn admit(&self, name: &str) -> Result<(), String> {
        ibex2::boundary::admit(
            &self.grants,
            &ibex2::grant::Operation::SecretKeep { name: name.into() },
        )
        .map_err(|error| error.to_string())
    }
}
impl exact_runner::SharedSecrets for GrantedSecrets {
    fn acquire(
        &self,
        name: &str,
        wake: Arc<dyn Fn() + Send + Sync>,
    ) -> Box<dyn exact_runner::SecretWait> {
        // Admission precedes acquisition: taking a name fences queued writes
        // and may delete a credential left by an abandoned effect.
        if let Err(error) = self.admit(name) {
            let wait = DeniedSecretWait(Some(Arc::new(DeniedSecret(error))));
            wake();
            return Box::new(wait);
        }
        self.coordinator.acquire(name, wake)
    }
}

struct DeniedSecret(String);
impl exact_runner::SecretLease for DeniedSecret {
    fn read(&self) -> Result<Option<String>, String> {
        Err(self.0.clone())
    }
    fn write(&self, _: Option<&str>) -> Result<(), String> {
        Err(self.0.clone())
    }
    fn begin_effect(&self) -> Result<(), String> {
        Err(self.0.clone())
    }
    fn finish(&self) -> Result<(), String> {
        Err(self.0.clone())
    }
}

struct DeniedSecretWait(Option<Arc<dyn exact_runner::SecretLease>>);
impl exact_runner::SecretWait for DeniedSecretWait {
    fn take(&mut self) -> Option<Result<Arc<dyn exact_runner::SecretLease>, String>> {
        self.0.take().map(Ok)
    }
}

pub(super) fn shared_secrets(
    app: &str,
    grants: &str,
) -> Option<std::sync::Arc<dyn exact_runner::SharedSecrets>> {
    let roots = store_roots(app);
    let namespace = secret_namespace(&roots)?;
    let root = roots.ok()??.1?;
    Some(GrantedSecrets::new(
        namespace,
        std::sync::Arc::new(SecretFiles(
            ibex2::secrets::FileStore::new(root.join("secrets")),
            root.join("secrets"),
        )),
        grants,
    ))
}

struct SecretMemory(ibex2::secrets::MemoryStore);
impl exact_data::secrets::Backend for SecretMemory {
    fn read(&self, name: &str) -> Result<Option<String>, String> {
        ibex2::secrets::SecretStore::get(&self.0, name).map_err(|error| error.to_string())
    }
    fn write(&self, name: &str, value: Option<&str>) -> Result<(), String> {
        match value {
            Some(value) => ibex2::secrets::SecretStore::set(&self.0, name, value),
            None => ibex2::secrets::SecretStore::forget(&self.0, name),
        }
        .map_err(|error| error.to_string())
    }
}

pub(super) struct SecretSession {
    pub shared: std::sync::Arc<dyn exact_runner::SharedSecrets>,
    memory: Option<(exact_data::secrets::Namespace, std::sync::Arc<SecretMemory>)>,
}
impl SecretSession {
    pub fn new(app: &str, grants: &str) -> Result<Self, String> {
        store_roots(app)?;
        if let Some(shared) = shared_secrets(app, grants) {
            return Ok(Self {
                shared,
                memory: None,
            });
        }
        let namespace = exact_data::secrets::Namespace::memory();
        let backend = std::sync::Arc::new(SecretMemory(ibex2::secrets::MemoryStore::new()));
        Ok(Self {
            shared: GrantedSecrets::new(namespace.clone(), backend.clone(), grants),
            memory: Some((namespace, backend)),
        })
    }
    pub fn seed(&self, snapshot: Vec<(String, Vec<u8>)>) {
        use exact_data::secrets::Backend;
        if let Some((_, backend)) = &self.memory {
            for (name, value) in snapshot {
                if ibex2::secrets::is_valid_name(&name) {
                    let _ = backend.write(&name, Some(&String::from_utf8_lossy(&value)));
                }
            }
        }
    }
    pub fn remember(&self, writes: &[exact_runner::StoreWrite]) -> Vec<String> {
        use exact_data::secrets::Backend;
        let Some((namespace, backend)) = &self.memory else {
            return Vec::new();
        };
        writes
            .iter()
            .filter(|write| !write.durable && ibex2::secrets::is_valid_name(&write.name))
            .filter_map(|write| {
                let stamp = namespace.stamp(&write.name);
                let value = write.value.as_deref().map(String::from_utf8_lossy);
                namespace
                    .write(&write.name, &stamp, || {
                        backend.write(&write.name, value.as_deref())
                    })
                    .err()
            })
            .collect()
    }
}

#[derive(Default)]
pub(super) struct StoreWrites(std::collections::BTreeMap<String, PendingWrite>);

struct PendingWrite {
    stamp: Option<exact_data::secrets::Stamp>,
    value: Option<Vec<u8>>,
    warned: bool,
}

/// Where an app's store writes go, as the environment says when they are
/// committed: the store's root and, where secrets are kept on disk, theirs;
/// `None` for a drive that keeps nothing (a nameless one), or why neither
/// could be found.
pub(super) type StoreRoots = Result<Option<(PathBuf, Option<PathBuf>)>, String>;

/// [`StoreRoots`] for app `app_id`, now.
pub(super) fn store_roots(app_id: &str) -> StoreRoots {
    let found = || -> Result<_, exact_runner::DataError> {
        let Some(root) = crate::picker::store_root(app_id)? else {
            return Ok(None);
        };
        Ok(Some((root, crate::picker::secret_root(app_id)?)))
    };
    found().map_err(|e| format!("{e:?}"))
}

impl StoreWrites {
    /// Whether a write failed and waits to be tried again.
    pub(super) fn pending(&self) -> bool {
        !self.0.is_empty()
    }

    /// Retry at each commit, with newer writes replacing pending values or removals.
    #[cfg(test)]
    pub(super) fn flush(
        &mut self,
        app_id: &str,
        writes: &[exact_runner::StoreWrite],
    ) -> Vec<String> {
        let roots = store_roots(app_id);
        let stamps = write_stamps(&roots, writes);
        self.flush_to(&roots, writes, &stamps)
    }

    /// [`StoreWrites::flush`] to `roots`, resolved when the writes were
    /// committed (the store's own thread writes them later).
    pub(super) fn flush_to(
        &mut self,
        roots: &StoreRoots,
        writes: &[exact_runner::StoreWrite],
        stamps: &[Option<exact_data::secrets::Stamp>],
    ) -> Vec<String> {
        for (write, stamp) in writes.iter().zip(stamps) {
            if write.durable {
                self.0.remove(&write.name);
                continue;
            }
            self.0.insert(
                write.name.clone(),
                PendingWrite {
                    stamp: stamp.clone(),
                    value: write.value.clone(),
                    warned: false,
                },
            );
        }
        if self.0.is_empty() {
            return Vec::new();
        }
        let stores = match roots {
            Ok(None) => {
                self.0.clear();
                return Vec::new();
            }
            Ok(Some((root, secrets))) => Ok((
                secrets
                    .as_ref()
                    .map(|root| ibex2::secrets::FileStore::new(root.join("secrets"))),
                ibex2::kv::FileStore::new(root.join("kv")),
            )),
            Err(error) => Err(error.clone()),
        };
        let namespace = secret_namespace(roots);
        let mut errors = Vec::new();
        self.0.retain(|name, pending| {
            let result = stores
                .as_ref()
                .map_err(|e| e.clone())
                .and_then(|(secrets, kv)| {
                    let owned = [
                        (KEPT, exact_runner::Store::KEPT),
                        (STATE, exact_runner::Store::STATE),
                    ]
                    .into_iter()
                    .find_map(|(scope, prefix)| Some((scope, name.strip_prefix(prefix)?)));
                    match owned {
                        Some((scope, key)) => match &pending.value {
                            Some(value) => ibex2::kv::KvStore::set(kv, scope, key, value),
                            None => ibex2::kv::KvStore::delete(kv, scope, key),
                        },
                        None => {
                            let write = || {
                                match secrets {
                                    Some(secrets) => match &pending.value {
                                        Some(value) => ibex2::secrets::SecretStore::set(
                                            secrets,
                                            name,
                                            &String::from_utf8_lossy(value),
                                        ),
                                        None => ibex2::secrets::SecretStore::forget(secrets, name),
                                    },
                                    None => Ok(()),
                                }
                                .map_err(|error| error.to_string())
                            };
                            return match (&namespace, &pending.stamp) {
                                (Some(namespace), Some(stamp)) => {
                                    namespace.write(name, stamp, write)
                                }
                                (Some(namespace), None) => namespace.write(
                                    name,
                                    &exact_data::secrets::Stamp::unresolved(),
                                    write,
                                ),
                                _ => write(),
                            };
                        }
                    }
                    .map_err(|e| e.to_string())
                });
            if let Err(error) = result {
                if !pending.warned {
                    errors.push(format!("store {name} failed: {error}"));
                    pending.warned = true;
                }
                true
            } else {
                false
            }
        });
        errors
    }
}

/// Physical memory in bytes, for the canvas budget (LLP 1056 D4): a quarter
/// of it, as WebKit charged on iOS. Linux reports it in `/proc/meminfo`;
/// elsewhere (the host run on a Mac) 8 GiB is assumed.
pub(super) fn physical_memory() -> u64 {
    std::fs::read_to_string("/proc/meminfo")
        .ok()
        .and_then(|m| {
            let line = m.lines().find(|l| l.starts_with("MemTotal:"))?;
            let kb: u64 = line.split_whitespace().nth(1)?.parse().ok()?;
            Some(kb * 1024)
        })
        .unwrap_or(8 << 30)
}
