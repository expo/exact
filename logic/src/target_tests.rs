use super::*;

#[path = "../tests/fixtures/targets.rs"]
mod fixture;

fn drive(data: &mut impl DataSource) {
    let mut store = Store::default();
    let args = [Value::str("http://127.0.0.1/fixture")];
    assert_eq!(
        data.query("read", &args).unwrap(),
        Value::str("bake placeholder")
    );
    let targets = [
        Target::Resource(0),
        Target::Resource(u32::MAX as usize),
        Target::Mutation(0),
        Target::Mutation(17),
        Target::Sent(0, 1),
        Target::Sent(0, u64::MAX),
    ];
    for target in targets {
        assert!(matches!(
            data.answer_for(target, &mut store, "read", &args).unwrap(),
            Answer::Later(_)
        ));
    }
    for (i, target) in targets.into_iter().enumerate().rev() {
        for step in 0..2 {
            let outcome = Outcome::Response(exact_runner::Response {
                status: 200,
                headers: vec![],
                body: b"reply".to_vec(),
            });
            let answer = if step == 0 {
                data.parse_for(target, &mut store, "read", &args, outcome)
            } else {
                data.parse_fetch_for(target, None, &mut store, "read", &args, outcome)
                    .map(Option::unwrap)
            }
            .unwrap();
            if step == 0 {
                assert!(matches!(answer, Answer::Later(_)));
            } else {
                assert_eq!(
                    answer,
                    Answer::Now(Value::str(&format!("{}:{}:reply", target.text(), i + 1)))
                );
            }
        }
    }
    for (i, target) in targets.into_iter().enumerate() {
        assert!(matches!(
            data.answer_for(target, &mut store, "prefetch", &args)
                .unwrap(),
            Answer::Later(_)
        ));
        let message = "exactPrefetch requires a plain HTTP read";
        let outcome = Outcome::Failed {
            kind: exact_runner::FailureKind::Refused,
            message: message.into(),
        };
        assert_eq!(
            data.parse_for(target, &mut store, "prefetch", &args, outcome)
                .unwrap(),
            Answer::Now(Value::str(&format!(
                "{}:{}:{message}",
                target.text(),
                targets.len() + i + 1
            )))
        );
    }
}

#[test]
fn target_aware_calls_survive_the_module_adapter() {
    drive(&mut Swappable::off(fixture::Targets::default()));
    struct Local(abi::Session<fixture::Targets>);
    impl Executor for Local {
        fn call(&mut self, bytes: &[u8]) -> Result<Vec<u8>, String> {
            self.0.dispatch(bytes)?;
            Ok(self.0.output().to_vec())
        }
    }
    let mut module = Swappable::wasm(fixture::Targets::default());
    module.embedded = None;
    module.executor = Some(Box::new(Local(abi::Session::new(
        fixture::Targets::default(),
    ))));
    drive(&mut module);
}

#[test]
#[ignore = "async lane: builds Rust logic modules"]
fn target_aware_calls_survive_wasm_and_native_modules() {
    use std::{fs, process::Command};
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap();
    let dir = root.join("target/logic-target-fixture");
    fs::create_dir_all(&dir).unwrap();
    fs::write(
        dir.join("Cargo.toml"),
        format!(
            r#"[workspace]
[package]
name = "logic-target-fixture"
version = "0.1.0"
edition = "2021"
[lib]
path = "{}/logic/tests/fixtures/targets.rs"
crate-type = ["cdylib"]
[dependencies]
exact-runner = {{ path = "{}/runner" }}
exact-logic-abi = {{ path = "{}/logic/abi" }}
"#,
            root.display(),
            root.display(),
            root.display()
        ),
    )
    .unwrap();
    let manifest = fs::read_to_string(root.join("Cargo.toml")).unwrap();
    let patches = manifest
        .split("[patch.crates-io]")
        .nth(1)
        .unwrap()
        .lines()
        .take_while(|line| !line.starts_with('['))
        .collect::<Vec<_>>()
        .join("\n")
        .replace("path = \"", &format!("path = \"{}/", root.display()));
    use std::io::Write;
    writeln!(
        fs::OpenOptions::new()
            .append(true)
            .open(dir.join("Cargo.toml"))
            .unwrap(),
        "[patch.crates-io]{patches}"
    )
    .unwrap();
    fs::copy(root.join("Cargo.lock"), dir.join("Cargo.lock")).unwrap();
    let target_dir = dir.join("target");
    let plan = exact_plan::builder::PlanBuilder::new(0, 0)
        .finish()
        .unwrap();
    fs::write(dir.join("app.bind.plan"), plan.encode()).unwrap();
    for target in ["wasm32-unknown-unknown", TARGET] {
        let built = Command::new("cargo")
            .args(["build", "--offline", "--manifest-path"])
            .arg(dir.join("Cargo.toml"))
            .args(["--target-dir"])
            .arg(&target_dir)
            .args(["--target", target])
            .output()
            .unwrap();
        assert!(
            built.status.success(),
            "{}",
            String::from_utf8_lossy(&built.stderr)
        );
        let filename = if target == "wasm32-unknown-unknown" {
            "logic_target_fixture.wasm"
        } else if cfg!(target_os = "macos") {
            "liblogic_target_fixture.dylib"
        } else if cfg!(target_os = "windows") {
            "logic_target_fixture.dll"
        } else {
            "liblogic_target_fixture.so"
        };
        let bytes = fs::read(target_dir.join(target).join("debug").join(filename)).unwrap();
        let mut module = if target == "wasm32-unknown-unknown" {
            Swappable::wasm(fixture::Targets::default())
        } else {
            Swappable::native(fixture::Targets::default())
        };
        module.embedded = None;
        module.bytes = Some(bytes);
        module.preloader = None;
        module.activate_module().unwrap();
        drive(&mut module);
    }
}
