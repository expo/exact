//! A module's substitute is a plan constant (`Plan::str` of its row; a
//! runner's plan is never written after boot, and a new plan is a new
//! runner): it is parsed once per runner and host target, not on each read.
//! A scroll dispatch, which polls every module's substitute, parses none
//! once they are read, and a runner booted on another plan reads that plan's.
use super::SUBSTITUTE_PARSES;
use crate::{Answer, DataError, DataSource, Event, Outcome, Runner, ScrollEvent, Store};
use exact_kernel::{Kernel, NodeType};
use exact_plan::{asm::Asm, builder::PlanBuilder, EventKind, Opcode, Plan, TypeKind, Value};

struct Source;

impl DataSource for Source {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!("the fixture answers nothing")
    }
    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::Now(Value::str("x")))
    }
    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        _: Outcome,
    ) -> Result<Answer, DataError> {
        unreachable!("answers now")
    }
}

/// Two native modules, each with a substitute that polls (`counts`), and a
/// view whose `scroll` counts each report in `scrolls` (a commit each).
fn plan(timeout: &str) -> Plan {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    let number = b.primitive(TypeKind::Number);
    for name in ["steps", "flights"] {
        let source = b.source(name, &[], string);
        // A permission read on `target-7` alone (`this_target` drops it elsewhere).
        let substitute = format!(
            r#"{{"counts":{{"unit":"count"}},"timeout":"{timeout}","permission":{{"name":"motion","targets":["target-7"]}}}}"#
        );
        b.module(
            source,
            ("test.motion", "@fixture/test.motion"),
            name,
            false,
            false,
            "never",
            "",
            &substitute,
        );
    }
    let mut zero = Asm::new();
    zero.number(0.);
    let zero = b.code(zero);
    let scrolls = b.slot("scrolls", number, zero);
    let mut body = Asm::new();
    body.load_slot(scrolls)
        .number(1.)
        .simple(Opcode::Add)
        .store_slot(scrolls);
    let body = b.code(body);
    let scrolled = b.action(
        "scrolled",
        &[("x", number), ("y", number)],
        &[scrolls],
        body,
    );
    b.node(
        NodeType::View as u8,
        None,
        None,
        0,
        &[],
        &[(EventKind::Scroll, scrolled, &[])],
        None,
    );
    b.finish().unwrap()
}

fn boot(plan: Plan) -> Runner<Source> {
    Runner::boot_stored(
        plan,
        Source,
        Kernel::with_monospace(),
        vec![],
        Default::default(),
        "/",
    )
    .unwrap()
}

/// A scroll report the view handles: the dispatch commits and its handler runs.
fn scroll(r: &mut Runner<Source>, at: f64) {
    let root = r.roots()[0];
    let before = r.slot("scrolls").cloned();
    let event = ScrollEvent {
        top: at,
        height: 2000.,
        client_height: 600.,
        ..Default::default()
    };
    let advanced = r.dispatch_at(root, Event::Scroll(event), at);
    assert!(advanced.error.is_none(), "{:?}", advanced.error);
    assert!(!advanced.receipts.is_empty(), "the dispatch committed");
    assert_ne!(r.slot("scrolls").cloned(), before, "its handler ran");
}

fn parses() -> usize {
    SUBSTITUTE_PARSES.with(|n| n.get())
}

#[test]
fn a_committed_scroll_dispatch_parses_no_substitute_once_read() {
    // The count is the thread's: from zero here, whatever ran on it before.
    SUBSTITUTE_PARSES.with(|n| n.set(0));
    let mut r = boot(plan("5000"));
    for (row, name) in r.plan.modules.clone().iter().zip(["steps", "flights"]) {
        assert!(r.substitute_polls(row), "{name} polls");
    }
    // Each substitute read once, before the dispatches measured.
    scroll(&mut r, 16.);
    let read = parses();
    assert!(read <= 2, "each substitute parsed at most once: {read}");
    for k in 2..40 {
        scroll(&mut r, 16. * k as f64);
    }
    assert_eq!(
        parses(),
        read,
        "38 committed scroll dispatches parsed no substitute again"
    );
}

#[test]
fn a_runner_on_another_plan_reads_that_plans_substitutes() {
    let first = boot(plan("5000"));
    let second = boot(plan("250"));
    let row = |r: &Runner<Source>| r.plan.modules[0].clone();
    assert_eq!(first.substitute(&row(&first))["timeout"], "5000");
    assert_eq!(second.substitute(&row(&second))["timeout"], "250");
    assert_eq!(
        first.substitute(&row(&first))["timeout"],
        "5000",
        "each runner its own plan's"
    );
}

/// The host target can be said again and again, any string: the cache holds
/// one entry a module row, and each read is for the target said last (a
/// permission on `target-7` alone shows there and nowhere else).
#[test]
fn a_changing_target_keeps_one_substitute_a_module_read_for_it() {
    let mut r = boot(plan("5000"));
    let rows = r.plan.modules.clone();
    for k in 0..50 {
        r.set_module_target(&format!("target-{k}"));
        for row in &rows {
            let permission = r.substitute(row)["permission"].is_object();
            assert_eq!(permission, k == 7, "target-{k}");
        }
        assert!(r.extensions.substitutes.borrow().len() <= rows.len());
    }
    r.set_module_target("ios");
    assert_eq!(r.substitute(&rows[0])["timeout"], "5000");
    assert!(!r.substitute(&rows[0])["permission"].is_object());
}

mod shared_permission {
    use crate::{Dispatch, Runner};
    use exact_kernel::{Kernel, NodeType};
    use exact_plan::{asm::Asm, builder::PlanBuilder, TypeKind};

    #[test]
    fn refreshing_one_shared_permission_waiter_keeps_the_others() {
        let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
        let string = b.primitive(TypeKind::String);
        let source = b.source("position", &[], string);
        b.module(source, ("testgeo", "@fixture/testgeo"), "position", false, false, "when-prompt", "", r#"{"default":"position","permission":{"name":"location","status":"status","field":"state","denied":"NotAllowedError"}}"#);
        let r0 = b.resource("first", "position", &[], string, None);
        b.resource("second", "position", &[], string, None);
        let mut refresh = Asm::new();
        refresh.refresh(r0);
        let code = b.code(refresh);
        b.action("refresh-first", &[], &[], code);
        b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
        let mut r = Runner::boot(
            b.finish().unwrap(),
            (),
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let initial = r.take_requests();
        assert_eq!(initial.len(), 2);
        assert_eq!(r.device_holds().len(), 1);
        for q in &initial {
            assert!(matches!(
                r.dispatch_work(q.request.continuation.unwrap()),
                Dispatch::Held
            ));
        }
        r.act("refresh-first", vec![]).unwrap();
        let fresh = r.take_requests();
        assert_eq!(r.device_holds().len(), 1);
        assert!(r.holds(initial[1].ticket));
        assert_eq!(fresh.len(), 1);
        assert!(matches!(
            r.dispatch_work(fresh[0].request.continuation.unwrap()),
            Dispatch::Held
        ));
        assert!(r.release_work().is_empty());
        let ticket = r.device_holds()[0].ticket;
        r.answer_hold(ticket, &crate::HoldAnswer::Choice("allow".into()))
            .unwrap();
        assert_eq!(
            r.release_work().len(),
            2,
            "both surviving waiters are released"
        );
    }
}

/// A source whose substitute's `default` is written `null` reads none, an
/// option's absent value; one with no `default` at all has no value.
#[test]
fn a_default_written_null_is_a_value() {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    for (name, substitute) in [("none", r#"{"default":null}"#), ("unset", "{}")] {
        let source = b.source(name, &[], string);
        b.module(
            source,
            ("test.push", "@fixture/test.push"),
            name,
            true,
            false,
            "never",
            "",
            substitute,
        );
    }
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    let r = boot(b.finish().unwrap());
    let rows = r.plan.modules.clone();
    assert_eq!(r.value_of(&rows[0]), Some(serde_json::Value::Null));
    assert_eq!(r.value_of(&rows[1]), None);
}

/// A source with an `ungranted` value reads it while its permission is not
/// granted, whatever it was given before, and its own value once granted
/// (a push subscription, LLP 1133.004 §9 P3).
#[test]
fn an_ungranted_value_stands_until_the_permission_is_granted() {
    use super::Decision;
    use serde_json::{json, Value};
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    let source = b.source("subscription", &[], string);
    let substitute = r#"{"default":null,"ungranted":null,"permission":{"name":"notifications","status":"status","field":"state","denied":"NotAllowedError"}}"#;
    b.module(
        source,
        ("test.push", "@fixture/test.push"),
        "subscription",
        true,
        false,
        "never",
        "",
        substitute,
    );
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    let mut r = boot(b.finish().unwrap());
    let row = r.plan.modules[0].clone();
    let token = json!({"apple": {"deviceToken": "ab12"}});
    r.extensions.preferred.insert(r.key(&row), token.clone());
    let read = |r: &Runner<Source>| match r.decide(&row, false, &json!({})) {
        Decision::Now(value) => value,
        _ => panic!("a read answers now"),
    };
    for state in ["prompt", "denied"] {
        r.set_permission("notifications", state);
        assert_eq!(read(&r), Value::Null, "{state}");
    }
    r.set_permission("notifications", "granted");
    assert_eq!(read(&r), token);
}

#[test]
fn member_only_routing_preserves_options_and_uses_the_members_binding() {
    use serde_json::{json, Value as Json};
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    for (extension, name, sub) in [
        (
            "owner",
            "get",
            json!({"implementation": true, "takes": ["publicKey", "mediation", "signal"], "ceremony": {"options": "publicKey"}}),
        ),
        (
            "one",
            "first",
            json!({"implementation": true, "memberOf": "owner.get", "member": "first"}),
        ),
        (
            "two",
            "second",
            json!({"implementation": true, "memberOf": "owner.get", "member": "second"}),
        ),
    ] {
        let source = b.source(&format!("{extension}.{name}"), &[], string);
        b.module(
            source,
            (extension, extension),
            name,
            true,
            false,
            "never",
            "",
            &sub.to_string(),
        );
    }
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    let mut r = boot(b.finish().unwrap());
    let owner = r.plan.modules[0].clone();
    let member = r.plan.modules[1].clone();
    r.extensions
        .bound
        .extend(["owner".into(), "one".into(), "two".into()]);
    let args = json!({"first": {"value": 7}, "mediation": "required", "signal": "signal-1"});
    assert!(r.members_refused(&owner, &args).is_none());
    let body: Json = serde_json::from_slice(&r.forward_body(&owner, args.clone(), None)).unwrap();
    assert_eq!(body["extension"], "one");
    assert_eq!(body["send"], "first");
    assert_eq!(body["args"], args);
    assert!(body["request"].is_number());

    let mut both = args.clone();
    both["publicKey"] = json!({"challenge": "abc"});
    let body: Json = serde_json::from_slice(&r.forward_body(&owner, both.clone(), None)).unwrap();
    assert_eq!(body["extension"], "owner");
    assert_eq!(body["send"], "get");
    assert_eq!(body["args"], both);

    let mut two = args.clone();
    two["second"] = json!({});
    assert_eq!(
        r.members_refused(&owner, &two).unwrap()["error"],
        "NotSupportedError"
    );
    assert!(r.member_forward(&owner, &two).is_none());
    r.extensions.bound.remove("one");
    assert!(r.members_refused(&owner, &both).is_none());
    let refused = r.members_refused(&owner, &args).unwrap();
    assert_eq!(refused["error"], "NotSupportedError");
    assert!(refused["message"].as_str().unwrap().contains("first"));

    r.extensions.agent.insert("one".into());
    r.extensions
        .preferred
        .insert(r.key(&owner), json!({"wrong": true}));
    r.extensions
        .preferred
        .insert(r.key(&member), json!({"member": true}));
    assert!(r.members_refused(&owner, &args).is_none());
    let body: Json = serde_json::from_slice(&r.forward_body(&owner, args.clone(), None)).unwrap();
    assert_eq!(body["preferred"], json!({"member": true}));
    r.extensions.bound.insert("one".into());
    let body: Json = serde_json::from_slice(&r.forward_body(&owner, args, None)).unwrap();
    assert!(body.get("preferred").is_none());
}

#[test]
fn a_dropped_bound_member_reply_retires_its_provisional_request() {
    use crate::{Dispatch, Reply, Request, Work};
    use serde_json::{json, Value as Json};
    use std::sync::{Arc, Mutex};
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    for (extension, name, sub) in [
        (
            "owner",
            "get",
            json!({"takes": ["publicKey"], "ceremony": {"options": "publicKey"}}),
        ),
        (
            "sign-in",
            "identity",
            json!({"memberOf": "owner.get", "member": "identity", "handles": {"handle": "identity"}}),
        ),
    ] {
        let source = b.source(&format!("{extension}.{name}"), &[], string);
        b.module(
            source,
            (extension, extension),
            name,
            true,
            false,
            "never",
            "",
            &sub.to_string(),
        );
    }
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    let mut r = boot(b.finish().unwrap());
    let heard: Arc<Mutex<Vec<Json>>> = Default::default();
    let seen = heard.clone();
    r.native_slot()
        .host(Some(Arc::new(move |body: Vec<u8>, reply: Reply| {
            let body: Json = serde_json::from_slice(&body).unwrap();
            seen.lock().unwrap().push(body.clone());
            reply.send(super::response(&json!({"error": "AbortError"})));
        })));
    r.bind_extensions(&["owner", "sign-in"], &[]);
    let body = json!({"extension": "owner", "send": "get", "args": {"identity": {}}})
        .to_string()
        .into_bytes();
    r.note_handles(42, "data", &body);
    let Dispatch::Run(Work::Later(work)) = r.native_work(&Request::native(body)) else {
        panic!("a bound member's reply is watched")
    };
    work(Reply::new(|_| {}));
    r.dispose_dropped();
    assert!(
        r.extensions.provisional.is_empty(),
        "a completed reply with no handle retires its request"
    );
    assert!(r.extensions.arrived.lock().unwrap().is_empty());
    assert!(heard
        .lock()
        .unwrap()
        .iter()
        .all(|b| b.get("dispose").is_none()));
}

#[test]
fn a_background_action_refuses_foreground_calls_even_in_a_visible_session() {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    let source = b.source("openBrowser", &[], string);
    b.module(
        source,
        ("browser", "@fixture/browser"),
        "openBrowser",
        true,
        false,
        "never",
        "",
        r#"{"origin":"app","foreground":true}"#,
    );
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    let mut r = boot(b.finish().unwrap());
    let row = r.plan.modules[0].clone();
    for hidden in [false, true] {
        r.page.hidden = hidden;
        assert!(
            r.admit(&row, &serde_json::json!({}), super::CallOrigin::Action)
                .is_err(),
            "hidden={hidden}"
        );
    }
}

#[test]
fn an_action_without_a_handler_has_a_terminal_outcome() {
    let mut r = boot(plan("never"));
    assert!(r
        .raise_action(
            "notifications",
            "notificationclick",
            "[]",
            crate::WakeHow {
                id: 0,
                cold: false,
                hidden: false
            },
            "unheard"
        )
        .is_err());
    assert_eq!(
        r.ended_actions(),
        serde_json::json!([{"id":"unheard","outcome":"unheard"}])
    );
}

#[test]
fn a_native_statement_is_owned_until_its_reply_or_expiry() {
    use std::sync::{Arc, Mutex};
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    let source = b.source("showNotification", &[], string);
    b.module(
        source,
        ("notifications", "@fixture/notifications"),
        "showNotification",
        true,
        false,
        "never",
        "",
        r#"{"effect":true,"takes":[]}"#,
    );
    let name = b.str("showNotification");
    let mut body = Asm::new();
    body.command(name, 0);
    let body = b.code(body);
    let action = b.action("clicked", &[], &[], body);
    b.module_event(
        ("notifications", "@fixture/notifications"),
        "notificationclick",
        action,
        "activation",
        16,
        0,
    );
    b.module_event(
        ("geofencing", "@fixture/geofencing"),
        "geofenceenter",
        action,
        "wake",
        16,
        30_000,
    );
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    let mut r = boot(b.finish().unwrap());
    let replies: Arc<Mutex<Vec<crate::Reply>>> = Default::default();
    let held = replies.clone();
    r.native_slot()
        .host(Some(Arc::new(move |body: Vec<u8>, reply: crate::Reply| {
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            if body["send"] != "showNotification" {
                return;
            }
            held.lock().unwrap().push(reply);
        })));
    r.bind_extensions(&["notifications"], &[]);
    for (id, outcome, expired) in [
        ("success", serde_json::json!(null), false),
        (
            "failure",
            serde_json::json!({"error":"NotAllowedError"}),
            false,
        ),
        ("expired", serde_json::json!(null), true),
    ] {
        r.raise_action(
            "notifications",
            "notificationclick",
            "[]",
            crate::WakeHow::default(),
            id,
        )
        .unwrap();
        assert!(r.step_wakes().1.is_none());
        assert!(
            !r.ended_actions()
                .as_array()
                .unwrap()
                .iter()
                .any(|a| a["id"] == id),
            "the native reply has not arrived"
        );
        if expired {
            assert!(r.expire_action(id).1.is_none());
            assert_eq!(
                r.ended_actions().as_array().unwrap().last().unwrap()["outcome"],
                "expired"
            );
        }
        replies
            .lock()
            .unwrap()
            .pop()
            .expect("the handler posted a notification")
            .send(super::response(&outcome));
        r.apply_announced();
        assert!(r.step_wakes().1.is_none());
        let ended = r.ended_actions();
        let mine: Vec<_> = ended
            .as_array()
            .unwrap()
            .iter()
            .filter(|a| a["id"] == id)
            .collect();
        assert_eq!(mine.len(), 1, "{ended}");
        assert_eq!(
            mine[0]["outcome"],
            if expired { "expired" } else { "completed" }
        );
    }
    r.raise_wake(
        "geofencing",
        "geofenceenter",
        "[]",
        crate::WakeHow {
            id: 1,
            cold: false,
            hidden: true,
        },
    )
    .unwrap();
    assert!(r.step_wakes().1.is_none());
    assert_eq!(
        replies.lock().unwrap().len(),
        1,
        "the wake posted a notification"
    );
    let ended = r.take_ended_wakes();
    assert_eq!(
        ended.len(),
        1,
        "a wake's statement does not wait for its native reply"
    );
    assert!(ended[0].contains("completed"), "{ended:?}");
    replies
        .lock()
        .unwrap()
        .pop()
        .unwrap()
        .send(super::response(&serde_json::json!(null)));
    r.apply_announced();
    assert!(r.step_wakes().1.is_none());
    assert!(
        r.take_ended_wakes().is_empty(),
        "a late statement reply cannot end the wake twice"
    );
}

#[test]
fn every_action_ending_survives_a_single_turn_until_the_host_drains_it() {
    use std::sync::{Arc, Mutex};
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    let source = b.source("post", &[], string);
    b.module(
        source,
        ("notifications", "@fixture/notifications"),
        "post",
        true,
        false,
        "never",
        "",
        r#"{"effect":true,"takes":[]}"#,
    );
    let name = b.str("post");
    let mut body = Asm::new();
    body.command(name, 0);
    let body = b.code(body);
    let clicked = b.action("clicked", &[], &[], body);
    b.module_event(
        ("notifications", "@fixture/notifications"),
        "notificationclick",
        clicked,
        "activation",
        16,
        0,
    );
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    let mut r = boot(b.finish().unwrap());
    let replies: Arc<Mutex<Vec<crate::Reply>>> = Default::default();
    let held = replies.clone();
    r.native_slot()
        .host(Some(Arc::new(move |_: Vec<u8>, reply: crate::Reply| {
            held.lock().unwrap().push(reply);
        })));
    r.bind_extensions(&["notifications"], &[]);
    for id in 0..40 {
        r.raise_action(
            "notifications",
            "notificationclick",
            "[]",
            crate::WakeHow::default(),
            &id.to_string(),
        )
        .unwrap();
    }
    assert!(r.step_wakes().1.is_none());
    assert!(r.ended_actions().as_array().unwrap().is_empty());
    r.native_slot()
        .host(Some(Arc::new(|_: Vec<u8>, reply: crate::Reply| {
            reply.send(super::response(&serde_json::json!(null)));
        })));
    replies
        .lock()
        .unwrap()
        .pop()
        .unwrap()
        .send(super::response(&serde_json::json!(null)));
    r.apply_announced();
    assert!(r.step_wakes().1.is_none());
    let ended = r.take_ended_actions();
    assert_eq!(
        ended.as_array().unwrap().len(),
        40,
        "no ending may be evicted before the host reads it"
    );
    assert!(r.take_ended_actions().as_array().unwrap().is_empty());
    assert_eq!(r.ended_actions().as_array().unwrap().len(), 32);
    for id in 0..40 {
        assert_eq!(ended[id]["id"], id.to_string());
        assert_eq!(ended[id]["outcome"], "completed");
    }
}
