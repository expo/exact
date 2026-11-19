//! A resource whose gate closes in a later pass of the same settlement
//! (LLP 1161.002 D2): the request its earlier pass parked never leaves the
//! runner, so the source hears it discarded, and an upload in flight goes on.
use exact_kernel::{Kernel, NodeType};
use exact_plan::{asm::Asm, builder::PlanBuilder, TypeKind};
use exact_runner::{Answer, DataError, DataSource, InFlight, Request, Runner, Store, Value};

/// The host work these tests hand out (LLP 1047.001 D12).
struct TestWork;
impl exact_runner::HostWork for TestWork {}

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
                Answer::Later(Request::continuation::<TestWork>(1))
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

/// An upload's arguments, its gate's store entry and a later resource that
/// writes it: one commit changes the arguments and closes the gate. The
/// upload of `3` answers at once, as one whose end the runner remembers does,
/// and the upload of `4` fails at once.
#[derive(Default)]
struct Uploads {
    asked: Vec<Value>,
}

impl DataSource for Uploads {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!()
    }
    fn answer(
        &mut self,
        store: &mut Store,
        source: &str,
        args: &[Value],
    ) -> Result<Answer, DataError> {
        Ok(match source {
            "isOpen" => Answer::Now(Value::Bool(store.get("closed").is_none())),
            "upload" if args[0] == Value::Number(3.) => {
                self.asked.push(args[0].clone());
                Answer::Now(Value::Number(3.))
            }
            "upload" if args[0] == Value::Number(4.) => {
                self.asked.push(args[0].clone());
                return Err(DataError::Interface("the upload failed".into()));
            }
            "upload" => {
                self.asked.push(args[0].clone());
                Answer::stream(
                    Request::post_json(&format!("https://example.test/{:?}", args[0]), "{}")
                        .upload_progress(),
                )
            }
            "close" => {
                if args[0] != Value::Number(1.) {
                    store.set("closed", "yes")?;
                }
                Answer::Now(Value::Bool(true))
            }
            _ => unreachable!(),
        })
    }
    fn grants(&self) -> &str {
        "secret.keep closed"
    }
}

/// The app: `sent` uploads `file` while `opened`, `closer` closes the gate
/// for any other `file`, and `busy` and `lost` read whether `sent` is
/// pending and whether it failed.
fn uploading() -> (Runner<Uploads>, u64) {
    let plan = contract::compile(
        r#"
component App
  state file = 1
  resource opened = isOpen() as shape bool
  resource sent = upload(file) when opened as shape number
  resource closer = close(file) as shape bool
  derive busy = pending(sent)
  derive lost = failed(sent)
  action choose(n: number)
    file = n
  view
    text `${sent}`
"#,
    )
    .unwrap();
    let mut r = Runner::boot(
        plan,
        Uploads::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.data_ready().unwrap();
    let out = r.take_requests();
    assert_eq!(out.len(), 1);
    assert!(out[0].request.upload_progress);
    (r, out[0].ticket)
}

/// An upload in flight when its gate closes goes on, pending, and nothing is
/// asked (LLP 1161.002 D2), though an earlier pass of the same settlement
/// asked its newer arguments: that request is never handed out, an answer
/// that pass took at once does not let the upload go, and a failure it took
/// is no failure while the resource is idle.
#[test]
fn a_gate_closed_in_a_later_pass_lets_an_upload_in_flight_go_on_and_asks_nothing() {
    for (file, case) in [
        (2., "a request"),
        (3., "an answer at once"),
        (4., "a failure at once"),
    ] {
        let (mut r, upload) = uploading();
        r.act("choose", vec![Value::Number(file)]).unwrap();
        assert_eq!(r.resource("opened"), Some(&Value::Bool(false)), "{case}");
        assert_eq!(
            r.data().asked,
            [Value::Number(1.), Value::Number(file)],
            "{case}"
        );
        assert!(
            r.take_requests().is_empty(),
            "{case}: nothing is asked under a closed gate"
        );
        assert!(
            r.pending().iter().any(|(_, t)| *t == upload),
            "{case}: the upload in flight goes on: {:?}",
            r.pending()
        );
        assert_eq!(
            r.derive("busy"),
            Some(&Value::Bool(true)),
            "{case}: the upload is pending"
        );
        assert_eq!(
            r.derive("lost"),
            Some(&Value::Bool(false)),
            "{case}: an idle resource has no failure"
        );
    }
}
