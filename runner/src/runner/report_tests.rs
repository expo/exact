//! A data module's report (LLP 1175 D3): its authority is the runner's
//! records of the turn it was made in, never the module's word, and it runs
//! as module work that no answer waits for.
use crate::{
    Answer, CallOrigin, DataError, DataSource, Outcome, Report, Reporter, Request, RequestOut,
    Runner, Store, Target,
};
use exact_kernel::Kernel;
use exact_plan::Value;
use std::sync::{Arc, Mutex};

/// `report()` answers now and makes a report by `by`; `hold()` asks a POST
/// that stays in flight. `landed` is the tokens delivered back.
struct Reporting {
    by: Reporter,
    made: Vec<Report>,
    aborted: Vec<u64>,
    landed: Arc<Mutex<Vec<u64>>>,
}

impl DataSource for Reporting {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!("the fixture implements answer")
    }
    fn answer(&mut self, _: &mut Store, source: &str, _: &[Value]) -> Result<Answer, DataError> {
        if source == "hold" {
            return Ok(Answer::Later(Request::post_json(
                "https://example.test/hold",
                "{}",
            )));
        }
        self.made.push(Report {
            token: 7,
            request: Request::get("https://example.test/report"),
            by: self.by,
        });
        Ok(Answer::Now(Value::Bool(true)))
    }
    fn take_reports(&mut self) -> Vec<Report> {
        std::mem::take(&mut self.made)
    }
    fn take_aborted(&mut self) -> Vec<u64> {
        std::mem::take(&mut self.aborted)
    }
    fn module_landed(&mut self, _: &mut Store, token: u64, _: Outcome) -> Result<(), DataError> {
        self.landed.lock().unwrap().push(token);
        Ok(())
    }
}

const SRC: &str = r#"component App
  mutation m as shape bool
  mutation held as shape bool
  action go
    send m = report()
  action hold
    send held = hold()
  view
    text "reports"
"#;

fn runner(by: Reporter) -> (Runner<Reporting>, Arc<Mutex<Vec<u64>>>) {
    let landed = Arc::new(Mutex::new(Vec::new()));
    let source = Reporting {
        by,
        made: Vec::new(),
        aborted: Vec::new(),
        landed: landed.clone(),
    };
    let plan = contract::compile(SRC).expect("the fixture compiles");
    let r = Runner::boot(
        plan,
        source,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    (r, landed)
}

fn reports(r: &mut Runner<Reporting>) -> Vec<RequestOut> {
    (r.take_requests().into_iter())
        .filter(|o| o.target == "module work")
        .collect()
}

#[test]
fn a_tool_s_report_keeps_the_tool_s_origin() {
    let (mut r, _) = runner(Reporter::Answer(Target::Mutation(0)));
    r.act_from(CallOrigin::Tool, "go", vec![]).unwrap();
    let out = reports(&mut r);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].request.reach.origin, CallOrigin::Tool);
}

/// With no answer's turn running, a report takes `data_origin`: here the
/// least privileged of the app's commit and a wake's request in flight. An
/// answer's report in the same commit keeps the app's.
#[test]
fn a_report_with_no_answer_takes_the_data_origin() {
    for (by, origin) in [
        (Reporter::None, CallOrigin::Wake),
        (Reporter::Answer(Target::Mutation(0)), CallOrigin::App),
    ] {
        let (mut r, _) = runner(by);
        r.act_from(CallOrigin::Wake, "hold", vec![]).unwrap();
        assert!(reports(&mut r).is_empty());
        r.act("go", vec![]).unwrap();
        let out = reports(&mut r);
        assert_eq!(out.len(), 1, "{out:?}");
        assert_eq!(out[0].request.reach.origin, origin, "{by:?}");
    }
}

/// A `GET` is kept too: it is in flight as module work until its reply is
/// delivered under the source's token, and no answer waits for it.
#[test]
fn a_get_report_is_kept_as_module_work() {
    let (mut r, landed) = runner(Reporter::Answer(Target::Mutation(0)));
    r.act("go", vec![]).unwrap();
    assert_eq!(
        r.slot("m"),
        Some(&Value::Option(Some(std::rc::Rc::new(Value::Bool(true)))))
    );
    let out = reports(&mut r);
    assert_eq!(out.len(), 1, "{out:?}");
    assert_eq!(out[0].request.method, "GET");
    assert_eq!(out[0].class, crate::RequestClass::Ordered);
    assert!(r.in_flight().iter().any(|(_, t)| *t == out[0].ticket));
    r.fulfill(
        out[0].ticket,
        Outcome::Failed {
            kind: crate::FailureKind::Network,
            message: "offline".into(),
        },
    )
    .unwrap();
    assert_eq!(*landed.lock().unwrap(), [7]);
    assert!(r.in_flight().is_empty(), "{:?}", r.in_flight());
}

/// A report the module aborted before the runner took it, as a listener
/// outside any answer may, is never handed out: its abort waits for the
/// request, and the report is answered aborted.
#[test]
fn a_report_aborted_before_it_is_taken_is_never_handed_out() {
    let (mut r, landed) = runner(Reporter::None);
    r.data().made.push(Report {
        token: 7,
        request: Request {
            fetch: Some(7),
            ..Request::get("https://example.test/report")
        },
        by: Reporter::None,
    });
    r.data().aborted.push(7);
    assert!(reports(&mut r).is_empty());
    let (ticket, outcome) = r.take_aborted_answer().expect("answered aborted");
    assert!(
        matches!(
            outcome,
            Outcome::Failed {
                kind: crate::FailureKind::Aborted,
                ..
            }
        ),
        "{outcome:?}"
    );
    r.fulfill(ticket, outcome).unwrap();
    assert_eq!(*landed.lock().unwrap(), [7]);
    assert!(r.in_flight().is_empty(), "{:?}", r.in_flight());
}

/// A delivery's continuation the host has not taken when the runner is
/// poisoned is dropped with its module work and discarded at the source
/// that minted it, so a lone worker's delivery stops holding its Store
/// (LLP 1027.002 Change 2).
#[test]
fn poison_discards_a_delivery_continuation_not_yet_handed_out() {
    struct Delivering {
        made: Vec<Report>,
        discarded: Arc<Mutex<Vec<u64>>>,
    }
    impl DataSource for Delivering {
        fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
            unreachable!("the fixture implements answer")
        }
        fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
            self.made.push(Report {
                token: 7,
                request: Request::get("https://example.test/report"),
                by: Reporter::Answer(Target::Mutation(0)),
            });
            Ok(Answer::Now(Value::Bool(true)))
        }
        fn take_reports(&mut self) -> Vec<Report> {
            std::mem::take(&mut self.made)
        }
        fn module_delivery(
            &mut self,
            _: &mut Store,
            _: u64,
            _: Outcome,
        ) -> Result<Option<Request>, DataError> {
            Ok(Some(Request::continuation(55)))
        }
        fn discard(&mut self, token: u64) {
            self.discarded.lock().unwrap().push(token);
        }
    }
    let discarded = Arc::new(Mutex::new(Vec::new()));
    let source = Delivering {
        made: Vec::new(),
        discarded: discarded.clone(),
    };
    let plan = contract::compile(SRC).expect("the fixture compiles");
    let mut r = Runner::boot(
        plan,
        source,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.act("go", vec![]).unwrap();
    let out: Vec<RequestOut> = (r.take_requests().into_iter())
        .filter(|o| o.target == "module work")
        .collect();
    assert_eq!(out.len(), 1, "{out:?}");
    r.fulfill(
        out[0].ticket,
        Outcome::Failed {
            kind: crate::FailureKind::Network,
            message: "offline".into(),
        },
    )
    .unwrap();
    assert!(discarded.lock().unwrap().is_empty());
    r.poison();
    assert_eq!(*discarded.lock().unwrap(), [55]);
}
