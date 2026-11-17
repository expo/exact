//! A resource whose gate closes in a later pass of the same settlement
//! (LLP 1161.002 D2): the request its earlier pass parked never leaves the
//! runner, so the source hears it discarded.
use exact_kernel::{Kernel, NodeType};
use exact_plan::{asm::Asm, builder::PlanBuilder, TypeKind};
use exact_runner::{Answer, DataError, DataSource, InFlight, Request, Runner, Store, Value};

#[derive(Default)]
struct Data {
    /// Answer `request` with a continuation token rather than a plain request.
    token: bool,
    live: Vec<Option<u64>>,
    discarded: Vec<u64>,
    forgotten: usize,
}

impl DataSource for Data {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!()
    }
    fn answer(
        &mut self,
        store: &mut Store,
        source: &str,
        _: &[Value],
    ) -> Result<Answer, DataError> {
        Ok(match source {
            "open" => Answer::Now(Value::Bool(store.get("closed").is_none())),
            "request" if self.token => {
                self.live.push(Some(1));
                Answer::Later(Request::continuation(1))
            }
            "request" => {
                self.live.push(None);
                Answer::Later(Request::get("https://example.test/request"))
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
        self.live.retain(|t| *t != Some(token));
    }
    fn forgotten(&mut self, _: &Store, active: &[InFlight<'_>]) {
        self.forgotten += 1;
        self.live
            .retain(|t| t.is_some_and(|t| active.iter().any(|p| p.continuation == Some(t))));
    }
    fn grants(&self) -> &str {
        "secret.keep closed"
    }
}

/// `open` reads the store, `request` is gated on it, and `close`, settled
/// after both, writes what `open` reads: the next pass closes the gate.
fn boot(token: bool) -> Runner<Data> {
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
    let data = Data {
        token,
        ..Data::default()
    };
    Runner::boot(
        b.finish().unwrap(),
        data,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

#[test]
fn a_gate_closed_by_a_later_write_discards_the_continuation_it_parked() {
    let mut r = boot(true);
    assert_eq!(r.resource("open"), Some(&Value::Bool(false)));
    assert!(r.pending().is_empty());
    assert!(r.take_requests().is_empty());
    assert_eq!(r.data().discarded, [1]);
    assert!(r.data().live.is_empty());
}

#[test]
fn a_gate_closed_by_a_later_write_has_the_source_forget_its_plain_request() {
    let mut r = boot(false);
    assert!(r.pending().is_empty());
    assert!(r.take_requests().is_empty());
    assert_eq!(r.data().forgotten, 1);
    assert!(r.data().live.is_empty());
}
