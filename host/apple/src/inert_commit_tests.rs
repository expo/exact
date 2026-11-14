//! A list's feedback or an action whose commit changes no node does no
//! layout (`inert_layout`), as an unchanged advance does: on a chat
//! benchmark six in ten such commits did a whole layout for nothing (LLP
//! 1167). A commit that changed something, a layout owed, and every other
//! entry (the viewport, the insets, the preferences, the text size, an
//! intrinsic size) still lay out in full.
use super::*;
use exact_kernel::MonospaceMeasurer;
use exact_plan::Value;
use exact_runner::{CollectionFeedback, DataError, Event, Preferences, RowMeasurement};

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}

/// A list of a hundred rows whose scrolls and start run actions no node
/// shows, a counter no node shows, one a text shows, an image, a box the
/// safe area pads, and the viewport, which no node reads.
const LIST: &str = r#"shape V
  height: number

component A
  resource viewport = exactViewport() as shape V
  state hidden = 0
  state shown = 0
  action hide
    hidden = hidden + 1
  action show
    shown = shown + 1
  action scrolled(left: number, top: number)
    hidden = hidden + 1
  action reached
    hidden = hidden + 1
  view
    column
      button press=hide testId="hide" width=60 height=20
      button press=show testId="show" width=60 height=20
      text `${shown}` testId="count"
      image "fixture.png" testId="image"
      box height=1 padding-top="env(safe-area-inset-top)"
      list scroll=scrolled reachstart=reached reach-margin="10px" virtualized=true height=160 width=240 testId="list"
        for i in range(100) key=i
          text `${i}` height=20
"#;

fn boot() -> Host<NoData> {
    let plan = contract::compile(LIST).unwrap().encode();
    let (mut host, _) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        390.0,
        844.0,
    )
    .unwrap();
    // The port's first reports, 35 points down, mount its rows and settle
    // their heights.
    let mut quiet = 0;
    for _ in 0..40 {
        quiet = if report(&mut host, 35.0).contains("\"op\":\"create\"") {
            0
        } else {
            quiet + 1
        };
        if quiet == 3 {
            return host;
        }
    }
    panic!("the list never settled")
}

/// The list's port at `offset`, with every mounted row measured 20 tall.
fn report(host: &mut Host<NoData>, offset: f64) -> String {
    let c = host.runner.collections().remove(0);
    let f = CollectionFeedback {
        view: c.view,
        revision: c.revision,
        scroll_sequence: c.scroll_sequence + 1,
        offset,
        port_cross: 240.,
        port_main: 160.,
        cross: 240.,
        padding: [0.0; 2],
        focus_view: None,
        interaction_view: None,
        measurements: c
            .rows
            .iter()
            .map(|r| RowMeasurement {
                view: r.view,
                epoch: r.epoch,
                size: 20.,
            })
            .collect(),
    };
    let reply = host.collection_feedback(&f.encode().unwrap(), 0.);
    assert!(reply.contains("\"error\":null"), "{reply}");
    reply
}

fn press(host: &mut Host<NoData>, test_id: &str) -> String {
    let k = host.runner.kernel();
    let id = k.node_by_key(k.find_by_test_id(test_id)[0]).unwrap().id;
    host.dispatch_at(id, Event::Press, 0.0)
}

#[test]
fn a_report_that_mounts_no_row_lays_nothing_out() {
    let mut host = boot();
    let layouts = host.layout_calls;
    // The port reaches the list's start, whose action changes what no node
    // shows; every row it shows is mounted.
    let batch = report(&mut host, 25.0);
    assert!(batch.contains("(reached)"), "the action ran: {batch}");
    assert!(!batch.contains("\"op\":\"create\""), "{batch}");
    assert!(!batch.contains("\"op\":\"destroy\""), "{batch}");
    assert_eq!(host.layout_calls, layouts, "{batch}");
    assert!(!batch.contains("\"op\":\"frame\""), "{batch}");
}

#[test]
fn an_action_nothing_shows_lays_nothing_out_and_one_shown_does() {
    let mut host = boot();
    let layouts = host.layout_calls;
    let batch = press(&mut host, "hide");
    assert!(batch.contains("(hide)"), "the action ran: {batch}");
    assert_eq!(host.layout_calls, layouts, "a counter no node reads");
    // The flag ends with the action: the viewport after it lays out.
    host.resize(392.0, 844.0);
    assert!(host.layout_calls > layouts, "a resize after it");
    let layouts = host.layout_calls;
    let batch = press(&mut host, "show");
    assert!(
        host.layout_calls > layouts,
        "the count's text changed: {batch}"
    );
}

#[test]
fn a_scroll_whose_action_nothing_shows_lays_nothing_out() {
    let mut host = boot();
    let list = host.runner.collections()[0].view;
    let layouts = host.layout_calls;
    let batch = host.dispatch_at(
        list,
        Event::scroll_payload("0,4,240,2000,240,160").unwrap(),
        1.0,
    );
    assert!(batch.contains("(scrolled)"), "the action ran: {batch}");
    assert_eq!(host.layout_calls, layouts, "{batch}");
}

#[test]
fn a_report_that_mounts_rows_lays_out() {
    let mut host = boot();
    let layouts = host.layout_calls;
    report(&mut host, 1200.0);
    assert!(host.layout_calls > layouts);
}

#[test]
fn a_report_while_a_layout_is_owed_lays_out() {
    let mut host = boot();
    host.layout_owed = true;
    let layouts = host.layout_calls;
    report(&mut host, 25.0);
    assert!(host.layout_calls > layouts);
}

#[test]
fn the_viewport_and_the_insets_lay_out_whatever_their_receipts() {
    let mut host = boot();
    let layouts = host.layout_calls;
    host.resize(392.0, 844.0);
    assert!(host.layout_calls > layouts, "a resize");
    let layouts = host.layout_calls;
    host.set_insets(47.0, 0.0, 34.0, 0.0);
    assert!(host.layout_calls > layouts, "the safe area");
}

#[test]
fn the_preferences_and_the_text_size_lay_out() {
    let mut host = boot();
    let layouts = host.layout_calls;
    host.set_preferences(Preferences {
        bold_text: true,
        ..Preferences::default()
    });
    assert!(host.layout_calls > layouts, "bold text");
    let layouts = host.layout_calls;
    host.set_root_font_size(20.0);
    assert!(host.layout_calls > layouts, "the text size");
}

/// The presenter's appearance re-targets paint only: a `light-dark()` colour
/// resolves in paint, and no box depends on the scheme.
#[test]
fn the_scheme_never_lays_out() {
    let mut host = boot();
    let layouts = host.layout_calls;
    for dark in [true, false, true] {
        let batch = host.set_scheme(dark);
        assert!(batch.contains("\"error\":null"), "{batch}");
        assert!(!batch.contains("\"op\":\"frame\""), "{batch}");
    }
    assert_eq!(host.layout_calls, layouts);
}

#[test]
fn an_intrinsic_size_lays_out() {
    let mut host = boot();
    let k = host.runner.kernel();
    let id = k.node_by_key(k.find_by_test_id("image")[0]).unwrap().id;
    let layouts = host.layout_calls;
    let batch = host.set_intrinsic(id, Some((30.0, 20.0)));
    assert!(batch.contains("\"error\":null"), "{batch}");
    assert!(host.layout_calls > layouts);
}

#[test]
fn a_panic_inside_an_inert_entry_ends_it() {
    let mut host = boot();
    let caught = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        host.in_inert_entry(|_| -> String { panic!("a commit failed") })
    }));
    assert!(caught.is_err());
    assert!(!host.inert_entry);
    // The viewport after it still lays out in full.
    let layouts = host.layout_calls;
    host.resize(392.0, 844.0);
    assert!(host.layout_calls > layouts);
}
