use super::*;
use base64::Engine;
use std::sync::atomic::{AtomicU64, Ordering};

// The JVM suite checks AES-GCM. This codec makes storage ordering and failures
// observable without substituting Android Keystore on a desktop.
fn codec(op: u32, context: &str, value: &str) -> Result<String, String> {
    let base64 = base64::engine::general_purpose::STANDARD;
    match op {
        1 => Ok(base64.encode(format!("{context}\0{value}"))),
        2 => String::from_utf8(base64.decode(value).map_err(|_| "invalid")?)
            .map_err(|_| "invalid".into())
            .and_then(|text| {
                text.strip_prefix(&format!("{context}\0"))
                    .map(str::to_owned)
                    .ok_or("wrong name".into())
            }),
        _ => unreachable!(),
    }
}
struct Fixture(PathBuf);
impl Fixture {
    fn new() -> Self {
        static NEXT: AtomicU64 = AtomicU64::new(0);
        let root = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("../../target/lane-687")
            .join(format!(
                "secrets-{}-{}",
                std::process::id(),
                NEXT.fetch_add(1, Ordering::Relaxed)
            ));
        std::fs::create_dir_all(&root).unwrap();
        Self(root)
    }
    fn backend(&self) -> EncryptedFiles {
        EncryptedFiles::new(self.0.clone(), codec)
    }
    fn plain(&self) -> FileStore {
        FileStore::new(self.0.join("secrets"))
    }
    fn sealed(&self) -> FileStore {
        FileStore::new(self.0.join("secrets-encrypted"))
    }
}
impl Drop for Fixture {
    fn drop(&mut self) {
        std::fs::remove_dir_all(&self.0).unwrap();
    }
}

#[test]
fn opening_removes_plaintext_without_reading_or_migrating_it() {
    let f = Fixture::new();
    f.plain().set("login.token", "old credential").unwrap();
    // Invalid UTF-8 would fail a plaintext read; opening must only unlink it.
    std::fs::write(f.0.join("secrets/transfer.123"), [0xff, 0xfe]).unwrap();
    let backend = f.backend();
    assert!(!f.0.join("secrets").exists());
    assert_eq!(backend.read("login.token").unwrap(), None);
    assert_eq!(backend.read("transfer.123").unwrap(), None);
    backend
        .write("login.token", Some("new credential"))
        .unwrap();
    assert!(!f
        .sealed()
        .get("login.token")
        .unwrap()
        .unwrap()
        .contains("credential"));
    assert_eq!(
        f.backend().read("login.token").unwrap().as_deref(),
        Some("new credential")
    );
    assert_eq!(names(&f.0), ["login.token"]);
    backend.write("login.token", None).unwrap();
    assert_eq!(backend.read("login.token").unwrap(), None);
}

#[test]
fn opening_removes_plaintext_even_when_the_key_is_unavailable() {
    let f = Fixture::new();
    f.plain().set("token", "remove me").unwrap();
    let unavailable = EncryptedFiles::new(f.0.clone(), |_, _, _| Err("locked".into()));
    assert!(!f.0.join("secrets").exists());
    assert_eq!(unavailable.read("token").unwrap(), None);
    assert!(unavailable.write("new", Some("secret")).is_err());
}

#[test]
fn corrupt_or_renamed_ciphertext_never_falls_back_to_plaintext() {
    let f = Fixture::new();
    f.backend().write("token", Some("new")).unwrap();
    f.plain().set("token", "old").unwrap();
    f.sealed().set("token", "corrupt").unwrap();
    assert!(f.backend().read("token").is_err());
    f.backend().write("other", Some("another")).unwrap();
    let other = f.sealed().get("other").unwrap().unwrap();
    f.sealed().set("token", &other).unwrap();
    assert!(f.backend().read("token").is_err());
    // Forgetting a corrupt entry does not require its key.
    f.backend().write("token", None).unwrap();
    assert_eq!(f.backend().read("token").unwrap(), None);
}

#[test]
fn independent_backends_serialize_removal_with_a_new_write() {
    let f = Fixture::new();
    f.plain().set("token", "old").unwrap();
    std::thread::scope(|scope| {
        let barrier = std::sync::Arc::new(std::sync::Barrier::new(2));
        let other = barrier.clone();
        let f = &f;
        let read = scope.spawn(move || {
            other.wait();
            f.backend().read("token").unwrap()
        });
        barrier.wait();
        f.backend().write("token", Some("new")).unwrap();
        read.join().unwrap();
    });
    assert_eq!(f.backend().read("token").unwrap().as_deref(), Some("new"));
    assert_eq!(f.plain().get("token").unwrap(), None);
}

#[test]
fn names_and_links_cannot_escape_the_store() {
    let f = Fixture::new();
    assert!(f.backend().write("../outside", Some("secret")).is_err());
    assert!(f.backend().read("..").is_err());
    let outside = f.0.join("outside");
    std::fs::write(&outside, "keep").unwrap();
    std::fs::create_dir(f.0.join("secrets")).unwrap();
    std::os::unix::fs::symlink(&outside, f.0.join("secrets/token")).unwrap();
    assert_eq!(f.backend().read("token").unwrap(), None);
    f.backend().write("token", Some("new")).unwrap();
    assert_eq!(std::fs::read_to_string(outside).unwrap(), "keep");
}
