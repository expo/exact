//! Resource request ownership and per-ticket turns (LLP 1162 D2).
#![cfg(exact_js_engine)]

use exact_js::Module;
use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{
    Answer, DataSource, Dispatch, InFlight, Outcome, Placement, Reply, Request, RequestClass,
    Response, Runner, Store, Target, Work,
};

const HBC: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/context.hbc"));
const GRANTS: &str = "net.fetch https://context.test\nsecret.keep token\n";
const SRC: &str = r#"
shape Result
  text: string
component App
  mutation output as shape Result
  action timerSet
    send output = timerSet()
  action batch(n: number)
    send output = batch(n)
  action writes
    send output = writes()
  action wave
    send output = wave()
  action shared
    send output = shared()
  action waiter(name: string)
    send output = waiter(name)
  action ownership(role: string)
    send output = ownership(role)
  action handoff(role: string)
    send output = handoff(role)
  action gone
    send output = gone()
  action rejected
    send output = rejected()
  action throws
    send output = throws()
  view
    text "sets"
"#;

struct RustSource(bool);
impl DataSource for RustSource {
    fn app_id(&self) -> &str {
        "test.context"
    }
    fn grants(&self) -> &str {
        "secret.keep token"
    }
    fn placement(&self) -> exact_runner::Placement {
        if self.0 {
            exact_runner::Placement::Worker
        } else {
            exact_runner::Placement::Main
        }
    }
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, exact_runner::DataError> {
        Ok(Value::Unit)
    }
}

fn module() -> Module {
    let mut module = Module::loaded(HBC.to_vec(), "test.context", GRANTS).unwrap();
    module.set_budget_ms(f64::INFINITY);
    module.bind(&contract::compile(SRC).unwrap());
    module
}

fn store() -> Store {
    Store::new(GRANTS, [])
}
fn later(answer: Answer) -> Request {
    match answer {
        Answer::Later(r) => r,
        other => panic!("{other:?}"),
    }
}
fn value(text: &str) -> Value {
    Value::record(vec![Value::str(text)])
}
fn response(text: &str) -> Outcome {
    Outcome::Response(Response {
        status: 200,
        headers: vec![],
        body: text.as_bytes().to_vec(),
    })
}
fn start(
    m: &mut Module,
    s: &mut Store,
    target: Target,
    source: &str,
    args: &[Value],
) -> Vec<Request> {
    let mut requests = vec![later(m.answer_for(target, s, source, args).unwrap())];
    requests.extend(m.more_for(target));
    assert!(
        m.more_for(target).is_empty(),
        "a turn hands over its set once"
    );
    requests
}
fn deliver(
    m: &mut Module,
    s: &mut Store,
    target: Target,
    source: &str,
    args: &[Value],
    request: &Request,
    text: &str,
) -> Option<Answer> {
    m.parse_fetch_for(target, request.fetch, s, source, args, response(text))
        .unwrap()
}

fn run(dispatch: Dispatch) -> Outcome {
    match dispatch {
        Dispatch::Again => Dispatch::again_outcome(),
        Dispatch::Run(Work::Now(work)) => work(),
        Dispatch::Run(Work::Later(work)) => {
            let (tx, rx) = std::sync::mpsc::channel();
            work(Reply::new(move |outcome| {
                tx.send(outcome).unwrap();
            }));
            rx.recv_timeout(std::time::Duration::from_secs(20))
                .expect("the owner replied")
        }
        _ => panic!("a runnable turn"),
    }
}
fn placed(placement: Placement) -> exact_data::Placed<Module> {
    let mut module = Module::new(HBC.to_vec(), "test.context", GRANTS);
    module.set_budget_ms(f64::INFINITY);
    let mut data = module.placed(placement);
    data.bind(&contract::compile(SRC).unwrap());
    data.activate().unwrap();
    data
}
fn worker_turn(
    data: &mut impl DataSource,
    s: &mut Store,
    target: Target,
    source: &str,
    args: &[Value],
    request: &Request,
) -> Option<Answer> {
    let outcome = run(data.dispatch(request.continuation.expect("an owner turn"), s));
    data.parse_fetch_for(target, request.fetch, s, source, args, outcome)
        .unwrap()
}

#[test]
fn ten_fetches_leave_in_order_after_one_turn_and_land_by_ticket() {
    let (mut m, mut s) = (module(), store());
    let target = Target::Resource(0);
    let args = [Value::Number(10.)];
    let requests = start(&mut m, &mut s, target, "batch", &args);
    assert_eq!(requests.len(), 10);
    for (i, r) in requests.iter().enumerate() {
        assert_eq!(r.url, format!("https://context.test/batch/{i}"));
        assert!(r.fetch.is_some());
    }
    assert!(requests.windows(2).all(|w| w[0].fetch < w[1].fetch));
    for i in (0..10).rev() {
        let answer = deliver(
            &mut m,
            &mut s,
            target,
            "batch",
            &args,
            &requests[i],
            &i.to_string(),
        );
        if i == 0 {
            assert_eq!(answer, Some(Answer::Now(value("0/1/2/3/4/5/6/7/8/9"))));
        } else {
            assert!(answer.is_none());
        }
    }
    assert_eq!(m.calls_open(), Some(0));
}

#[test]
fn one_fetch_can_continue_with_a_set_of_three() {
    let (mut m, mut s) = (module(), store());
    let target = Target::Resource(0);
    let first = start(&mut m, &mut s, target, "wave", &[]);
    assert_eq!(first.len(), 1);
    let answer = deliver(&mut m, &mut s, target, "wave", &[], &first[0], "start").unwrap();
    let mut next = vec![later(answer)];
    next.extend(m.more_for(target));
    assert_eq!(next.len(), 3);
    for (i, request) in next.iter().enumerate() {
        let answer = deliver(&mut m, &mut s, target, "wave", &[], request, &i.to_string());
        if i == 2 {
            assert_eq!(answer, Some(Answer::Now(value("0/1/2"))));
        } else {
            assert!(answer.is_none());
        }
    }
}

#[test]
fn an_unsettled_reply_turn_keeps_its_store_write_read_and_log() {
    let (mut m, mut s) = (module(), store());
    let target = Target::Resource(0);
    let requests = start(&mut m, &mut s, target, "writes", &[]);
    assert_eq!(requests.len(), 3);
    let reads = s.reads();
    assert!(deliver(
        &mut m,
        &mut s,
        target,
        "writes",
        &[],
        &requests[1],
        "second"
    )
    .is_none());
    assert_eq!(s.get("token"), Some("second"));
    assert!(s.reads() > reads);
    assert!(DataSource::take_logs(&mut m)
        .iter()
        .any(|l| l.contains("wrote second")));
}

#[test]
fn a_mutations_promise_all_still_hands_over_one_request_at_a_time() {
    let (mut m, mut s) = (module(), store());
    let target = Target::Mutation(0);
    let args = [Value::Number(3.)];
    let first = start(&mut m, &mut s, target, "batch", &args);
    assert_eq!(first.len(), 1);
    let mut request = first[0].clone();
    for i in 0..3 {
        assert_eq!(request.url, format!("https://context.test/batch/{i}"));
        let answer = deliver(
            &mut m,
            &mut s,
            target,
            "batch",
            &args,
            &request,
            &i.to_string(),
        )
        .unwrap();
        assert!(m.more_for(target).is_empty());
        if i == 2 {
            assert_eq!(answer, Answer::Now(value("0/1/2")));
        } else {
            request = later(answer);
        }
    }
}

#[test]
fn twenty_waiters_own_the_fetches_a_shared_reply_runs() {
    let (mut m, mut s) = (module(), store());
    let shared = Target::Resource(0);
    let first = start(&mut m, &mut s, shared, "shared", &[]);
    let args: Vec<_> = (1..=20)
        .map(|i| vec![Value::str(&format!("row-{i}"))])
        .collect();
    let waits: Vec<_> = args
        .iter()
        .enumerate()
        .map(|(i, a)| start(&mut m, &mut s, Target::Resource(i + 1), "waiter", a).remove(0))
        .collect();
    let tail = later(deliver(&mut m, &mut s, shared, "shared", &[], &first[0], "shared").unwrap());
    assert_eq!(tail.url, "https://context.test/shared-tail");
    assert!(m.more_for(shared).is_empty());
    let waiting: Vec<_> = args
        .iter()
        .enumerate()
        .map(|(i, a)| InFlight {
            target: Target::Resource(i + 1),
            source: "waiter",
            args: a,
            fetch: waits[i].fetch,
            continuation: waits[i].continuation,
        })
        .collect();
    m.forgotten(&s, &waiting);
    assert_eq!(m.in_flight(), 20);
    let mut requests: Vec<Option<Request>> = (0..20).map(|_| None).collect();
    for (i, a) in args.iter().enumerate().rev() {
        let target = Target::Resource(i + 1);
        assert!(waits[i].continuation.is_some());
        let answer = m
            .parse_fetch_for(target, None, &mut s, "waiter", a, Dispatch::again_outcome())
            .unwrap()
            .unwrap();
        let r = later(answer);
        assert_eq!(r.url, format!("https://context.test/row-{}", i + 1));
        assert!(m.more_for(target).is_empty());
        requests[i] = Some(r);
    }
    let flight: Vec<_> = args
        .iter()
        .enumerate()
        .map(|(i, a)| InFlight {
            target: Target::Resource(i + 1),
            source: "waiter",
            args: a,
            fetch: requests[i].as_ref().unwrap().fetch,
            continuation: None,
        })
        .collect();
    m.forgotten(&s, &flight);
    assert_eq!(m.in_flight(), 20);
    for i in (0..20).rev() {
        assert_eq!(
            deliver(
                &mut m,
                &mut s,
                Target::Resource(i + 1),
                "waiter",
                &args[i],
                requests[i].as_ref().unwrap(),
                "mine"
            ),
            Some(Answer::Now(value("mine")))
        );
    }
    assert_eq!(m.calls_open(), Some(0));
}

#[test]
fn an_observer_cannot_claim_a_live_owners_continuation_fetch() {
    let (mut m, mut s) = (module(), store());
    let loader = Target::Resource(0);
    let observer = Target::Resource(1);
    let owner = Target::Resource(2);
    let loader_args = [Value::str("loader")];
    let observer_args = [Value::str("observer")];
    let owner_args = [Value::str("owner")];
    let load = start(&mut m, &mut s, loader, "ownership", &loader_args);
    let b = start(&mut m, &mut s, observer, "ownership", &observer_args).remove(0);
    assert!(matches!(
        m.dispatch(b.continuation.unwrap(), &s),
        Dispatch::Held
    ));
    let a = start(&mut m, &mut s, owner, "ownership", &owner_args).remove(0);
    assert!(matches!(
        m.dispatch(a.continuation.unwrap(), &s),
        Dispatch::Held
    ));
    assert_eq!(
        deliver(
            &mut m,
            &mut s,
            loader,
            "ownership",
            &loader_args,
            &load[0],
            "loaded"
        ),
        Some(Answer::Now(value("loaded")))
    );
    let mut released = m.release(&s);
    assert_eq!(released.len(), 1, "only the live owner's promise moved");
    let (token, dispatch) = released.pop().unwrap();
    assert_eq!(token, a.continuation.unwrap());
    let outcome = run(dispatch);
    let detail = later(
        m.parse_for(owner, &mut s, "ownership", &owner_args, outcome)
            .unwrap(),
    );
    assert_eq!(detail.url, "https://context.test/detail");
    assert!(m.more_for(observer).is_empty());
    assert!(m.release(&s).is_empty(), "the observer stays parked");
    assert_eq!(
        deliver(
            &mut m,
            &mut s,
            owner,
            "ownership",
            &owner_args,
            &detail,
            "detail"
        ),
        Some(Answer::Now(value("detail")))
    );
    let mut released = m.release(&s);
    assert_eq!(
        released.len(),
        1,
        "the detail completed the observer's promise"
    );
    let (token, dispatch) = released.pop().unwrap();
    assert_eq!(token, b.continuation.unwrap());
    assert_eq!(
        m.parse_for(observer, &mut s, "ownership", &observer_args, run(dispatch))
            .unwrap(),
        Answer::Now(value("detail"))
    );
    assert_eq!(m.in_flight(), 0);
    assert_eq!(m.calls_open(), Some(0));
}

#[test]
fn a_waiter_claims_a_finished_owners_continuation_fetch() {
    let (mut m, mut s) = (module(), store());
    let owner = Target::Resource(0);
    let waiter = Target::Resource(1);
    let owner_args = [Value::str("owner")];
    let waiter_args = [Value::str("waiter")];
    let load = start(&mut m, &mut s, owner, "handoff", &owner_args);
    let wait = start(&mut m, &mut s, waiter, "handoff", &waiter_args).remove(0);
    assert!(matches!(
        m.dispatch(wait.continuation.unwrap(), &s),
        Dispatch::Held
    ));
    let other = Target::Resource(2);
    let other_wait = start(&mut m, &mut s, other, "handoff", &waiter_args).remove(0);
    assert!(matches!(
        m.dispatch(other_wait.continuation.unwrap(), &s),
        Dispatch::Held
    ));
    assert_eq!(
        deliver(
            &mut m,
            &mut s,
            owner,
            "handoff",
            &owner_args,
            &load[0],
            "loaded"
        ),
        Some(Answer::Now(value("owner")))
    );
    let mut released = m.release(&s);
    assert_eq!(released.len(), 1, "one waiter claims the orphan");
    let (token, dispatch) = released.pop().unwrap();
    assert_eq!(token, wait.continuation.unwrap());
    assert!(matches!(dispatch, Dispatch::Again));
    let orphan = later(
        m.parse_for(waiter, &mut s, "handoff", &waiter_args, run(dispatch))
            .unwrap(),
    );
    assert_eq!(orphan.url, "https://context.test/orphan");
    assert!(m.release(&s).is_empty(), "the orphan was claimed once");
    assert_eq!(
        deliver(
            &mut m,
            &mut s,
            waiter,
            "handoff",
            &waiter_args,
            &orphan,
            "adopted"
        ),
        Some(Answer::Now(value("adopted")))
    );
    let mut released = m.release(&s);
    assert_eq!(
        released.len(),
        1,
        "the orphan's reply completed the other waiter"
    );
    let (token, dispatch) = released.pop().unwrap();
    assert_eq!(token, other_wait.continuation.unwrap());
    assert_eq!(
        m.parse_for(other, &mut s, "handoff", &waiter_args, run(dispatch))
            .unwrap(),
        Answer::Now(value("adopted"))
    );
    assert_eq!(m.in_flight(), 0);
    assert_eq!(m.calls_open(), Some(0));
}

#[test]
fn letting_go_rejects_all_three_fetches_and_leaves_the_next_answer_alone() {
    let (mut m, mut s) = (module(), store());
    let requests = start(&mut m, &mut s, Target::Resource(0), "gone", &[]);
    assert_eq!(requests.len(), 3);
    m.forgotten(&s, &[]);
    assert_eq!(m.in_flight(), 0);
    assert_eq!(
        m.answer(&mut s, "rejected", &[]).unwrap(),
        Answer::Now(value("3"))
    );
    let args = [Value::Number(3.)];
    let next = start(&mut m, &mut s, Target::Resource(1), "batch", &args);
    for (i, request) in next.iter().enumerate() {
        let answer = deliver(
            &mut m,
            &mut s,
            Target::Resource(1),
            "batch",
            &args,
            request,
            "ok",
        );
        if i == 2 {
            assert_eq!(answer, Some(Answer::Now(value("ok/ok/ok"))));
        } else {
            assert!(answer.is_none());
        }
    }
}

#[test]
fn a_fetch_before_a_throw_is_still_handed_over() {
    let (mut m, mut s) = (module(), store());
    let target = Target::Resource(0);
    let requests = start(&mut m, &mut s, target, "throws", &[]);
    assert_eq!(requests.len(), 1);
    assert_eq!(requests[0].url, "https://context.test/before-throw");
    let error = m
        .parse_fetch_for(
            target,
            requests[0].fetch,
            &mut s,
            "throws",
            &[],
            response("ok"),
        )
        .unwrap_err();
    assert!(format!("{error:?}").contains("after sending"));
}

#[test]
fn the_runner_emits_five_reads_from_one_turn_and_settles_on_the_last_reply() {
    use exact_data::{Mixed, Placed};
    use exact_data_host::Storage;
    use exact_logic::Swappable;
    for (worker, ordered) in [(false, false), (false, true), (true, false), (true, true)] {
        let module = placed(if worker {
            Placement::Worker
        } else {
            Placement::Main
        });
        let data = Mixed::new(module, RustSource(ordered), &["batch"], &[]).unwrap();
        let data = Placed::built(data, exact_runner::Placement::Main, |_, _| {
            unreachable!("inline")
        });
        let mut data = Swappable::off(Storage::new(data));
        data.activate().unwrap();
        let plan = contract::compile(
            r#"
shape Result
  text: string
component App
  resource result = batch(5) as shape Result
  view
    text "sets"
"#,
        )
        .unwrap();
        let mut r = Runner::boot(
            plan,
            data,
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let mut requests = r.take_requests();
        if ordered || worker {
            assert_eq!(requests.len(), 1);
            let turn = requests.remove(0);
            let outcome = run(r.dispatch_work(turn.request.continuation.unwrap()));
            r.fulfill(turn.ticket, outcome).unwrap();
            requests = r.take_requests();
        }
        assert_eq!(requests.len(), 5);
        for (i, request) in requests.iter().enumerate() {
            assert_eq!(request.class, RequestClass::Read);
            assert_eq!(
                request.request.url,
                format!("https://context.test/batch/{i}")
            );
        }
        let initial = r.resource("result").cloned();
        for i in (0..5).rev() {
            r.fulfill(requests[i].ticket, response(&i.to_string()))
                .unwrap();
            if ordered || worker {
                let turns = r.take_requests();
                assert_eq!(turns.len(), 1);
                let turn = &turns[0];
                let outcome = run(r.dispatch_work(turn.request.continuation.unwrap()));
                r.fulfill(turn.ticket, outcome).unwrap();
            }
            assert!(r.take_requests().is_empty());
            if i != 0 {
                assert_eq!(r.resource("result"), initial.as_ref());
                assert!(r.has_pending());
            }
        }
        assert_eq!(r.resource("result"), Some(&value("0/1/2/3/4")));
        assert!(!r.has_pending());
    }
}

#[test]
fn an_ordered_child_keeps_each_fetch_ticket_and_pending_turn_writes() {
    use exact_data::Mixed;
    for worker in [false, true] {
        let child = placed(if worker {
            Placement::Worker
        } else {
            Placement::Main
        });
        let mut data = Mixed::new(child, RustSource(true), &["writes"], &[]).unwrap();
        data.activate().unwrap();
        let mut s = store();
        let target = Target::Resource(0);
        let turn = later(data.answer_for(target, &mut s, "writes", &[]).unwrap());
        let outcome = run(data.dispatch(turn.continuation.unwrap(), &s));
        let first = later(
            data.parse_fetch_for(target, turn.fetch, &mut s, "writes", &[], outcome)
                .unwrap()
                .unwrap(),
        );
        let mut requests = vec![first];
        requests.extend(data.more_for(target));
        assert_eq!(requests.len(), 3);
        for (step, i) in [2, 0, 1].into_iter().enumerate() {
            let turn = later(
                data.parse_fetch_for(
                    target,
                    requests[i].fetch,
                    &mut s,
                    "writes",
                    &[],
                    response(&i.to_string()),
                )
                .unwrap()
                .unwrap(),
            );
            assert_eq!(turn.fetch, requests[i].fetch);
            let before = format!("committed-{step}");
            s.set("token", &before).unwrap();
            let reads = s.reads();
            let outcome = run(data.dispatch(turn.continuation.unwrap(), &s));
            let answer = data
                .parse_fetch_for(target, turn.fetch, &mut s, "writes", &[], outcome)
                .unwrap();
            assert!(s.reads() > reads, "the pending turn's read crosses");
            assert_eq!(s.get("token"), Some(i.to_string().as_str()));
            assert!(
                data.take_logs()
                    .iter()
                    .any(|l| l.contains(&format!("wrote {i} after {before}"))),
                "the turn read the store committed at dispatch"
            );
            if step == 2 {
                assert_eq!(answer, Some(Answer::Now(value("0/1/2"))));
            } else {
                assert!(answer.is_none());
            }
        }
    }
}

#[test]
fn a_timer_adopts_the_fetch_it_names_when_siblings_have_different_wakes() {
    let (mut m, mut s) = (module(), store());
    let target = Target::Resource(0);
    let requests = start(&mut m, &mut s, target, "timerSet", &[]);
    assert_eq!(requests.len(), 2);
    assert_eq!(requests[0].wake_ms, None);
    assert_eq!(requests[1].wake_ms, Some(100));
    let in_flight = InFlight {
        target,
        source: "timerSet",
        args: &[],
        fetch: requests[1].fetch,
        continuation: None,
    };
    let (token, request) = m
        .timer_before_reply(&s, in_flight, requests[1].fetch)
        .unwrap();
    assert_eq!(Some(token), requests[1].fetch);
    assert!(request.continuation.is_some());
}

#[test]
fn a_worker_drops_all_three_fetches_and_ignores_their_late_replies() {
    fn check(mut data: impl DataSource) {
        let mut s = store();
        let target = Target::Resource(0);
        let turn = later(data.answer_for(target, &mut s, "gone", &[]).unwrap());
        let mut old = vec![later(
            worker_turn(&mut data, &mut s, target, "gone", &[], &turn).unwrap(),
        )];
        old.extend(data.more_for(target));
        assert_eq!(old.len(), 3);
        data.forgotten(&s, &[]);
        let check = Target::Mutation(7);
        let turn = later(data.answer_for(check, &mut s, "rejected", &[]).unwrap());
        assert_eq!(
            worker_turn(&mut data, &mut s, check, "rejected", &[], &turn),
            Some(Answer::Now(value("3")))
        );
        let turn = later(data.answer_for(target, &mut s, "gone", &[]).unwrap());
        let mut next = vec![later(
            worker_turn(&mut data, &mut s, target, "gone", &[], &turn).unwrap(),
        )];
        next.extend(data.more_for(target));
        assert_eq!(next.len(), 3);
        for request in &old {
            assert!(data
                .parse_fetch_for(target, request.fetch, &mut s, "gone", &[], response("late"))
                .unwrap()
                .is_none());
        }
        for (i, request) in next.iter().enumerate().rev() {
            let turn = later(
                data.parse_fetch_for(target, request.fetch, &mut s, "gone", &[], response("ok"))
                    .unwrap()
                    .unwrap(),
            );
            let answer = worker_turn(&mut data, &mut s, target, "gone", &[], &turn);
            if i == 0 {
                assert_eq!(answer, Some(Answer::Now(value("done"))));
            } else {
                assert!(answer.is_none());
            }
        }
        assert!(data.more_for(target).is_empty());
    }
    check(placed(Placement::Worker));
    check(
        exact_data::Mixed::new(
            placed(Placement::Worker),
            RustSource(true),
            &["gone", "rejected"],
            &[],
        )
        .unwrap(),
    );
}

#[test]
fn a_worker_mutations_promise_all_hands_over_one_request_at_a_time() {
    fn check(mut data: impl DataSource) {
        let mut s = store();
        let target = Target::Mutation(0);
        let args = [Value::Number(5.)];
        let turn = later(data.answer_for(target, &mut s, "batch", &args).unwrap());
        let mut request =
            later(worker_turn(&mut data, &mut s, target, "batch", &args, &turn).unwrap());
        for i in 0..5 {
            assert_eq!(request.url, format!("https://context.test/batch/{i}"));
            assert!(data.more_for(target).is_empty());
            let turn = later(
                data.parse_fetch_for(
                    target,
                    request.fetch,
                    &mut s,
                    "batch",
                    &args,
                    response(&i.to_string()),
                )
                .unwrap()
                .unwrap(),
            );
            let answer = worker_turn(&mut data, &mut s, target, "batch", &args, &turn).unwrap();
            if i == 4 {
                assert_eq!(answer, Answer::Now(value("0/1/2/3/4")));
            } else {
                request = later(answer);
            }
        }
    }
    check(placed(Placement::Worker));
    check(
        exact_data::Mixed::new(placed(Placement::Worker), RustSource(true), &["batch"], &[])
            .unwrap(),
    );
}

#[test]
fn a_worker_reply_can_hand_over_a_new_set() {
    let mut data = placed(Placement::Worker);
    let mut s = store();
    let target = Target::Resource(0);
    let turn = later(data.answer_for(target, &mut s, "wave", &[]).unwrap());
    let first = later(worker_turn(&mut data, &mut s, target, "wave", &[], &turn).unwrap());
    assert!(data.more_for(target).is_empty());
    let turn = later(
        data.parse_fetch_for(target, first.fetch, &mut s, "wave", &[], response("start"))
            .unwrap()
            .unwrap(),
    );
    let mut next = vec![later(
        worker_turn(&mut data, &mut s, target, "wave", &[], &turn).unwrap(),
    )];
    next.extend(data.more_for(target));
    assert_eq!(next.len(), 3);
    for (i, request) in next.iter().enumerate().rev() {
        let turn = later(
            data.parse_fetch_for(
                target,
                request.fetch,
                &mut s,
                "wave",
                &[],
                response(&i.to_string()),
            )
            .unwrap()
            .unwrap(),
        );
        let answer = worker_turn(&mut data, &mut s, target, "wave", &[], &turn);
        if i == 0 {
            assert_eq!(answer, Some(Answer::Now(value("0/1/2"))));
        } else {
            assert!(answer.is_none());
        }
    }
}

#[test]
fn an_unnamed_reply_requires_a_single_fetch_on_placed_and_mixed_sources() {
    use exact_data::Mixed;
    use exact_runner::DataError;
    let sources: Vec<Box<dyn DataSource>> = vec![
        Box::new(placed(Placement::Worker)),
        Box::new(Mixed::new(placed(Placement::Main), RustSource(true), &["batch"], &[]).unwrap()),
        Box::new(Mixed::new(placed(Placement::Worker), RustSource(true), &["batch"], &[]).unwrap()),
    ];
    for mut data in sources {
        data.activate().unwrap();
        let mut s = store();
        let target = Target::Resource(0);
        let args = [Value::Number(2.)];
        let turn = later(data.answer_for(target, &mut s, "batch", &args).unwrap());
        let outcome = run(data.dispatch(turn.continuation.unwrap(), &s));
        let first = later(
            data.parse_for(target, &mut s, "batch", &args, outcome)
                .unwrap(),
        );
        let mut requests = vec![first];
        requests.extend(data.more_for(target));
        assert_eq!(requests.len(), 2);
        assert_eq!(
            data.parse_for(target, &mut s, "batch", &args, response("ambiguous")),
            Err(DataError::Unavailable(
                "`batch`: more than one request is in flight; the reply must name its fetch".into()
            )),
        );
        for (i, request) in requests.iter().enumerate() {
            let outcome = response(&i.to_string());
            let turn = if i == 0 {
                data.parse_fetch_for(target, request.fetch, &mut s, "batch", &args, outcome)
                    .unwrap()
                    .unwrap()
            } else {
                data.parse_for(target, &mut s, "batch", &args, outcome)
                    .unwrap()
            };
            let turn = later(turn);
            assert_eq!(turn.fetch, request.fetch);
            let outcome = run(data.dispatch(turn.continuation.unwrap(), &s));
            let answer = data
                .parse_fetch_for(target, turn.fetch, &mut s, "batch", &args, outcome)
                .unwrap();
            if i == 0 {
                assert!(answer.is_none());
            } else {
                assert_eq!(answer, Some(Answer::Now(value("0/1"))));
            }
        }
    }
}
