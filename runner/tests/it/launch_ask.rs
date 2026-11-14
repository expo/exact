//! The asks a launch makes in place of a stand-in leave as ordinary requests
//! (LLP 1162.000 D1): nobody asked for fresh data, so the executor may answer
//! one with what it fetched for this launch. A `refresh` leaves as a refresh,
//! whenever it was asked.
use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{
    Answer, DataError, DataSource, Outcome, Request, RequestOut, Response, Runner, Store,
};

const SOURCE: &str = r##"component App
  resource feed = feed() as shape string
  action again
    refresh feed
  view
    text feed
"##;

struct Feed {
    ready: bool,
    overruns: usize,
}

#[test]
fn shelf_asks_for_discover_only_on_its_tab() {
    #[derive(Default)]
    struct Shelf(std::collections::BTreeMap<String, Value>);
    impl DataSource for Shelf {
        fn bind(&mut self, plan: &exact_plan::Plan) {
            for row in &plan.resources {
                if let Ok(value) = exact_runner::surface_record::decode(plan, row.ty, None) {
                    self.0.insert(plan.str(row.source).into(), value);
                }
            }
        }
        fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
            match source {
                "library" => Ok(Value::record(vec![
                    Value::Bool(true),
                    Value::list(vec![]),
                    Value::Number(12.),
                    Value::Number(0.),
                    Value::str("—"),
                ])),
                _ => self
                    .0
                    .get(source)
                    .cloned()
                    .ok_or_else(|| DataError::UnknownSource(source.into())),
            }
        }

        fn answer(
            &mut self,
            _: &mut Store,
            source: &str,
            args: &[Value],
        ) -> Result<Answer, DataError> {
            match source {
                "subjectWorks" | "setPage" | "removeBook" => Ok(Answer::Later(Request::get(
                    &format!("https://example.test/{source}"),
                ))),
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
            Ok(Answer::Now(if source == "subjectWorks" {
                Value::record(vec![Value::Bool(true), Value::str(""), Value::list(vec![])])
            } else {
                Value::record(vec![Value::Bool(true)])
            }))
        }
    }

    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../apps/shelf/app.contract");
    let mut r = Runner::boot(
        contract::compile_path(&path).unwrap(),
        Shelf::default(),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    assert!(r.pending().is_empty(), "the library asks no Discover work");
    assert!(r.take_requests().is_empty());
    for (action, args) in [
        ("turnTo", vec![Value::str("seed-2"), Value::Number(130.)]),
        ("swipeRemove", vec![Value::str("seed-2")]),
    ] {
        r.act(action, args).unwrap();
        let request = one_request(&mut r);
        assert_eq!(r.pending(), [("changed".into(), request.ticket)]);
        r.fulfill(
            request.ticket,
            Outcome::Response(Response {
                status: 200,
                headers: vec![],
                body: vec![],
            }),
        )
        .unwrap();
        r.land_then();
        assert!(
            r.pending().is_empty(),
            "the mutation settled without Discover"
        );
    }
    r.act("pick", vec![Value::str("discover")]).unwrap();
    let request = one_request(&mut r);
    assert_eq!(r.pending(), [("works".into(), request.ticket)]);
    r.act("pick", vec![Value::str("stats")]).unwrap();
    assert!(
        !r.holds(request.ticket),
        "leaving Discover cancels its read"
    );
    assert!(r.pending().is_empty());
    r.act("chooseSubject", vec![Value::str("fantasy")]).unwrap();
    assert!(r.take_requests().is_empty());
    r.act("pick", vec![Value::str("discover")]).unwrap();
    let request = one_request(&mut r);
    r.fulfill(
        request.ticket,
        Outcome::Response(Response {
            status: 200,
            headers: vec![],
            body: vec![],
        }),
    )
    .unwrap();
    assert!(r.pending().is_empty());
    assert!(r
        .failed_resources()
        .iter()
        .all(|(name, _)| *name != "works"));
}

#[test]
fn changing_only_prefetch_keeps_the_request_but_changing_its_body_replaces_it() {
    struct Source;
    impl DataSource for Source {
        fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
            Err(DataError::UnknownSource(source.into()))
        }
        fn answer(&mut self, _: &mut Store, _: &str, args: &[Value]) -> Result<Answer, DataError> {
            let mut request = Request::post_json("https://example.test/feed", "{}");
            request.prefetch = args[0] == Value::Number(1.);
            if args[0] == Value::Number(2.) {
                request.body = b"{\"changed\":true}".to_vec();
            }
            Ok(Answer::Later(request))
        }
    }
    let plan = contract::compile("component App\n  state n = 0\n  resource feed = feed(n) as shape string\n  action next\n    n = n + 1\n  view\n    text feed\n").unwrap();
    let mut r = Runner::boot(
        plan,
        Source,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let first = r.take_requests();
    assert_eq!(first.len(), 1);
    let ticket = first[0].ticket;
    r.act("next", vec![]).unwrap();
    assert!(r.take_requests().is_empty());
    assert_eq!(r.pending(), [("feed".into(), ticket)]);
    assert!(r.holds(ticket));
    r.act("next", vec![]).unwrap();
    let replaced = r.take_requests();
    assert_eq!(replaced.len(), 1);
    assert_ne!(replaced[0].ticket, ticket);
    assert_eq!(replaced[0].request.body, b"{\"changed\":true}");
    assert!(!r.holds(ticket));
}

impl DataSource for Feed {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }

    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        if self.overruns > 0 {
            self.overruns -= 1;
            return Err(DataError::Overrun("turn budget".into()));
        }
        Ok(Answer::Later(Request::get("https://example.test/feed")))
    }

    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        _: Outcome,
    ) -> Result<Answer, DataError> {
        Ok(Answer::Now(Value::str("fresh")))
    }

    fn ready(&self) -> bool {
        self.ready
    }
}

fn launch() -> Runner<Feed> {
    let mut r = Runner::boot(
        contract::compile(SOURCE).unwrap(),
        Feed {
            ready: false,
            overruns: 0,
        },
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    assert!(r.take_requests().is_empty(), "the source is not ready");
    r
}

#[track_caller]
fn one_request(r: &mut Runner<impl DataSource>) -> RequestOut {
    let mut requests = r.take_requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    requests.pop().unwrap()
}

fn answered(r: &mut Runner<Feed>, ticket: u64) {
    let reply = Response {
        status: 200,
        headers: Vec::new(),
        body: Vec::new(),
    };
    r.fulfill(ticket, Outcome::Response(reply)).unwrap();
    assert_eq!(r.resource("feed"), Some(&Value::str("fresh")));
}

#[test]
fn the_ask_made_when_the_source_becomes_ready_is_not_a_refresh() {
    let mut r = launch();
    r.data().ready = true;
    r.data_ready().unwrap();
    let first = one_request(&mut r);
    assert!(!first.forced, "the launch's own ask");
    answered(&mut r, first.ticket);
    r.act("again", vec![]).unwrap();
    assert!(one_request(&mut r).forced, "the app's refresh");
}

#[test]
fn a_launch_ask_over_its_budget_stays_unforced_at_its_reask() {
    let mut r = launch();
    r.data().ready = true;
    r.data().overruns = 1;
    r.data_ready().unwrap();
    assert!(r.take_requests().is_empty());
    let due = r.reask_due_ms().expect("a held re-ask");
    r.advance(due).unwrap();
    let request = one_request(&mut r);
    assert!(!request.forced, "the launch's re-ask");
    answered(&mut r, request.ticket);
    r.act("again", vec![]).unwrap();
    assert!(one_request(&mut r).forced, "the app's refresh");
}

#[test]
fn a_refresh_asked_before_the_source_is_ready_stays_a_refresh() {
    let mut r = launch();
    r.act("again", vec![]).unwrap();
    assert!(r.take_requests().is_empty(), "the source is not ready");
    r.data().ready = true;
    r.data_ready().unwrap();
    assert!(one_request(&mut r).forced);
}

struct ImmediateLaunch {
    ready: bool,
    asks: usize,
    fail: bool,
}
impl DataSource for ImmediateLaunch {
    fn grants(&self) -> &str {
        "secret.keep peer\nnet.fetch https://example.test"
    }
    fn ready(&self) -> bool {
        self.ready
    }
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!()
    }
    fn answer(&mut self, store: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        let _ = store.get("peer");
        self.asks += 1;
        if self.asks == 1 {
            if self.fail {
                return Err(DataError::Unavailable("launch failed".into()));
            }
            return Ok(Answer::Now(Value::str("launch")));
        }
        Ok(Answer::Later(Request::get("https://example.test/feed")))
    }
}

#[test]
fn a_launch_ask_ending_now_or_failed_does_not_mark_a_later_store_refresh() {
    let mut unforced = Vec::new();
    for fail in [false, true] {
        let mut r = Runner::boot(
            contract::compile(SOURCE).unwrap(),
            ImmediateLaunch {
                ready: false,
                asks: 0,
                fail,
            },
            Kernel::with_monospace(),
            Default::default(),
            "/",
        )
        .unwrap();
        r.data().ready = true;
        r.data_ready().unwrap();
        assert!(r.take_requests().is_empty());
        assert_eq!(r.data().asks, 1);
        r.store_changed(vec![("peer".into(), b"changed".to_vec())])
            .unwrap();
        let requests = r.take_requests();
        assert_eq!(requests.len(), 1, "{requests:?}");
        if !requests[0].forced {
            unforced.push(fail);
        }
    }
    assert!(
        unforced.is_empty(),
        "unforced store refreshes after launch (failed): {unforced:?}"
    );
}

#[test]
fn a_refresh_replaces_a_launch_ask_that_waits_over_its_budget() {
    let mut r = launch();
    r.data().ready = true;
    r.data().overruns = 1;
    r.data_ready().unwrap();
    assert!(r.take_requests().is_empty());
    r.act("again", vec![]).unwrap();
    assert!(one_request(&mut r).forced, "the app's refresh");
}

const LIST: &str = r##"component App
  state names: string[] = ["a"]
  resource feeds = feed(n) for n in names as shape string
  action again
    refresh feeds
  view
    text feeds.join(",")
"##;

fn launch_list() -> Runner<Feed> {
    let mut r = Runner::boot(
        contract::compile(LIST).unwrap(),
        Feed {
            ready: false,
            overruns: 0,
        },
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    assert!(r.take_requests().is_empty(), "the source is not ready");
    r.data().ready = true;
    r.data().overruns = 1;
    r.data_ready().unwrap();
    assert!(
        r.take_requests().is_empty(),
        "its member is over its budget"
    );
    r
}

#[test]
fn a_list_members_launch_ask_over_its_budget_stays_unforced_at_its_reask() {
    let mut r = launch_list();
    let due = r.reask_due_ms().expect("a held re-ask");
    r.advance(due).unwrap();
    assert!(!one_request(&mut r).forced, "the launch's re-ask");
}

#[test]
fn refreshing_a_list_refreshes_a_member_whose_launch_ask_waits() {
    let mut r = launch_list();
    r.act("again", vec![]).unwrap();
    assert!(one_request(&mut r).forced, "the app's refresh");
}

const SENDS: &str = r##"component App
  resource feed = feed() as shape string
  mutation saved as shape string
  action save
    send saved = save("draft")
  view
    text feed
"##;

#[test]
fn the_ask_made_with_sends_waiting_for_the_source_is_not_a_refresh() {
    let mut r = Runner::boot(
        contract::compile(SENDS).unwrap(),
        Feed {
            ready: false,
            overruns: 0,
        },
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.act("save", vec![]).unwrap();
    assert!(r.take_requests().is_empty(), "the source is not ready");
    r.data().ready = true;
    r.data_ready().unwrap();
    let requests = r.take_requests();
    assert_eq!(requests.len(), 2, "{requests:?}");
    let read = requests.iter().find(|o| o.target == "feed").unwrap();
    assert!(!read.forced, "the launch's own ask");
}

const LIST_REFRESHED: &str = r##"component App
  state names: string[] = ["a"]
  resource feeds = feed(n) for n in names as shape string
  mutation saved as shape string refreshes feeds
  action save
    send saved = save("draft")
  view
    text feeds.join(",")
"##;

struct Saves {
    ready: bool,
}
impl DataSource for Saves {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
    fn answer(&mut self, _: &mut Store, source: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(match source {
            "save" => Answer::Now(Value::str("saved")),
            _ => Answer::Later(Request::get("https://example.test/feed")),
        })
    }
    fn ready(&self) -> bool {
        self.ready
    }
}

#[test]
fn a_send_that_refreshes_a_list_at_activation_refreshes_its_members() {
    let mut r = Runner::boot(
        contract::compile(LIST_REFRESHED).unwrap(),
        Saves { ready: false },
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.act("save", vec![]).unwrap();
    assert!(r.take_requests().is_empty(), "the source is not ready");
    r.data().ready = true;
    r.data_ready().unwrap();
    let requests = r.take_requests();
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert!(requests[0].forced, "the send's declared refresh");
}

#[test]
fn a_refresh_made_while_the_source_is_away_ends_a_waiting_launch_asks_exemption() {
    let mut r = launch();
    r.data().ready = true;
    r.data().overruns = 1;
    r.data_ready().unwrap();
    assert!(r.take_requests().is_empty());
    r.data().ready = false;
    r.act("again", vec![]).unwrap();
    assert!(r.take_requests().is_empty(), "the source is away");
    r.data().ready = true;
    let due = r.reask_due_ms().expect("a held re-ask");
    r.advance(due).unwrap();
    assert!(one_request(&mut r).forced, "the app's refresh");
}
