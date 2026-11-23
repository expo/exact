//! LLP 1133.023 D7a: what the host observes of the page is the device's,
//! and a world's message under a tool's mark keeps D4a's command rules.

use crate::request::Request;
use crate::{Answer, CallOrigin, DataError, DataSource, Event, Outcome, Runner, Store, Value};
use exact_kernel::Kernel;

/// Every send answers later.
struct Later;

impl DataSource for Later {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        unreachable!("answers through answer")
    }

    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::Later(Request::native(b"{}".to_vec())))
    }

    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        _: Outcome,
    ) -> Result<Answer, DataError> {
        Ok(Answer::Now(Value::Number(0.0)))
    }
}

const FRAME: &str = "component App\n  mutation m as shape number\n  tool ping writes \"Ping.\" = loaded\n  action loaded\n    send m = save(1)\n  action heard(text: string)\n    focus(\"x\")\n  view\n    iframe \"/deck/index.html\" load=loaded message=heard testId=\"deck\"\n";

fn boot() -> (Runner<Later>, exact_kernel::ViewId) {
    let plan = contract::compile(FRAME).unwrap();
    let r = Runner::boot(
        plan,
        Later,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let key = r.kernel().find_by_test_id("deck")[0];
    let id = r.kernel().node_by_key(key).unwrap().id;
    (r, id)
}

#[test]
fn a_load_the_host_observes_is_the_devices() {
    let (mut r, frame) = boot();
    r.take_requests();
    r.dispatch(frame, Event::Load).unwrap();
    let sent = r.take_requests();
    assert_eq!(sent[0].request.reach.origin, CallOrigin::Device);
}

#[test]
fn a_worlds_message_under_a_tools_mark_keeps_the_calls_command_rules() {
    let (mut r, frame) = boot();
    r.dispatch(frame, Event::Message("\u{1}origin:tool\u{1}hi".into()))
        .unwrap();
    let lines: Vec<String> = r.journal().map(str::to_owned).collect();
    assert!(
        lines
            .iter()
            .any(|l| l.contains("focus: skipped in a tool call")),
        "{lines:?}"
    );
    // The app's own message runs it.
    r.dispatch(frame, Event::Message("hi".into())).unwrap();
    let skipped = r.journal().filter(|l| l.contains("focus: skipped")).count();
    assert_eq!(skipped, 1);
}
