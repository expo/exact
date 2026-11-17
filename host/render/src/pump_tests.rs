use super::*;
#[path = "../../apple/src/pump_fixture.rs"]
mod fixture;
use exact_kernel::{Kernel, MonospaceMeasurer};
use fixture::{plan, Replies};

#[test]
fn ready_replies_share_a_render_pump_with_count_and_time_bounds() {
    for (count, advance_after, first) in [(20, None, 20), (40, None, 32), (10, Some(3), 3)] {
        let mut source = Replies::new();
        source.advance_after = advance_after;
        let clock = source.clock.clone();
        let plan = exact_plan::Plan::decode(&plan(count)).unwrap();
        let kernel = Kernel::new(Box::new(MonospaceMeasurer::default()));
        let mut runner = Runner::boot(plan, source, kernel, Default::default(), "/").unwrap();
        let seq = runner.seq();
        let executor = Executor::start("");
        assert_eq!(
            settle_with_clock(
                &mut runner,
                &executor,
                Instant::now() + std::time::Duration::from_secs(5),
                || clock.get(),
                || executor.drain().pop()
            )
            .unwrap(),
            Settled::Complete
        );
        assert_eq!(runner.data_ref().parsed, (0..count).collect::<Vec<_>>());
        assert_eq!(runner.seq() - seq, count as u64);
        // Release is called once between pumps, never between their replies.
        let released = &runner.data_ref().released_at;
        if first < count {
            assert!(released.contains(&first), "{released:?}");
        } else {
            assert!(
                !released.contains(&count),
                "completion needs no further hand-out"
            );
        }
        assert!(
            !released.iter().any(|n| *n > 0 && *n < first),
            "{released:?}"
        );
    }
}

#[test]
fn a_render_error_keeps_prior_transactions_and_leaves_later_replies_queued() {
    let source = Replies::new();
    let clock = source.clock.clone();
    let plan = exact_plan::Plan::decode(&fixture::fault_plan()).unwrap();
    let kernel = Kernel::new(Box::new(MonospaceMeasurer::default()));
    let mut runner = Runner::boot(plan, source, kernel, Default::default(), "/").unwrap();
    let seq = runner.seq();
    let executor = Executor::start("");
    let deadline = Instant::now() + std::time::Duration::from_secs(5);
    assert!(settle_with_clock(
        &mut runner,
        &executor,
        deadline,
        || clock.get(),
        || executor.drain().pop()
    )
    .is_err());
    assert_eq!(runner.data_ref().parsed, [0, 1, 2]);
    assert_eq!(runner.seq() - seq, 2);
    assert_eq!(
        settle_with_clock(
            &mut runner,
            &executor,
            deadline,
            || clock.get(),
            || executor.drain().pop()
        )
        .unwrap(),
        Settled::Complete
    );
    assert_eq!(runner.data_ref().parsed, [0, 1, 2, 3]);
}

#[test]
fn a_completed_render_leaves_the_next_stream_delivery_queued() {
    use exact_runner::{Answer, DataError, Message, Outcome, Request, Store, Value};
    use std::cell::Cell;
    use std::collections::VecDeque;
    struct Feed(Vec<String>);
    impl DataSource for Feed {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            Ok(Value::str(""))
        }
        fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
            Ok(Answer::stream(Request::get("https://held.test/feed")))
        }
        fn parse(
            &mut self,
            _: &mut Store,
            _: &str,
            _: &[Value],
            outcome: Outcome,
        ) -> Result<Answer, DataError> {
            let Outcome::Message(message) = outcome else {
                panic!("only scripted messages");
            };
            self.0.push(message.data.clone());
            Ok(if message.data == "later" {
                Answer::stream(Request::get("https://held.test/replacement"))
            } else {
                Answer::Now(Value::str(&message.data))
            })
        }
    }
    let plan = contract::compile(
        "component App\n  resource feed = feed() as shape string\n  view\n    text feed\n",
    )
    .unwrap();
    let message = |data: &str| Message {
        data: data.into(),
        ..Default::default()
    };
    for first in [vec!["first"], vec!["first", "second"]] {
        let mut runner = Runner::boot(
            plan.clone(),
            Feed(vec![]),
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        let ticket = runner.take_requests().remove(0).ticket;
        let seq = runner.seq();
        let executor = Executor::start("");
        let now = Instant::now();
        let deadline = now + std::time::Duration::from_secs(5);
        let clock = Cell::new(now);
        let outcome = if first.len() == 1 {
            Outcome::Message(message(first[0]))
        } else {
            Outcome::Messages(first.iter().map(|data| message(data)).collect())
        };
        let mut deliveries: VecDeque<_> = [outcome, Outcome::Message(message("later"))]
            .into_iter()
            .map(|outcome| (ticket, outcome, None))
            .collect();
        let settled = settle_with_clock(
            &mut runner,
            &executor,
            deadline,
            || clock.get(),
            || {
                let delivery = deliveries.pop_front();
                // Exhaustion ends a broken settle without a real-time wait.
                if delivery.is_none() {
                    clock.set(deadline);
                }
                delivery
            },
        )
        .unwrap();
        assert_eq!(settled, Settled::Complete);
        assert_eq!(runner.data_ref().0, first);
        assert_eq!(
            runner.resource("feed"),
            Some(&Value::str(first.last().unwrap()))
        );
        assert_eq!(runner.seq() - seq, 1, "one delivery commits once");
        assert_eq!(deliveries.len(), 1);
        assert_eq!(
            clock.get(),
            now,
            "completion needed no further outcome or clock advance"
        );
    }
}
