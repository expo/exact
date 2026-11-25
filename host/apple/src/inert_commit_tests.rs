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
    boot_with(LIST)
}

fn boot_with(source: &str) -> Host<NoData> {
    let plan = contract::compile(source).unwrap().encode();
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
fn an_action_nothing_shows_owes_no_route_overflow_check() {
    let mut host = boot();
    host.route_overflow.dirty = false;
    press(&mut host, "hide");
    assert!(!host.route_overflow.dirty, "a counter no node reads");
    press(&mut host, "show");
    assert!(host.route_overflow.dirty, "the count's text changed");
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

#[test]
fn an_empty_report_reuses_finish_only_while_the_deadline_is_unchanged() {
    let mut host = boot();
    let before = host.finish_passes;
    for offset in [35.1, 35.2, 35.3] {
        let reply = report(&mut host, offset);
        assert!(reply.starts_with("{\"ops\":[]"), "{reply}");
    }
    assert_eq!(
        host.finish_passes, before,
        "three reports do no finish passes"
    );

    let mut batch = Batch::new();
    batch.destroy(1);
    host.finish(batch, None);
    assert_eq!(host.finish_passes, before + 1, "a changed batch finishes");
    let passes = host.finish_passes;
    host.finish(Batch::new(), Some("refused".into()));
    assert!(!host.route_overflow.settled);
    host.finish_collection();
    assert_eq!(
        host.finish_passes,
        passes + 2,
        "a refused finish is not reused"
    );
    assert!(host.route_overflow.settled);

    let plan = contract::compile(
        r#"component A
  state done = false
  task end mount
    after(100, end)
  action end
    done = true
  view
    text "timer"
"#,
    )
    .unwrap()
    .encode();
    let (mut host, _) = Host::boot(
        &plan,
        NoData,
        Box::new(MonospaceMeasurer::default()),
        100.,
        100.,
    )
    .unwrap();
    let first = host.finish_collection();
    assert!(first.contains("\"timer_due_ms\":100"), "{first}");
    let before = host.finish_passes;
    host.finish_collection();
    assert_eq!(host.finish_passes, before);
    host.runner.advance_timed(100.);
    let reply = host.finish_collection();
    assert!(!reply.contains("timer_due_ms"), "{reply}");
    assert_eq!(
        host.finish_passes,
        before + 1,
        "a deadline change finishes even without ops"
    );
}

#[test]
fn a_hundred_offset_reports_write_no_batch_and_one_shot_output_is_not_replayed() {
    let mut host = boot();
    report(&mut host, 35.1);
    let reply = report(&mut host, 35.2);
    let writes = host.batch_writes;
    for n in 0..100 {
        assert_eq!(report(&mut host, 35.2 + f64::from(n) / 1000.), reply);
    }
    assert_eq!(
        host.batch_writes, writes,
        "no batch construction or serialization"
    );

    host.log("between reports");
    let logged = report(&mut host, 35.4);
    assert!(logged.contains("between reports"), "{logged}");
    assert_eq!(host.batch_writes, writes + 1);
    assert!(!report(&mut host, 35.5).contains("between reports"));
    let writes = host.batch_writes;
    report(&mut host, 35.6);
    assert_eq!(host.batch_writes, writes);

    host.runner.store().announce_topic("collection-test");
    let announced = report(&mut host, 35.7);
    assert!(announced.contains("collection-test"), "{announced}");
    assert!(!report(&mut host, 35.8).contains("collection-test"));

    host.runner.use_native_timers(true);
    let native = host.finish(Batch::new(), None);
    assert!(native.contains("\"nativeTimers\":[]"), "{native}");
    host.runner.use_native_timers(false);
    let ordinary = report(&mut host, 35.9);
    assert!(!ordinary.contains("nativeTimers"), "{ordinary}");

    let writes = host.batch_writes;
    report(&mut host, 1200.);
    assert!(
        host.batch_writes > writes,
        "a changed window writes its batch"
    );
}

#[test]
fn an_empty_report_delivers_resize_notifications_left_by_the_depth_limit() {
    let mut host = boot_with(
        r#"component A
  state side = 100
  state observed = 0
  state growing = false
  action grow
    growing = true
    side = 110
  action resized(w: number, h: number, r: DOMRectReadOnly)
    observed = w
    if growing && w < 130
      side = w + 10
  view
    column
      button press=grow testId="grow" width=60 height=20
      box width=side height=20 resize=resized
      list virtualized=true height=160 width=240 testId="list"
        for i in range(100) key=i
          text `${i}` height=20
"#,
    );
    assert_eq!(host.runner.slot("observed"), Some(&Value::Number(100.)));
    press(&mut host, "grow");
    assert_eq!(host.runner.slot("observed"), Some(&Value::Number(110.)));
    assert!(!host.runner.resize_due(0).is_empty());
    assert!(!host.layout_owed);
    assert_eq!(host.runner.timer_due_ms(), None);

    let before = host.finish_passes;
    for (offset, width) in [(35.1, 120.), (35.2, 130.)] {
        report(&mut host, offset);
        assert_eq!(host.runner.slot("observed"), Some(&Value::Number(width)));
    }
    assert_eq!(host.finish_passes, before + 2);
    assert!(host.runner.resize_due(0).is_empty());
    let reply = report(&mut host, 35.3);
    assert!(reply.starts_with("{\"ops\":[]"), "{reply}");
    assert_eq!(
        host.finish_passes,
        before + 2,
        "settled reports reuse finish"
    );
}

#[test]
fn a_reused_finish_samples_the_deadline_after_writing() {
    use std::sync::{
        atomic::{AtomicUsize, Ordering},
        Arc,
    };
    struct Clock(AtomicUsize);
    impl exact_runner::process::WorkClock for Clock {
        fn advance(&self, _: f64) {}
        fn due(&self) -> Option<f64> {
            (self.0.fetch_add(1, Ordering::Relaxed) != 0).then_some(100.)
        }
        fn owed(&self) -> bool {
            false
        }
    }
    let mut host = boot();
    host.runner
        .set_work_clock(Some(Arc::new(Clock(AtomicUsize::new(0)))));
    let before = host.finish_passes;
    let reply = host.finish_collection();
    assert!(reply.contains("\"timer_due_ms\":100"), "{reply}");
    assert_eq!(host.finish_passes, before);
}
