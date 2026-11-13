//! Work a data source holds: parked by the bridge until the source releases
//! it, and let go with the request it was for.
use super::*;

/// Holds every call it is asked for until `release` names one.
#[derive(Clone, Default)]
struct Holding(Arc<Mutex<(u64, Option<u64>)>>);

impl DataSource for Holding {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(name.into()))
    }
    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        let mut asked = self.0.lock().unwrap();
        asked.0 += 1;
        Ok(Answer::Later(exact_runner::Request::continuation(asked.0)))
    }
    fn dispatch(&mut self, _: u64, _: &Store) -> exact_runner::Dispatch {
        exact_runner::Dispatch::Held
    }
    fn release(&mut self, _: &Store) -> Vec<(u64, exact_runner::Dispatch)> {
        let token = self.0.lock().unwrap().1.take();
        token
            .map(|t| vec![(t, exact_runner::Dispatch::Again)])
            .unwrap_or_default()
    }
    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        _: exact_runner::Outcome,
    ) -> Result<Answer, DataError> {
        Ok(Answer::Now(Value::Number(1.0)))
    }
}

fn press(bridge: &mut Bridge<Holding>, test_id: &str) {
    let kernel = bridge.host.as_ref().unwrap().runner().kernel();
    let view = kernel
        .node_by_key(kernel.find_by_test_id(test_id)[0])
        .unwrap()
        .id;
    bridge.dispatch(view, 0, 0, 0.0);
}

/// A held request newer arguments replaced goes with them; the one still in
/// flight stays parked, and runs when its source releases it.
#[test]
fn a_held_request_the_runner_let_go_is_not_kept() {
    let plan = contract::compile("component App\n  state n = 0\n  state m = 0\n  resource item = item(n) as shape number\n  action bump\n    n = n + 1\n  action nudge\n    m = m + 1\n  view\n    column\n      button press=bump testId=\"bump\"\n        text `${n}`\n      button press=nudge testId=\"nudge\"\n        text `${m}`\n").unwrap().encode();
    let source = Holding::default();
    let mut bridge = Bridge::new();
    bridge.boot(&plan, source.clone(), Hooks::none(), 390.0, 844.0);
    bridge.data_ready();
    assert_eq!(bridge.parked.len(), 1);
    for _ in 0..3 {
        press(&mut bridge, "bump");
    }
    assert_eq!(source.0.lock().unwrap().0, 4);
    let parked: Vec<u64> = bridge.parked.keys().copied().collect();
    assert_eq!(parked, [4], "only the request in flight is parked");
    // A commit that changes nothing it reads keeps it, until released.
    press(&mut bridge, "nudge");
    assert!(bridge.parked.contains_key(&4));
    source.0.lock().unwrap().1 = Some(4);
    press(&mut bridge, "nudge");
    assert!(bridge.parked.is_empty(), "a request released runs");
}
