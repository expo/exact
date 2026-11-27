//! The vault over the reader's Keystore and where none is (LLP 1133.010 D6,
//! D15): a hook that writes the file as `Presence.kt` does stands for the
//! reader.

use super::*;
use exact_runner::presence::{Checked, Done, Env, Event, Faults, Occupancy, Op};

const GRANTS: &str =
    "secret.keep token user-presence optional\nsecret.keep key biometry-any android.strongBox";

/// The reader's op 1, as `Presence.add` keeps it: the item under its
/// generation's name in the directory, the line's words recorded beside.
fn keystore(op: u32, request: &str, value: &str) -> Result<String, String> {
    let r: serde_json::Value = serde_json::from_str(request).unwrap();
    match op {
        1 => {
            let dir = PathBuf::from(r["dir"].as_str().unwrap());
            std::fs::create_dir_all(&dir).unwrap();
            let sealed = format!("{}|{value}", r["policy"].as_str().unwrap());
            std::fs::write(dir.join(r["account"].as_str().unwrap()), sealed).unwrap();
            Ok(String::new())
        }
        _ => Ok("1".into()),
    }
}

fn vault(root: &std::path::Path, locked: Locked) -> HostVault {
    HostVault {
        secrets: plain_backend(root),
        kv: ibex2::kv::FileStore::new(root.join("kv")),
        locked,
    }
}

fn pump(backend: &mut Backend, can_lock: bool) -> Vec<Event> {
    backend.pump(&Env {
        can_lock,
        moves: true,
        now_ms: 0.0,
        faults: Faults::NONE,
    })
}

fn scratch(name: &str) -> PathBuf {
    let dir = std::env::temp_dir().join(format!("exact-presence-{name}-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    dir
}

#[cfg(unix)] // The child redirects storage through XDG.
#[test]
fn a_live_drive_forgets_only_its_own_locked_item() {
    const CHILD: &str = "EXACT_TEST_LIVE_PRESENCE";
    const APP: &str = "dev.exact.presence";
    let Some(root) = std::env::var_os(CHILD).map(PathBuf::from) else {
        let root = scratch("live");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("a_live_drive_forgets_only_its_own_locked_item")
            .env(CHILD, &root)
            .env("HOME", &root)
            .env("XDG_DATA_HOME", root.join("share"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("EXACT_AGENT", "live")
            .env("EXACT_AGENT_STORAGE", "review")
            .env_remove("EXACT_AGENT_STORAGE_FRESH")
            .env_remove("EXACT_STORE")
            .output()
            .unwrap();
        std::fs::remove_dir_all(root).unwrap();
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    on_keystore(keystore);
    let dir = root.join("exact-presence").join(APP);
    std::fs::create_dir_all(&dir).unwrap();
    let installed = dir.join("key#1");
    std::fs::write(&installed, "installed secret").unwrap();

    let mut drive = backend(APP, GRANTS).unwrap();
    drive.backend.enqueue("key", Op::Write(None), None);
    pump(&mut drive.backend, true);
    assert_eq!(
        std::fs::read_to_string(installed).unwrap(),
        "installed secret"
    );
    assert!(drive.keystore.is_none());
}

#[test]
fn the_reader_s_items_are_files_by_name_and_generation_never_plain() {
    on_keystore(keystore);
    let root = scratch("reader");
    let dir = root.join("exact-presence");
    let make = || {
        Backend::new(
            Box::new(vault(&root, Locked::Keystore(dir.clone()))),
            lines(GRANTS),
        )
    };
    let mut backend = make();
    backend.enqueue("key", Op::Write(Some(b"secret-1".to_vec())), None);
    pump(&mut backend, true);
    let item = |account: &str| std::fs::read_to_string(dir.join(account)).ok();
    assert_eq!(
        item("key#1").as_deref(),
        Some("biometry-any android.strongBox|secret-1")
    );
    assert!(!root.join("secrets/key").exists());
    // An overwrite adds the next generation before the last is deleted.
    backend.enqueue("key", Op::Write(Some(b"secret-2".to_vec())), None);
    pump(&mut backend, true);
    assert_eq!((item("key#1"), item("key#2").is_some()), (None, true));
    // A relaunch lists the directory, and the reader's read answers an unlock.
    let mut again = make();
    assert_eq!(
        (again.occupancy("key"), again.generation("key")),
        (Some(Occupancy::Locked), 2)
    );
    again.enqueue("key", Op::Unlock, Some(3));
    assert!(pump(&mut again, true)
        .iter()
        .any(|e| matches!(e, Event::Check { id: 3, .. })));
    again.checked(3, Checked::Read(Some(b"secret-2".to_vec())));
    let events = pump(&mut again, true);
    assert!(events.iter().any(
        |e| matches!(e, Event::Done { id: 3, done: Done::Value(Some(v)) } if v == b"secret-2")
    ));
    let _ = std::fs::remove_dir_all(&root);
}

#[test]
fn where_nothing_is_kept_locked_an_optional_name_stays_plain() {
    let root = scratch("none");
    let mut backend = Backend::new(Box::new(vault(&root, Locked::None)), lines(GRANTS));
    backend.enqueue("token", Op::Write(Some(b"open".to_vec())), None);
    pump(&mut backend, false);
    assert_eq!(backend.occupancy("token"), Some(Occupancy::Plain));
    assert_eq!(
        std::fs::read_to_string(root.join("secrets/token"))
            .ok()
            .as_deref(),
        Some("open")
    );
    // A protect there is refused, and the plain copy stays.
    backend.enqueue("token", Op::Protect, Some(1));
    let events = pump(&mut backend, false);
    assert!(
        events.iter().any(|e| matches!(
            e,
            Event::Done {
                id: 1,
                done: Done::Failed(..)
            }
        )),
        "{events:?}"
    );
    assert_eq!(backend.occupancy("token"), Some(Occupancy::Plain));
    let _ = std::fs::remove_dir_all(&root);
}

#[cfg(unix)]
#[test]
fn an_optional_unprompted_secret_uses_the_encrypted_backend() {
    use base64::Engine;
    let root = scratch("encrypted");
    let mut vault = vault(&root, Locked::None);
    vault.secrets = std::sync::Arc::new(crate::host::system::secret_files::EncryptedFiles::new(
        root.clone(),
        |op, _, value| {
            let codec = base64::engine::general_purpose::STANDARD;
            if op == 1 {
                Ok(codec.encode(value))
            } else {
                String::from_utf8(codec.decode(value).unwrap()).map_err(err)
            }
        },
    ));
    let mut backend = Backend::new(Box::new(vault), lines(GRANTS));
    backend.enqueue("token", Op::Write(Some(b"credential".to_vec())), None);
    pump(&mut backend, false);
    assert_eq!(backend.occupancy("token"), Some(Occupancy::Plain));
    assert!(!root.join("secrets/token").exists());
    assert_ne!(
        std::fs::read(root.join("secrets-encrypted/token")).unwrap(),
        b"credential"
    );
    backend.enqueue("token", Op::Write(None), None);
    pump(&mut backend, false);
    assert!(!root.join("secrets-encrypted/token").exists());
    std::fs::remove_dir_all(root).unwrap();
}

#[test]
fn desktop_presence_writes_and_forgets_without_secret_lease_durability() {
    let root = scratch("desktop-write-forget");
    let mut backend = Backend::new(Box::new(vault(&root, Locked::Agent)), lines(GRANTS));
    backend.enqueue("token", Op::Write(Some(b"kept".to_vec())), None);
    pump(&mut backend, false);
    assert!(backend.take_log().is_empty());
    assert_eq!(backend.occupancy("token"), Some(Occupancy::Plain));
    assert_eq!(backend.plain_value("token"), Some(b"kept".to_vec()));
    backend.enqueue("token", Op::Write(None), None);
    pump(&mut backend, false);
    assert!(backend.take_log().is_empty());
    assert_eq!(backend.occupancy("token"), Some(Occupancy::Absent));
    assert_eq!(vault(&root, Locked::Agent).plain("token").unwrap(), None);
    std::fs::remove_dir_all(root).unwrap();
}

#[cfg(unix)]
#[test]
fn removing_last_presence_policy_tears_down_keystore_and_agent_vaults() {
    const CHILD: &str = "EXACT_TEST_PRESENCE_TEARDOWN";
    const APP: &str = "dev.exact.presence.teardown";
    let Some(mode) = std::env::var(CHILD).ok() else {
        for mode in ["keystore", "agent"] {
            let root = scratch(&format!("teardown-{mode}"));
            let mut cmd = std::process::Command::new(std::env::current_exe().unwrap());
            cmd.arg("removing_last_presence_policy_tears_down_keystore_and_agent_vaults")
                .env(CHILD, mode)
                .env("HOME", &root)
                .env("XDG_DATA_HOME", root.join("share"))
                .env("XDG_CACHE_HOME", root.join("cache"))
                .env_remove("EXACT_AGENT_STORAGE_FRESH");
            if mode == "agent" {
                cmd.env("EXACT_AGENT", "1")
                    .env("EXACT_AGENT_STORAGE", "teardown")
                    .env_remove("EXACT_STORE");
            } else {
                cmd.env_remove("EXACT_AGENT")
                    .env_remove("EXACT_AGENT_STORAGE")
                    .env("EXACT_STORE", "real");
            }
            let output = cmd.output().unwrap();
            let _ = std::fs::remove_dir_all(&root);
            assert!(
                output.status.success(),
                "{mode}: {}{}",
                String::from_utf8_lossy(&output.stdout),
                String::from_utf8_lossy(&output.stderr)
            );
        }
        return;
    };
    if mode == "keystore" {
        on_keystore(keystore);
    }
    let mut v1 = backend(APP, "secret.keep token biometry-any").unwrap();
    v1.backend
        .enqueue("token", Op::Write(Some(b"secret-v1".to_vec())), None);
    pump(&mut v1.backend, true);
    assert_eq!(v1.backend.occupancy("token"), Some(Occupancy::Locked));

    let root = crate::picker::secret_root(APP).unwrap().unwrap();
    let kv = ibex2::kv::FileStore::new(root.join("kv"));
    assert!(kv.get(RECORDS, "token").unwrap().is_some());
    let locked_file = PathBuf::from(std::env::var_os("HOME").unwrap())
        .join("exact-presence")
        .join(APP)
        .join("token#1");
    if mode == "keystore" {
        assert!(locked_file.exists());
    } else {
        assert!(kv.get(LOCKED, "token#1").unwrap().is_some());
    }

    // Version 2 removes the last presence policy.
    assert!(backend(APP, "secret.keep token").is_none());
    assert_eq!(kv.get(RECORDS, "token").unwrap(), None);
    if mode == "keystore" {
        assert!(!locked_file.exists(), "locked keystore file was orphaned");
    } else {
        assert!(
            kv.keys(LOCKED).unwrap().is_empty(),
            "locked agent item orphaned"
        );
    }
}

#[cfg(unix)]
#[test]
fn uncommitted_candidates_and_validation_do_not_mutate_presence_vault() {
    const CHILD: &str = "EXACT_TEST_PRESENCE_UNCOMMITTED";
    const APP: &str = "dev.exact.presence.uncommitted";
    let Some(_) = std::env::var_os(CHILD) else {
        let root = scratch("uncommitted");
        let output = std::process::Command::new(std::env::current_exe().unwrap())
            .arg("uncommitted_candidates_and_validation_do_not_mutate_presence_vault")
            .env(CHILD, "1")
            .env("HOME", &root)
            .env("XDG_DATA_HOME", root.join("share"))
            .env("XDG_CACHE_HOME", root.join("cache"))
            .env("EXACT_AGENT", "1")
            .env("EXACT_AGENT_STORAGE", "uncommitted")
            .env_remove("EXACT_AGENT_STORAGE_FRESH")
            .env_remove("EXACT_STORE")
            .output()
            .unwrap();
        let _ = std::fs::remove_dir_all(&root);
        assert!(
            output.status.success(),
            "{}{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return;
    };
    #[derive(Clone)]
    struct PresenceSource {
        fail_boot: std::sync::Arc<std::sync::atomic::AtomicUsize>,
    }
    impl exact_runner::DataSource for PresenceSource {
        fn app_id(&self) -> &str {
            APP
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
        fn configure_secrets(
            &mut self,
            _: std::sync::Arc<dyn exact_runner::SharedSecrets>,
        ) -> Result<(), exact_runner::DataError> {
            let left = self.fail_boot.load(std::sync::atomic::Ordering::SeqCst);
            if left == 1 {
                self.fail_boot.store(0, std::sync::atomic::Ordering::SeqCst);
                return Err(exact_runner::DataError::Unavailable("refused".into()));
            }
            if left > 1 {
                self.fail_boot
                    .store(left - 1, std::sync::atomic::Ordering::SeqCst);
            }
            Ok(())
        }
    }

    let ok_plan = crate::test_plan("component App\n  view\n    text \"ok\"\n");
    let bad_plan =
        crate::test_plan("component App\n  derive boom = 1 / 0\n  view\n    text `${boom}`\n");
    let fail_boot = std::sync::Arc::new(std::sync::atomic::AtomicUsize::new(0));
    let source = || PresenceSource {
        fail_boot: fail_boot.clone(),
    };
    let (mut presenter, error) = crate::Presenter::boot_with(
        &ok_plan,
        source(),
        (200., 200.),
        1.,
        PathBuf::new(),
        crate::presenter::PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    presenter.first_pixel_unshown();
    assert!(presenter.first_pixel_told());

    let root = crate::picker::secret_root(APP).unwrap().unwrap();
    let kv = ibex2::kv::FileStore::new(root.join("kv"));
    let plain = plain_backend(&root);
    // (a) pending Op::Move: plain copy + locked intent record
    plain.write("key", Some("pending-secret")).unwrap();
    kv.set(RECORDS, "key", b"locked biometry-any android.strongBox")
        .unwrap();
    // (b) in-flight unprotect step 2: plain copy beside locked copy + plain record
    plain.write("token", Some("unprotect-plain")).unwrap();
    kv.set(LOCKED, "token#1", b"locked-copy").unwrap();
    kv.set(RECORDS, "token", b"plain").unwrap();

    let assert_untouched = |stage: &str| {
        assert_eq!(
            plain.read("key").unwrap().as_deref(),
            Some("pending-secret"),
            "{stage}: pending plain copy mutated"
        );
        assert_eq!(
            kv.get(LOCKED, "key#1").unwrap(),
            None,
            "{stage}: Op::Move ran before commit"
        );
        assert_eq!(
            plain.read("token").unwrap().as_deref(),
            Some("unprotect-plain"),
            "{stage}: in-flight unprotect plain copy deleted"
        );
        assert_eq!(
            kv.get(LOCKED, "token#1").unwrap().as_deref(),
            Some(&b"locked-copy"[..]),
            "{stage}: locked copy mutated"
        );
        assert_eq!(
            kv.get(RECORDS, "token").unwrap().as_deref(),
            Some(&b"plain"[..]),
            "{stage}: record mutated"
        );
    };

    // Uncommitted cold candidate must only read occupancy and never mutate the vault.
    let (candidate, err) = crate::Host::boot_candidate(
        &ok_plan,
        source(),
        Box::new(exact_kernel::MonospaceMeasurer::default()),
        200.,
        200.,
        None,
        None,
    )
    .unwrap();
    assert!(err.is_none(), "{err:?}");
    drop(candidate);
    assert_untouched("cold boot_candidate");

    // Candidate failing in prepare_logic disposable validation must not mutate.
    let module = Some(crate::delivery::Module {
        receipt: String::new(),
        bytes: Vec::new(),
    });
    assert!(presenter
        .reload_module(&bad_plan, source(), module.clone(), true)
        .is_err());
    assert_untouched("failed prepare_logic validation");

    // Validation succeeds (1st candidate boot), real candidate boot refuses (2nd): still no mutation.
    fail_boot.store(2, std::sync::atomic::Ordering::SeqCst);
    assert!(presenter
        .reload_module(&ok_plan, source(), module.clone(), true)
        .is_err());
    assert_eq!(fail_boot.load(std::sync::atomic::Ordering::SeqCst), 0);
    assert_untouched("accepted validation + refused real candidate");

    // Committing a reload reconciles and pumps the pending move.
    assert!(matches!(
        presenter.reload_module(&ok_plan, source(), module, true),
        Ok(crate::presenter::ReloadResult::Applied(None))
    ));
    assert_eq!(plain.read("key").unwrap(), None);
    assert_eq!(
        kv.get(LOCKED, "key#1").unwrap().as_deref(),
        Some(&b"pending-secret"[..])
    );
    assert_eq!(plain.read("token").unwrap(), None);
    assert_eq!(
        kv.get(RECORDS, "token").unwrap().as_deref(),
        Some(&b"locked user-presence optional"[..])
    );
}
