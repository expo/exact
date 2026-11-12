//! A resource over a list (LLP 1017.000 P4c, amended 2026-11-04): one
//! request for each element, keyed as a `for` row is. An element that joins
//! starts its request; one that leaves lets only its own request go; the
//! others go on. The resource reads its answers in the list's order, pending
//! or failed when any element's request is.
use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{
    Answer, DataError, DataSource, InFlight, Message, Outcome, Overlaid, Request, Response, Runner,
    Store, Write,
};
use std::cell::RefCell;
use std::rc::Rc;

#[test]
fn runner_owned_sources_cannot_be_list_resources() {
    for (source, field) in [
        ("exactSurface(\"board\")", "count: number"),
        ("exactTransport()", "state: string"),
        ("exactMIDI()", "state: string"),
    ] {
        let error = contract::compile(&format!(
            "shape Fact\n  {field}\ncomponent App\n  state xs = [1]\n  resource facts = {source} for x in xs as shape Fact\n  view\n    text facts.length.toString()\n"
        ))
        .unwrap_err();
        assert_eq!(error.id, "lower-resource-for-source", "{source}: {error}");
    }
}

const SOURCE: &str = r##"shape Got
  phase: string
component App
  state urls: string[] = ["a", "b", "c"]
  resource got = receive(u, "callback") for u in urls as shape Got
  action add(u: string)
    urls = urls.concat([u])
  action drop(u: string)
    urls = urls.filter(x => x != u)
  derive phases = got.map(g => g.phase).join(",")
  derive waiting = pending(got)
  derive broken = failed(got)
  view
    text phases
"##;

/// `receive(u, what)` waits for a GET of `https://example.test/<u>`, and its
/// reply's body is the phase; a 500 fails it. What the runner let go is
/// heard as the requests still in flight.
#[derive(Default)]
struct Receiver {
    asked: Rc<RefCell<Vec<String>>>,
    in_flight: Rc<RefCell<Vec<String>>>,
}

impl DataSource for Receiver {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }

    fn answer(&mut self, _: &mut Store, source: &str, args: &[Value]) -> Result<Answer, DataError> {
        assert_eq!(source, "receive");
        assert_eq!(
            args[1],
            Value::str("callback"),
            "the other arguments as written"
        );
        let u = args[0].as_str().unwrap().to_string();
        self.asked.borrow_mut().push(u.clone());
        Ok(Answer::Later(Request::get(&format!(
            "https://example.test/{u}"
        ))))
    }

    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        if let Outcome::Failed { message, .. } = &outcome {
            return Err(DataError::Unavailable(message.clone()));
        }
        let Outcome::Response(r) = outcome else {
            return Err(DataError::Unavailable("no reply".into()));
        };
        if r.status != 200 {
            return Err(DataError::Unavailable("the callback failed".into()));
        }
        let phase = String::from_utf8(r.body).unwrap();
        Ok(Answer::Now(Value::record(vec![Value::str(&phase)])))
    }

    fn forgotten(&mut self, _: &Store, in_flight: &[InFlight<'_>]) {
        *self.in_flight.borrow_mut() = in_flight
            .iter()
            .map(|f| f.args[0].as_str().unwrap().to_string())
            .collect();
    }
}

fn reply(status: u16, body: &str) -> Outcome {
    Outcome::Response(Response {
        status,
        headers: Vec::new(),
        body: body.as_bytes().to_vec(),
    })
}

/// The tickets the host was handed, by element.
fn asked(r: &mut Runner<Receiver>) -> Vec<(String, u64)> {
    (r.take_requests().iter())
        .map(|o| {
            (
                o.request.url.rsplit('/').next().unwrap().to_string(),
                o.ticket,
            )
        })
        .collect()
}

fn text(r: &Runner<Receiver>, derive: &str) -> String {
    match r.derive(derive) {
        Some(Value::Bool(b)) => b.to_string(),
        Some(v) => v.as_str().unwrap().to_string(),
        None => "?".into(),
    }
}

#[test]
fn each_element_has_its_own_request_and_only_a_leaving_one_lets_go() {
    let source = Receiver::default();
    let (asks, in_flight) = (source.asked.clone(), source.in_flight.clone());
    let plan = contract::compile(SOURCE).unwrap();
    let mut r = Runner::boot(
        plan,
        source,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let mut tickets = asked(&mut r);
    assert_eq!(
        tickets.iter().map(|t| t.0.as_str()).collect::<Vec<_>>(),
        ["a", "b", "c"]
    );
    assert_eq!(
        text(&r, "phases"),
        ",,",
        "each element shows its shape's zero"
    );
    assert_eq!(text(&r, "waiting"), "true");
    assert_eq!(r.pending().len(), 3, "three wait at once");

    r.act("add", vec![Value::str("d")]).unwrap();
    tickets.extend(asked(&mut r));
    assert_eq!(
        *asks.borrow(),
        ["a", "b", "c", "d"],
        "a joining element alone is asked"
    );
    assert_eq!(r.pending().len(), 4, "four wait at once");

    let ticket = |name: &str| tickets.iter().find(|t| t.0 == name).unwrap().1;
    r.fulfill(ticket("b"), reply(200, "received")).unwrap();
    assert_eq!(text(&r, "phases"), ",received,,");
    assert_eq!(text(&r, "waiting"), "true", "the others still wait");

    r.act("drop", vec![Value::str("a")]).unwrap();
    assert!(asked(&mut r).is_empty(), "nobody is asked again");
    assert_eq!(
        *in_flight.borrow(),
        ["c", "d"],
        "only `a`'s request is let go"
    );
    assert_eq!(text(&r, "phases"), "received,,");
    assert_eq!(
        r.fulfill(ticket("a"), reply(200, "late")).unwrap(),
        None,
        "a let-go reply lands nowhere"
    );

    r.fulfill(ticket("c"), reply(500, "")).unwrap();
    assert_eq!(
        text(&r, "broken"),
        "true",
        "an element's failure fails the list"
    );
    r.fulfill(ticket("d"), reply(200, "received")).unwrap();
    assert_eq!(text(&r, "phases"), "received,,received");
    assert_eq!(text(&r, "waiting"), "false");

    r.act("drop", vec![Value::str("c")]).unwrap();
    assert_eq!(text(&r, "broken"), "false", "the failed element left");
    r.act("add", vec![Value::str("a")]).unwrap();
    assert_eq!(
        asked(&mut r)
            .iter()
            .map(|t| t.0.as_str())
            .collect::<Vec<_>>(),
        ["a"],
        "a returning element asks again"
    );
    assert_eq!(text(&r, "phases"), "received,received,");
}

/// `listen(u)` streams the events of `https://example.test/<u>`, each one's
/// data its value.
struct Listener;

impl DataSource for Listener {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }

    fn answer(&mut self, _: &mut Store, _: &str, args: &[Value]) -> Result<Answer, DataError> {
        let u = args[0].as_str().unwrap();
        Ok(Answer::stream(Request::get(&format!(
            "https://example.test/{u}"
        ))))
    }

    fn parse(
        &mut self,
        _: &mut Store,
        source: &str,
        _: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        match outcome {
            Outcome::Message(m) => Ok(Answer::Now(Value::str(&m.data))),
            _ => Err(DataError::UnknownSource(source.into())),
        }
    }
}

/// A rendered page's checkpoint lists a list as open when any of its
/// elements' streams is open (LLP 1048.000 D6): an element's answer is never
/// listed.
#[test]
fn a_list_whose_elements_streams_are_open_is_listed_open() {
    let plan = contract::compile(
        "component App\n  state urls: string[] = [\"a\", \"b\"]\n  resource heard = listen(u) for u in urls as shape string\n  view\n    text heard.join(\",\")\n",
    )
    .unwrap();
    let mut r = Runner::boot(
        plan,
        Listener,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let tickets: Vec<u64> = r.take_requests().iter().map(|o| o.ticket).collect();
    assert_eq!(tickets.len(), 2);
    for ticket in tickets {
        let message = Message {
            data: "first".into(),
            ..Message::default()
        };
        r.fulfill(ticket, Outcome::Message(message)).unwrap();
    }
    let checkpoint = r.document_checkpoint("/");
    assert_eq!(checkpoint.open, ["heard"]);
    assert!(checkpoint.pending.is_empty(), "{:?}", checkpoint.pending);
}

/// A row a returning element takes asks anew: the reply to the request its
/// earlier element let go lands nowhere.
#[test]
fn a_taken_row_hears_only_its_own_request() {
    let plan = contract::compile(SOURCE).unwrap();
    let mut r = Runner::boot(
        plan,
        Receiver::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let first = asked(&mut r);
    let rows = r.plan().resources.len();
    r.act("drop", vec![Value::str("a")]).unwrap();
    r.act("add", vec![Value::str("d")]).unwrap();
    let d = asked(&mut r);
    assert_eq!(d.len(), 1);
    assert_eq!(r.plan().resources.len(), rows, "`d` takes `a`'s row");
    assert_eq!(r.fulfill(first[0].1, reply(200, "late")).unwrap(), None);
    assert_eq!(text(&r, "phases"), ",,");
    r.fulfill(d[0].1, reply(200, "received")).unwrap();
    assert_eq!(text(&r, "phases"), ",,received");
}

const WRITES: &str = r#"component App
  state urls: string[] = ["a", "b"]
  state refusing = false
  resource got = look(u) for u in urls as shape string
  resource one = look("z") as shape string
  mutation saved as shape bool refreshes got, one
  action save
    send saved = save()
  action change(next: string[])
    urls = next
  action refuse(next: string[])
    urls = next
    refusing = true
  action reload
    refresh got
  task guard key=(refusing ? 0 / 0 : 1)
    after(100000, reload)
  view
    text got.join(",") + one
"#;

#[derive(Default)]
struct Writer {
    later: bool,
    keep: bool,
    show_landed: bool,
    no_overlay: bool,
    overlaid: Vec<String>,
}

impl DataSource for Writer {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }

    fn answer(&mut self, _: &mut Store, source: &str, args: &[Value]) -> Result<Answer, DataError> {
        match source {
            "look" if !self.later => Ok(Answer::Now(Value::str("server"))),
            "look" => Ok(Answer::Later(Request::get(&format!(
                "https://example.test/{}",
                args[0].as_str().unwrap()
            )))),
            "save" => Ok(Answer::Later(Request::get("https://example.test/save"))),
            _ => self.query(source, args).map(Answer::Now),
        }
    }

    fn parse(
        &mut self,
        _: &mut Store,
        source: &str,
        _: &[Value],
        _: Outcome,
    ) -> Result<Answer, DataError> {
        Ok(Answer::Now(if source == "save" {
            Value::Bool(true)
        } else {
            Value::str("fresh")
        }))
    }

    fn overlay(
        &mut self,
        _: &str,
        args: &[Value],
        answer: &Value,
        writes: &[Write<'_>],
    ) -> Result<Option<Overlaid>, DataError> {
        if self.no_overlay {
            return Ok(None);
        }
        self.overlaid.push(args[0].as_str().unwrap().into());
        let keep: Vec<_> = writes
            .iter()
            .filter(|w| self.keep && args[0].as_str() == Some("b") && w.answered)
            .map(|w| w.id)
            .collect();
        Ok(Some(Overlaid {
            value: if self.show_landed || !keep.is_empty() || writes.iter().any(|w| !w.answered) {
                Value::str("writing")
            } else {
                answer.clone()
            },
            keep,
        }))
    }
}

fn writer() -> Runner<Writer> {
    Runner::boot(
        contract::compile(WRITES).unwrap(),
        Writer::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

fn change(r: &mut Runner<Writer>, urls: &[&str]) {
    r.act(
        "change",
        vec![Value::list(urls.iter().map(|u| Value::str(u)).collect())],
    )
    .unwrap();
}

#[test]
fn writes_overlay_each_member_and_retire_after_refresh() {
    let mut r = writer();
    let answer = Value::list(vec![Value::str("server"); 2]);
    for show_landed in [false, true, false] {
        r.data().show_landed = show_landed;
        r.data().overlaid.clear();
        r.act("save", vec![]).unwrap();
        let requests = r.take_requests();
        assert_eq!(requests.len(), 1, "a send asks no resource again");
        assert_eq!(
            r.shown_resource("got"),
            Some(&Value::list(vec![Value::str("writing"); 2]))
        );
        assert_eq!(r.shown_resource("one"), Some(&Value::str("writing")));
        assert_eq!(r.resource("got"), Some(&answer));
        assert_eq!(r.resource("one"), Some(&Value::str("server")));
        assert_eq!(
            r.document_checkpoint("/").answers,
            [
                ("got".into(), "look".into(), vec![], answer.clone()),
                (
                    "one".into(),
                    "look".into(),
                    vec![Value::str("z")],
                    Value::str("server")
                )
            ]
        );
        assert!(r.carry().resources.is_empty());
        let mut overlaid = r.data().overlaid.clone();
        overlaid.sort();
        assert_eq!(overlaid, ["a", "b", "z"]);
        r.fulfill(requests[0].ticket, reply(200, "")).unwrap();
        let shown = if show_landed { "writing" } else { "server" };
        assert_eq!(
            r.shown_resource("got"),
            Some(&Value::list(vec![Value::str(shown); 2]))
        );
        assert!(r.writes().is_empty(), "all refreshed members answered");
        assert_eq!(r.resource("got"), Some(&answer));
        assert_eq!(r.document_checkpoint("/").answers[0].3, answer);
        assert_eq!(r.carry().resources[0].3, answer);
    }
}

#[test]
fn repeated_landed_writes_over_a_list_leave_no_records() {
    let mut r = writer();
    r.data().no_overlay = true;
    for _ in 0..3 {
        r.act("save", vec![]).unwrap();
        let send = r.take_requests()[0].ticket;
        r.fulfill(send, reply(200, "")).unwrap();
    }
    assert!(r.writes().is_empty(), "retained writes: {:?}", r.writes());
}

#[test]
fn a_landed_write_waits_for_each_member_until_answered_or_removed() {
    let mut r = writer();
    r.data().later = true;
    r.data().keep = true;
    r.act("save", vec![]).unwrap();
    let send = r.take_requests()[0].ticket;
    change(&mut r, &["b", "a", "c", "b"]);
    let joined = r.take_requests()[0].ticket;
    r.fulfill(joined, reply(200, "")).unwrap();
    let answer = Value::list(
        ["server", "server", "fresh", "server"]
            .map(Value::str)
            .to_vec(),
    );
    assert_eq!(r.resource("got"), Some(&answer));
    assert_eq!(r.document_checkpoint("/").answers[0].3, answer);
    r.fulfill(send, reply(200, "")).unwrap();
    let requests = r.take_requests();
    assert_eq!(requests.len(), 4);
    for request in requests {
        r.fulfill(request.ticket, reply(200, "")).unwrap();
    }
    assert_eq!(r.writes().len(), 1, "b's overlay keeps the answered write");
    assert!(r.carry().resources.iter().all(|(name, ..)| name != "got"));
    assert_eq!(
        r.shown_resource("got"),
        Some(&Value::list(
            ["writing", "fresh", "fresh", "writing"]
                .map(Value::str)
                .to_vec()
        ))
    );
    change(&mut r, &["b"]);
    let rows = r.plan().resources.len();
    assert!(r
        .act(
            "refuse",
            vec![Value::list(["b", "d"].map(Value::str).to_vec())]
        )
        .is_err());
    assert_eq!(r.writes().len(), 1, "a refused join keeps the write");
    change(&mut r, &["b", "d"]);
    assert_eq!(
        r.plan().resources.len(),
        rows,
        "d takes a departed member's row"
    );
    assert_eq!(
        r.shown_resource("got"),
        Some(&Value::list(vec![Value::str("writing"); 2]))
    );
    let joined = r.take_requests()[0].ticket;
    r.fulfill(joined, reply(200, "")).unwrap();
    assert!(r.act("refuse", vec![Value::list(vec![])]).is_err());
    assert_eq!(r.writes().len(), 1, "a refused removal keeps the write");
    assert_eq!(
        r.shown_resource("got"),
        Some(&Value::list(["writing", "fresh"].map(Value::str).to_vec()))
    );
    change(&mut r, &["d"]);
    assert!(
        r.writes().is_empty(),
        "the last member keeping the write left"
    );

    change(&mut r, &[]);
    r.data().later = false;
    r.act("save", vec![]).unwrap();
    let send = r.take_requests()[0].ticket;
    r.fulfill(send, reply(200, "")).unwrap();
    assert!(r.writes().is_empty(), "an empty list holds no landed write");
}

#[test]
fn inspection_names_a_list_once_and_keeps_its_failure_history() {
    let mut r = Runner::boot(
        contract::compile(SOURCE).unwrap(),
        Receiver::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.measure(true, false);
    for (i, (_, ticket)) in asked(&mut r).into_iter().enumerate() {
        r.advance((i + 1) as f64 * 10.).unwrap();
        r.fulfill(
            ticket,
            Outcome::Failed {
                kind: exact_runner::FailureKind::Network,
                message: format!("{i} failed"),
            },
        )
        .unwrap();
    }
    assert_eq!(r.failed_resources(), [("got", "0 failed")]);
    let counts = r.failure_counts();
    assert_eq!(counts.len(), 1);
    assert_eq!(
        (counts[0].1.count, counts[0].1.first, counts[0].1.last),
        (3, 10., 30.)
    );
    for u in ["a", "b", "c"] {
        r.act("drop", vec![Value::str(u)]).unwrap();
    }
    r.act("add", vec![Value::str("d")]).unwrap();
    let ticket = asked(&mut r)[0].1;
    r.advance(40.).unwrap();
    r.fulfill(
        ticket,
        Outcome::Failed {
            kind: exact_runner::FailureKind::Network,
            message: "d failed".into(),
        },
    )
    .unwrap();
    let state = exact_runner::agent::state(&r);
    // Count before JSON parsing, which silently accepts duplicate object keys.
    assert_eq!(state.matches("\"got\":").count(), 4, "{state}");
    let state: serde_json::Value = serde_json::from_str(&state).unwrap();
    assert_eq!(state["failed"]["got"], "d failed");
    assert_eq!(
        state["failures"]["got"],
        serde_json::json!({"count":4,"first":10,"last":40})
    );
    assert_eq!(state["resourceStatus"].as_object().unwrap().len(), 1);
}

#[test]
fn list_members_do_not_change_the_plan() {
    let plan = contract::compile(SOURCE).unwrap();
    let encoded = plan.encode();
    let mut r = Runner::boot(
        plan,
        Receiver::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    assert_eq!(r.plan().encode(), encoded);
    r.act("add", vec![Value::str("d")]).unwrap();
    assert_eq!(r.plan().encode(), encoded);
}
