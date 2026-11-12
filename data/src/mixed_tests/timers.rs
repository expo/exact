use super::{run, token, value_of};
use crate::{Mixed, Placed};
use exact_runner::{
    Answer, DataError, DataSource, Dispatch, InFlight, Outcome, Placement, Request, Store, Target,
    Value,
};

struct Clocked;

impl DataSource for Clocked {
    fn app_id(&self) -> &str {
        "test.clock"
    }
    fn grants(&self) -> &str {
        "secret.keep shared"
    }
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        Ok(Value::Unit)
    }
    fn answer(
        &mut self,
        store: &mut Store,
        source: &str,
        _: &[Value],
    ) -> Result<Answer, DataError> {
        if source == "sleep" {
            return Ok(Answer::Later(
                Request::continuation(1).due(store.clock_ms() as u64 + 450),
            ));
        }
        store.set("shared", &store.clock_ms().to_string())?;
        Ok(Answer::Now(Value::Unit))
    }
    fn parse(
        &mut self,
        store: &mut Store,
        _: &str,
        _: &[Value],
        _: Outcome,
    ) -> Result<Answer, DataError> {
        Ok(Answer::Now(Value::str(&format!(
            "{} at {}",
            store.get("shared").unwrap_or("missing"),
            store.clock_ms()
        ))))
    }
    fn module_landed(&mut self, store: &mut Store, _: u64, _: Outcome) -> Result<(), DataError> {
        store.set("shared", &store.clock_ms().to_string())?;
        Ok(())
    }
}

#[test]
fn a_worker_timer_reserves_its_next_turn_and_reads_the_committed_store() {
    for worker_on_left in [true, false] {
        let worker = Placed::new(Clocked, Placement::Worker);
        let main = Placed::new(Clocked, Placement::Main);
        let mut mixed = if worker_on_left {
            Mixed::new(worker, main, &["sleep"], &["write"])
        } else {
            Mixed::new(main, worker, &["write"], &["sleep"])
        }
        .unwrap();
        mixed.activate().unwrap();
        let mut store = Store::new(mixed.grants(), []);
        store.set_clock_ms(5000.);
        let target = Target::Mutation(0);
        let initial = token(mixed.answer_for(target, &mut store, "sleep", &[]).unwrap());
        let outcome = run(mixed.dispatch(initial, &store));
        let Answer::Later(timer) = mixed
            .parse_for(target, &mut store, "sleep", &[], outcome)
            .unwrap()
        else {
            panic!("a timer wait");
        };
        assert_eq!(timer.due_ms, Some(5450));
        let timer = timer.continuation.unwrap();
        mixed.forgotten(
            &store,
            &[InFlight {
                target,
                source: "sleep",
                args: &[],
                fetch: None,
                continuation: Some(timer),
            }],
        );
        let write = token(mixed.answer(&mut store, "write", &[]).unwrap());
        let outcome = run(mixed.dispatch(write, &store));
        store.set_clock_ms(5450.);
        assert!(matches!(mixed.dispatch(timer, &store), Dispatch::Held));
        value_of(mixed.parse(&mut store, "write", &[], outcome));
        let mut ready = mixed.release(&store);
        assert_eq!(ready.len(), 1);
        let (released, dispatch) = ready.remove(0);
        assert_eq!(released, timer);
        assert_eq!(
            value_of(mixed.parse_for(target, &mut store, "sleep", &[], run(dispatch))),
            Value::str("5000 at 5450")
        );
        assert!(mixed.release(&store).is_empty());
    }
}

#[test]
fn an_ordered_main_childs_kept_reply_uses_the_dispatch_clock() {
    let mut mixed = Mixed::new(
        Placed::new(Clocked, Placement::Main),
        Placed::new(Clocked, Placement::Worker),
        &["sleep"],
        &["write"],
    )
    .unwrap();
    mixed.activate().unwrap();
    let mut store = Store::new(mixed.grants(), []);
    store.set_clock_ms(5000.);
    let request = mixed
        .module_delivery(&mut store, 7, Dispatch::again_outcome())
        .unwrap()
        .unwrap();
    store.set_clock_ms(5450.);
    let outcome = run(mixed.dispatch(request.continuation.unwrap(), &store));
    assert!(mixed
        .module_delivery(&mut store, 7, outcome)
        .unwrap()
        .is_none());
    assert_eq!(store.get("shared"), Some("5450"));
}
