use super::*;
use exact_plan::bytes::Writer;
use exact_runner::Dispatch;
use serde_json::json;
use std::sync::Arc;

struct Fixture;
impl DataSource for Fixture {
    fn app_id(&self) -> &str {
        "test.logic"
    }
    fn grants(&self) -> &str {
        "secret.keep token"
    }
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        Ok(Value::Number(1.))
    }
    fn dispatch(&mut self, token: u64, _: &Store) -> Dispatch {
        match token.checked_add(10) {
            Some(registry) => Dispatch::Host(registry),
            None => Dispatch::Missing,
        }
    }
    fn host_continuation(&self, token: u64) -> bool {
        token == 7
    }
    fn keeps_sent(&self, fetch: u64) -> bool {
        fetch == 9
    }
    fn module_delivery(
        &mut self,
        store: &mut Store,
        token: u64,
        outcome: Outcome,
    ) -> Result<Option<exact_runner::Request>, DataError> {
        let Outcome::Response(response) = outcome else {
            panic!("a kept reply")
        };
        store.set_durable("token", std::str::from_utf8(&response.body).unwrap())?;
        Ok(Some(exact_runner::Request::continuation(token + 1)))
    }
}

crate::configured!(ConfiguredFixture, Fixture, || Swappable::off(Fixture));

#[test]
fn configured_entry_preserves_a_kept_delivery_continuation() {
    let mut source = ConfiguredFixture::default();
    let mut store = Store::new(source.grants(), []);
    let outcome = Outcome::Response(exact_runner::Response {
        status: 200,
        headers: vec![],
        body: b"renewed".to_vec(),
    });
    let request = source
        .module_delivery(&mut store, 7, outcome)
        .unwrap()
        .unwrap();
    assert_eq!(request.continuation, Some(8));
    assert_eq!(store.get("token"), Some("renewed"));
    assert!(store.take_writes()[0].durable);
}

#[test]
fn configured_embedded_executor_preserves_browser_continuation_routing() {
    let mut source = ConfiguredFixture::default();
    let store = Store::default();
    assert!(matches!(source.dispatch(7, &store), Dispatch::Host(17)));
    assert!(matches!(
        source.dispatch(u64::MAX, &store),
        Dispatch::Missing
    ));
    assert_eq!(source.placement(), exact_runner::Placement::Main);
}

#[test]
fn configured_entry_preserves_host_continuations_and_kept_fetches() {
    let source = ConfiguredFixture::default();
    assert!(source.host_continuation(7));
    assert!(!source.host_continuation(8));
    assert!(source.keeps_sent(9));
    assert!(!source.keeps_sent(10));
}

#[test]
fn configured_entry_passes_the_shared_secret_backend_and_configuration_errors() {
    #[derive(Default)]
    struct Source(Option<Arc<dyn exact_runner::SharedSecrets>>);
    impl DataSource for Source {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            Ok(Value::Bool(self.0.is_some()))
        }
        fn configure_secrets(
            &mut self,
            secrets: Arc<dyn exact_runner::SharedSecrets>,
        ) -> Result<(), DataError> {
            if self.0.is_some() {
                return Err(DataError::Unavailable("already configured".into()));
            }
            self.0 = Some(secrets);
            Ok(())
        }
    }
    struct Secrets;
    impl exact_runner::SharedSecrets for Secrets {
        fn acquire(
            &self,
            _: &str,
            _: Arc<dyn Fn() + Send + Sync>,
        ) -> Box<dyn exact_runner::SecretWait> {
            panic!("configuration must not acquire a secret")
        }
    }
    crate::configured!(Entry, Source, || Swappable::off(Source::default()));
    let mut source = Entry::default();
    let secrets: Arc<dyn exact_runner::SharedSecrets> = Arc::new(Secrets);
    source.configure_secrets(secrets.clone()).unwrap();
    assert_eq!(source.query("configured", &[]).unwrap(), Value::Bool(true));
    assert!(Arc::ptr_eq(
        source.0.embedded.as_ref().unwrap().0.as_ref().unwrap(),
        &secrets,
    ));
    assert!(matches!(
        source.configure_secrets(secrets),
        Err(DataError::Unavailable(message)) if message == "already configured"
    ));
}

/// A source that names no app id takes the manifest's, which the host keys
/// its storage roots by; one that names its own keeps it, and a replacement
/// keeps the id the binary admitted.
#[test]
fn a_source_without_an_id_takes_the_manifests() {
    #[derive(Default)]
    struct Unnamed;
    impl DataSource for Unnamed {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            Ok(Value::Number(1.))
        }
    }
    crate::configured!(Entry, Unnamed, || Swappable::off(Unnamed)
        .named("com.example.unnamed"));
    assert_eq!(Entry::default().app_id(), "com.example.unnamed");
    assert_eq!(Swappable::off(Unnamed).app_id(), "");
    assert_eq!(
        Swappable::wasm(Fixture).named("com.example.other").app_id(),
        "test.logic"
    );
    let native = Swappable::wasm(Unnamed)
        .with_native("abc", || Unnamed)
        .named("com.example.unnamed");
    let mut plan = exact_plan::builder::PlanBuilder::new(0, 0)
        .finish()
        .unwrap();
    plan.app_id = "com.example.unnamed".into();
    let plan = plan.encode();
    let module = wasm("");
    let mut meta: serde_json::Value = serde_json::from_str(&receipt(&plan, &module)).unwrap();
    meta["appId"] = json!("com.example.unnamed");
    meta["grants"] = json!("");
    meta["source"] = json!("abc");
    let replaced = native
        .replacement(&plan, &meta.to_string(), module)
        .unwrap();
    assert!(replaced.embedded.is_some());
    assert_eq!(replaced.app_id(), "com.example.unnamed");
}

fn encoded(op: u8, f: impl FnOnce(&mut Writer)) -> Vec<u8> {
    let mut w = Writer::default();
    w.u32(abi::ABI);
    w.u8(op);
    f(&mut w);
    w.into_vec()
}
fn wasm(call: &str) -> Vec<u8> {
    wasm_with_contract(call, false)
}
pub(crate) fn wasm_with_contract(call: &str, stateless: bool) -> Vec<u8> {
    let metadata = encoded(0, |w| {
        w.string("test.logic");
        w.string("secret.keep token")
    });
    let unit = encoded(1, |w| {
        w.u8(0);
        Value::Unit.encode(w)
    });
    let answer = encoded(2, |w| {
        w.u8(1);
        w.u32(1);
        w.string("token");
        w.u8(1);
        w.string("new");
        // No socket operation, no overload (LLP 1016.003 D5).
        w.u32(0);
        w.u8(0);
        w.string("");
        w.u8(0);
        Value::Number(42.).encode(w)
    });
    let escape = |bytes: &[u8]| {
        bytes
            .iter()
            .map(|b| format!("\\{b:02x}"))
            .collect::<String>()
    };
    wat::parse_str(format!(
        r#"(module
        (memory (export "memory") 1 2048)
        (global $out (mut i32) (i32.const 0))
        (global $len (mut i32) (i32.const 0))
        (data (i32.const 0) "{}")
        (data (i32.const 128) "{}")
        (data (i32.const 256) "{}")
        (func (export "exact_logic_stateless") (result i32) i32.const {stateless})
        (func (export "exact_logic_abi") (result i32) i32.const {abi})
        (func (export "exact_logic_create") (result i32) i32.const 1)
        (func (export "exact_logic_destroy") (param i32))
        (func (export "exact_logic_alloc") (param i32) (result i32) i32.const 4096)
        (func (export "exact_logic_dealloc") (param i32 i32))
        (func (export "exact_logic_output") (param i32) (result i32) global.get $out)
        (func (export "exact_logic_output_len") (param i32) (result i32) global.get $len)
        (func (export "exact_logic_call") (param i32 i32 i32) (result i32)
          local.get 1 i32.const 4 i32.add i32.load8_u
          i32.eqz
          if i32.const 0 global.set $out i32.const {} global.set $len
          else
            local.get 1 i32.const 4 i32.add i32.load8_u i32.const 3 i32.lt_u
            if i32.const 128 global.set $out i32.const {} global.set $len
            else {} i32.const 256 global.set $out i32.const {} global.set $len
            end
          end
          i32.const 0)
    )"#,
        escape(&metadata),
        escape(&unit),
        escape(&answer),
        metadata.len(),
        unit.len(),
        call,
        answer.len(),
        abi = abi::ABI,
        stateless = u8::from(stateless)
    ))
    .unwrap()
}
fn plan() -> Vec<u8> {
    let mut p = exact_plan::builder::PlanBuilder::new(0, 0)
        .finish()
        .unwrap();
    p.app_id = "test.logic".into();
    p.encode()
}
fn receipt(plan: &[u8], bytes: &[u8]) -> String {
    let card = |file: &str, bytes: &[u8]| json!({"file":file,"bytes":bytes.len(),"sha256":format!("{:x}",Sha256::digest(bytes))});
    json!({"version":1,"kind":"rust","abi":abi::ABI,"appId":"test.logic","grants":"secret.keep token","target":"wasm32-unknown-unknown","executor":"wasm","plan":card("app.plan",plan),"module":card("app.module.wasm",bytes)}).to_string()
}

#[test]
fn deferred_replacement_executes_only_after_activation_and_copies_store_effects() {
    let mut embedded = Swappable::wasm(Fixture);
    assert_eq!(embedded.query("x", &[]).unwrap(), Value::Number(1.));
    let plan = plan();
    let module = wasm("");
    let mut candidate = embedded
        .replacement(&plan, &receipt(&plan, &module), module)
        .unwrap();
    assert!(!candidate.ready());
    assert!(candidate.executor.is_none());
    assert!(candidate.query("x", &[]).is_err());
    candidate.activate_for_validation().unwrap();
    assert!(candidate.ready());
    let mut store = Store::new("secret.keep token", []);
    assert_eq!(
        candidate.answer(&mut store, "x", &[]).unwrap(),
        Answer::Now(Value::Number(42.))
    );
    assert_eq!(store.reads(), 1);
    assert_eq!(store.take_writes()[0].value.as_deref(), Some(&b"new"[..]));
    assert_eq!(embedded.query("x", &[]).unwrap(), Value::Number(1.));
}

/// A candidate built from the source the binary compiled natively runs that
/// native code; one built from other source is interpreted.
#[test]
fn a_candidate_of_the_native_source_runs_natively() {
    let plan = plan();
    let module = wasm("");
    let stamped = |source: &str| {
        let mut meta: serde_json::Value = serde_json::from_str(&receipt(&plan, &module)).unwrap();
        meta["source"] = json!(source);
        meta.to_string()
    };
    let installed = Swappable::wasm(Fixture).with_native("abc", || Fixture);
    let mut same = installed
        .replacement(&plan, &stamped("abc"), module.clone())
        .unwrap();
    assert!(same.embedded.is_some() && same.bytes.is_none());
    assert_eq!(same.query("x", &[]).unwrap(), Value::Number(1.));
    // It stays native-aware: a later candidate is judged the same way.
    let changed = same
        .replacement(&plan, &stamped("def"), module.clone())
        .unwrap();
    assert!(changed.embedded.is_none() && changed.bytes.is_some());
    assert!(changed
        .replacement(&plan, &stamped("abc"), module.clone())
        .unwrap()
        .embedded
        .is_some());
    // A binary that names no native source interprets every candidate.
    let unnamed = Swappable::wasm(Fixture)
        .replacement(&plan, &stamped("abc"), module)
        .unwrap();
    assert!(unnamed.embedded.is_none());
}

#[test]
fn refuses_mismatched_pair_identity_grants_target_and_disabled_policy() {
    let plan = plan();
    let module = wasm("");
    let meta = receipt(&plan, &module);
    for (key, value) in [
        ("appId", json!("another")),
        ("grants", json!("secret.keep extra")),
        ("abi", json!(abi::ABI + 1)),
        ("target", json!(TARGET)),
        ("executor", json!("native")),
    ] {
        let mut changed: serde_json::Value = serde_json::from_str(&meta).unwrap();
        changed[key] = value;
        assert!(
            Swappable::wasm(Fixture)
                .replacement(&plan, &changed.to_string(), module.clone())
                .is_err(),
            "{key}"
        );
    }
    let mut corrupt = module.clone();
    corrupt.push(0);
    assert!(Swappable::wasm(Fixture)
        .replacement(&plan, &meta, corrupt)
        .is_err());
    assert!(Swappable::off(Fixture)
        .replacement(&plan, &meta, module)
        .is_err());
}

#[test]
fn wasm_fuel_memory_imports_and_export_bounds_are_enforced() {
    for body in [
        "(loop $forever br $forever)",
        "i32.const 1024 memory.grow drop",
    ] {
        let plan = plan();
        let module = wasm(body);
        let mut candidate = Swappable::wasm(Fixture)
            .replacement(&plan, &receipt(&plan, &module), module)
            .unwrap();
        candidate.activate().unwrap();
        assert!(matches!(
            candidate.query("x", &[]),
            Err(DataError::Interface(_))
        ));
    }
    let imported =
        wat::parse_str("(module (import \"wasi_snapshot_preview1\" \"fd_write\" (func)))").unwrap();
    assert!(crate::wasm::load(&imported)
        .err()
        .unwrap()
        .contains("imports"));
    let starting = wat::parse_str("(module (func $start) (start $start))").unwrap();
    assert!(crate::wasm::load(&starting).is_err());
    let excessive = wasm("i32.const 0 global.set $out i32.const 0 global.set $len");
    let mut executor = crate::wasm::load(&excessive).unwrap();
    assert!(executor.call(&vec![0; abi::MAX_MESSAGE + 1]).is_err());
}

// A trap leaves the instance's memory as the call left it. The module keeps
// the pages in use at address 1024: "x" needs 600 free pages above them,
// "leaky" takes 300 and traps before it frees them. Of the 1,024 pages the
// executor allows, one instance runs out on the second round.
#[test]
fn a_call_after_a_trap_runs_in_a_fresh_instance() {
    let store = Store::new("secret.keep token", []);
    let good = abi::call_request(&store, "x", &[], None).unwrap().len();
    let used = "i32.const 1024 i32.load";
    let reach = |pages: u32| {
        format!(
            "(block $done (loop $grow memory.size {used} i32.const {pages} i32.add \
            i32.ge_u br_if $done i32.const 1 memory.grow drop br $grow))"
        )
    };
    let module = wasm(&format!(
        "local.get 2 i32.const {good} i32.gt_u
        if {} i32.const 1024 {used} i32.const 300 i32.add i32.store unreachable end
        {}",
        reach(301),
        reach(601),
    ));
    let plan = plan();
    let mut candidate = Swappable::wasm(Fixture)
        .replacement(&plan, &receipt(&plan, &module), module)
        .unwrap();
    candidate.activate().unwrap();
    for _ in 0..3 {
        assert_eq!(candidate.query("x", &[]).unwrap(), Value::Number(42.));
        let trapped = candidate.query("leaky", &[]).unwrap_err();
        assert!(
            matches!(&trapped, DataError::Interface(e) if e.contains("unreachable")),
            "{trapped:?}"
        );
        assert_eq!(candidate.query("x", &[]).unwrap(), Value::Number(42.));
    }
}

// This crate's dev-dependencies compile wasmi's `simd` and `memory64` in, as
// another crate of an app may: the engine still refuses both proposals.
#[test]
fn wasm_refuses_simd_and_memory64_whatever_wasmi_compiles_in() {
    let excessive = wasm("i32.const 0 global.set $out i32.const 0 global.set $len");
    assert!(crate::wasm::load(&excessive).is_ok());
    let simd = wasm("i32.const 0 i32x4.splat drop");
    let refused = crate::wasm::load(&simd).err().unwrap();
    assert!(refused.contains("SIMD"), "{refused}");
    let memory64 = wat::parse_str("(module (memory i64 1))").unwrap();
    let refused = crate::wasm::load(&memory64).err().unwrap();
    assert!(refused.contains("memory64"), "{refused}");
}

// @ref LLP 1029.000 — work in proportion to the input passes the flat base: 100
// loop turns a byte over 60,000 bytes is past 20 million fuel, within the
// per-byte allowance.
#[test]
fn wasm_fuel_grows_with_the_input_it_is_handed() {
    let turns = "local.get 2 i32.const 100 i32.mul global.set $len \
        (loop $l global.get $len i32.const 1 i32.sub global.set $len global.get $len br_if $l)";
    let mut input = vec![0; 60_000];
    input[4] = 9;
    let mut executor = crate::wasm::load(&wasm(turns)).unwrap();
    assert!(executor.call(&input).is_ok(), "{:?}", executor.call(&input));
    assert!(crate::wasm::budget(input.len()) > 20_000_000);
}

// @ref LLP 1029.000 — the same input's loop with no bound still stops, at the
// fuel that input bought. A test of its own, so it runs beside the one above:
// each spends tens of millions of fuel.
#[test]
fn wasm_fuel_stops_a_runaway_loop_at_the_inputs_budget() {
    let mut input = vec![0; 60_000];
    input[4] = 9;
    let mut runaway = crate::wasm::load(&wasm("(loop $forever br $forever)")).unwrap();
    let refused = runaway.call(&input).unwrap_err();
    assert!(
        refused.contains("fuel") && refused.contains("60000 input bytes"),
        "{refused}"
    );
}
