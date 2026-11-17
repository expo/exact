//! A tip over an element (LLP 1035.006.000): its parts reach the kernel as
//! props, and a tip with no `id`, a part with no `tip` and an image that is
//! no symbol are refused where they are written.

use exact_kernel::{Kernel, PropId};
use exact_runner::{DataError, DataSource, Event, Runner, Value};

struct NoData;

impl DataSource for NoData {
    fn query(&mut self, source: &str, _args: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.to_string()))
    }
}

fn app(attrs: &str) -> String {
    format!(
        "component App\n  state changed = false\n  action change\n    changed = true\n  view\n    column\n      button press=change testId=\"change\"{attrs}\n        text \"Change\"\n"
    )
}

fn refused(attrs: &str) -> (String, String) {
    let e = contract::compile(&app(attrs)).unwrap_err();
    (e.id.to_string(), e.message)
}

#[test]
fn a_tips_parts_are_props_and_done_follows_state() {
    let plan = contract::compile(&app(
        " id=\"change-station\" tip=\"Pick your station\" tipMessage=\"Tap to choose where you leave from.\" tipImage=\"symbol:gearshape\" tipDone=changed",
    ))
    .unwrap_or_else(|e| panic!("{e}"));
    let mut r = Runner::boot(
        plan,
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let key = r.kernel().find_by_test_id("change")[0];
    let id = r.kernel().node_by_key(key).unwrap().id;
    let node = r.kernel().node(id).unwrap();
    assert_eq!(node.props.str(PropId::Tip), Some("Pick your station"));
    assert_eq!(
        node.props.str(PropId::TipMessage),
        Some("Tap to choose where you leave from.")
    );
    // A role by another spelling is written as the role, as an image's source is.
    assert_eq!(node.props.str(PropId::TipImage), Some("symbol:settings"));
    assert_eq!(node.props.bool(PropId::TipDone), Some(false));
    r.dispatch(id, Event::Press).unwrap();
    let node = r.kernel().node(id).unwrap();
    assert_eq!(node.props.bool(PropId::TipDone), Some(true));
}

#[test]
fn a_tip_without_an_id_a_part_without_a_tip_and_an_image_that_is_no_symbol_are_refused() {
    let (id, message) = refused(" tip=\"Pick your station\"");
    assert_eq!(id, "lower-tip-id");
    assert!(message.contains("`id`"), "{message}");
    for part in [
        "tipMessage=\"More\"",
        "tipImage=\"symbol:add\"",
        "tipDone=changed",
    ] {
        let (id, message) = refused(&format!(" id=\"change-station\" {part}"));
        assert_eq!(id, "lower-tip-parts", "{part}");
        assert!(message.contains("beside a `tip`"), "{message}");
    }
    let (id, message) = refused(" id=\"s\" tip=\"Pick\" tipImage=\"images/tram.png\"");
    assert_eq!(id, "lower-tip-image");
    assert!(message.contains("symbol:"), "{message}");
    let (id, _) = refused(" id=\"s\" tip=\"Pick\" tipImage=\"symbol:not-a-role\"");
    assert_eq!(id, "lower-attr-value");
}
