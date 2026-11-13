use super::*;
use serde_json::json;

#[test]
fn an_absent_preload_preserves_the_baked_module_bytes() {
    let meta = json!({"appId":"test.preload", "grants":"", "module":{"sha256":"hash"}});
    assert_eq!(module_constants(&meta, false, "web", false, None, &serde_json::Value::Null).unwrap(), "pub const APP: &str = \"test.preload\";\npub const GRANTS: &str = \"\";\npub const REVISION: &str = \"hash\";\npub const RUST_UPDATES: bool = false;\n");
    let stage = Scratch::new(&std::env::temp_dir()).unwrap();
    let mut manifest = json!({"name":"Preload", "app":{"id":"test.preload", "name":"Preload"}});
    let bake = |manifest: &serde_json::Value| {
        std::fs::write(stage.0.join("app.json"), manifest.to_string()).unwrap();
        let manifest = contract::Manifest::read(&stage.0).unwrap();
        exact_bake::compatibility_id_sources(
            &stage.0,
            "web",
            "wasm32-unknown-unknown",
            &manifest,
            Some(""),
            None,
        )
        .unwrap()
    };
    let before = bake(&manifest);
    manifest["preload"] = json!({});
    let empty_bake = bake(&manifest);
    assert_eq!(before.to_json(), empty_bake.to_json());
    assert!(empty_bake.inputs.get("preload").is_none());
    let empty = json!([]);
    assert_eq!(
        module_constants(&meta, false, "web", false, None, &empty).unwrap(),
        module_constants(&meta, false, "web", false, None, &serde_json::Value::Null).unwrap()
    );
}

macro_rules! compiled_definition {
    ($expression:expr) => {
        const DEFINITIONS: &[exact_runner::request::Preload] = $expression;
        const EXPRESSION: &str = stringify!($expression);
    };
}

#[rustfmt::skip]
compiled_definition!(&[
    exact_logic::exact_runner::request::Preload { name: "z-feed", method: "GET", url: &[exact_logic::exact_runner::request::PreloadPart::Literal("https://api.example/"),exact_logic::exact_runner::request::PreloadPart::Secret("session")], headers: &[], body: None, timeout_ms: None, when: exact_logic::exact_runner::request::PreloadWhen::Start, secrets: &["session"] },
    exact_logic::exact_runner::request::Preload { name: "a-feed", method: "POST", url: &[exact_logic::exact_runner::request::PreloadPart::Literal("https://api.example/second")], headers: &[("Authorization", &[exact_logic::exact_runner::request::PreloadPart::Literal("Bearer "),exact_logic::exact_runner::request::PreloadPart::Secret("session")])], body: Some(&[exact_logic::exact_runner::request::PreloadPart::Literal("{"),exact_logic::exact_runner::request::PreloadPart::Secret("session")]), timeout_ms: Some(99), when: exact_logic::exact_runner::request::PreloadWhen::Foreground, secrets: &["session"] }
]);

fn without_token_whitespace(code: &str) -> String {
    let mut result = String::new();
    let (mut quoted, mut escaped) = (false, false);
    for c in code.chars() {
        if quoted {
            result.push(c);
            if escaped {
                escaped = false;
            } else if c == '\\' {
                escaped = true;
            } else if c == '"' {
                quoted = false;
            }
        } else if c == '"' {
            quoted = true;
            result.push(c);
        } else if !c.is_whitespace() {
            result.push(c);
        }
    }
    result
}

#[test]
fn baked_preloads_reach_typescript_rust_and_mixed_accessors_without_an_engine() {
    use exact_runner::{DataError, DataSource, Value};
    struct Rust;
    impl DataSource for Rust {
        fn app_id(&self) -> &str {
            "test.preload"
        }
        fn grants(&self) -> &str {
            "net.fetch https://api.example\nsecret.keep session"
        }
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            panic!("app code ran")
        }
    }
    fn check(source: &dyn DataSource) {
        assert_eq!(source.preloads(), DEFINITIONS);
        let request = source.preloads()[1]
            .fill(|name| {
                assert_eq!(name, "session");
                Some("present".into())
            })
            .unwrap();
        assert_eq!(
            request.headers,
            [("Authorization".into(), "Bearer present".into())]
        );
        assert_eq!(request.body, b"{present");
    }
    let stage = Scratch::new(&std::env::temp_dir()).unwrap();
    let app = &stage.0;
    std::fs::write(app.join("app.json"), r#"{"name":"Preload","app":{"id":"test.preload","name":"Preload"},"deploy":{"store":{"web":"0"}},"preload":{"z-feed":{"url":"https://api.example/{secret:session}"},"a-feed":{"url":"https://api.example/second","method":"POST","headers":{"Authorization":"Bearer {secret:session}"},"body":"{{{secret:session}","when":"foreground","timeoutMs":99}}}"#).unwrap();
    let manifest = contract::Manifest::read(app).unwrap();
    let grants = "net.fetch https://api.example\nsecret.keep session";
    for (name, javascript, rust) in [
        ("typescript", Some(grants), None),
        ("rust", None, Some(grants)),
        (
            "mixed",
            Some("net.fetch https://api.example"),
            Some("secret.keep session"),
        ),
    ] {
        let compat = exact_bake::compatibility_id_sources(
            app,
            "web",
            "wasm32-unknown-unknown",
            &manifest,
            javascript,
            rust,
        )
        .unwrap();
        let expression = contract::preload_expression(&compat.inputs["preload"]);
        assert_eq!(
            without_token_whitespace(&expression),
            without_token_whitespace(EXPRESSION),
            "{name}: the emitted expression must be the one Cargo compiled"
        );
        let meta = json!({"appId":"test.preload", "grants":javascript.unwrap_or(""), "module":{"sha256":"hash"}});
        for (platform, linked) in [("web", false), ("ios", true)] {
            let constants = module_constants(
                &meta,
                false,
                platform,
                linked,
                None,
                &compat.inputs["preload"],
            )
            .unwrap();
            assert!(constants.contains(&format!(
                "pub const PRELOAD: &[exact_logic::exact_runner::request::Preload] = {expression};"
            )));
            if linked {
                assert!(constants.starts_with(linked::LINKED_IDENTITY));
                assert!(
                    constants.find("pub const PRELOAD").unwrap() >= linked::LINKED_IDENTITY.len()
                );
            }
        }
        let entry = contract::rust_entry(
            "ExactEmbeddedData",
            "embedded_data()",
            "off",
            &compat.inputs,
        )
        .unwrap();
        assert!(entry.contains(&format!(".with_preloads({expression})")));
        let module = Module::new(Vec::<u8>::new(), "test.preload", javascript.unwrap_or(""))
            .with_preloads(DEFINITIONS);
        check(&module);
        assert!(!module.is_loaded());
        match name {
            "typescript" => check(&exact_logic::Swappable::off(
                module.placed(exact_runner::Placement::Worker),
            )),
            "rust" => {
                exact_logic::configured!(RustApp, Rust, || exact_logic::Swappable::off(Rust)
                    .with_preloads(DEFINITIONS));
                check(&RustApp::default());
            }
            _ => {
                let mixed = exact_data::Mixed::new(module, Rust, &[], &[]).unwrap();
                check(&mixed);
                check(&exact_logic::Swappable::off(mixed));
            }
        }
    }
}
