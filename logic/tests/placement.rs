//! A Rust-only app's source as its generated entry places it (`rust.placement`, LLP 1027.002): `Placed` over the
//! source inside the app's `configured!` type answers on an owner of its own on `worker`, inline on `main`.
#![cfg(not(target_arch = "wasm32"))]

use exact_logic::exact_data::Placed;
use exact_logic::exact_runner::{
    Answer, DataError, DataSource, Dispatch, Outcome, Placement, Reply, Store, Value, Work,
};
use exact_logic::Swappable;

/// Answers with the name of the thread it ran on.
#[derive(Default)]
struct Leaf {
    active: bool,
}

impl DataSource for Leaf {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        self.active = false;
        Ok(Value::str(std::thread::current().name().unwrap_or("?")))
    }

    fn activate(&mut self) -> Result<(), DataError> {
        self.active = true;
        Ok(())
    }

    fn queued_send_replacer(
        &mut self,
        source: Option<&str>,
    ) -> Option<exact_logic::exact_runner::QueuedSendReplacer> {
        (self.active && source == Some("save")).then_some(|_, _, next| Some(next.to_vec()))
    }
}

exact_logic::configured!(Generated, Placed<Leaf>, || Swappable::off(Placed::new(
    Leaf::default(),
    Placement::Worker
)));

exact_logic::configured!(GeneratedMain, Leaf, || {
    Swappable::off(Leaf::default())
        .named("com.example.placed")
        .with_fixed_send_replacers()
});

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

#[test]
fn a_main_placed_rust_only_source_keeps_its_activated_rule() {
    let mut plan = exact_logic::exact_plan::builder::PlanBuilder::new(0, 0);
    let text = plan.primitive(exact_logic::exact_plan::TypeKind::String);
    plan.source("save", &[text], text);
    let mut data = GeneratedMain::default();
    assert_eq!(data.placement(), Placement::Main);
    data.bind(&plan.finish().unwrap());
    assert!(data.queued_send_replacer(Some("save")).is_none());
    data.activate().unwrap();
    assert!(data.queued_send_replacer(Some("save")).is_some());

    let mut store = Store::new("", []);
    let Answer::Now(value) = data.answer(&mut store, "save", &[]).unwrap() else {
        panic!("an inline answer");
    };
    assert_eq!(value.as_str(), std::thread::current().name());
    let replace = data
        .queued_send_replacer(Some("save"))
        .expect("the first answer must not change the bound rule");
    let next = [Value::str("new")];
    assert_eq!(
        replace("save", &[Value::str("old")], &next),
        Some(next.to_vec())
    );
}
