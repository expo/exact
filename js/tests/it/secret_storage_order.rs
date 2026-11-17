//! Native lease acquisition precedes storage delivery. The wasm browser alone
//! drains already-issued storage before asking its host for a lease.
#![cfg(exact_js_engine)]

use exact_plan::Value;
use exact_runner::{
    Answer, DataSource, Dispatch, Outcome, Placement, Reply, SecretLease, SecretWait,
    SharedSecrets, Store, Target, Work,
};
use std::sync::{Arc, Mutex};
use std::time::Duration;

use super::storage::{args, text, Root, GRANTS};

struct Disk {
    value: Option<String>,
    held: bool,
}

#[derive(Clone)]
struct Secrets(Arc<Mutex<Disk>>);
struct Wait(Secrets, Arc<dyn Fn() + Send + Sync>);
struct Lease(Secrets, Arc<dyn Fn() + Send + Sync>);

impl SharedSecrets for Secrets {
    fn acquire(&self, name: &str, wake: Arc<dyn Fn() + Send + Sync>) -> Box<dyn SecretWait> {
        assert_eq!(name, "session");
        Box::new(Wait(self.clone(), wake))
    }
}

impl SecretWait for Wait {
    fn take(&mut self) -> Option<Result<Arc<dyn SecretLease>, String>> {
        let mut disk = self.0 .0.lock().unwrap();
        if disk.held {
            return None;
        }
        disk.held = true;
        Some(Ok(Arc::new(Lease(self.0.clone(), self.1.clone()))))
    }
}

impl SecretLease for Lease {
    fn read(&self) -> Result<Option<String>, String> {
        Ok(self.0 .0.lock().unwrap().value.clone())
    }

    fn write(&self, value: Option<&str>) -> Result<(), String> {
        self.0 .0.lock().unwrap().value = value.map(str::to_owned);
        Ok(())
    }
}

impl Drop for Lease {
    fn drop(&mut self) {
        self.0 .0.lock().unwrap().held = false;
        (self.1)();
    }
}

fn run(dispatch: Dispatch) -> Outcome {
    match super::runnable(dispatch) {
        Dispatch::Run(Work::Now(work)) => work(),
        Dispatch::Run(Work::Later(work)) => {
            let (send, receive) = std::sync::mpsc::channel();
            work(Reply::new(move |outcome| send.send(outcome).unwrap()));
            receive
                .recv_timeout(Duration::from_secs(20))
                .expect("worker turn completed")
        }
        _ => panic!("native turn did not run"),
    }
}

fn request(answer: &Answer) -> &exact_runner::Request {
    let Answer::Later(request) = answer else {
        panic!("expected a continuation, got {answer:?}")
    };
    request
}

fn until_host(
    module: &mut impl DataSource,
    store: &mut Store,
    target: Target,
    call_args: &[Value],
    mut answer: Answer,
) -> Answer {
    for _ in 0..8 {
        let token = request(&answer).continuation.expect("continuation");
        if module.host_continuation(token) {
            return answer;
        }
        let outcome = run(module.dispatch(token, store));
        answer = module
            .parse_for(target, store, "work", call_args, outcome)
            .unwrap();
    }
    panic!("the answer did not reach its lease")
}

fn acquire(
    module: &mut impl DataSource,
    store: &mut Store,
    target: Target,
    call_args: &[Value],
    answer: Answer,
) -> Answer {
    let token = request(&answer).continuation.expect("lease continuation");
    assert!(module.host_continuation(token));
    let outcome = run(module.dispatch(token, store));
    module
        .parse_for(target, store, "work", call_args, outcome)
        .unwrap()
}

fn finish_storage(
    module: &mut impl DataSource,
    store: &mut Store,
    target: Target,
    call_args: &[Value],
    mut answer: Answer,
) -> Answer {
    for _ in 0..8 {
        let Answer::Later(request) = answer else {
            return answer;
        };
        let token = request.continuation.expect("storage continuation");
        assert!(!module.host_continuation(token));
        let outcome = run(module.dispatch(token, store));
        answer = module
            .parse_for(target, store, "work", call_args, outcome)
            .unwrap();
    }
    panic!("the storage answer did not settle")
}

fn fixture(placement: Placement) -> (Root, exact_data::Placed<exact_js::Module>, Store, Secrets) {
    let root = Root::new();
    std::fs::create_dir_all(root.0.join("data")).unwrap();
    std::fs::write(root.0.join("data/native-order"), b"read landed").unwrap();
    let secrets = Secrets(Arc::new(Mutex::new(Disk {
        value: Some("initial".into()),
        held: false,
    })));
    let mut module = root.module().placed(placement);
    module.configure_secrets(Arc::new(secrets.clone())).unwrap();
    module.activate().unwrap();
    let store = Store::new(GRANTS, [("session".into(), b"initial".to_vec())]);
    (root, module, store, secrets)
}

fn begin(
    module: &mut impl DataSource,
    store: &mut Store,
    target: Target,
    call_args: &[Value],
) -> Answer {
    let answer = module.answer_for(target, store, "work", call_args).unwrap();
    until_host(module, store, target, call_args, answer)
}

#[test]
fn native_storage_before_exclusive_runs_the_body_and_settles() {
    for placement in [Placement::Main, Placement::Worker] {
        let (_root, mut module, mut store, secrets) = fixture(placement);
        let first_args = args("read-before-exclusive", "first");
        let first = begin(&mut module, &mut store, Target::Mutation(0), &first_args);
        let first = acquire(
            &mut module,
            &mut store,
            Target::Mutation(0),
            &first_args,
            first,
        );
        assert!(
            !module.host_continuation(request(&first).continuation.unwrap()),
            "the body did not reach its storage wait: {first:?}"
        );
        let first = finish_storage(
            &mut module,
            &mut store,
            Target::Mutation(0),
            &first_args,
            first,
        );
        let Answer::Now(value) = first else {
            panic!("the first answer did not settle")
        };
        assert_eq!(text(value), "read landed first");
        assert_eq!(store.get("session"), Some("initial"));
        assert_eq!(secrets.0.lock().unwrap().value.as_deref(), Some("initial"));

        let following_args = args("exclusive-only", "following");
        let following = begin(
            &mut module,
            &mut store,
            Target::Mutation(1),
            &following_args,
        );
        let following = acquire(
            &mut module,
            &mut store,
            Target::Mutation(1),
            &following_args,
            following,
        );
        let Answer::Now(value) = finish_storage(
            &mut module,
            &mut store,
            Target::Mutation(1),
            &following_args,
            following,
        ) else {
            panic!("the following lease did not settle")
        };
        assert_eq!(text(value), "entered following");
        assert_eq!(
            secrets.0.lock().unwrap().value.as_deref(),
            Some("following")
        );
    }
}

#[test]
fn a_second_main_placed_native_lease_enters_while_the_first_read_is_outstanding() {
    let (_root, mut module, mut store, secrets) = fixture(Placement::Main);
    let first_args = args("read-before-exclusive", "first");
    let first = begin(&mut module, &mut store, Target::Mutation(0), &first_args);
    let first = acquire(
        &mut module,
        &mut store,
        Target::Mutation(0),
        &first_args,
        first,
    );
    assert!(
        !module.host_continuation(request(&first).continuation.unwrap()),
        "the body did not reach its storage wait: {first:?}"
    );
    assert!(!secrets.0.lock().unwrap().held);
    assert!(matches!(first, Answer::Later(_)));

    let second_args = args("exclusive-only", "second");
    let second = begin(&mut module, &mut store, Target::Mutation(1), &second_args);
    let second = acquire(
        &mut module,
        &mut store,
        Target::Mutation(1),
        &second_args,
        second,
    );
    let Answer::Now(value) = finish_storage(
        &mut module,
        &mut store,
        Target::Mutation(1),
        &second_args,
        second,
    ) else {
        panic!("the second lease did not settle")
    };
    assert_eq!(text(value), "entered second");
    assert_eq!(secrets.0.lock().unwrap().value.as_deref(), Some("second"));

    let Answer::Now(value) = finish_storage(
        &mut module,
        &mut store,
        Target::Mutation(0),
        &first_args,
        first,
    ) else {
        panic!("the first answer did not settle after its read")
    };
    assert_eq!(text(value), "read landed first");
    assert_eq!(secrets.0.lock().unwrap().value.as_deref(), Some("second"));
}
