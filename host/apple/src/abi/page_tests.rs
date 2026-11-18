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
        // Without I/O the bridge refuses the stream (`without_io`), but it is
        // pending until a pump settles that: the freeze is what releases it.
        let host = bridge.host.as_mut().unwrap();
        let first = host.runner().pending()[0].1;
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
        let host = bridge.host.as_ref().unwrap();
        assert!(
            host.runner().pending().iter().any(|(_, t)| *t != first),
            "foreground opens a fresh stream"
        );
    }
}

/// LLP 1069.000 D2 r7 on Apple: with the first-frame latch on, the page asks
/// for a frame though nothing animates; that frame's tick sets `presented`,
/// and what the boot began starts at it (LLP 1003.001 D9); `prefer
/// presented` holds the fact.
#[test]
fn the_first_frame_presents_the_page() {
    const SPLASH: &str = r##"keyframes enter
  from opacity=0
shape Page
  presented: bool
component A
  resource page = exactPage() as shape Page
  view
    column
      box testId="logo" width=24 height=24 animation="enter 400ms ease-out both"
"##;
    let plan = contract::compile(SPLASH).unwrap().encode();
    let mut bridge = Bridge::new();
    bridge.start_on_frame(true, 0.0);
    let len = bridge.boot(&plan, Feed, Hooks::none(), 390., 844.);
    let booted = String::from_utf8_lossy(bridge.output_bytes(len as usize)).into_owned();
    assert!(
        booted.contains("\"motion\":true"),
        "the link runs for it: {booted}"
    );
    let seen = |bridge: &Bridge<Feed>| {
        bridge
            .host
            .as_ref()
            .unwrap()
            .runner()
            .resource("page")
            .cloned()
            == Some(Value::record(vec![Value::Bool(true)]))
    };
    assert!(!seen(&bridge));
    let host = bridge.host.as_mut().unwrap();
    host.tick(5.0);
    assert!(
        !host.runner().presented().seen,
        "a tick with no frame is no present"
    );
    host.tick_at(10.0, 16.0);
    assert!(host.runner().presented().seen);
    let logo = {
        let k = host.runner().kernel();
        exact_kernel::motion::motion_node(k.find_by_test_id("logo")[0])
    };
    assert_eq!(host.engine().animation_plays(logo)[0].start, 0.016);
    assert!(seen(&bridge));
    bridge.host.as_mut().unwrap().prefer_presented(Some(false));
    assert!(!seen(&bridge), "held by the agent");
    bridge.host.as_mut().unwrap().prefer_presented(None);
    assert!(seen(&bridge));
}

/// LLP 1164 D4 on Apple: the motion a still page reports only to await its
/// first present says so (`presentOnly`), so the host enters that tick as no
/// event; once it has presented, or while anything animates, it does not.
#[test]
fn a_still_page_awaiting_its_present_says_its_motion_is_only_that() {
    let boot = |source: &str| {
        let plan = contract::compile(source).unwrap().encode();
        let mut bridge = Bridge::new();
        bridge.start_on_frame(true, 0.0);
        let len = bridge.boot(&plan, Feed, Hooks::none(), 390., 844.);
        let booted = String::from_utf8_lossy(bridge.output_bytes(len as usize)).into_owned();
        (bridge, booted)
    };
    let (mut still, booted) =
        boot("component A\n  view\n    column\n      box width=24 height=24\n");
    assert!(
        booted.contains("\"motion\":true") && booted.contains("\"presentOnly\":true"),
        "{booted}"
    );
    let presented = still.host.as_mut().unwrap().tick_at(10.0, 16.0);
    assert!(presented.contains("\"motion\":false"), "{presented}");
    assert!(!presented.contains("\"presentOnly\""), "{presented}");
    let (_, animating) = boot(
        "keyframes enter\n  from opacity=0\ncomponent A\n  view\n    column\n      box width=24 height=24 animation=\"enter 400ms ease-out both\"\n",
    );
    assert!(
        animating.contains("\"motion\":true") && !animating.contains("\"presentOnly\""),
        "{animating}"
    );
    let (_, canvas) =
        boot("component A\n  view\n    column\n      canvas surface=tile() width=100 height=50\n");
    assert!(
        canvas.contains("\"motion\":true") && !canvas.contains("\"presentOnly\""),
        "a canvas's ticks draw: {canvas}"
    );
}
