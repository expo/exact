//! A kept reply's delivery through `Storage` (LLP 1146 D2).

use super::*;

/// A child whose answers wait on its continuation 40 and whose kept reply's
/// delivery waits on its continuation 1; it records what it dispatches.
struct Delivering(std::rc::Rc<std::cell::RefCell<Vec<u64>>>);

impl DataSource for Delivering {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        Ok(Value::Unit)
    }
    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::Later(Request::continuation(40)))
    }
    fn module_delivery(
        &mut self,
        _: &mut Store,
        _: u64,
        _: Outcome,
    ) -> Result<Option<Request>, DataError> {
        Ok(Some(Request::continuation(1)))
    }
    fn host_continuation(&self, token: u64) -> bool {
        token == 1
    }
    fn dispatch(&mut self, token: u64, _: &Store) -> exact_runner::Dispatch {
        self.0.borrow_mut().push(token);
        exact_runner::Dispatch::Host(token)
    }
}

/// LLP 1146 D2: a kept reply's delivery continuation goes out under one of
/// `Storage`'s tokens, which names the child's on every call, and a commit
/// that lets requests go does not let it go: the runner holds it as module
/// work, not in flight.
#[test]
fn a_kept_replys_delivery_dispatches_the_childs_continuation() {
    let dispatched = std::rc::Rc::new(std::cell::RefCell::new(Vec::new()));
    let mut host = Storage::new(Delivering(dispatched.clone()));
    host.activate().unwrap();
    let mut store = Store::default();
    let target = exact_runner::Target::Resource(0);
    // An answer in flight takes `Storage`'s first token for the child's 40.
    let Ok(Answer::Later(waiting)) = host.answer_for(target, &mut store, "wait", &[]) else {
        panic!("a continuation");
    };
    let waiting = waiting.continuation.expect("a continuation");
    let delivery = host
        .module_delivery(&mut store, 7, exact_runner::Dispatch::again_outcome())
        .unwrap()
        .expect("a request")
        .continuation
        .expect("a continuation");
    assert_ne!(
        delivery, waiting,
        "each continuation has a token of its own"
    );
    assert!(
        host.host_continuation(delivery),
        "the child's 1 is host work"
    );
    assert!(!host.host_continuation(waiting), "the child's 40 is not");
    host.forgotten(
        &store,
        &[exact_runner::InFlight {
            target,
            source: "wait",
            args: &[],
            fetch: None,
            continuation: Some(waiting),
        }],
    );
    assert!(matches!(
        host.dispatch(delivery, &store),
        exact_runner::Dispatch::Host(1)
    ));
    assert!(matches!(
        host.dispatch(waiting, &store),
        exact_runner::Dispatch::Host(40)
    ));
    assert_eq!(dispatched.borrow().as_slice(), [1, 40]);
}
