use crate::{
    Answer, DataError, DataSource, Dispatch, Outcome, Reply, Request, Runner, Store, Work,
};
use exact_kernel::{Kernel, NodeType};
use exact_plan::{builder::PlanBuilder, TypeKind, Value};
use serde_json::{json, Value as Json};
use std::sync::{Arc, Mutex};

struct Grants(&'static str);

impl DataSource for Grants {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!("the fixture answers nothing")
    }
    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::Now(Value::str("x")))
    }
    fn grants(&self) -> &str {
        self.0
    }
}

#[test]
fn data_code_module_calls_enforce_narrowed_request_grants() {
    let app_grants = "device.camera\nfs.read app:/secret\nnet.fetch https://private.api\nauth.passkey example.com\ndevice.notify\n";
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    let specs: [(&str, Json); 5] = [
        ("snap", json!({ "nameGrants": ["device.camera"] })),
        (
            "loadFile",
            json!({ "paths": { "from": { "access": "read" } } }),
        ),
        (
            "fetchJob",
            json!({ "admits": { "requests": { "url": "net.fetch" } } }),
        ),
        (
            "passkey",
            json!({ "nameGrants": { "rp": { "line": "auth.passkey", "matches": "target", "refuse": "SecurityError" } } }),
        ),
        (
            "notify",
            json!({ "effect": true, "nameGrants": ["device.notify"] }),
        ),
    ];
    for (name, sub) in specs {
        let source = b.source(&format!("mix.{name}"), &[], string);
        b.module(
            source,
            ("mix", "@fixture/mix"),
            name,
            true,
            false,
            "never",
            "",
            &sub.to_string(),
        );
    }
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    let mut r = Runner::boot_stored(
        b.finish().unwrap(),
        Grants(app_grants),
        Kernel::with_monospace(),
        vec![],
        Default::default(),
        "/",
    )
    .unwrap();
    let heard: Arc<Mutex<Vec<Json>>> = Default::default();
    let seen = heard.clone();
    r.native_slot()
        .host(Some(Arc::new(move |body: Vec<u8>, reply: Reply| {
            seen.lock()
                .unwrap()
                .push(serde_json::from_slice(&body).unwrap());
            reply.send(super::response(&json!("ok")));
        })));
    r.bind_extensions(&["mix"], &[]);
    let run = |r: &mut Runner<Grants>, name: &str, args: Json, grants: Option<&str>| -> Json {
        let body = json!({ "extension": "mix", "send": name, "args": args });
        let mut req = Request::native(body.to_string().into_bytes());
        req.grants = grants.map(Into::into);
        let outcome = match r.as_person(|r| r.native_work(&req)) {
            Dispatch::Run(Work::Now(work)) => work(),
            Dispatch::Run(Work::Later(work)) => {
                let slot = Arc::new(Mutex::new(None));
                let out = slot.clone();
                work(Reply::new(move |o| *out.lock().unwrap() = Some(o)));
                let settled = slot.lock().unwrap().take().expect("settled");
                settled
            }
            _ => panic!("expected module dispatch"),
        };
        let Outcome::Response(resp) = outcome else {
            panic!("expected response: {outcome:?}");
        };
        serde_json::from_slice(&resp.body).unwrap()
    };
    let cases: [(&str, Json, &str, &str); 5] = [
        ("snap", json!({}), "NotAllowedError", "device.camera"),
        (
            "loadFile",
            json!({ "from": "app:/secret/key.txt" }),
            "NotAllowedError",
            "fs.read app:/secret",
        ),
        (
            "fetchJob",
            json!({ "requests": [{ "url": "https://private.api/data" }] }),
            "NotAllowedError",
            "net.fetch https://private.api",
        ),
        (
            "passkey",
            json!({ "rp": "example.com" }),
            "SecurityError",
            "auth.passkey example.com",
        ),
        ("notify", json!({}), "NotAllowedError", "device.notify"),
    ];
    for (name, args, error, grant) in cases {
        let denied = run(&mut r, name, args.clone(), Some(""));
        assert_eq!(denied["error"], error, "{name} refused under empty scope");
        assert!(
            heard.lock().unwrap().is_empty(),
            "{name} must not reach the host"
        );
        let allowed = run(&mut r, name, args, Some(grant));
        assert_eq!(allowed, json!("ok"), "{name} allowed under matching scope");
        let forwarded = heard.lock().unwrap().pop().expect("forwarded");
        assert_eq!(
            forwarded["authority"], grant,
            "{name} forwards narrowed authority"
        );
    }
}
