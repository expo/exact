use exact_logic::Swappable;
use exact_plan::{bytes::Writer, Value};
use exact_runner::DataSource;
use sha2::{Digest, Sha256};
use std::io::{Read, Write};
use std::os::{fd::AsRawFd, unix::net::UnixStream};
use std::path::PathBuf;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::time::Duration;

pub const HANG: Duration = Duration::from_secs(30);
pub type Source = Swappable<()>;

pub struct Fixture {
    directory: PathBuf,
    gate: UnixStream,
    _loader: UnixStream,
    bytes: Vec<u8>,
}

impl Fixture {
    pub fn new(refuse: bool) -> Self {
        static NEXT: AtomicUsize = AtomicUsize::new(0);
        let stamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let id = NEXT.fetch_add(1, Ordering::Relaxed);
        let directory = std::env::temp_dir().join(format!(
            "exact-activation-{}-{stamp}-{id}",
            std::process::id()
        ));
        std::fs::create_dir(&directory).unwrap();
        let (gate, loader) = UnixStream::pair().unwrap();
        gate.set_read_timeout(Some(HANG)).unwrap();
        let encode = |op, body: &dyn Fn(&mut Writer)| {
            let mut writer = Writer::default();
            writer.u32(exact_logic::ABI);
            writer.u8(op);
            body(&mut writer);
            writer
                .into_vec()
                .iter()
                .map(u8::to_string)
                .collect::<Vec<_>>()
                .join(",")
        };
        let metadata = encode(0, &|w| {
            w.string("test.exact.activation");
            w.string("");
        });
        let unit = encode(1, &|w| {
            w.u8(0);
            Value::Unit.encode(w);
        });
        let answer = encode(2, &|w| {
            w.u8(0);
            for _ in 0..4 {
                w.u32(0);
            }
            w.u8(0);
            w.string("");
            w.u8(0);
            Value::Number(99.).encode(w);
        });
        let source = include_str!("activation.c")
            .replace("@ENTERED@", &loader.as_raw_fd().to_string())
            .replace("@RELEASE@", &loader.as_raw_fd().to_string())
            .replace("@ABI@", &exact_logic::ABI.to_string())
            .replace("@REFUSE@", &u8::from(refuse).to_string())
            .replace("@METADATA@", &metadata)
            .replace("@UNIT@", &unit)
            .replace("@ANSWER@", &answer);
        let c = directory.join("activation.c");
        let library = directory.join(if cfg!(target_os = "macos") {
            "activation.dylib"
        } else {
            "activation.so"
        });
        std::fs::write(&c, source).unwrap();
        let compiled = std::process::Command::new("cc")
            .args(["-shared", "-fPIC"])
            .arg(c)
            .arg("-o")
            .arg(&library)
            .output()
            .unwrap();
        assert!(
            compiled.status.success(),
            "{}",
            String::from_utf8_lossy(&compiled.stderr)
        );
        let bytes = std::fs::read(library).unwrap();
        Self {
            directory,
            gate,
            _loader: loader,
            bytes,
        }
    }

    pub fn entered(&mut self) {
        let mut byte = [0];
        self.gate
            .read_exact(&mut byte)
            .expect("the loader entered dlopen");
    }

    pub fn release(&mut self) {
        self.gate.write_all(&[1]).unwrap();
    }

    pub fn source(&self, plan: &[u8]) -> Source {
        self.source_factory(plan)()
    }

    pub fn source_factory(&self, plan: &[u8]) -> impl FnOnce() -> Source + Send + 'static {
        let card = |file: &str, bytes: &[u8]| {
            serde_json::json!({
                "file": file, "bytes": bytes.len(), "sha256": format!("{:x}", Sha256::digest(bytes))
            })
        };
        let receipt = serde_json::json!({
            "version": 1, "kind": "rust", "abi": exact_logic::ABI,
            "appId": "test.exact.activation", "grants": "",
            "executor": "native", "target": exact_logic::TARGET,
            "plan": card("app.plan", plan),
            "module": card(if cfg!(target_os = "macos") { "app.module.dylib" } else { "app.module.so" }, &self.bytes)
        });
        let plan = plan.to_vec();
        let bytes = self.bytes.clone();
        move || {
            Swappable::native(())
                .named("test.exact.activation")
                .replacement(&plan, &receipt.to_string(), bytes)
                .unwrap()
        }
    }
}

impl Drop for Fixture {
    fn drop(&mut self) {
        let _ = self.gate.shutdown(std::net::Shutdown::Both);
        let _ = std::fs::remove_dir_all(&self.directory);
    }
}

pub fn plan() -> Vec<u8> {
    let mut plan = contract::compile(include_str!("activation.contract")).unwrap();
    plan.app_id = "test.exact.activation".into();
    plan.encode()
}
