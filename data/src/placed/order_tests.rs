//! A lone worker's ordered set: how each reservation ends (tests).
use super::*;
use std::sync::mpsc::{channel, Receiver};
use std::time::{Duration, Instant};

/// `bump` reads the kept `count` and keeps one more; a kept reply's landing
/// writes `landed`, or exhausts the VM when `exhaust` says so.
#[derive(Default)]
struct Counter {
    identity: exact_runner::ModuleIdentity,
    exhaust: bool,
    /// When set, a kept reply's landing waits for the test before it lands.
    hold: Option<Receiver<()>>,
}
impl DataSource for Counter {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        Ok(Value::Number(0.0))
    }
    fn app_id(&self) -> &str {
        "com.exact.test"
    }
    fn grants(&self) -> &str {
        "secret.keep count\nsecret.keep landed"
    }
    fn module_identity(&self, _: &str) -> Option<exact_runner::ModuleIdentity> {
        Some(self.identity.clone())
    }
    fn answer(&mut self, store: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        let count: u32 = store.get("count").unwrap_or("0").parse().unwrap();
        store.set("count", &(count + 1).to_string())?;
        Ok(Answer::Now(Value::Number(f64::from(count + 1))))
    }
    fn module_landed(&mut self, store: &mut Store, _: u64, _: Outcome) -> Result<(), DataError> {
        if let Some(hold) = &self.hold {
            hold.recv().expect("the test releases the kept reply");
        }
        if self.exhaust {
            self.identity.retire("heap exhausted".into());
            return Err(DataError::Failed(
                exact_runner::failure::FailureCode::Error,
                "heap exhausted".into(),
                String::new(),
            ));
        }
        store.set("landed", "yes")?;
        Ok(())
    }
}

fn worker(source: Counter) -> (Placed<Counter>, Store) {
    let mut placed = Placed::new(source, Placement::Worker);
    placed.activate().unwrap();
    let store = Store::new("secret.keep count\nsecret.keep landed", []);
    (placed, store)
}

fn turn(placed: &mut Placed<Counter>, store: &mut Store, target: Target) -> u64 {
    let Answer::Later(request) = placed.answer_for(target, store, "bump", &[]).unwrap() else {
        panic!("an owner turn")
    };
    request.continuation.unwrap()
}

fn run(dispatch: Dispatch) -> Receiver<Outcome> {
    let Dispatch::Run(Work::Later(work)) = dispatch else {
        panic!("an owner turn")
    };
    let (tx, rx) = channel();
    work(Reply::new(move |outcome| {
        let _ = tx.send(outcome);
    }));
    rx
}

/// A kept reply sent to the owner now (nothing holds the Store): its
/// continuation, which the test makes differ from its kept token.
fn delivery(placed: &mut Placed<Counter>, store: &mut Store, kept: u64) -> u64 {
    placed.kept.insert(kept, Some(9));
    let continuation = placed
        .module_delivery(store, kept, Dispatch::again_outcome())
        .unwrap()
        .unwrap()
        .continuation
        .unwrap();
    assert_ne!(continuation, kept);
    continuation
}

/// Wait until `release` hands out `token`'s envelope.
fn envelope(placed: &mut Placed<Counter>, store: &Store, token: u64) -> Outcome {
    // The host dispatches the continuation first; it is held until the
    // owner's reply fills its slot.
    match placed.dispatch(token, store) {
        Dispatch::Run(Work::Now(reply)) => return reply(),
        Dispatch::Held => {}
        _ => panic!("a delivery is held or ready"),
    }
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        let mut ready = placed.release(store);
        if let Some(i) = ready.iter().position(|(t, _)| *t == token) {
            let (_, Dispatch::Run(Work::Now(reply))) = ready.remove(i) else {
                panic!("the delivery's envelope")
            };
            return reply();
        }
        assert!(Instant::now() < deadline, "{token} never came back");
        std::thread::sleep(Duration::from_millis(5));
    }
}

/// Round 2's wedge: a kept reply exhausts the VM on the owner, with a turn
/// waiting behind it; the runner then drops its ticket without a second
/// `module_delivery`. The failed generation frees the Store and drops the
/// dead generation's waiters: through the host's wake (which leads to
/// `release_work`, whose `take_aborted` syncs the generation first), and on
/// the next dispatch alike.
#[test]
fn a_delivery_whose_vm_fails_does_not_hold_the_next_generation() {
    for wake in [true, false] {
        let (go, hold) = channel();
        let (mut placed, mut store) = worker(Counter {
            exhaust: true,
            hold: Some(hold),
            ..Default::default()
        });
        let (woke, heard) = channel();
        placed.abort_wake(std::sync::Arc::new(move || {
            let _ = woke.send(());
        }));
        let waiting = turn(&mut placed, &mut store, Target::Mutation(1));
        let continuation = delivery(&mut placed, &mut store, 77);
        assert!(matches!(
            placed.dispatch(continuation, &store),
            Dispatch::Held
        ));
        assert!(matches!(placed.dispatch(waiting, &store), Dispatch::Held));
        // Only now does the owner run the delivery and exhaust the VM.
        go.send(()).unwrap();
        let deadline = Instant::now() + Duration::from_secs(5);
        while placed
            .generation
            .as_ref()
            .and_then(|g| g.failure())
            .is_none()
        {
            assert!(Instant::now() < deadline, "the delivery never ran");
            std::thread::sleep(Duration::from_millis(5));
        }
        if wake {
            heard
                .recv_timeout(Duration::from_secs(5))
                .expect("the owner's failure wakes the host");
            placed.take_aborted();
            assert!(placed.running.is_none() && placed.held.is_empty());
            assert!(placed.deliveries_of.is_empty());
            assert!(
                placed.release(&store).is_empty(),
                "the dead generation's waiter is dropped"
            );
        }
        let next = turn(&mut placed, &mut store, Target::Mutation(0));
        assert!(
            matches!(placed.dispatch(next, &store), Dispatch::Run(_)),
            "the next generation's turn runs (wake {wake})"
        );
    }
}

/// A delivery's final acknowledgement frees the Store whatever it holds (a
/// failure, `Aborted`, a refusal): the turn waiting behind it goes next.
#[test]
fn a_delivery_s_failed_acknowledgement_lets_the_next_turn_go() {
    for outcome in [
        Outcome::Failed {
            kind: exact_runner::FailureKind::Aborted,
            message: "aborted".into(),
        },
        Outcome::Failed {
            kind: exact_runner::FailureKind::Refused,
            message: "refused".into(),
        },
    ] {
        let (mut placed, mut store) = worker(Counter::default());
        let continuation = delivery(&mut placed, &mut store, 77);
        let waiting = turn(&mut placed, &mut store, Target::Mutation(1));
        assert!(matches!(placed.dispatch(waiting, &store), Dispatch::Held));
        let _ = envelope(&mut placed, &store, continuation);
        let _ = placed.module_delivery(&mut store, 77, outcome);
        let released: Vec<u64> = placed.release(&store).iter().map(|(t, _)| *t).collect();
        assert_eq!(released, [waiting]);
    }
}

/// A delivery's continuation discarded while it runs, or while it waits,
/// frees the Store; after its outcome slot was consumed too; twice is a
/// no-op.
#[test]
fn a_discarded_delivery_frees_the_store() {
    // Running, its envelope already handed out.
    let (mut placed, mut store) = worker(Counter::default());
    let continuation = delivery(&mut placed, &mut store, 77);
    let _ = envelope(&mut placed, &store, continuation);
    placed.discard(continuation);
    placed.discard(continuation);
    assert!(placed.running.is_none() && placed.deliveries_of.is_empty());
    let next = turn(&mut placed, &mut store, Target::Mutation(0));
    assert!(matches!(placed.dispatch(next, &store), Dispatch::Run(_)));

    // Waiting behind a running turn.
    let (mut placed, mut store) = worker(Counter::default());
    let a = turn(&mut placed, &mut store, Target::Mutation(0));
    let b = turn(&mut placed, &mut store, Target::Mutation(1));
    let running = run(placed.dispatch(a, &store));
    let continuation = delivery(&mut placed, &mut store, 77);
    assert!(matches!(placed.dispatch(b, &store), Dispatch::Held));
    placed.discard(continuation);
    let outcome = running.recv_timeout(Duration::from_secs(5)).unwrap();
    placed
        .parse_for(Target::Mutation(0), &mut store, "bump", &[], outcome)
        .unwrap();
    let released: Vec<u64> = placed.release(&store).iter().map(|(t, _)| *t).collect();
    assert_eq!(released, [b], "the discarded delivery never takes its turn");
}

/// A dispatched turn whose continuation is discarded leaves no stage: a
/// later call with the same key parses its own reply and frees the Store.
#[test]
fn a_discarded_dispatched_turn_leaves_no_stage_for_its_key() {
    let (mut placed, mut store) = worker(Counter::default());
    let a = turn(&mut placed, &mut store, Target::Mutation(0));
    drop(placed.dispatch(a, &store));
    placed.discard(a);
    let b = turn(&mut placed, &mut store, Target::Mutation(0));
    let outcome = run(placed.dispatch(b, &store))
        .recv_timeout(Duration::from_secs(5))
        .unwrap();
    placed
        .parse_for(Target::Mutation(0), &mut store, "bump", &[], outcome)
        .unwrap();
    assert!(placed.running.is_none(), "b's reply freed the Store");
    let c = turn(&mut placed, &mut store, Target::Mutation(1));
    assert!(matches!(placed.dispatch(c, &store), Dispatch::Run(_)));
}

/// A worker granted no Store name is one ordered set too (LLP 1027.002
/// Change 2: the worker module alone is a set).
#[test]
fn a_grantless_worker_orders_its_turns() {
    struct Bare;
    impl DataSource for Bare {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            Ok(Value::Number(0.0))
        }
        fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
            Ok(Answer::Now(Value::Unit))
        }
    }
    let mut placed = Placed::new(Bare, Placement::Worker);
    placed.activate().unwrap();
    let mut store = Store::new("", []);
    let mut started = |target| {
        let Answer::Later(r) = placed.answer_for(target, &mut store, "x", &[]).unwrap() else {
            panic!("an owner turn")
        };
        r.continuation.unwrap()
    };
    let (a, b) = (started(Target::Mutation(0)), started(Target::Mutation(1)));
    let _running = placed.dispatch(a, &store);
    assert!(matches!(placed.dispatch(b, &store), Dispatch::Held));
}

/// On an owner, a token nothing here recorded (unknown, or ended) is never
/// forwarded to the child, whose own tokens it would collide with: a built
/// source keeps its template as the child it would have reached.
#[test]
fn an_unknown_token_is_not_the_child_s() {
    #[derive(Clone)]
    struct Hears(std::sync::Arc<std::sync::Mutex<Vec<u64>>>);
    impl DataSource for Hears {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            Ok(Value::Number(0.0))
        }
        fn discard(&mut self, token: u64) {
            self.0.lock().unwrap().push(token);
        }
    }
    fn build(template: &Hears, _: std::sync::Arc<std::sync::atomic::AtomicBool>) -> Obtain<Hears> {
        let copy = template.clone();
        Box::new(move || Ok(copy))
    }
    let heard = std::sync::Arc::default();
    let mut placed = Placed::built(
        Hears(std::sync::Arc::clone(&heard)),
        Placement::Worker,
        build,
    );
    placed.activate().unwrap();
    assert!(placed.inner.is_some(), "the template stays here");
    placed.discard(4242);
    assert!(heard.lock().unwrap().is_empty());
}
