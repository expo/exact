use super::*;
use exact_runner::{Answer, DataError, Message, Outcome, Request, Store, Value};

struct Feed;
impl DataSource for Feed {
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> {
        Ok(Value::Number(0.))
    }
    fn answer(&mut self, _: &mut Store, _: &str, _: &[Value]) -> Result<Answer, DataError> {
        Ok(Answer::stream(Request::get("https://example.test/feed")))
    }
    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        _: Outcome,
    ) -> Result<Answer, DataError> {
        Ok(Answer::Now(Value::Number(1.)))
    }
}

#[test]
fn sound_batches_carry_transport_eligibility_even_without_voice_ops() {
    let plan = contract::compile(include_str!("../../tests/fixtures/page-freeze.contract"))
        .unwrap()
        .encode();
    let mut bridge = Bridge::new();
    bridge.boot(&plan, Feed, Hooks::none(), 390., 844.);
    let host = bridge.host.as_ref().unwrap();
    let kernel = host.runner().kernel();
    let play = kernel
        .node_by_key(kernel.find_by_test_id("play")[0])
        .unwrap()
        .id;
    bridge.dispatch(play, 0, 0, 0.);
    let sound = |bridge: &Bridge<Feed>| {
        let batch: serde_json::Value = serde_json::from_slice(&bridge.output).unwrap();
        batch["ops"]
            .as_array()
            .unwrap()
            .iter()
            .find(|op| op["op"] == "sound")
            .cloned()
            .unwrap()
    };
    assert_eq!(sound(&bridge)["playing"], true);
    bridge.advance(600., 0);
    assert_eq!(sound(&bridge)["playing"], true);
    assert_eq!(sound(&bridge)["ops"], serde_json::json!([]));
    bridge.set_page(128);
    assert_eq!(sound(&bridge)["playing"], false);
}

#[test]
fn a_boot_without_sequences_clears_the_previous_transport_eligibility() {
    let source = include_str!("../../tests/fixtures/page-freeze.contract");
    let plan = contract::compile(source).unwrap().encode();
    let replacement = contract::compile(&source.replace(
        "  sequence pulse when playing\n    play([Track(beats=4, loops=0, offset=0, output=\"\", steps=[])], bpm=60)\n",
        "",
    )).unwrap().encode();
    let mut bridge = Bridge::new();
    bridge.boot(&plan, Feed, Hooks::none(), 390., 844.);
    let kernel = bridge.host.as_ref().unwrap().runner().kernel();
    let play = kernel
        .node_by_key(kernel.find_by_test_id("play")[0])
        .unwrap()
        .id;
    bridge.dispatch(play, 0, 0, 0.);
    bridge.boot(&replacement, Feed, Hooks::none(), 390., 844.);
    let batch: serde_json::Value = serde_json::from_slice(&bridge.output).unwrap();
    let sound = batch["ops"]
        .as_array()
        .unwrap()
        .iter()
        .rfind(|op| op["op"] == "sound")
        .unwrap();
    assert_eq!(sound["playing"], false);
}

#[test]
fn the_freeze_bit_releases_pending_and_delivered_streams_before_returning() {
    let plan = contract::compile(
        "shape Page\n  frozen: bool\ncomponent App\n  resource page = exactPage() as shape Page\n  resource feed = feed() when not page.frozen as shape number else empty()\n  view\n    text `${page.frozen} ${feed}`\n",
    ).unwrap().encode();
    for delivered in [false, true] {
        let mut links = crate::link::Links::ALL;
        links.io = None;
        let mut bridge = Bridge::with_links(links);
        bridge.boot(&plan, Feed, Hooks::none(), 390., 844.);
        let host = bridge.host.as_mut().unwrap();
        let first = host.take_requests().pop().unwrap().ticket;
        if delivered {
            host.runner_mut()
                .fulfill(first, Outcome::Message(Message::default()))
                .unwrap();
        }
        assert!(host.runner().holds(first));
        bridge.set_page(1);
        assert!(
            bridge.host.as_ref().unwrap().runner().holds(first),
            "visibility alone keeps the stream"
        );
        bridge.set_page(1 | 128);
        let host = bridge.host.as_mut().unwrap();
        assert!(host.runner().page().frozen);
        assert!(!host.runner().holds(first));
        assert!(host.runner().streams().is_empty());
        bridge.set_page(0);
        let host = bridge.host.as_mut().unwrap();
        assert_ne!(
            host.take_requests().pop().unwrap().ticket,
            first,
            "foreground opens a fresh stream"
        );
    }
}
