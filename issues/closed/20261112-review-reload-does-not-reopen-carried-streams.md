# Reloading a runner preserves a stream's value but does not reopen the stream

**Status:** Closed
**Resolution:** `Carried` names the resources whose answer came from a stream still open, and a replacement that takes such an answer shows it and asks again, now or at readiness, opening the stream anew.
**Systems:** runner
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `Runner::carry` includes a resource's last stream value once
its first message has committed: `pending` is then false. It records no
indication that the resource still has an open stream
(`runner/src/runner/carry.rs:64-86`). `boot_carrying` restores that value as
an ordinary settled answer when the source and logic identity are unchanged
(`runner/src/runner.rs:932-961`). Settlement reuses it without asking the
source, so the replacement has no stream ticket or request to reopen it.

A dev reload or plan replacement can therefore freeze live data at its last
value. The old runner's transport ends with that runner. The replacement
continues showing the value, reports no pending work, and receives no more
events until arguments change or the app explicitly refreshes the resource.
This applies to an ordinary streaming resource, including a Rust source
whose logic revision is `None`.

The following complete public-API probe opens a stream, delivers one
message, and replaces the runner with its carried state:

```rust
use exact_runner::{Runner, DataSource, DataError, Value, Store, Answer, Request, Outcome, Message};
use exact_kernel::{Kernel, NodeType};
use exact_plan::{builder::PlanBuilder, TypeKind};

#[derive(Default)]
struct Data { asks: usize }
impl DataSource for Data {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> { unreachable!() }
    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        self.asks += 1;
        Ok(Answer::stream(Request::get("https://example.test/events")))
    }
    fn parse(&mut self, _: &mut Store, _: &str, _: &[Value], outcome: Outcome)
        -> Result<Answer, DataError> {
        let Outcome::Message(message) = outcome else { unreachable!() };
        Ok(Answer::Now(Value::str(&message.data)))
    }
}
fn main() {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let string = b.primitive(TypeKind::String);
    b.resource("events", "events", &[], string, None);
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    let plan = b.finish().unwrap();
    let mut original = Runner::boot(plan.clone(), Data::default(),
        Kernel::with_monospace(), Default::default(), "/").unwrap();
    let request = original.take_requests().remove(0);
    original.fulfill(request.ticket, Outcome::Message(Message {
        data: "first event".into(), ..Message::default()
    })).unwrap();
    let carried = original.carry();
    println!("before reload: streams={}, value={:?}",
        original.streams().len(), original.resource("events"));
    drop(original);
    let mut reloaded = Runner::boot_carrying(plan, Data::default(),
        Kernel::with_monospace(), &carried, Default::default(), "/").unwrap();
    let handed = reloaded.take_requests().len();
    println!("after reload: streams={}, value={:?}, requests={handed}",
        reloaded.streams().len(), reloaded.resource("events"));
    println!("source asks after reload={}", reloaded.data().asks);
}
```

Linked against the checkout's built libraries, it prints:

```text
before reload: streams=1, value=Some(Str("first event"))
after reload: streams=0, value=Some(Str("first event")), requests=0
source asks after reload=0
```

The probe and output are at
`/tmp/exact-review-20261112/carried-stream/{main.rs,repro,output.txt}`.
Expected: the replacement keeps the displayed answer while asking the source
once to reopen its stream. A new message should then update that answer.

Carry which resource answers came from still-open streams, and re-ask those
resources on replacement, including when the source becomes ready later.
The document-checkpoint path already preserves this distinction in
`Checkpoint::open` and re-asks it at `runner/src/runner.rs:994-1002`.
Cover a stream after its first message, a stream still awaiting its first
message, and a stream that ended before the reload.

Duplicate check: searched all open and closed filesystem issues and all
cached GitHub issue titles and bodies for carried streams, reload,
replacement, WebSocket and resubscription failures. No matching report was
found.
