//! LLP 1070.000 §15: an authored smooth `scrollIntoView` owns a list that
//! follows its end until it settles. While it runs, `scrollFollowEnd` makes
//! no move of its own, whatever the app sets it to; once it settles, the
//! list follows its end only if it rests there, as after a reader's scroll.
use exact_kernel::Kernel;
use exact_plan::Value;
use exact_runner::{
    CollectionFeedback, CollectionFill, CollectionSnapshot, DataError, DataSource, RowMeasurement,
    Runner,
};

/// Eleven rows 100 tall in a port 100 tall: the end is at 1000 plus the
/// padding after the rows. `following` is the app's, as a chat's is: off
/// once the reader leaves the end, on again with the jump back to it.
const APP: &str = r#"component App
  state ids = [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10]
  state following = true
  action away
    following = false
  action toEnd(n: number)
    following = true
    scrollIntoView("feed", n, block="end", behavior="smooth")
  action add(n: number)
    ids = ids.concat([n])
  view
    list id="feed" virtualized=true scrollFollowEnd=following height=100 width=400 estimated-item-height=100 PADDING
      for i in ids key=i
        text `${i}` height=100
"#;

struct NoData;
impl DataSource for NoData {
    fn query(&mut self, source: &str, _: &[Value]) -> Result<Value, DataError> {
        Err(DataError::UnknownSource(source.into()))
    }
}
/// The app with `padding` on its list, every row measured, the port at its
/// end (`1000 + end`).
fn boot(padding: &str, end: f64) -> (Runner<NoData>, u32) {
    let mut r = Runner::boot(
        contract::compile(&APP.replace("PADDING", padding)).unwrap(),
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let feed = r.collections()[0].view;
    for _ in 0..4 {
        report(&mut r, feed, 1000.0 + end, end, CollectionFill::default());
    }
    (r, feed)
}
fn snapshot(r: &Runner<NoData>, view: u32) -> CollectionSnapshot {
    r.collections()
        .into_iter()
        .find(|c| c.view == view)
        .unwrap()
}
/// One report from the port at `offset`, every mounted row 100 tall, `end`
/// pixels of padding after the rows.
fn report(r: &mut Runner<NoData>, view: u32, offset: f64, end: f64, fill: CollectionFill) {
    let c = snapshot(r, view);
    r.collection_feedback_filled(
        CollectionFeedback {
            view,
            revision: c.revision,
            scroll_sequence: c.scroll_sequence + 1,
            offset,
            port_main: 100.0,
            port_cross: 400.0,
            cross: 400.0,
            padding: [0.0, end],
            measurements: c
                .rows
                .iter()
                .map(|row| RowMeasurement {
                    view: row.view,
                    epoch: row.epoch,
                    size: 100.0,
                })
                .collect(),
            focus_view: None,
            interaction_view: None,
        },
        fill,
    )
    .unwrap();
}
/// A report while the host animates a smooth correction: it reports where
/// the animation is headed (LLP 1070.000 §11).
fn animating() -> CollectionFill {
    CollectionFill {
        animating: true,
        ..Default::default()
    }
}
/// The reader's drag, as iOS reports it: travel, with a jump ending it.
fn dragging() -> CollectionFill {
    CollectionFill {
        velocity: -2000.0,
        ..Default::default()
    }
}
fn status(r: &Runner<NoData>) -> String {
    exact_runner::agent::state(r)
}
/// The host at rest: it takes each correction, then reports where it put
/// the port, until the list asks for nothing more. Where it ends.
fn rest(r: &mut Runner<NoData>, view: u32, mut offset: f64, end: f64) -> f64 {
    for _ in 0..8 {
        if let Some(correction) = snapshot(r, view).correction {
            offset = correction.offset;
        }
        report(r, view, offset, end, CollectionFill::default());
        let after = snapshot(r, view);
        if after.correction.is_none() && !after.pending {
            break;
        }
    }
    offset
}

/// The reporter's handoff (expo/exact#370), with nothing in the app to
/// hold the follow off: the reader drags away, the app turns following on
/// and asks for the last row smoothly. Rows arriving while it animates move
/// nothing of the follow's, the animation lands at the end, and from there
/// the list follows the end again.
#[test]
fn a_smooth_request_owns_a_followed_list_until_it_lands_at_the_end() {
    let (mut r, feed) = boot("", 0.0);
    report(&mut r, feed, 600.0, 0.0, dragging());
    report(&mut r, feed, 400.0, 0.0, dragging());
    r.act("away", vec![]).unwrap();
    report(&mut r, feed, 400.0, 0.0, CollectionFill::default());
    assert!(
        snapshot(&r, feed).correction.is_none(),
        "away, nothing followed"
    );

    r.act("toEnd", vec![Value::Number(10.0)]).unwrap();
    let c = snapshot(&r, feed).correction.expect("the request's move");
    assert!(c.smooth && c.offset == 1000.0, "{c:?}");
    // Mid-flight the host reports the destination, and rows arrive: the
    // follow makes no move of its own, the request heads on for row 10.
    report(&mut r, feed, 1000.0, 0.0, animating());
    r.act("add", vec![Value::Number(11.0)]).unwrap();
    let c = snapshot(&r, feed).correction;
    assert!(c.is_none_or(|c| c.offset == 1000.0), "{c:?}");
    report(&mut r, feed, 1000.0, 0.0, animating());
    assert!(
        status(&r).contains("\"status\":\"pending\""),
        "{}",
        status(&r)
    );
    // It lands where row 10 ends, short of the new end: not following.
    assert_eq!(rest(&mut r, feed, 1000.0, 0.0), 1000.0);
    assert!(status(&r).contains("\"status\":\"done\""), "{}", status(&r));
    r.act("add", vec![Value::Number(12.0)]).unwrap();
    assert!(snapshot(&r, feed)
        .correction
        .is_none_or(|c| c.from.is_some()));

    // A request that lands at the end leaves the list following it.
    r.act("toEnd", vec![Value::Number(12.0)]).unwrap();
    report(&mut r, feed, 1200.0, 0.0, animating());
    assert_eq!(rest(&mut r, feed, 1200.0, 0.0), 1200.0);
    assert!(status(&r).contains("\"status\":\"done\""), "{}", status(&r));
    r.act("add", vec![Value::Number(13.0)]).unwrap();
    let c = snapshot(&r, feed).correction.expect("the end is followed");
    assert_eq!(c.offset, 1300.0, "{c:?}");
}

/// Padding after the rows is past a row's `block="end"`, as on the web: a
/// request for the last row rests that far short of the end and does not
/// follow it. `scroll-padding-bottom` as tall as the padding aligns the
/// row's end above it, which is the end, and the list follows from there.
#[test]
fn the_last_rows_end_is_the_lists_end_only_through_scroll_padding() {
    for (padding, follows) in [
        ("padding-bottom=40", false),
        ("padding-bottom=40 scroll-padding-bottom=40", true),
    ] {
        let (mut r, feed) = boot(padding, 40.0);
        report(&mut r, feed, 500.0, 40.0, dragging());
        report(&mut r, feed, 500.0, 40.0, CollectionFill::default());
        r.act("toEnd", vec![Value::Number(10.0)]).unwrap();
        let headed = snapshot(&r, feed).correction.expect("its move").offset;
        report(&mut r, feed, headed, 40.0, animating());
        let at = rest(&mut r, feed, headed, 40.0);
        assert_eq!(at, if follows { 1040.0 } else { 1000.0 }, "{padding}");
        r.act("add", vec![Value::Number(11.0)]).unwrap();
        let c = snapshot(&r, feed).correction;
        if follows {
            assert_eq!(c.map(|c| c.offset), Some(1140.0), "{padding}: {c:?}");
        } else {
            assert!(c.is_none_or(|c| c.from.is_some()), "{padding}: {c:?}");
        }
    }
}
