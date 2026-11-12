//! Selective re-asks over held, scripted deliveries (LLP 1162 D3).
#![cfg(exact_js_engine)]

use exact_js::Module;
use exact_plan::Value;
use exact_runner::{Answer, DataSource, Dispatch, Outcome, Request, Response, Store, Target, Work};
use std::collections::{HashMap, VecDeque};

const HBC: &[u8] = include_bytes!(concat!(env!("OUT_DIR"), "/shared-load.hbc"));
const GRANTS: &str = "net.fetch https://shared-load.test\n";

struct Call {
    source: &'static str,
    args: Vec<Value>,
    asks: usize,
    done: Option<Value>,
}

struct Drive {
    module: Module,
    store: Store,
    calls: Vec<Call>,
    held: HashMap<u64, (usize, Request)>,
    ready: VecDeque<(usize, Request, Option<Outcome>)>,
}

impl Drive {
    fn new() -> Self {
        let mut module = Module::loaded(HBC.to_vec(), "test.shared-load", GRANTS).unwrap();
        module.set_budget_ms(f64::INFINITY);
        let plan = contract::compile(
            "shape Row\n  text: string\ncomponent App\n  mutation result as shape Row\n  action completed\n    send result = completed()\n  action row(i: number)\n    send result = row(i)\n  action quiet(i: number)\n    send result = quiet(i)\n  action unrelated(i: number)\n    send result = unrelated(i)\n  action forever\n    send result = forever()\n  action timed\n    send result = timed()\n  view\n    text \"waiters\"\n",
        )
        .unwrap();
        module.bind(&plan);
        Self {
            module,
            store: Store::new(GRANTS, []),
            calls: Vec::new(),
            held: HashMap::new(),
            ready: VecDeque::new(),
        }
    }

    fn start(&mut self, source: &'static str, args: Vec<Value>) -> usize {
        let id = self.calls.len();
        let answer = self
            .module
            .answer_for(Target::Resource(id), &mut self.store, source, &args)
            .unwrap();
        self.calls.push(Call {
            source,
            args,
            asks: 1,
            done: None,
        });
        self.accept(id, answer);
        self.release();
        id
    }

    fn accept(&mut self, id: usize, answer: Answer) {
        match answer {
            Answer::Now(value) => self.calls[id].done = Some(value),
            Answer::Later(request) => {
                if let Some(token) = request.continuation {
                    let dispatch = self.module.dispatch(token, &self.store);
                    self.dispatch(id, request, dispatch);
                } else {
                    self.ready.push_back((id, request, None));
                }
            }
        }
        assert!(self.module.more_for(Target::Resource(id)).is_empty());
    }

    fn dispatch(&mut self, id: usize, request: Request, dispatch: Dispatch) {
        let outcome = match dispatch {
            Dispatch::Held => {
                assert!(self
                    .held
                    .insert(request.continuation.unwrap(), (id, request))
                    .is_none());
                return;
            }
            Dispatch::Again => {
                self.calls[id].asks += 1;
                Dispatch::again_outcome()
            }
            Dispatch::Run(Work::Now(work)) => std::thread::spawn(work).join().unwrap(),
            _ => panic!("expected main-thread work or a re-ask"),
        };
        self.ready.push_back((id, request, Some(outcome)));
    }

    fn release(&mut self) {
        for (token, dispatch) in self.module.release(&self.store) {
            let (id, request) = self.held.remove(&token).expect("released a held call");
            self.dispatch(id, request, dispatch);
        }
    }

    fn deliver(&mut self, id: usize) -> Result<(), exact_runner::DataError> {
        let at = self
            .ready
            .iter()
            .position(|(call, _, _)| *call == id)
            .expect("a ready delivery");
        let (_, request, outcome) = self.ready.remove(at).unwrap();
        let outcome = outcome.unwrap_or_else(|| {
            Outcome::Response(Response {
                status: 200,
                headers: vec![],
                body: request
                    .url
                    .trim_start_matches("https://shared-load.test/")
                    .replace('/', "-")
                    .into_bytes(),
            })
        });
        let call = &self.calls[id];
        let answer = self.module.parse_fetch_for(
            Target::Resource(id),
            request.fetch,
            &mut self.store,
            call.source,
            &call.args,
            outcome,
        )?;
        self.accept(id, answer.expect("a single request per turn"));
        self.release();
        Ok(())
    }

    fn finish(&mut self) {
        for _ in 0..2000 {
            self.release();
            let Some((id, _, _)) = self.ready.front() else {
                break;
            };
            self.deliver(*id).unwrap();
        }
        assert!(
            self.ready.is_empty() && self.held.is_empty(),
            "nothing stranded"
        );
        assert!(self.calls.iter().all(|c| c.done.is_some()));
        assert_eq!(self.module.calls_open(), Some(0));
        assert_eq!(self.module.in_flight(), 0);
    }
}

fn text(text: &str) -> Value {
    Value::record(vec![Value::str(text)])
}

fn shared_rows(n: usize, source: &'static str) {
    let mut drive = Drive::new();
    for i in 0..n {
        drive.start(source, vec![Value::Number(i as f64)]);
    }
    drive.finish();
    for (i, call) in drive.calls.iter().enumerate() {
        assert_eq!(
            call.done,
            Some(text(&format!("{source}-{i} after prefs+labelers #1")))
        );
    }
    let asks: usize = drive.calls.iter().map(|c| c.asks).sum();
    // N initial asks; only the final shared reply moves the N-1 waiters.
    let bound = n + (n - 1);
    assert!(asks <= bound, "{n} answers took {asks} asks, bound {bound}");
}

#[test]
fn twenty_shared_rows_are_asked_only_when_their_promises_move() {
    shared_rows(20, "row");
}

#[test]
fn three_hundred_shared_rows_are_asked_only_when_their_promises_move() {
    shared_rows(300, "quiet");
}

#[test]
fn five_unrelated_deliveries_leave_an_unmoved_waiter_parked() {
    let mut drive = Drive::new();
    drive.start("quiet", vec![Value::Number(0.)]);
    let waiter = drive.start("quiet", vec![Value::Number(1.)]);
    for i in 0..5 {
        let unrelated = drive.start("unrelated", vec![Value::Number(i as f64)]);
        drive.deliver(unrelated).unwrap();
        assert_eq!(
            drive.calls[waiter].asks, 1,
            "delivery {i} did not move the waiter"
        );
    }
    drive.finish();
    assert_eq!(drive.calls[waiter].asks, 2);
}

#[test]
fn a_completed_call_is_asked_without_a_delivery_of_its_own() {
    let mut drive = Drive::new();
    let owner = drive.start("quiet", vec![Value::Number(0.)]);
    let waiter = drive.start("completed", vec![]);
    drive.deliver(owner).unwrap();
    assert_eq!(drive.calls[waiter].asks, 1);
    drive.deliver(owner).unwrap();
    assert_eq!(
        drive.calls[waiter].asks, 2,
        "the plain .then completed its call"
    );
    drive.finish();
    assert_eq!(drive.calls[waiter].done, Some(text("prefs+labelers")));
}

#[test]
fn the_last_answer_pending_on_nothing_is_failed() {
    let mut drive = Drive::new();
    let owner = drive.start("quiet", vec![Value::Number(0.)]);
    let waiter = drive.start("forever", vec![]);
    drive.deliver(owner).unwrap();
    assert_eq!(drive.calls[waiter].asks, 1);
    drive.deliver(owner).unwrap();
    assert_eq!(
        drive.calls[waiter].asks, 2,
        "the last waiter gets its final settle"
    );
    let error = drive.deliver(waiter).unwrap_err();
    assert!(error.to_string().contains("pending on nothing"), "{error}");
    assert!(drive.held.is_empty() && drive.ready.is_empty());
    assert_eq!(drive.module.calls_open(), Some(0));
}

#[test]
fn a_waiters_own_timer_wakes_it_on_the_drivers_clock() {
    let mut drive = Drive::new();
    drive.start("quiet", vec![Value::Number(0.)]);
    let waiter = drive.start("timed", vec![]);
    drive.store.set_clock_ms(49.);
    drive.release();
    assert_eq!(drive.calls[waiter].asks, 1);
    drive.store.set_clock_ms(50.);
    drive.release();
    assert_eq!(drive.calls[waiter].asks, 2);
    drive.deliver(waiter).unwrap();
    assert_eq!(drive.calls[waiter].done, Some(text("timer")));
    drive.finish();
}

#[test]
fn only_the_waiter_at_the_new_storage_head_is_asked() {
    use super::storage::{args, Root, GRANTS as STORAGE_GRANTS};
    let root = Root::new();
    let mut module = root.module();
    module.activate().unwrap();
    let mut drive = Drive {
        module,
        store: Store::new(STORAGE_GRANTS, []),
        calls: Vec::new(),
        held: HashMap::new(),
        ready: VecDeque::new(),
    };
    let a = drive.start("work", args("append", "A1,A2"));
    let b = drive.start("work", args("append", "B"));
    drive.start("work", args("append", "C"));
    assert_eq!(
        drive.calls.iter().map(|c| c.asks).collect::<Vec<_>>(),
        [1, 1, 1]
    );
    drive.deliver(a).unwrap();
    assert_eq!(
        drive.calls.iter().map(|c| c.asks).collect::<Vec<_>>(),
        [1, 2, 1]
    );
    drive.deliver(b).unwrap();
    drive.deliver(b).unwrap();
    assert_eq!(
        drive.calls.iter().map(|c| c.asks).collect::<Vec<_>>(),
        [1, 2, 2]
    );
    drive.finish();
    assert_eq!(
        drive.calls.iter().map(|c| c.asks).collect::<Vec<_>>(),
        [2, 2, 2]
    );
    assert_eq!(
        std::fs::read_to_string(root.0.join("data/log")).unwrap(),
        "A1;B;C;A2;"
    );
}
