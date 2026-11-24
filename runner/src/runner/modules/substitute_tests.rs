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

#[test]
fn an_implementation_substitute_reads_another_packages_display_fixture() {
    use serde_json::{json, Value as Json};
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    for (extension, name, send, sub) in [
        (
            "window-management",
            "getScreenDetails",
            false,
            json!({"default":{"screens":[{"width":80}]}}),
        ),
        (
            "screen-capture",
            "captureScreens",
            true,
            json!({"implementation":true,"from":"window-management.getScreenDetails"}),
        ),
    ] {
        let source = b.source(name, &[], string);
        b.module(
            source,
            (extension, extension),
            name,
            send,
            false,
            "never",
            "",
            &sub.to_string(),
        );
    }
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    let mut runner = boot(b.finish().unwrap());
    runner.extensions.agent.insert("screen-capture".into());
    let capture = runner.plan.modules[1].clone();
    let body: Json =
        serde_json::from_slice(&runner.forward_body(&capture, json!({}), None)).unwrap();
    assert_eq!(body["preferred"]["screens"][0]["width"], 80);
    let screens = json!({"screens":[{"left":-100,"width":100},{"left":0,"width":160}]});
    runner
        .extensions
        .preferred
        .insert("window-management.getScreenDetails".into(), screens.clone());
    let body: Json =
        serde_json::from_slice(&runner.forward_body(&capture, json!({}), None)).unwrap();
    assert_eq!(body["preferred"], screens);
    runner.extensions.bound.insert("screen-capture".into());
    let body: Json =
        serde_json::from_slice(&runner.forward_body(&capture, json!({}), None)).unwrap();
    assert!(
        body.get("preferred").is_none(),
        "a bound capture sees only the real displays"
    );
}

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
fn unchanged_poll_inputs_do_no_discovery_and_every_changed_input_refreshes_it() {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    let source = b.source("sample", &[], string);
    b.module(
        source,
        ("test.poll", "@fixture/test.poll"),
        "sample",
        false,
        true,
        "never",
        "",
        r#"{"counts":{},"poll":{"option":"interval","default":250},"default":"x"}"#,
    );
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    let mut r = boot(b.finish().unwrap());
    r.set_live_clock(Some(Box::new(|| 0.)));
    let scans = || super::POLL_SCANS.with(|n| n.get());
    let before = scans();
    for _ in 0..100 {
        assert_eq!(r.collection_timer_due_ms(), None);
    }
    assert_eq!(scans(), before, "no source has armed a poll");
    let row = r.plan.modules[0].clone();
    let key = r.key(&row);
    r.note_poll(&row);
    assert_eq!(r.collection_timer_due_ms(), Some(250.));
    let before = scans();
    for _ in 0..100 {
        assert_eq!(r.collection_timer_due_ms(), Some(250.));
    }
    assert_eq!(scans(), before, "an unchanged poll does no discovery");

    for input in 0..7 {
        match input {
            0 => {
                r.extensions.polls.insert(key.clone(), 10.);
            }
            1 => {
                r.extensions
                    .readers
                    .entry(key.clone())
                    .or_default()
                    .insert(0, r#"{"interval":500}"#.into());
            }
            2 => {
                r.extensions.failed.insert(key.clone());
            }
            3 => {
                r.extensions
                    .within
                    .entry(key.clone())
                    .or_default()
                    .insert(0, false);
            }
            4 => {
                r.extensions.bound.insert("test.poll".into());
            }
            5 => {
                r.extensions.agent.insert("test.poll".into());
            }
            6 => r.set_module_target("target-changed"),
            _ => unreachable!(),
        }
        let before = scans();
        r.collection_timer_due_ms();
        assert_eq!(scans(), before + 1, "input {input}");
    }
    r.extensions.failed.clear();
    r.extensions.within.clear();
    assert_eq!(r.collection_timer_due_ms(), Some(510.));
    // A stale-source deadline remains independently live when no poll is armed.
    r.extensions.polls.clear();
    let at = r.wall_time().epoch_at_zero + 100.;
    r.extensions.stale.insert(key, at);
    assert_eq!(r.collection_timer_due_ms(), Some(100.));
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
    let key = r.key(&row);
    r.extensions.preferred.insert(key, token.clone());
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
    let key = r.key(&owner);
    r.extensions.preferred.insert(key, json!({"wrong": true}));
    let key = r.key(&member);
    r.extensions.preferred.insert(key, json!({"member": true}));
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
        r#"{"foreground":true}"#,
    );
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    let mut r = boot(b.finish().unwrap());
    let row = r.plan.modules[0].clone();
    for hidden in [false, true] {
        r.page.hidden = hidden;
        // The person's commit joined by a background action: the
        // foreground rule refuses it while the page shows.
        let action = crate::runner::provenance::Cause::of(super::CallOrigin::Action, 0, false);
        let cause = r.join_causes(crate::runner::provenance::Cause::person(), action);
        let refused = r.admit(&row, &serde_json::json!({}), cause);
        assert_eq!(
            refused,
            Err("not in the foreground".into()),
            "hidden={hidden}"
        );
    }
}

/// A model handler that a call joining the device and a background action
/// granted keeps the action's foreground rule, though `device` is the folded
/// origin.
#[test]
fn a_handler_granted_under_a_background_action_refuses_foreground_calls() {
    use crate::runner::provenance::Cause;
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
        r#"{"foreground":true}"#,
    );
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    let mut r = boot(b.finish().unwrap());
    let row = r.plan.modules[0].clone();
    r.page.hidden = false;
    let device = Cause::of(super::CallOrigin::Device, 0, false);
    let action = Cause::of(super::CallOrigin::Action, 0, false);
    let granted = r.join_causes(device, action);
    let handler = r.handler_cause(granted);
    assert_eq!(handler.origin, super::CallOrigin::Device);
    let refused = r.admit(&row, &serde_json::json!({}), handler);
    assert_eq!(refused, Err("not in the foreground".into()));
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
        assert!(r.step_invocations().1.is_none());
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
        assert!(r.step_invocations().1.is_none());
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
    assert!(r.step_invocations().1.is_none());
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
    assert!(r.step_invocations().1.is_none());
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
    assert!(r.step_invocations().1.is_none());
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
    assert!(r.step_invocations().1.is_none());
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

#[derive(Default)]
struct DeferredModuleReader {
    ready: bool,
}

impl DataSource for DeferredModuleReader {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!("the fixture implements answer")
    }

    fn ready(&self) -> bool {
        self.ready
    }

    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::Later(crate::Request::native(
            br#"{"extension":"camera","source":"devices"}"#.to_vec(),
        )))
    }

    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        let Outcome::Response(response) = outcome else {
            panic!("a module response")
        };
        let value: serde_json::Value = serde_json::from_slice(&response.body).unwrap();
        if value["error"] == "NotSupportedError" {
            return Err(DataError::Failed(
                crate::failure::FailureCode::Error,
                "camera.devices is not bound".into(),
                String::new(),
            ));
        }
        Ok(Answer::Now(Value::str(value.as_str().unwrap())))
    }
}

fn deferred_module_reader(data_code: bool) -> Runner<DeferredModuleReader> {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    let source = b.source("devices", &[], string);
    b.module(
        source,
        ("camera", "@fixture/camera"),
        "devices",
        false,
        false,
        "never",
        "",
        "{}",
    );
    b.resource(
        "devices",
        if data_code { "readDevices" } else { "devices" },
        &[],
        string,
        Some(&Value::str("no cameras")),
    );
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    let mut r = Runner::boot(
        b.finish().unwrap(),
        DeferredModuleReader::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    assert!(r.take_requests().is_empty());
    r.data.ready = true;
    r.data_ready().unwrap();
    r
}

fn run_module_request(r: &mut Runner<DeferredModuleReader>, request: &crate::Request) -> Outcome {
    use crate::{Dispatch, Reply, Work};
    match r.native_work(request) {
        Dispatch::Run(Work::Now(work)) => work(),
        Dispatch::Run(Work::Later(work)) => {
            let answer = std::sync::Arc::new(std::sync::Mutex::new(None));
            let out = answer.clone();
            work(Reply::new(move |v| *out.lock().unwrap() = Some(v)));
            let outcome = answer
                .lock()
                .unwrap()
                .take()
                .expect("the fixture answers immediately");
            outcome
        }
        _ => panic!("the module request runs"),
    }
}

fn bind_camera(r: &mut Runner<DeferredModuleReader>) {
    r.native_slot()
        .host(Some(std::sync::Arc::new(|body, reply| {
            let body: serde_json::Value = serde_json::from_slice(&body).unwrap();
            assert_eq!(body["extension"], "camera");
            assert_eq!(body["source"], "devices");
            reply.send(super::response(&serde_json::json!("back camera")));
        })));
    r.bind_extensions(&["camera"], &[]);
    assert!(r.apply_announced().1.is_none());
}

#[test]
fn binding_recovers_a_failed_contract_module_reader_after_data_activation() {
    let mut r = deferred_module_reader(false);
    assert_eq!(r.failed_resources().len(), 1);
    bind_camera(&mut r);
    let requests = r.take_requests();
    assert_eq!(requests.len(), 1);
    let answer = run_module_request(&mut r, &requests[0].request);
    r.fulfill(requests[0].ticket, answer).unwrap();
    assert_eq!(r.resource("devices"), Some(&Value::str("back camera")));
    assert!(r.failed_resources().is_empty());
}

#[test]
fn binding_recovers_a_failed_data_module_reader_after_its_reply() {
    let mut r = deferred_module_reader(true);
    let requests = r.take_requests();
    assert_eq!(requests.len(), 1);
    let answer = run_module_request(&mut r, &requests[0].request);
    r.fulfill(requests[0].ticket, answer).unwrap();
    assert_eq!(r.failed_resources().len(), 1);
    bind_camera(&mut r);
    let requests = r.take_requests();
    assert_eq!(requests.len(), 1);
    let answer = run_module_request(&mut r, &requests[0].request);
    r.fulfill(requests[0].ticket, answer).unwrap();
    assert_eq!(r.resource("devices"), Some(&Value::str("back camera")));
}

#[test]
fn binding_recovers_a_data_module_reader_before_its_failed_reply_lands() {
    let mut r = deferred_module_reader(true);
    let requests = r.take_requests();
    assert_eq!(requests.len(), 1);
    let answer = run_module_request(&mut r, &requests[0].request);
    bind_camera(&mut r);
    assert!(
        r.take_requests().is_empty(),
        "the outstanding reply lands first"
    );
    r.fulfill(requests[0].ticket, answer).unwrap();
    let requests = r.take_requests();
    assert_eq!(requests.len(), 1, "binding asks the failed reader again");
    let answer = run_module_request(&mut r, &requests[0].request);
    r.fulfill(requests[0].ticket, answer).unwrap();
    assert_eq!(r.resource("devices"), Some(&Value::str("back camera")));
    assert!(r.failed_resources().is_empty());
}

#[test]
fn a_data_send_carries_the_grants_it_was_admitted_under() {
    use crate::{Dispatch, Reply, Request, Work};
    use serde_json::{json, Value as Json};
    use std::sync::{Arc, Mutex};
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    for (name, send) in [("devices", false), ("capture", true)] {
        let source = b.source(&format!("camera.{name}"), &[], string);
        b.module(
            source,
            ("camera", "@fixture/camera"),
            name,
            send,
            false,
            "never",
            "",
            "{}",
        );
    }
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    let mut r = boot(b.finish().unwrap());
    let heard: Arc<Mutex<Vec<Json>>> = Default::default();
    let seen = heard.clone();
    r.native_slot()
        .host(Some(Arc::new(move |body: Vec<u8>, reply: Reply| {
            seen.lock()
                .unwrap()
                .push(serde_json::from_slice(&body).unwrap());
            reply.send(super::response(&json!("done")));
        })));
    r.bind_extensions(&["camera"], &[]);
    let scope = "net.fetch https://source.test";
    for (verb, name, grants) in [
        ("send", "capture", Some(scope)),
        ("send", "capture", None),
        ("source", "devices", Some(scope)),
    ] {
        // What the body claims for itself is never forwarded.
        let body =
            json!({"extension": "camera", verb: name, "args": {}, "authority": "net.fetch *"});
        let mut request = Request::native(body.to_string().into_bytes());
        request.grants = grants.map(Into::into);
        match r.native_work(&request) {
            Dispatch::Run(Work::Later(work)) => work(Reply::new(|_| {})),
            Dispatch::Run(Work::Now(work)) => drop(work()),
            _ => panic!("the module request runs"),
        }
    }
    let heard = heard.lock().unwrap();
    assert_eq!(heard[0]["authority"], scope);
    assert_eq!(heard[1]["authority"], json!(r.data.grants()));
    assert_eq!(heard[2].get("authority"), None, "a read starts no work");
}

/// HTML's transient activation, on the session's clock: a call the app makes
/// within five seconds of a person's press says so to its implementation.
#[test]
fn a_call_inside_a_persons_activation_window_says_so() {
    use crate::{Dispatch, Reply, Request, Work};
    use serde_json::{json, Value as Json};
    use std::sync::{Arc, Mutex};
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    let number = b.primitive(TypeKind::Number);
    let source = b.source("camera.capture", &[], string);
    b.module(
        source,
        ("camera", "@fixture/camera"),
        "capture",
        true,
        false,
        "never",
        "",
        "{}",
    );
    let mut nothing = Asm::new();
    nothing.simple(Opcode::Unit).simple(Opcode::Return);
    let nothing = b.code(nothing);
    let pressed = b.action("pressed", &[], &[], nothing);
    let scrolled = b.action("scrolled", &[("x", number), ("y", number)], &[], nothing);
    b.node(
        NodeType::View as u8,
        None,
        None,
        0,
        &[],
        &[
            (EventKind::Press, pressed, &[]),
            (EventKind::Scroll, scrolled, &[]),
        ],
        None,
    );
    let mut r = boot(b.finish().unwrap());
    let heard: Arc<Mutex<Vec<Json>>> = Default::default();
    let seen = heard.clone();
    r.native_slot()
        .host(Some(Arc::new(move |body: Vec<u8>, reply: Reply| {
            seen.lock()
                .unwrap()
                .push(serde_json::from_slice(&body).unwrap());
            reply.send(super::response(&json!("done")));
        })));
    r.bind_extensions(&["camera"], &[]);
    // The data-code call, as the app's own or a tool's; whether its
    // implementation heard it as inside the window.
    let call = |r: &mut Runner<Source>, origin: super::CallOrigin| {
        // What the body claims for itself is never forwarded.
        let body =
            json!({"extension": "camera", "send": "capture", "args": {}, "userActivation": true});
        let request = Request::native(body.to_string().into_bytes());
        match r.as_cause(
            crate::runner::provenance::Cause::of(origin, 0, false),
            |r| r.native_work(&request),
        ) {
            Dispatch::Run(Work::Later(work)) => work(Reply::new(|_| {})),
            Dispatch::Run(Work::Now(work)) => drop(work()),
            _ => panic!("the module request runs"),
        }
        let heard = heard.lock().unwrap().pop().expect("forwarded");
        heard.get("userActivation").is_some_and(|v| v == true)
    };
    let app = super::CallOrigin::App;
    let root = r.roots()[0];
    let at = |r: &mut Runner<Source>, event: Event, ms: f64| {
        let advanced = r.dispatch_at(root, event, ms);
        assert!(advanced.error.is_none(), "{:?}", advanced.error);
    };
    assert!(
        !call(&mut r, app),
        "no input yet: a resource's or a timer's"
    );
    at(&mut r, Event::Scroll(ScrollEvent::default()), 100.);
    assert!(!call(&mut r, app), "a scroll activates nothing");
    at(&mut r, Event::Press, 1_000.);
    assert!(call(&mut r, app), "a press handler's call");
    assert!(call(&mut r, app), "reading the window does not consume it");
    assert!(
        !call(&mut r, super::CallOrigin::Tool),
        "a tool's call inside the window is not the person's"
    );
    assert!(r.advance_timed(5_999.).error.is_none());
    assert!(
        call(&mut r, app),
        "a timer inside the window, as on the web"
    );
    assert!(r.advance_timed(6_000.).error.is_none());
    assert!(!call(&mut r, app), "the window expired");
    at(&mut r, Event::Press, 7_000.);
    assert!(call(&mut r, app), "each press starts it again");
    // On a live clock no commit need come between the press and the call.
    let clock = std::rc::Rc::new(std::cell::Cell::new(7_000.));
    let sample = clock.clone();
    r.set_live_clock(Some(Box::new(move || sample.get())));
    clock.set(20_000.);
    assert!(!call(&mut r, app), "the call's own time is past the window");
}

/// A notification's tap is a person's act though no input event carries it:
/// the activation the host raises opens the window. What a package raises
/// for itself, a wake and a tool open none.
#[test]
fn a_host_activation_opens_the_activation_window_and_no_other_origin_does() {
    use crate::{Dispatch, Reply, Request, Work};
    use serde_json::{json, Value as Json};
    use std::sync::{Arc, Mutex};
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    let source = b.source("camera.capture", &[], string);
    b.module(
        source,
        ("camera", "@fixture/camera"),
        "capture",
        true,
        false,
        "never",
        "",
        "{}",
    );
    let mut nothing = Asm::new();
    nothing.simple(Opcode::Unit).simple(Opcode::Return);
    let nothing = b.code(nothing);
    let heard_it = b.action("heard", &[], &[], nothing);
    let notifications = ("notifications", "@fixture/notifications");
    b.module_event(
        notifications,
        "notificationclick",
        heard_it,
        "activation",
        16,
        0,
    );
    b.module_event(notifications, "notificationshown", heard_it, "event", 16, 0);
    b.module_event(
        ("tide", "@fixture/tide"),
        "tide",
        heard_it,
        "wake",
        16,
        30_000,
    );
    b.node(
        NodeType::View as u8,
        None,
        None,
        0,
        &[],
        &[(EventKind::Press, heard_it, &[])],
        None,
    );
    let mut r = boot(b.finish().unwrap());
    let heard: Arc<Mutex<Vec<Json>>> = Default::default();
    let seen = heard.clone();
    r.native_slot()
        .host(Some(Arc::new(move |body: Vec<u8>, reply: Reply| {
            seen.lock()
                .unwrap()
                .push(serde_json::from_slice(&body).unwrap());
            reply.send(super::response(&json!("done")));
        })));
    r.bind_extensions(&["camera"], &[]);
    // Whether the app's own call now is heard as inside the window.
    let activated = |r: &mut Runner<Source>| {
        let body = json!({"extension": "camera", "send": "capture", "args": {}});
        let request = Request::native(body.to_string().into_bytes());
        let app = crate::runner::provenance::Cause::person();
        match r.as_cause(app, |r| r.native_work(&request)) {
            Dispatch::Run(Work::Later(work)) => work(Reply::new(|_| {})),
            Dispatch::Run(Work::Now(work)) => drop(work()),
            _ => panic!("the module request runs"),
        }
        let heard = heard.lock().unwrap().pop().expect("forwarded");
        heard.get("userActivation").is_some_and(|v| v == true)
    };
    let (app, device) = (super::CallOrigin::App, super::CallOrigin::Device);
    let root = r.roots()[0];
    assert!(r.dispatch_at(root, Event::Press, 1_000.).error.is_none());
    assert!(activated(&mut r));
    assert!(r.advance_timed(7_000.).error.is_none());
    assert!(!activated(&mut r), "the press's window expired");

    // The tap of a notification, which only the host raises as the app's.
    r.raise_module_event("notifications", "notificationclick", "[]", app)
        .unwrap();
    assert!(r.apply_announced().1.is_none());
    assert!(activated(&mut r), "the tap is the person's act");
    assert!(r.advance_timed(11_999.).error.is_none());
    assert!(activated(&mut r), "five seconds from the tap");
    assert!(r.advance_timed(12_000.).error.is_none());
    assert!(!activated(&mut r), "the tap's window expired");

    // A package's own raising of the activation has the device's origin.
    r.raise_module_event("notifications", "notificationclick", "[]", device)
        .unwrap();
    assert!(r.apply_announced().1.is_none());
    assert!(!activated(&mut r), "a device's event");
    // An event that is no activation is the device's whoever raises it.
    r.raise_module_event("notifications", "notificationshown", "[]", app)
        .unwrap();
    assert!(r.apply_announced().1.is_none());
    assert!(!activated(&mut r), "an event that is no activation");
    r.raise_wake("tide", "tide", "[]", crate::WakeHow::default())
        .unwrap();
    assert!(r.step_invocations().1.is_none());
    assert!(!activated(&mut r), "a wake");
    r.act_from(super::CallOrigin::Tool, "heard", vec![])
        .unwrap();
    assert!(!activated(&mut r), "a tool's invocation");
    let ran = r
        .journal()
        .filter(|line| line.ends_with(": heard by heard"));
    assert_eq!(ran.count(), 4, "the three events and the wake ran it");
}
