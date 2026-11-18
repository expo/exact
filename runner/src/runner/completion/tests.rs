use super::*;
use crate::{
    Answer, DataError, FailureKind, InFlight, Request, Response, Store, SurfaceOutcome, Value,
};
use exact_kernel::{Kernel, NodeType};
use exact_plan::{asm::Asm, builder::PlanBuilder, Plan, TypeKind};

#[derive(Default)]
struct Source {
    parses: usize,
    stream: bool,
    long_poll: bool,
    yielding: bool,
    messages: Vec<String>,
    race: bool,
    landed: Vec<(u64, Outcome)>,
    retained: bool,
    reports: Vec<crate::Report>,
    module_yields: bool,
    rounds: u64,
    background_out: bool,
}

impl DataSource for Source {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!()
    }

    fn answer(&mut self, _: &mut Store, _: &str, args: &[Value]) -> Result<Answer, DataError> {
        if args == [Value::Number(0.)] {
            return Ok(Answer::Now(Value::Number(0.)));
        }
        let mut request = if self.retained {
            Request::post_json("https://example.test/answer", "{}")
        } else {
            Request::get("https://example.test/answer")
        };
        request.wake_ms = self.race.then_some(10);
        if self.long_poll {
            request = request.long_poll("since", "seq");
        }
        Ok(if self.stream {
            Answer::stream(request)
        } else {
            Answer::Later(request)
        })
    }

    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        self.parses += 1;
        for message in outcome.messages() {
            self.messages.push(message.data.clone());
        }
        Ok(match outcome {
            Outcome::Message(m) if self.yielding && m.data == "yield" => {
                Answer::Later(Request::continuation::<crate::host_work::TestWork>(9))
            }
            Outcome::Response(r) if r.body == b"more" => {
                Answer::Later(Request::get("https://example.test/next"))
            }
            _ => Answer::Now(Value::Number(self.parses as f64)),
        })
    }

    fn timer_before_reply(
        &mut self,
        _: &Store,
        _: InFlight<'_>,
        _: Option<u64>,
    ) -> Option<(u64, Request)> {
        self.race
            .then(|| (7, Request::continuation::<crate::host_work::TestWork>(8)))
    }

    fn keep_sent(&mut self, _: &Store, flight: &[InFlight<'_>], _: bool) -> Vec<Option<u64>> {
        flight.iter().map(|_| self.retained.then_some(7)).collect()
    }

    fn take_reports(&mut self) -> Vec<crate::Report> {
        std::mem::take(&mut self.reports)
    }

    fn module_delivery(
        &mut self,
        store: &mut Store,
        token: u64,
        outcome: Outcome,
    ) -> Result<Option<Request>, DataError> {
        self.module_landed(store, token, outcome)?;
        Ok(std::mem::take(&mut self.module_yields)
            .then(|| Request::continuation::<crate::host_work::TestWork>(55)))
    }

    fn background(&mut self, _: &Store) -> Option<Request> {
        if self.rounds == 0 || self.background_out {
            return None;
        }
        self.background_out = true;
        Some(Request::continuation::<crate::host_work::TestWork>(
            crate::BACKGROUND,
        ))
    }

    fn background_landed(
        &mut self,
        store: &Store,
        outcome: Outcome,
    ) -> Result<Option<Request>, DataError> {
        self.landed.push((crate::BACKGROUND, outcome));
        self.rounds -= 1;
        self.background_out = false;
        Ok(self.background(store))
    }

    fn module_landed(
        &mut self,
        _: &mut Store,
        token: u64,
        outcome: Outcome,
    ) -> Result<(), DataError> {
        self.landed.push((token, outcome));
        Ok(())
    }
}

fn plan() -> Plan {
    plan_at(1.)
}

fn plan_at(initial: f64) -> Plan {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let number = b.primitive(TypeKind::Number);
    let one = b.constant(&Value::Number(initial));
    let wave = b.slot("wave", number, one);
    let mut arg = Asm::new();
    arg.load_slot(wave);
    let arg = b.code(arg);
    b.resource("answer", "read", &[arg], number, None);
    let mut body = Asm::new();
    body.load_param(0).store_slot(wave);
    let body = b.code(body);
    b.action("wave", &[("value", number)], &[wave], body);
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    b.finish().unwrap()
}

fn boot() -> Runner<Source> {
    Runner::boot(
        plan(),
        Source::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn request(r: &mut Runner<Source>) -> RequestOut {
    let mut requests = r.take_requests();
    assert_eq!(requests.len(), 1);
    requests.pop().unwrap()
}

fn capability(request: &RequestOut) -> CompletionCapability {
    request.completion.as_ref().unwrap().take().unwrap()
}

fn metadata(cap: &CompletionCapability) -> (Identity, u64, Owner, Protocol) {
    let b = cap.binding.0.lock().unwrap();
    (b.identity.clone(), b.ticket, b.owner, b.protocol)
}

fn completion(outcome: Outcome) -> Completion {
    Completion::Reply {
        outcome,
        timing: None,
    }
}

fn response(body: &str) -> Outcome {
    Outcome::Response(Response {
        status: 200,
        headers: vec![],
        body: body.as_bytes().to_vec(),
    })
}

#[test]
fn authority_is_minted_only_when_work_is_handed_out_and_can_be_taken_once() {
    fn send_sync<T: Send + Sync>() {}
    send_sync::<CompletionCapability>();
    let mut r = boot();
    assert!(r.completion_identity.get().is_none());
    assert!(r.pending[0].completion.is_none());
    let request = request(&mut r);
    let copied = request.clone();
    let cap = capability(&request);
    assert_eq!(metadata(&cap).1, request.ticket);
    assert_eq!(metadata(&cap).2, Owner::Ui(Target::Resource(0)));
    assert!(matches!(metadata(&cap).2, Owner::Ui(_)));
    let identity = r.completion_identity.get().unwrap();
    assert!(Arc::ptr_eq(&metadata(&cap).0.session, &identity.session));
    assert!(Arc::ptr_eq(
        &metadata(&cap).0.generation,
        &identity.generation
    ));
    assert!(copied.completion.unwrap().take().is_none());
    assert!(r.pending[0].completion.as_ref().unwrap().take().is_none());
    r.deliver(cap, completion(response("answer")))
        .unwrap()
        .unwrap();
    assert_eq!(r.resource("answer"), Some(&Value::Number(1.)));
}

#[test]
fn typed_outcomes_preserve_the_existing_reply_handler() {
    for outcome in [
        response("answer"),
        Outcome::Storage(vec![1, 2]),
        Outcome::Surface(SurfaceOutcome::Captured(vec![3])),
        Outcome::Surface(SurfaceOutcome::Restored),
        Outcome::Failed {
            kind: FailureKind::Refused,
            message: "admission refused".into(),
        },
    ] {
        let (mut typed, mut integer) = (boot(), boot());
        let (a, b) = (request(&mut typed), request(&mut integer));
        let receipt = typed
            .deliver(capability(&a), completion(outcome.clone()))
            .unwrap();
        assert_eq!(receipt, integer.fulfill(b.ticket, outcome).unwrap());
        assert_eq!(typed.resource("answer"), integer.resource("answer"));
        assert_eq!(typed.data().parses, integer.data().parses);
        assert_eq!(
            typed.journal().collect::<Vec<_>>(),
            integer.journal().collect::<Vec<_>>()
        );
        assert!(
            b.completion.unwrap().take().is_none(),
            "integer entry consumed the issued authority"
        );
    }
}

#[test]
fn measured_integer_entry_consumes_authority_and_keeps_executor_timing() {
    let mut r = boot();
    let request = request(&mut r);
    r.fulfill_measured(
        request.ticket,
        response("answer"),
        Some(crate::CompletionTiming {
            cached: true,
            elapsed_ms: 7,
            delivery_wait_ms: 3,
        }),
    )
    .unwrap()
    .unwrap();
    assert!(request.completion.unwrap().take().is_none());
    assert!(r
        .journal()
        .any(|line| line.contains("HTTP cache; wall 7 ms; delivery wait 3 ms")));
}

#[test]
fn each_continuation_gets_authority_for_its_new_request() {
    let mut r = boot();
    let first = request(&mut r);
    let cap = capability(&first);
    let session = metadata(&cap).0.session.clone();
    let generation = metadata(&cap).0.generation.clone();
    r.deliver(cap, completion(response("more")))
        .unwrap()
        .unwrap();
    let next = request(&mut r);
    let cap = capability(&next);
    assert_ne!(first.ticket, next.ticket);
    assert!(Arc::ptr_eq(&session, &metadata(&cap).0.session));
    assert!(Arc::ptr_eq(&generation, &metadata(&cap).0.generation));
    r.deliver(cap, completion(response("done")))
        .unwrap()
        .unwrap();
    assert_eq!(r.data().parses, 2);
    assert!(!r.has_pending());
}

#[test]
fn timer_race_refusal_keeps_the_continuation_and_adopted_fetch_authorities_apart() {
    for hand_out in [false, true] {
        let mut r = Runner::boot(
            plan(),
            Source {
                race: true,
                ..Default::default()
            },
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let fetch = request(&mut r);
        assert_eq!(r.data_timer_due_ms(), Some(10.));
        r.advance(10.).unwrap();
        let next = r
            .pending()
            .into_iter()
            .find(|(name, _)| name == "answer")
            .unwrap()
            .1;
        assert_ne!(next, fetch.ticket);
        if hand_out {
            let continuation = request(&mut r);
            assert_eq!(continuation.ticket, next);
            assert_eq!(continuation.request.continuation, Some(8));
        }
        r.fulfill_measured(
            next,
            Outcome::Failed {
                kind: FailureKind::Refused,
                message: "continuation refused".into(),
            },
            None,
        )
        .unwrap()
        .unwrap();
        assert!(!r.holds(next), "the refusal settles the continuation");
        assert!(r.holds(fetch.ticket), "the fetch still runs as module work");
        assert!(r.data().landed.is_empty());
        assert_eq!(r.data().parses, 1);
        assert_eq!(r.resource("answer"), Some(&Value::Number(1.)));

        r.deliver(capability(&fetch), completion(response("late")))
            .unwrap();
        assert_eq!(r.data().landed, vec![(7, response("late"))]);
        assert!(!r.holds(fetch.ticket));
        assert!(!r.has_pending());
    }
}

#[test]
fn retired_and_duplicate_integer_replies_keep_their_existing_drop_behavior() {
    let mut r = boot();
    let first = request(&mut r);
    let cap = capability(&first);
    r.act("wave", vec![Value::Number(0.)]).unwrap();
    assert!(r
        .deliver(cap, completion(response("late")))
        .unwrap()
        .is_none());
    assert_eq!(r.data().parses, 0);
    assert_eq!(r.resource("answer"), Some(&Value::Number(0.)));
    assert!(r
        .fulfill(first.ticket, response("duplicate"))
        .unwrap()
        .is_none());
}

#[test]
fn carry_adds_no_cross_session_or_generation_refusal() {
    let (mut original, mut other) = (boot(), boot());
    let old = request(&mut original);
    let fresh = request(&mut other);
    let cap = capability(&old);
    let next = capability(&fresh);
    assert_eq!(metadata(&cap).1, metadata(&next).1);
    assert!(!Arc::ptr_eq(
        &metadata(&cap).0.session,
        &metadata(&next).0.session
    ));
    assert!(!Arc::ptr_eq(
        &metadata(&cap).0.generation,
        &metadata(&next).0.generation
    ));
    // Shadow and refusal are later steps: carry still uses today's ticket semantics.
    other
        .deliver(cap, completion(response("answer")))
        .unwrap()
        .unwrap();
    assert_eq!(other.data().parses, 1);
}

#[test]
fn subscriptions_are_bound_only_at_handoff_and_keep_one_authority_for_ordered_deliveries() {
    for long_poll in [false, true] {
        let mut r = subscription_runner(long_poll, false);
        assert!(r.completion_identity.get().is_none());
        assert!(r.pending[0].completion.is_none());
        let request = request(&mut r);
        let copied = request.clone();
        let slot = request.completion.as_ref().unwrap();
        assert!(
            slot.take().is_none(),
            "a subscription cannot be consumed as a reply"
        );
        let subscription = slot.subscription().unwrap();
        let again = copied.completion.unwrap().subscription().unwrap();
        assert!(Arc::ptr_eq(&subscription.binding.0, &again.binding.0));
        let cap: CompletionCapability = (&subscription).into();
        assert_eq!(metadata(&cap).1, request.ticket);
        assert_eq!(metadata(&cap).2, Owner::Ui(Target::Resource(0)));
        assert!(matches!(metadata(&cap).2, Owner::Ui(_)));
        assert!(matches!(metadata(&cap).3, Protocol::Subscription));
        let identity = r.completion_identity.get().unwrap().clone();
        assert!(Arc::ptr_eq(&metadata(&cap).0.session, &identity.session));
        assert!(Arc::ptr_eq(
            &metadata(&cap).0.generation,
            &identity.generation
        ));

        if long_poll {
            r.deliver(
                &subscription,
                subscription_completion(Outcome::Message(crate::Message {
                    event: crate::LONG_POLL_REPLY.into(),
                    ..Default::default()
                })),
            )
            .unwrap();
            assert_eq!(r.data().parses, 0);
            assert_eq!(r.held_replies(), vec![("answer".into(), 1)]);
        }
        r.deliver(&subscription, subscription_completion(message("one")))
            .unwrap()
            .unwrap();
        // Both entries share the binding without consuming it.
        r.fulfill(request.ticket, message("two")).unwrap().unwrap();
        r.deliver(
            &again,
            subscription_completion(Outcome::Messages(vec![
                crate::Message {
                    data: "three".into(),
                    ..Default::default()
                },
                crate::Message {
                    data: "four".into(),
                    ..Default::default()
                },
            ])),
        )
        .unwrap()
        .unwrap();
        assert_eq!(r.data().messages, ["one", "two", "three", "four"]);
        assert!(r.holds(request.ticket));
        let later: CompletionCapability = (&again).into();
        assert!(Arc::ptr_eq(
            &metadata(&cap).0.session,
            &metadata(&later).0.session
        ));
        assert!(Arc::ptr_eq(
            &metadata(&cap).0.generation,
            &metadata(&later).0.generation
        ));
        assert!(slot.subscription().is_some());

        r.deliver(&subscription, subscription_completion(response("end")))
            .unwrap()
            .unwrap();
        assert!(!r.holds(request.ticket));
        let parses = r.data().parses;
        assert!(r
            .deliver(&subscription, subscription_completion(message("late")))
            .unwrap()
            .is_none());
        assert_eq!(r.data().parses, parses);
    }
}

fn subscription_runner(long_poll: bool, yielding: bool) -> Runner<Source> {
    Runner::boot(
        plan(),
        Source {
            stream: true,
            long_poll,
            yielding,
            ..Default::default()
        },
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn subscription_completion(outcome: Outcome) -> Completion {
    Completion::Subscription {
        outcome,
        timing: None,
    }
}

fn message(text: &str) -> Outcome {
    Outcome::Message(crate::Message {
        data: text.into(),
        ..Default::default()
    })
}

#[test]
fn subscription_deliveries_wait_behind_mapper_turns_in_arrival_order() {
    let mut r = subscription_runner(false, true);
    let stream = request(&mut r);
    let subscription = stream.completion.unwrap().subscription().unwrap();
    for text in ["yield", "two", "three"] {
        r.deliver(&subscription, subscription_completion(message(text)))
            .unwrap();
    }
    r.deliver(&subscription, subscription_completion(response("end")))
        .unwrap();
    assert_eq!(r.data().messages, ["yield"]);
    let ticket = r.requests[0].ticket;
    assert!(r.request_completion(ticket).is_none());
    let turn = request(&mut r);
    assert_eq!(turn.request.continuation, Some(9));
    let cap = capability(&turn);
    assert_eq!(
        metadata(&cap).2,
        Owner::Mapper {
            stream: stream.ticket,
            target: Target::Resource(0)
        }
    );
    assert!(turn.completion.as_ref().unwrap().take().is_none());
    r.deliver(cap, completion(response("resumed"))).unwrap();
    for expected in [vec!["yield", "two"], vec!["yield", "two", "three"]] {
        let held = request(&mut r);
        let cap = capability(&held);
        assert!(matches!(metadata(&cap).3, Protocol::Reply));
        assert!(held.completion.as_ref().unwrap().take().is_none());
        r.deliver(cap, completion(Outcome::Storage(vec![])))
            .unwrap();
        assert_eq!(r.data().messages, expected);
        assert!(r.holds(stream.ticket));
    }
    let end = request(&mut r);
    r.fulfill(end.ticket, Outcome::Storage(vec![])).unwrap();
    assert!(end.completion.as_ref().unwrap().take().is_none());
    assert!(!r.holds(stream.ticket));
}

#[test]
fn retiring_a_subscription_preserves_integer_drop_behavior() {
    let mut r = subscription_runner(false, false);
    let stream = request(&mut r);
    let subscription = stream.completion.unwrap().subscription().unwrap();
    r.act("wave", vec![Value::Number(0.)]).unwrap();
    assert!(!r.holds(stream.ticket));
    assert!(r
        .deliver(&subscription, subscription_completion(message("late")))
        .unwrap()
        .is_none());
    assert_eq!(r.data().parses, 0);
}

#[path = "retained_tests.rs"]
mod retained_tests;
