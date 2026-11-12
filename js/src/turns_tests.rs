//! Selective readiness against the conservative re-ask rule (LLP 1162 D3).
#![cfg(exact_js_engine)]

use super::Module;
use exact_plan::Value;
use exact_runner::{
    Answer, DataError, DataSource, Dispatch, InFlight, Message, Outcome, Request, Response, Store,
    Target, Work,
};

const HBC: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/liveness.hbc"));
const GRANTS: &str = "net.fetch https://liveness.test\nnet.websocket wss://liveness.test\nfs.read app:/data\nfs.write app:/data\n";

type Event = (usize, Result<Value, DataError>);
type Trace = Vec<Vec<Event>>;

struct Call {
    args: Vec<Value>,
    target: Target,
    requests: Vec<Request>,
}

struct Drive {
    data: Module,
    old: bool,
    store: Store,
    calls: Vec<Call>,
    events: Vec<Event>,
}

fn response() -> Outcome {
    Outcome::Response(Response {
        status: 200,
        headers: vec![],
        body: vec![],
    })
}

fn run(dispatch: Dispatch) -> Outcome {
    match dispatch {
        Dispatch::Again => Dispatch::again_outcome(),
        Dispatch::Run(Work::Now(work)) => work(),
        Dispatch::Run(Work::Later(work)) => {
            let (tx, rx) = std::sync::mpsc::channel();
            work(exact_runner::Reply::new(move |outcome| {
                tx.send(outcome).unwrap();
            }));
            rx.recv_timeout(std::time::Duration::from_secs(20)).unwrap()
        }
        _ => panic!("a runnable turn"),
    }
}

impl Drive {
    fn new(mut data: Module, old: bool) -> Self {
        data.bind(&contract::compile("shape Row\n  text: string\ncomponent App\n  mutation result as shape Row\n  action work(kind: string, role: string)\n    send result = work(kind, role)\n  view\n    text \"liveness\"\n").unwrap());
        data.activate().unwrap();
        let store = Store::new(GRANTS, []);
        Self {
            data,
            old,
            store,
            calls: vec![],
            events: vec![],
        }
    }

    fn start(&mut self, kind: &str, role: &str) -> usize {
        let id = self.calls.len();
        let args = vec![Value::str(kind), Value::str(role)];
        let target = if kind == "mutation" && role == "owner" {
            Target::Mutation(id)
        } else {
            Target::Resource(id)
        };
        let answer = self.data.answer_for(target, &mut self.store, "work", &args);
        self.calls.push(Call {
            args,
            target,
            requests: vec![],
        });
        self.accept(id, answer.map(Some));
        id
    }

    fn accept(&mut self, id: usize, answer: Result<Option<Answer>, DataError>) {
        match answer {
            Ok(Some(Answer::Now(value))) => self.events.push((id, Ok(value))),
            Ok(Some(Answer::Later(request))) => self.calls[id].requests.push(request),
            Ok(None) => {}
            Err(error) => self.events.push((id, Err(error))),
        }
        let more = self.data.more_for(self.calls[id].target);
        self.calls[id].requests.extend(more);
    }

    fn deliver(&mut self, id: usize, at: usize, outcome: Outcome) {
        let request = self.calls[id].requests.remove(at);
        let call = &self.calls[id];
        let answer = self.data.parse_fetch_for(
            call.target,
            request.fetch,
            &mut self.store,
            "work",
            &call.args,
            outcome,
        );
        if request.stream {
            self.calls[id].requests.push(request);
        }
        self.accept(id, answer);
    }

    // Bound a re-park to one turn per sweep so the reference rule cannot spin on held work (LLP 1162 D3).
    fn sweep(&mut self) {
        self.data.refresh_waiting();
        if self.old {
            self.data.readied(None);
        }
        let mut released = self.data.release_ready(self.store.clock_ms());
        for id in 0..self.calls.len() {
            let Some(at) = self.calls[id]
                .requests
                .iter()
                .position(|r| r.continuation.is_some())
            else {
                continue;
            };
            let request = &self.calls[id].requests[at];
            if request
                .due_ms
                .is_some_and(|due| due as f64 > self.store.clock_ms())
            {
                continue;
            }
            let token = request.continuation.unwrap();
            let dispatch = match released.iter().position(|(t, _)| *t == token) {
                Some(at) => released.remove(at).1,
                None if self
                    .data
                    .parked
                    .iter()
                    .any(|(_, p)| p.call == token && p.ticket != super::WAITING) =>
                {
                    self.data.dispatch(token, &self.store)
                }
                None => match self.data.wake(token, self.store.clock_ms()) {
                    Some(dispatch) => dispatch,
                    None => {
                        if !self.data.waiters.contains(&token) {
                            self.data.waiters.push(token);
                        }
                        Dispatch::Held
                    }
                },
            };
            if !matches!(dispatch, Dispatch::Held | Dispatch::Missing) {
                self.deliver(id, at, run(dispatch));
            }
        }
    }

    fn settle(&mut self) {
        for _ in 0..8 {
            self.sweep();
        }
    }
}

struct Root(std::path::PathBuf);
impl Root {
    fn new() -> Self {
        static NEXT: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
        ibex2_sqlite::link();
        Self(std::env::temp_dir().join(format!(
            "exact-liveness-{}-{}",
            std::process::id(),
            NEXT.fetch_add(1, std::sync::atomic::Ordering::Relaxed)
        )))
    }
}
impl Drop for Root {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.0);
    }
}

fn module(root: &Root) -> Module {
    let mut module = Module::new(HBC.to_vec(), "test.liveness", GRANTS);
    module.set_budget_ms(f64::INFINITY);
    module
        .configure_storage(
            root.0.join("data"),
            root.0.join("cache"),
            root.0.join("tmp"),
        )
        .unwrap();
    module
}

fn program(kind: &str, old: bool) -> Trace {
    if kind == "worker" {
        return worker_program();
    }
    let root = Root::new();
    let mut d = Drive::new(module(&root), old);
    let owner = d.start(kind, "owner");
    let waiter = d.start(kind, "waiter");
    if kind == "module" {
        d.start(kind, "second-waiter");
    }
    if kind == "storage-head" {
        d.start(kind, "second-waiter");
    }
    let held = if kind.starts_with("timer-") {
        None
    } else {
        Some(d.start(kind, "held"))
    };
    let mut trace = vec![];
    d.sweep();
    if kind == "module" || kind.starts_with("forget") {
        let calls: Vec<_> = d
            .calls
            .iter()
            .enumerate()
            .filter(|(id, _)| *id != owner)
            .map(|(_, c)| InFlight {
                target: c.target,
                source: "work",
                args: c.args.as_slice(),
                fetch: c.requests.first().and_then(|r| r.fetch),
                continuation: c.requests.first().and_then(|r| r.continuation),
            })
            .collect();
        let kept = if kind == "module" {
            let c = &d.calls[owner];
            vec![InFlight {
                target: c.target,
                source: "work",
                args: c.args.as_slice(),
                fetch: c.requests[0].fetch,
                continuation: None,
            }]
        } else {
            vec![]
        };
        d.data.forgotten(&d.store, &calls);
        if kind == "module" {
            let tokens = d.data.keep_sent(&d.store, &kept, false);
            d.data
                .module_landed(&mut d.store, tokens[0].unwrap(), response())
                .unwrap();
        }
        d.calls[owner].requests.clear();
    } else if kind == "yield-resume" || kind == "storage" || kind == "storage-head" {
        d.settle();
    } else {
        let outcome = if kind.starts_with("stream") || kind.starts_with("socket") {
            Outcome::Message(Message {
                data: "message".into(),
                ..Default::default()
            })
        } else {
            response()
        };
        d.deliver(owner, 0, outcome);
    }
    match kind {
        "overlay" => {
            d.data
                .overlay(
                    "work",
                    &d.calls[owner].args,
                    &Value::record(vec![Value::str("owner")]),
                    &[],
                )
                .unwrap();
        }
        "draw" => {
            d.data
                .engine
                .as_mut()
                .unwrap()
                .call("__exact_draw", ["{}", "", ""])
                .unwrap();
        }
        "retire" => d.data.canvases_retired(&[]),
        _ => {}
    }
    d.settle();
    trace.push(d.events.clone());
    if kind == "timer-set" {
        assert!(!d.events.iter().any(|(id, _)| *id == waiter));
        d.store.set_clock_ms(50.);
        d.settle();
        trace.push(d.events.clone());
    }
    if kind == "mutation" {
        d.deliver(owner, 0, response());
        d.settle();
        trace.push(d.events.clone());
    }
    if kind == "repark" {
        assert!(!d.events.iter().any(|(id, _)| *id == waiter));
        let release = d.start(kind, "release");
        d.deliver(release, 0, response());
        d.settle();
        trace.push(d.events.clone());
    }
    assert!(
        d.events.iter().any(|(id, _)| *id == waiter),
        "{kind} old={old}: waiter stranded: {:?}",
        d.events
    );
    for (id, call) in d
        .calls
        .iter()
        .enumerate()
        .filter(|(_, c)| c.args[1].as_str().unwrap().contains("waiter"))
    {
        assert!(call.requests.is_empty(), "{kind}: waiter {id} still parked");
        assert!(
            d.events.iter().any(|(done, _)| *done == id),
            "{kind}: waiter {id} never answered"
        );
    }
    if let Some(held) = held {
        assert!(
            !d.events.iter().any(|(id, _)| *id == held),
            "the unrelated fetch stays held"
        );
        d.deliver(held, 0, response());
        d.settle();
        trace.push(d.events.clone());
    }
    trace
}

fn worker_program() -> Trace {
    let root = Root::new();
    let mut data = module(&root).placed(exact_runner::Placement::Worker);
    data.bind(&contract::compile("shape Row\n  text: string\ncomponent App\n  mutation result as shape Row\n  action work(kind: string, role: string)\n    send result = work(kind, role)\n  view\n    text \"worker\"\n").unwrap());
    data.activate().unwrap();
    let mut store = Store::new(GRANTS, []);
    let mut requests = Vec::new();
    let mut events = Vec::new();
    for (id, role) in ["owner", "waiter"].into_iter().enumerate() {
        let args = vec![Value::str("worker"), Value::str(role)];
        let Answer::Later(turn) = data
            .answer_for(Target::Resource(id), &mut store, "work", &args)
            .unwrap()
        else {
            panic!("an owner-thread turn");
        };
        let outcome = run(data.dispatch(turn.continuation.unwrap(), &store));
        match data.parse_fetch_for(
            Target::Resource(id),
            None,
            &mut store,
            "work",
            &args,
            outcome,
        ) {
            Ok(Some(Answer::Later(request))) if id == 0 => requests.push((args, request)),
            Err(error) if id == 1 => {
                assert!(
                    error
                        .to_string()
                        .contains("worker-placed source cannot wait"),
                    "{error}"
                );
                events.push((1, Err(error)));
            }
            answer => panic!("a worker's fetch or shared-wait refusal: {answer:?}"),
        }
    }
    // A worker runs one answer at a time; a shared wait is refused under either readiness rule.
    let (args, request) = &requests[0];
    let Answer::Later(turn) = data
        .parse_fetch_for(
            Target::Resource(0),
            request.fetch,
            &mut store,
            "work",
            args,
            response(),
        )
        .unwrap()
        .unwrap()
    else {
        panic!("an owner-thread turn");
    };
    let outcome = run(data.dispatch(turn.continuation.unwrap(), &store));
    let Some(Answer::Now(value)) = data
        .parse_fetch_for(
            Target::Resource(0),
            request.fetch,
            &mut store,
            "work",
            args,
            outcome,
        )
        .unwrap()
    else {
        panic!("the worker's answer");
    };
    events.push((0, Ok(value)));
    vec![events]
}

#[test]
fn shared_waits_match_asking_every_parked_answer_after_each_delivery() {
    let cases = [
        "fetch",
        "completion",
        "worker",
        "then",
        "timer-set",
        "yield-reply",
        "yield-resume",
        "timer-clear",
        "timer-rearm",
        "abort",
        "storage",
        "storage-head",
        "stream-sync",
        "stream-async",
        "socket-sync",
        "socket-async",
        "mutation",
        "module",
        "repark",
        "forget",
        "forget-yield",
        "serialize",
        "serialize-throw",
        "checkpoint",
        "overlay",
        "draw",
        "retire",
    ];
    let failures: Vec<_> = cases
        .into_iter()
        .filter_map(|kind| {
            let old = std::panic::catch_unwind(|| program(kind, true));
            let selective = std::panic::catch_unwind(|| program(kind, false));
            match (old, selective) {
                (Ok(old), Ok(selective)) if old == selective => None,
                (old, selective) => Some(format!("{kind}: old={old:?}, selective={selective:?}")),
            }
        })
        .collect();
    assert!(failures.is_empty(), "{}", failures.join("\n"));
}

#[test]
fn a_timer_cleared_after_its_owner_parked_alone_is_rechecked() {
    let mut traces = Vec::new();
    for old in [true, false] {
        let root = Root::new();
        let mut d = Drive::new(module(&root), old);
        let waiter = d.start("timer-first", "waiter");
        d.sweep();
        let owner = d.start("timer-first", "owner");
        d.deliver(owner, 0, response());
        d.settle();
        let error = d
            .events
            .iter()
            .find(|(id, _)| *id == waiter)
            .expect("clearing the only timer wakes its parked owner")
            .1
            .as_ref()
            .unwrap_err();
        assert!(error.to_string().contains("pending on nothing"), "{error}");
        assert!(d.calls[waiter].requests.is_empty());
        traces.push(d.events);
    }
    assert_eq!(traces[0], traces[1]);
}
