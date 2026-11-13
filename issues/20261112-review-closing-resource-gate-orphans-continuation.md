# Closing a resource gate during settlement orphans its undispatched continuation

**Status:** Open
**Systems:** runner
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P3

At `f6be99af`, a resource can answer `Later` during one settlement pass and
become inactive in the next pass of that same commit. The closed-gate branch
overwrites its `RequestEffect::Later` with `Answered` without discarding the
requests (`runner/src/runner/settlement.rs:585-593`). The source never receives
`discard(token)` for the continuation it allocated. If there was no earlier
request in flight, the later `forget` also has nothing to retire and does not
trigger `DataSource::forgotten`.

This occurs with ordinary store updates: an early resource reads a stored
flag, a second resource is gated by that flag, and a later source writes the
flag. Settlement repeats to reconcile the earlier store reader. The second
resource becomes inactive before its request leaves the runner, but its
source still owns the parked call and its allocations. Repeating the
transition can retain one undispatched call each time.

The following complete probe uses the public runner and plan-builder APIs:

```rust
use exact_runner::{Runner, DataSource, DataError, Value, Store, Answer, Request, InFlight};
use exact_kernel::{Kernel, NodeType};
use exact_plan::{builder::PlanBuilder, asm::Asm, TypeKind};

#[derive(Default)]
struct Data { live: Vec<u64>, discarded: Vec<u64>, forgotten: usize }
impl DataSource for Data {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> { unreachable!() }
    fn answer(&mut self, store: &mut Store, source: &str, _: &[Value])
        -> Result<Answer, DataError> {
        Ok(match source {
            "open" => Answer::Now(Value::Bool(store.get("closed").is_none())),
            "request" => {
                self.live.push(1);
                Answer::Later(Request::continuation(1))
            }
            "close" => {
                store.set("closed", "yes")?;
                Answer::Now(Value::Bool(true))
            }
            _ => unreachable!(),
        })
    }
    fn discard(&mut self, token: u64) {
        self.discarded.push(token);
        self.live.retain(|t| *t != token);
    }
    fn forgotten(&mut self, _: &Store, active: &[InFlight<'_>]) {
        self.forgotten += 1;
        self.live.retain(|t| active.iter().any(|p| p.continuation == Some(*t)));
    }
    fn grants(&self) -> &str { "secret.keep closed" }
}
fn main() {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let boolean = b.primitive(TypeKind::Bool);
    let number = b.primitive(TypeKind::Number);
    let open = b.resource("open", "open", &[], boolean, None);
    b.set_resource_reader(open, true);
    let request = b.resource("request", "request", &[], number, None);
    let mut gate = Asm::new();
    gate.load_resource(open);
    let gate = b.code(gate);
    b.set_resource_gate(request, gate);
    b.resource("close", "close", &[], boolean, None);
    b.node(NodeType::View as u8, None, None, 0, &[], &[], None);
    let mut r = Runner::boot(b.finish().unwrap(), Data::default(),
        Kernel::with_monospace(), Default::default(), "/").unwrap();
    let handed = r.take_requests().len();
    println!("gate={:?}; pending={:?}; handed_out={handed}", r.resource("open"), r.pending());
    let data = r.data();
    println!("live={:?}; discarded={:?}; forgotten={}",
        data.live, data.discarded, data.forgotten);
}
```

The probe was linked to the checkout's built libraries and run at
`/tmp/exact-review-20261112/settlement-gate-discard/repro`. It prints:

```text
gate=Some(Bool(false)); pending=[]; handed_out=0
live=[1]; discarded=[]; forgotten=0
```

Expected: the closed gate still leaves no pending or handed-out request,
and the source receives `discard(1)`, leaving `live=[]`.

Discard each request in an earlier pass's `Later` effect before replacing
that effect on gate closure. The re-query path already does this at
`runner/src/runner/settlement.rs:976-983`; use the same cleanup contract.
Add coverage for a gate closed by a later store write within one commit,
including a source that parks a plain request and relies on `forgotten`.

Duplicate check: searched open and closed filesystem issues and all cached
GitHub issue titles and bodies for gate/settlement cleanup, discarded
continuations and orphaned requests. No matching report was found.
