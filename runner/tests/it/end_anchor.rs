//! LLP 1168.000 D1.2: a list that opens at its end (`scroll-start="end"`)
//! keeps its last visible row, by key and by its distance from the port's
//! bottom, when an update puts rows before the window: a page that lands
//! above a reader at the top of a transcript's loaded window leaves them on
//! the message they were reading. Rows of 32 px in a 320 px port.

use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{CollectionFeedback, CollectionFill, DataError, DataSource, Runner};

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, name: &str, _: &[Value]) -> Result<Value, DataError> {
        panic!("queried {name}")
    }
}

/// Plain messages, keyed by their number: a page puts 20 older ones first.
const PLAIN: &str = "msgs";
/// A leading row ("Start of the chat") that stays first.
const LEADING: &str = "[\"start\"].concat(msgs)";
/// A leading run of two rows: "Start of the chat" and today's separator,
/// which a page of today's older messages lands after.
const LEADING_RUN: &str = "[\"start\", \"today\"].concat(msgs)";
/// Day separators at the top of each day's run: messages before -10 are the
/// day before, so a page moves today's separator down to the top of its
/// grown run and puts the day before's above it.
const DAYS: &str = "[\"day-before\"].slice(older > 10 ? 0 : 1).concat(nums.filter(k => k < -10).map(k => `${k}`)).concat([\"today\"]).concat(nums.filter(k => k >= -10).map(k => `${k}`))";

fn boot(rows: &str, margin: &str) -> Runner<NoData> {
    let source = format!(
        "component App
  state n = 200
  state older = 0
  state extra = -1
  state starts = 0
  state gen = 0
  state meta = 0
  state tick = 0
  state refuse = false
  action onStart
    starts = starts + range(refuse ? 65537 : 1).length
  action refuseStarts
    refuse = true
  action touchMeta
    meta = meta + 1
  action tickBody
    tick = tick + 1
  action prepend
    n = n + 20
    older = older + 20
  action prepend11
    n = n + 11
    older = older + 11
  action answer
    gen = gen + 1
  action append
    n = n + 1
  action insert(at: number)
    extra = at
  derive nums = range(n + gen * 0).map(x => x - older)
  derive msgs = nums.map(k => `${{k}}`)
  derive base = {rows}
  derive shown = extra < 0 ? base : base.slice(0, extra).concat([\"extra\"]).concat(base.slice(extra))
  view
    list virtualized=true height=320 estimated-item-height=32 scroll-start=\"end\" reachstart=onStart{margin}
      for row in shown.slice(meta * 0).map(k => [k]) key=(row.join(\"\") + `${{meta * 0}}`)
        text `${{row.join(\"\")}}${{tick > 0 ? \"\" : \"\"}}` height=32
"
    );
    Runner::boot(
        contract::compile(&source).unwrap(),
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

/// A report at `offset` moving at `velocity` px/s, measuring every mounted
/// row at 32 px.
fn report(r: &mut Runner<NoData>, offset: f64, velocity: f64) {
    report_padded(r, offset, velocity, 0.);
}

/// [`report`] with `leading` px of padding before the first row.
fn report_padded(r: &mut Runner<NoData>, offset: f64, velocity: f64, leading: f64) {
    let error = feedback(r, offset, velocity, leading);
    assert!(error.is_none(), "{error:?}");
}

/// A report, and the error an action it ran refused with.
fn feedback(r: &mut Runner<NoData>, offset: f64, velocity: f64, leading: f64) -> Option<String> {
    let c = &r.collections()[0];
    let feedback = CollectionFeedback {
        view: c.view,
        revision: c.revision,
        scroll_sequence: c.scroll_sequence + 1,
        offset,
        port_cross: 640.,
        port_main: 320.,
        cross: 640.,
        padding: [leading, 0.],
        measurements: c
            .rows
            .iter()
            .map(|row| exact_runner::RowMeasurement {
                view: row.view,
                epoch: row.epoch,
                size: 32.,
            })
            .collect(),
        focus_view: None,
        interaction_view: None,
    };
    let fill = CollectionFill {
        velocity,
        ..Default::default()
    };
    let result = r.collection_feedback_filled(feedback, fill).unwrap();
    result.error.map(|e| format!("{e:?}"))
}

/// Opened at its end, then scrolled by the reader to `offset` and at rest.
fn read_at(rows: &str, margin: &str, offset: f64) -> Runner<NoData> {
    let mut r = boot(rows, margin);
    let end = r.collections()[0]
        .correction
        .expect("opens at its end")
        .offset;
    report(&mut r, end, 0.);
    report(&mut r, end, 0.);
    report(&mut r, offset + 1000., -4000.);
    report(&mut r, offset, -4000.);
    report(&mut r, offset, 0.);
    r
}

fn correction(r: &Runner<NoData>) -> Option<f64> {
    r.collections()[0].correction.map(|c| c.offset)
}

fn starts(r: &Runner<NoData>) -> f64 {
    r.slot("starts").unwrap().as_number().unwrap()
}

/// At a zero offset, where a list that does not open at its end anchors
/// nothing, a page moves the port down by its rows: a leading row that stays
/// first, or a separator that moves with the page, takes the reader nowhere.
#[test]
fn a_page_above_a_reader_at_the_top_keeps_the_last_visible_row() {
    for (rows, moved) in [(PLAIN, 640.), (LEADING, 640.), (DAYS, 672.)] {
        let mut r = read_at(rows, "", 0.);
        assert_eq!(correction(&r), None, "{rows}: at rest at the top");
        r.act("prepend", vec![]).unwrap();
        assert_eq!(correction(&r), Some(moved), "{rows}");
    }
}

/// Scrolled into the list, a page above keeps the last visible row as the
/// first visible row's anchor would.
#[test]
fn a_page_above_a_reader_inside_the_list_moves_the_port_by_the_page() {
    let mut r = read_at(PLAIN, "", 645.);
    r.act("prepend", vec![]).unwrap();
    assert_eq!(correction(&r), Some(1285.));
}

/// An edit inside the window anchors as before: the first visible row holds
/// (nothing at a zero offset), and the rows below it move down. At the top,
/// the `reachstart` the reader's arrival fired is answered first (a row
/// appended at the end): while it is unanswered, rows put there are its page.
#[test]
fn a_row_put_inside_the_window_anchors_as_before() {
    for (rows, offset, at) in [(PLAIN, 0., 5.), (PLAIN, 640., 25.), (LEADING, 0., 3.)] {
        let mut r = read_at(rows, "", offset);
        r.act("append", vec![]).unwrap();
        r.act("insert", vec![Value::Number(at)]).unwrap();
        assert_eq!(correction(&r), None, "{rows} {offset}: the port stays");
    }
}

/// LLP 1168.000 D1.2: while a `reachstart` is unanswered, rows that land
/// after a leading run at the top (a leading row, then a separator that
/// stays) are its page, and the last visible row holds. An answer that
/// changes the rows ends that, so the same rows then show at the top; so
/// does the start re-arming, here while the reader is away and back on a
/// build-only report, which runs no action. An insert inside the window
/// while it is unanswered is taken for the page.
#[test]
fn a_page_after_a_leading_run_is_seen_while_reachstart_is_unanswered() {
    let mut r = read_at(LEADING_RUN, "", 0.);
    assert_eq!(starts(&r), 1.);
    r.act("prepend", vec![]).unwrap();
    assert_eq!(correction(&r), Some(640.), "the page");
    let mut r = read_at(LEADING_RUN, "", 0.);
    r.act("append", vec![]).unwrap();
    r.act("prepend", vec![]).unwrap();
    assert_eq!(correction(&r), None, "answered: rows show at the top");
    let mut r = read_at(LEADING_RUN, "", 0.);
    report(&mut r, 1000., 0.);
    report(&mut r, 1000., 0.);
    build_only(&mut r, 0.);
    assert_eq!(starts(&r), 1., "no action on a build-only report");
    r.act("prepend", vec![]).unwrap();
    assert_eq!(correction(&r), None, "re-armed: rows show at the top");
    let mut r = read_at(PLAIN, "", 0.);
    r.act("insert", vec![Value::Number(5.)]).unwrap();
    assert_eq!(correction(&r), Some(32.), "taken for the page");
}

/// Pinned, as LLP 1010's D1.2 note records: an empty or identical answer
/// changes no row, so `reachstart` stays unanswered until the start
/// re-arms, and an insert inside the window at the top is still taken for
/// its page.
#[test]
fn an_empty_answer_leaves_reachstart_unanswered() {
    let mut r = read_at(PLAIN, "", 0.);
    r.act("answer", vec![]).unwrap();
    r.act("insert", vec![Value::Number(5.)]).unwrap();
    assert_eq!(correction(&r), Some(32.));
}

/// A `reachstart` that is refused, or waits on a covered route, asked
/// nothing: an insert inside the window at the top right after it anchors
/// as before.
#[test]
fn a_refused_start_leaves_nothing_unanswered() {
    let mut r = boot(PLAIN, "");
    r.act("refuseStarts", vec![]).unwrap();
    let end = r.collections()[0]
        .correction
        .expect("opens at its end")
        .offset;
    report(&mut r, end, 0.);
    report(&mut r, end, 0.);
    report(&mut r, 1000., -4000.);
    assert!(feedback(&mut r, 0., -4000., 0.).is_some(), "refused");
    assert_eq!(starts(&r), 0.);
    r.act("insert", vec![Value::Number(5.)]).unwrap();
    assert_eq!(correction(&r), None);
}

/// What changes no row's key or value answers nothing, in either
/// evaluation mode: an answer's other fields (the subject and the key read
/// `meta`, and each row is a fresh list equal to the last), or an input only
/// a row's body reads (`tick`). `reachstart` stays unanswered, and an insert
/// inside the window is still its page.
#[test]
fn an_answer_with_the_same_rows_leaves_reachstart_unanswered() {
    for full in [false, true] {
        for action in ["touchMeta", "tickBody"] {
            let mut r = read_at(PLAIN, "", 0.);
            r.set_full_evaluation(full);
            r.act(action, vec![]).unwrap();
            r.act("insert", vec![Value::Number(5.)]).unwrap();
            assert_eq!(correction(&r), Some(32.), "{action}, full {full}");
        }
    }
}

/// A build-only report at rest at `offset`, measuring every mounted row at
/// 32 px: it runs no edge action (LLP 1072 §5).
fn build_only(r: &mut Runner<NoData>, offset: f64) {
    let c = &r.collections()[0];
    let feedback = CollectionFeedback {
        view: c.view,
        revision: c.revision,
        scroll_sequence: c.scroll_sequence + 1,
        offset,
        port_cross: 640.,
        port_main: 320.,
        cross: 640.,
        padding: [0.0; 2],
        measurements: (c.rows.iter())
            .map(|row| exact_runner::RowMeasurement {
                view: row.view,
                epoch: row.epoch,
                size: 32.,
            })
            .collect(),
        focus_view: None,
        interaction_view: None,
    };
    let fill = CollectionFill {
        create_only: true,
        ..Default::default()
    };
    let result = r.collection_feedback_filled(feedback, fill).unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
}

/// With D1.1: the page's rows, a port above the reader, are outside a
/// one-port band, so `reachstart` waits for the reader to scroll; a band of
/// five ports reaches them and asks for the next page once they are measured.
#[test]
fn the_start_waits_for_the_reader_unless_the_band_reaches_the_page() {
    for (margin, next) in [("", 1.), (" reach-margin=\"500%\"", 2.)] {
        let mut r = read_at(PLAIN, margin, 0.);
        assert_eq!(starts(&r), 1., "{margin}: fired at the top");
        r.act("prepend", vec![]).unwrap();
        report(&mut r, 640., 0.);
        report(&mut r, 640., 0.);
        assert_eq!(starts(&r), next, "{margin}");
        report(&mut r, 0., -4000.);
        report(&mut r, 0., 0.);
        assert_eq!(starts(&r), next.max(2.), "{margin}: the reader scrolled up");
    }
}

/// Pinned, as LLP 1010's D1.2 note records: by the order of keys alone, a
/// row put right after a leading row at the top is a page landing there
/// (D1.1's rule), answered or not, so it moves the port by its height.
#[test]
fn what_order_alone_cannot_tell_apart_is_pinned() {
    let mut r = read_at(LEADING, "", 0.);
    r.act("append", vec![]).unwrap();
    r.act("insert", vec![Value::Number(1.)]).unwrap();
    assert_eq!(correction(&r), Some(32.), "taken for a page");
}

/// A report at rest at `offset`, with `leading` px of padding before the
/// first row, measuring each mounted row at `size(its index)`.
fn sized(r: &mut Runner<NoData>, offset: f64, leading: f64, size: impl Fn(usize) -> f64) {
    let c = &r.collections()[0];
    let feedback = CollectionFeedback {
        view: c.view,
        revision: c.revision,
        scroll_sequence: c.scroll_sequence + 1,
        offset,
        port_cross: 640.,
        port_main: 320.,
        cross: 640.,
        padding: [leading, 0.],
        measurements: (c.rows.iter())
            .map(|row| exact_runner::RowMeasurement {
                view: row.view,
                epoch: row.epoch,
                size: size(row.index),
            })
            .collect(),
        focus_view: None,
        interaction_view: None,
    };
    let result = r
        .collection_feedback_filled(feedback, CollectionFill::default())
        .unwrap();
    assert!(result.error.is_none(), "{:?}", result.error);
}

/// LLP 1168.000 §2.4's boundary with D1.2 holding the reader: a page of
/// 330 px (a first row of 32 and ten of 29.8) lands above a reader at the
/// top, the last visible row keeps the port 330 px down once the page is
/// measured, and the new first row ends 22 px inside the one-port band, so
/// `reachstart` fires again; a page of 352 px leaves it outside.
#[test]
fn a_held_reader_330_px_below_a_page_is_offered_the_next() {
    for (small, fires) in [(29.8, 2.), (32., 1.)] {
        let mut r = read_at(PLAIN, "", 0.);
        r.act("prepend11", vec![]).unwrap();
        assert_eq!(correction(&r), Some(352.), "on the estimate");
        let size = |i: usize| match i {
            0 => 32.,
            1..=10 => small,
            _ => 32.,
        };
        let page = 32. + 10. * small;
        sized(&mut r, 352., 0., size);
        let held = correction(&r).unwrap_or(352.);
        assert!((held - page).abs() < 0.01, "{small}: held at {held}");
        sized(&mut r, held, 0., size);
        sized(&mut r, held, 0., size);
        assert_eq!(starts(&r), fires, "{small}");
    }
}

/// A reader in the padding above the first row (32 px of it, the port at
/// -32) is kept there too: a row put first moves the port to 0, from -32, so
/// the last visible row stays off the port's bottom (`at_target` reads a
/// bottom anchor's port where it is, not at the rows' start).
#[test]
fn a_reader_in_the_leading_padding_keeps_the_last_visible_row() {
    let mut r = boot(PLAIN, "");
    for _ in 0..3 {
        let end = correction(&r).unwrap_or(200. * 32. + 32. - 320.);
        report_padded(&mut r, end, 0., 32.);
    }
    report_padded(&mut r, 968., -4000., 32.);
    report_padded(&mut r, -32., -4000., 32.);
    report_padded(&mut r, -32., 0., 32.);
    assert_eq!(correction(&r), None, "at rest in the padding");
    r.act("insert", vec![Value::Number(0.)]).unwrap();
    let c = r.collections()[0].correction.expect("a correction");
    assert_eq!((c.offset, c.from), (0., Some(-32.)));
}

/// A page that lands above a reader in the leading padding leaves the port's
/// top inside its last row, still at the estimate. Its mounted rows measured
/// at 26 px, the first old row, which the reader sees, keeps its place 16 px
/// below the port's top, where an anchor on that estimated row would move it
/// by the estimate's error.
#[test]
fn a_page_measured_above_a_reader_in_the_padding_keeps_the_rows_they_read() {
    let mut r = boot(PLAIN, "");
    for _ in 0..3 {
        let end = correction(&r).unwrap_or(200. * 32. + 16. - 320.);
        report_padded(&mut r, end, 0., 16.);
    }
    report_padded(&mut r, 984., -4000., 16.);
    report_padded(&mut r, -16., -4000., 16.);
    report_padded(&mut r, -16., 0., 16.);
    r.act("prepend", vec![]).unwrap();
    assert_eq!(correction(&r), Some(20. * 32. - 16.), "on the estimate");
    let size = |i: usize| if i < 20 { 26. } else { 32. };
    let measured = r.collections()[0]
        .rows
        .iter()
        .filter(|row| row.index < 20)
        .count() as f64;
    sized(&mut r, 624., 16., size);
    assert_eq!(
        correction(&r),
        Some(20. * 32. - 6. * measured - 16.),
        "the page measured"
    );
}

/// A row measured before, with the port's top inside it, keeps the place
/// when a report finds it taller, as a streaming reply's row grows at its
/// end: the lines its reader reads stay, and the rows below it move down.
#[test]
fn a_row_measured_again_taller_keeps_its_readers_place() {
    let mut r = read_at(PLAIN, "", 100. * 32. + 10.);
    sized(&mut r, 100. * 32. + 10., 0., |i| {
        if i == 100 {
            52.
        } else {
            32.
        }
    });
    assert!(
        correction(&r).is_none_or(|offset| offset == 100. * 32. + 10.),
        "{:?}",
        correction(&r)
    );
}

/// A page's row measured for the first time at the estimate yields the place
/// too, its place having been the estimate's: with 48 px of leading padding
/// the port's top is in page row 18, measured at 32, and the page's other
/// mounted rows at 26; the first old row stays 48 px below the port's top.
#[test]
fn a_page_row_measured_at_the_estimate_yields_the_place_too() {
    let mut r = boot(PLAIN, "");
    for _ in 0..3 {
        let end = correction(&r).unwrap_or(200. * 32. + 48. - 320.);
        report_padded(&mut r, end, 0., 48.);
    }
    report_padded(&mut r, 1016., -4000., 48.);
    report_padded(&mut r, -48., -4000., 48.);
    report_padded(&mut r, -48., 0., 48.);
    r.act("prepend", vec![]).unwrap();
    assert_eq!(correction(&r), Some(20. * 32. - 48.), "on the estimate");
    let rows = r.collections()[0]
        .rows
        .iter()
        .map(|row| row.index)
        .collect::<Vec<_>>();
    assert!(
        rows.contains(&18) && rows.contains(&19),
        "the page's last rows are mounted: {rows:?}"
    );
    let size = |i: usize| match i {
        18 => 32.,
        i if i < 20 => 26.,
        _ => 32.,
    };
    let shrunk = rows.iter().filter(|&&i| i < 20 && i != 18).count() as f64;
    sized(&mut r, 592., 48., size);
    assert_eq!(
        correction(&r),
        Some(20. * 32. - 6. * shrunk - 48.),
        "the page measured"
    );
}
