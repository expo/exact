//! A Rust-only app's source as its generated entry places it (`rust.placement`, LLP 1027.002): `Placed` over the
//! source inside the app's `configured!` type answers on an owner of its own on `worker`, inline on `main`.
#![cfg(not(target_arch = "wasm32"))]

use exact_logic::exact_data::Placed;
use exact_logic::exact_runner::{
    Answer, DataError, DataSource, Dispatch, Outcome, Placement, Reply, Store, Value, Work,
};
use exact_logic::Swappable;
use std::sync::{Arc, Mutex};

/// Answers with the name of the thread it ran on.
#[derive(Default)]
struct Leaf {
    threads: Arc<Mutex<Vec<String>>>,
}

impl DataSource for Leaf {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        let thread = std::thread::current().name().unwrap_or("?").to_string();
        self.threads.lock().unwrap().push(thread.clone());
        Ok(Value::str(&thread))
    }
}

exact_logic::configured!(Generated, Placed<Leaf>, || Swappable::off(Placed::new(
    Leaf::default(),
    Placement::Worker
)));

fn run(dispatch: Dispatch) -> Outcome {
    match dispatch {
        Dispatch::Run(Work::Now(work)) => work(),
        Dispatch::Run(Work::Later(work)) => {
            let (tx, rx) = std::sync::mpsc::channel();
            work(Reply::new(move |outcome| {
                let _ = tx.send(outcome);
            }));
            rx.recv_timeout(std::time::Duration::from_secs(5))
                .expect("a reply")
        }
        other => panic!("unexpected dispatch {}", matches!(other, Dispatch::Again)),
    }
}

/// What the generated entry builds for `worker`: the answer is `Later`, and the source runs on the owner thread.
#[test]
fn a_worker_placed_rust_only_source_answers_on_its_owner() {
    let mut data = Generated::default();
    assert_eq!(data.placement(), Placement::Worker);
    data.activate().unwrap();
    let mut store = Store::new("", []);
    let Answer::Later(request) = data.answer(&mut store, "where", &[]).unwrap() else {
        panic!("a worker's answer is Later");
    };
    let outcome = run(data.dispatch(request.continuation.expect("a turn"), &store));
    let Answer::Now(value) = data.parse(&mut store, "where", &[], outcome).unwrap() else {
        panic!("the turn's value");
    };
    assert_eq!(value.as_str(), Some("exact-owner"));
}

/// `main` is the source itself, inline, as an entry without `Placed` is.
#[test]
fn a_main_placed_rust_only_source_answers_inline() {
    let threads = Arc::new(Mutex::new(Vec::new()));
    let mut data = Swappable::off(Placed::new(
        Leaf {
            threads: threads.clone(),
        },
        Placement::Main,
    ));
    data.activate().unwrap();
    let mut store = Store::new("", []);
    assert!(matches!(
        data.answer(&mut store, "where", &[]).unwrap(),
        Answer::Now(_)
    ));
    assert_eq!(
        threads.lock().unwrap()[0],
        std::thread::current().name().unwrap_or("?")
    );
}
