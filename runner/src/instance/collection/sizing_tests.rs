//! A row list's `gap` and a list's estimate, read again as they change (LLP
//! 1070, as amended for #226): items start where CSS's flex row lays them.
use super::*;
use crate::Runner;

const STRIP: &str = r#"component App
  state w = 100
  state e = 100
  state g = 14
  action wider
    w = 150
    e = 150
  action closer
    g = 4
  action bad
    e = 0 - 1
  view
    list virtualized=true display="flex" width=320 height=(w * 1.3) overflow-x="scroll" overflow-y="hidden" estimated-item-width=e gap=g
      for x in range(1000) key=x
        view width=w height=100 flex-shrink=0
"#;

fn boot(source: &str) -> Runner<NoData> {
    let plan = contract::compile(source).unwrap();
    Runner::boot(
        plan,
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap()
}

/// A report of the port at `offset`, measuring each mounted row at `size`
/// (none for `None`).
fn report(r: &Runner<NoData>, offset: f64, size: Option<f64>) -> CollectionFeedback {
    let s = r.collections()[0].clone();
    CollectionFeedback {
        view: s.view,
        revision: s.revision,
        scroll_sequence: s.scroll_sequence + 1,
        offset,
        port_cross: 130.0,
        port_main: 320.0,
        cross: 130.0,
        padding: [0.0; 2],
        measurements: size.map_or_else(Vec::new, |size| {
            (s.rows.iter())
                .map(|row| RowMeasurement {
                    view: row.view,
                    epoch: row.epoch,
                    size,
                })
                .collect()
        }),
        focus_view: None,
        interaction_view: None,
    }
}

fn starts(r: &Runner<NoData>) -> Vec<(usize, f64, f64)> {
    (r.collections()[0].rows.iter())
        .map(|row| (row.index, row.start, row.size))
        .collect()
}

#[test]
fn a_row_lists_items_start_where_css_lays_them_with_its_gap() {
    let mut r = boot(STRIP);
    r.collection_feedback(report(&r, 0.0, None)).unwrap();
    r.collection_feedback(report(&r, 0.0, Some(100.0))).unwrap();
    let s = r.collections()[0].clone();
    // Each card's start is CSS's: the cards before it and a gap after each.
    for (index, start, size) in starts(&r) {
        assert_eq!((start, size), (index as f64 * 114.0, 100.0), "card {index}");
    }
    // The strip ends at its last card's end: no gap after it.
    assert_eq!(s.total_extent, 1000.0 * 114.0 - 14.0);
    // A jump to a card aligns its box, not its gap.
    r.scroll_into_view(IntoView {
        list: String::new(),
        key: Value::Number(500.0),
        block: Align::Start,
        inline: Align::End,
        row: None,
        view: Some(s.view),
        smooth: false,
        offset: None,
    })
    .unwrap();
    assert_eq!(
        r.collections()[0].correction.unwrap().offset,
        500.0 * 114.0 + 100.0 - 320.0
    );
}

#[test]
fn a_new_gap_moves_every_card_and_keeps_their_measurements() {
    let mut r = boot(STRIP);
    r.collection_feedback(report(&r, 0.0, None)).unwrap();
    r.collection_feedback(report(&r, 0.0, Some(100.0))).unwrap();
    r.act("closer", vec![]).unwrap();
    for (index, start, size) in starts(&r) {
        assert_eq!((start, size), (index as f64 * 104.0, 100.0), "card {index}");
    }
    assert_eq!(r.collections()[0].total_extent, 1000.0 * 104.0 - 4.0);
}

#[test]
fn a_new_estimate_is_every_unmounted_cards_and_the_list_keeps_what_shows() {
    let mut r = boot(STRIP);
    r.collection_feedback(report(&r, 0.0, None)).unwrap();
    r.collection_feedback(report(&r, 0.0, Some(100.0))).unwrap();
    // Scrolled to the tenth card, measured.
    r.collection_feedback(report(&r, 1140.0, None)).unwrap();
    r.collection_feedback(report(&r, 1140.0, Some(100.0)))
        .unwrap();
    let mounted: Vec<usize> = starts(&r).iter().map(|s| s.0).collect();
    r.act("wider", vec![]).unwrap();
    let s = r.collections()[0].clone();
    // The cards it did not mount take the new estimate, the measured ones
    // before them included: past the mounted ones, every card is 150 and its gap.
    let extent = |i: usize| if mounted.contains(&i) { 114.0 } else { 164.0 };
    assert_eq!(s.total_extent, (0..1000).map(extent).sum::<f64>() - 14.0);
    // What showed stays where it was: the tenth card at the port's start.
    let tenth: f64 = (0..10).map(extent).sum();
    assert_eq!(s.correction.map_or(1140.0, |c| c.offset), tenth);
    // The mounted cards keep their sizes until the host measures them again.
    assert!(s
        .rows
        .iter()
        .all(|row| row.size == 100.0 || !mounted.contains(&row.index)));
    // An estimate that is no positive number changes nothing.
    let revision = r.collections()[0].revision;
    r.act("bad", vec![]).unwrap();
    assert_eq!(r.collections()[0].total_extent, s.total_extent);
    assert!(r.collections()[0].revision >= revision);
}

/// CSS Flexbox 1 §4: an item that is not displayed is no flex item, and no
/// gap is laid out around it. A hidden card's wrapper goes with it, and its
/// extent is nothing.
#[test]
fn a_card_that_is_not_displayed_has_no_gap_around_it() {
    let mut r = boot(
        r#"component App
  state hide = false
  action toggle
    hide = not hide
  view
    list virtualized=true display="flex" width=320 height=130 overflow-x="scroll" overflow-y="hidden" estimated-item-width=100 gap=14
      for x in range(1000) key=x
        view width=100 height=100 flex-shrink=0 display=(hide && (x == 1 || x == 3) ? "none" : "flex")
"#,
    );
    let wrapper_shown = |r: &Runner<NoData>, index: usize| {
        let row = r.collections()[0]
            .rows
            .iter()
            .find(|row| row.index == index)
            .unwrap()
            .clone();
        r.kernel().node(row.view).unwrap().style.display != exact_kernel::Display::None
    };
    r.collection_feedback(report(&r, 0.0, None)).unwrap();
    r.collection_feedback(report(&r, 0.0, Some(100.0))).unwrap();
    r.act("toggle", vec![]).unwrap();
    assert!(!wrapper_shown(&r, 1) && !wrapper_shown(&r, 3) && wrapper_shown(&r, 2));
    // The host measures the hidden cards' wrappers at nothing.
    let mut f = report(&r, 0.0, Some(100.0));
    for (m, row) in f.measurements.iter_mut().zip(&r.collections()[0].rows) {
        if row.index == 1 || row.index == 3 {
            m.size = 0.0;
        }
    }
    r.collection_feedback(f).unwrap();
    // Cards 0, 2 and 4 lie where CSS's flex row lays the shown ones: 0, 114, 228.
    let at = |r: &Runner<NoData>, i: usize| starts(r).into_iter().find(|s| s.0 == i);
    assert_eq!(
        [0, 2, 4].map(|i| at(&r, i).map(|s| s.1)),
        [Some(0.0), Some(114.0), Some(228.0)]
    );
    // A hidden card, still mounted or let go as rows of no size are, is nothing.
    for i in [1, 3] {
        assert!(at(&r, i).is_none_or(|s| s.2 == 0.0), "card {i}");
    }
    // Shown again, the cards take their gaps back.
    r.act("toggle", vec![]).unwrap();
    r.collection_feedback(report(&r, 0.0, Some(100.0))).unwrap();
    assert!(wrapper_shown(&r, 1) && wrapper_shown(&r, 3));
    assert_eq!(at(&r, 4).map(|s| s.1), Some(4.0 * 114.0));
}

/// A new gap moves the shown cards, and a hidden card, measured at nothing,
/// takes none.
#[test]
fn a_new_gap_leaves_a_hidden_card_at_nothing() {
    let mut r = boot(
        r#"component App
  state g = 14
  action closer
    g = 4
  view
    list virtualized=true display="flex" width=320 height=130 overflow-x="scroll" overflow-y="hidden" estimated-item-width=100 gap=g
      for x in range(1000) key=x
        view width=100 height=100 flex-shrink=0 display=(x == 1 || x == 3 ? "none" : "flex")
"#,
    );
    r.collection_feedback(report(&r, 0.0, None)).unwrap();
    let measure = |r: &Runner<NoData>| {
        let mut f = report(r, 0.0, Some(100.0));
        for (m, row) in f.measurements.iter_mut().zip(&r.collections()[0].rows) {
            if row.index == 1 || row.index == 3 {
                m.size = 0.0;
            }
        }
        f
    };
    r.collection_feedback(measure(&r)).unwrap();
    let start =
        |r: &Runner<NoData>, i: usize| starts(r).into_iter().find(|s| s.0 == i).map(|s| s.1);
    assert_eq!(start(&r, 4), Some(2.0 * 114.0));
    r.act("closer", vec![]).unwrap();
    // Cards 0 and 2 and their gaps before card 4, and nothing for 1 and 3.
    assert_eq!(start(&r, 4), Some(2.0 * 104.0));
    assert_eq!(start(&r, 6), Some(4.0 * 104.0));
}

#[test]
fn a_vertical_list_lays_out_no_gap() {
    let r = boot(
        r#"component App
  view
    list virtualized=true height=300 estimated-item-height=50 gap=0
      for x in range(100) key=x
        view height=50
"#,
    );
    assert_eq!(r.collections()[0].total_extent, 5000.0);
}
