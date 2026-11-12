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
}

impl DataSource for Feed {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }

    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
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
        Feed { ready: false },
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    assert!(r.take_requests().is_empty(), "the source is not ready");
    r
}

fn one_request(r: &mut Runner<Feed>) -> RequestOut {
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
fn a_refresh_asked_before_the_source_is_ready_stays_a_refresh() {
    let mut r = launch();
    r.act("again", vec![]).unwrap();
    assert!(r.take_requests().is_empty(), "the source is not ready");
    r.data().ready = true;
    r.data_ready().unwrap();
    assert!(one_request(&mut r).forced);
}
