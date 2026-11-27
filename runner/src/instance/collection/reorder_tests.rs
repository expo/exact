use crate::{CollectionFeedback, DataError, DataSource, ReorderProgress, RowMeasurement, Runner};
use exact_kernel::{Kernel, NodeKey};
use exact_plan::Value;

struct Rows(Vec<String>);
impl DataSource for Rows {
    fn query(&mut self, name: &str, args: &[Value]) -> Result<Value, DataError> {
        if name == "refuse" || (name == "refuse_second" && args[0].as_str() == Some("11")) {
            return Ok(Value::Bool(false));
        }
        if name == "trim" {
            self.0.drain(..10);
        }
        if name == "reset" {
            self.0 = (0..30).map(|i| i.to_string()).collect();
        }
        if name == "empty" {
            return Ok(Value::list(vec![]));
        }
        // `move2` applies two moves in one reply.
        for args in args
            .chunks(2)
            .filter(|_| ["move", "move2", "remove"].contains(&name))
        {
            let at = self
                .0
                .iter()
                .position(|k| Some(k.as_str()) == args[0].as_str())
                .unwrap();
            let item = self.0.remove(at);
            let before = match &args[1] {
                Value::Option(Some(v)) => v.as_str(),
                _ => None,
            };
            let to = self
                .0
                .iter()
                .position(|k| Some(k.as_str()) == before)
                .unwrap_or(self.0.len());
            if name != "remove" {
                self.0.insert(to, item);
            }
        }
        Ok(Value::list(self.0.iter().map(|k| Value::str(k)).collect()))
    }
}
const SOURCE: &str = r#"component App
  resource initial = rows() as shape string[]
  mutation changed as shape string[]
  derive rows = match changed { case some(value) => value, case none => initial }
  action receive(item: string, before: option<string>)
    send changed = move(item, before)
  view
    list id="arrange" testId="list" virtualized=true estimated-item-height=20 height=100 reorderdrop=receive
      for x in rows key=x
        column reorderFor="arrange" testId=`grip-${x}` height=20
          text x
"#;
fn report(r: &mut Runner<Rows>, offset: f64, pin: Option<NodeKey>) {
    for c in r.collections() {
        let source = c.view == r.collections()[0].view;
        r.collection_feedback(CollectionFeedback {
            view: c.view,
            revision: c.revision,
            scroll_sequence: c.scroll_sequence + 1,
            offset: if source { offset } else { 0. },
            port_cross: 320.,
            port_main: 100.,
            cross: 320.,
            padding: [0.; 2],
            focus_view: None,
            interaction_view: pin
                .filter(|_| source)
                .map(|k| r.kernel().node_by_key(k).unwrap().id),
            measurements: c
                .rows
                .iter()
                .map(|row| RowMeasurement {
                    view: row.view,
                    epoch: row.epoch,
                    size: 20.,
                })
                .collect(),
        })
        .unwrap();
    }
}
fn fixture(offset: f64) -> (Runner<Rows>, NodeKey) {
    fixture_with(SOURCE, offset, 10)
}
fn fixture_with(source: &str, offset: f64, item: usize) -> (Runner<Rows>, NodeKey) {
    let mut r = Runner::boot(
        contract::compile(source).unwrap(),
        Rows((0..30).map(|i| i.to_string()).collect()),
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    for _ in 0..3 {
        report(&mut r, 0., None);
    }
    report(&mut r, offset, None);
    let grip = r.kernel().find_by_test_id(&format!("grip-{item}"))[0];
    for _ in 0..2 {
        report(&mut r, offset, Some(grip));
    }
    (r, grip)
}

#[test]
fn dropped_top_row_anchors_its_visible_successor() {
    let (mut r, grip) = fixture(200.);
    let b = r.reorder_binding(grip).unwrap();
    let g = r.reorder_geometry(b.list).unwrap();
    let token = r.begin_reorder(b, g.clone()).unwrap().unwrap().token;
    assert!(matches!(
        r.preview_reorder(token, g.clone(), 300.).unwrap(),
        ReorderProgress::Accepted { .. }
    ));
    assert!(r.drop_reorder(token, g).unwrap().is_some());
    assert_eq!(r.data_ref().0[14], "10");
    assert_eq!(r.collections()[0].correction.unwrap().offset, 180.);
    assert_eq!(r.reorder_geometry(b.list).unwrap().scroll_top, 180.);
}

#[test]
fn drop_anchors_preserve_partial_visibility_start_and_end_policy() {
    for (offset, item, y, expected) in [
        (207., 10, 300., 187.),
        (0., 0, 100., 0.),
        (500., 25, 600., 480.),
        (180., 10, 280., 180.),
    ] {
        for grouped in [false, true] {
            let source = SOURCE.replace(
                "virtualized=true",
                if grouped {
                    "virtualized=true scrollFollowEnd=true reorderGroup=\"cards\""
                } else {
                    "virtualized=true scrollFollowEnd=true"
                },
            );
            let (mut r, grip) = fixture_with(&source, offset, item);
            let b = r.reorder_binding(grip).unwrap();
            let g = r.reorder_geometry(b.list).unwrap();
            let start = if grouped {
                r.begin_group_reorder(b, g.clone(), false)
            } else {
                r.begin_reorder(b, g.clone())
            };
            let token = start.unwrap().unwrap().token;
            assert!(matches!(
                r.preview_reorder(token, g.clone(), y).unwrap(),
                ReorderProgress::Accepted { .. }
            ));
            r.drop_reorder(token, g).unwrap().unwrap();
            assert_eq!(
                r.reorder_geometry(b.list).unwrap().scroll_top,
                expected,
                "{offset}, grouped={grouped}"
            );
        }
    }
}

#[test]
fn cancellation_ordinary_moves_and_preview_measurements_keep_their_anchor() {
    for preview in [false, true] {
        let (mut r, grip) = fixture(200.);
        let b = r.reorder_binding(grip).unwrap();
        if preview {
            let g = r.reorder_geometry(b.list).unwrap();
            let token = r.begin_reorder(b, g.clone()).unwrap().unwrap().token;
            r.preview_reorder(token, g, 300.).unwrap();
            r.cancel_reorder(token).unwrap();
            r.finish_reorder(token).unwrap();
        }
        r.act(
            "receive",
            vec![Value::str("10"), Value::some(Value::str("15"))],
        )
        .unwrap();
        assert_eq!(r.reorder_geometry(b.list).unwrap().scroll_top, 280.);
    }
    let (mut r, grip) = fixture(200.);
    let b = r.reorder_binding(grip).unwrap();
    let g = r.reorder_geometry(b.list).unwrap();
    let token = r.begin_reorder(b, g.clone()).unwrap().unwrap().token;
    r.preview_reorder(token, g, 300.).unwrap();
    let c = r.collections().remove(0);
    let source = c.rows.iter().find(|row| row.index == 10).unwrap();
    r.collection_feedback(CollectionFeedback {
        view: c.view,
        revision: c.revision,
        scroll_sequence: c.scroll_sequence,
        offset: 200.,
        port_cross: 320.,
        port_main: 100.,
        cross: 320.,
        padding: [0.; 2],
        focus_view: None,
        interaction_view: Some(r.kernel().node_by_key(grip).unwrap().id),
        measurements: vec![RowMeasurement {
            view: source.view,
            epoch: source.epoch,
            size: 40.,
        }],
    })
    .unwrap();
    assert_eq!(r.reorder_geometry(b.list).unwrap().scroll_top, 200.);
}

#[test]
fn outgoing_group_drop_keeps_the_deleted_source_neighbour_fallback() {
    let mut source = SOURCE
        .replace(
            "  resource initial",
            "  resource empty = empty() as shape string[]\n  resource initial",
        )
        .replace("move(item, before)", "remove(item, before)")
        .replace(
            "virtualized=true",
            "virtualized=true reorderGroup=\"cards\"",
        );
    let (head, body) = source.split_once("  view\n").unwrap();
    source = format!(
        "{head}  view\n    column\n{}",
        body.lines().map(|l| format!("  {l}\n")).collect::<String>()
    );
    source.push_str(r#"      list id="target" testId="target" virtualized=true reorderGroup="cards" reorderdrop=receive height=100
        for x in empty key=x
          text x
"#);
    let (mut r, grip) = fixture_with(&source, 207., 10);
    let b = r.reorder_binding(grip).unwrap();
    let g = r.reorder_geometry(b.list).unwrap();
    let token = r.begin_group_reorder(b, g, true).unwrap().unwrap().token;
    let target = r.kernel().find_by_test_id("target")[0];
    let g = r.reorder_geometry(target).unwrap();
    assert!(matches!(
        r.preview_reorder_into(token, target, g.clone(), 0.)
            .unwrap(),
        ReorderProgress::Accepted { .. }
    ));
    r.drop_reorder(token, g).unwrap().unwrap();
    assert!(!r.data_ref().0.iter().any(|k| k == "10"));
    assert_eq!(r.reorder_geometry(b.list).unwrap().scroll_top, 207.);
    assert_eq!(
        r.reorder_frame(token).unwrap().ending,
        Some(crate::ReorderEnding::Gone)
    );
}

#[test]
fn deleting_rows_above_the_anchor_keeps_the_correction_relative() {
    let source = SOURCE.replace(
        "  view",
        "  action trim()\n    send changed = trim()\n  view",
    );
    let (mut r, _) = fixture_with(&source, 200., 10);
    r.act("trim", vec![]).unwrap();
    let correction = r.collections()[0].correction.unwrap();
    assert_eq!(correction.offset, 0.);
    assert_eq!(correction.from, Some(200.));
}

#[test]
fn same_slot_drop_does_not_anchor_a_later_move() {
    for grouped in [false, true] {
        let source = SOURCE.replace(
            "virtualized=true",
            if grouped {
                "virtualized=true reorderGroup=\"cards\""
            } else {
                "virtualized=true"
            },
        );
        let (mut r, grip) = fixture_with(&source, 200., 10);
        let b = r.reorder_binding(grip).unwrap();
        let g = r.reorder_geometry(b.list).unwrap();
        let token = if grouped {
            r.begin_group_reorder(b, g.clone(), false)
        } else {
            r.begin_reorder(b, g.clone())
        }
        .unwrap()
        .unwrap()
        .token;
        r.preview_reorder(token, g.clone(), 200.).unwrap();
        r.drop_reorder(token, g).unwrap().unwrap();
        r.finish_reorder(token).unwrap();
        r.act(
            "receive",
            vec![Value::str("10"), Value::some(Value::str("12"))],
        )
        .unwrap();
        assert_eq!(
            r.reorder_geometry(b.list).unwrap().scroll_top,
            220.,
            "grouped={grouped}"
        );
    }
}

#[test]
fn refused_or_cancelled_drop_does_not_anchor_a_later_move() {
    for (cancel, grouped, refused, finish_first) in [
        (false, false, true, false),
        (true, false, true, false),
        (false, true, true, false),
        (true, false, false, false),
        (true, false, false, true),
    ] {
        let source = SOURCE.replace("send changed = move(item, before)", if refused { "send changed = refuse(item, before)" } else { "send changed = unchanged()" })
            .replace("  view", "  action move_now(item: string, before: option<string>)\n    send changed = move(item, before)\n  view");
        let source = source.replace(
            "virtualized=true",
            if grouped {
                "virtualized=true reorderGroup=\"cards\""
            } else {
                "virtualized=true"
            },
        );
        let (mut r, grip) = fixture_with(&source, 200., 10);
        let b = r.reorder_binding(grip).unwrap();
        let g = r.reorder_geometry(b.list).unwrap();
        let token = if grouped {
            r.begin_group_reorder(b, g.clone(), false)
        } else {
            r.begin_reorder(b, g.clone())
        }
        .unwrap()
        .unwrap()
        .token;
        r.preview_reorder(token, g.clone(), 300.).unwrap();
        assert_eq!(r.drop_reorder(token, g).is_err(), refused);
        if grouped && refused {
            let frame = r.reorder_frame(token).unwrap();
            assert_eq!(frame.phase, crate::ReorderPhase::Settling);
            assert_eq!(frame.ending, None);
        }
        if finish_first {
            r.finish_reorder(token).unwrap();
        }
        if cancel {
            r.cancel_reorder(token).unwrap();
        }
        if !finish_first {
            r.finish_reorder(token).unwrap();
        }
        r.act(
            "move_now",
            vec![Value::str("10"), Value::some(Value::str("15"))],
        )
        .unwrap();
        assert_eq!(
            r.reorder_geometry(b.list).unwrap().scroll_top,
            280.,
            "cancel={cancel}, grouped={grouped}, refused={refused}"
        );
    }
}

#[test]
fn pending_drop_anchors_only_the_dispatched_destination() {
    let source = SOURCE.replace("send changed = move(item, before)", "send changed = unchanged()")
        .replace("  view", "  action move_now(item: string, before: option<string>)\n    send changed = move(item, before)\n  view");
    let (mut r, grip) = fixture_with(&source, 200., 10);
    let b = r.reorder_binding(grip).unwrap();
    let g = r.reorder_geometry(b.list).unwrap();
    let token = r.begin_reorder(b, g.clone()).unwrap().unwrap().token;
    r.preview_reorder(token, g.clone(), 300.).unwrap();
    r.drop_reorder(token, g).unwrap().unwrap();
    r.act(
        "move_now",
        vec![Value::str("10"), Value::some(Value::str("12"))],
    )
    .unwrap();
    assert_eq!(r.reorder_geometry(b.list).unwrap().scroll_top, 220.);
    report(&mut r, 220., Some(grip));
    r.act(
        "move_now",
        vec![Value::str("10"), Value::some(Value::str("15"))],
    )
    .unwrap();
    assert_eq!(r.reorder_geometry(b.list).unwrap().scroll_top, 200.);
}

#[test]
fn deleting_the_dropped_key_or_destination_ends_its_anchor_exclusion() {
    let source = SOURCE.replace("send changed = move(item, before)", "send changed = unchanged()")
        .replace("  view", "  action move_now(item: string, before: option<string>)\n    send changed = move(item, before)\n  action remove_now(item: string, before: option<string>)\n    send changed = remove(item, before)\n  action reset_now()\n    send changed = reset()\n  view");
    for removed in ["10", "15"] {
        let (mut r, grip) = fixture_with(&source, 207., 10);
        let b = r.reorder_binding(grip).unwrap();
        let g = r.reorder_geometry(b.list).unwrap();
        let token = r.begin_reorder(b, g.clone()).unwrap().unwrap().token;
        r.preview_reorder(token, g.clone(), 300.).unwrap();
        r.drop_reorder(token, g).unwrap().unwrap();
        r.finish_reorder(token).unwrap();
        r.act("remove_now", vec![Value::str(removed), Value::Option(None)])
            .unwrap();
        r.act("reset_now", vec![]).unwrap();
        report(&mut r, 207., None);
        r.act(
            "move_now",
            vec![Value::str("10"), Value::some(Value::str("15"))],
        )
        .unwrap();
        assert_eq!(r.reorder_geometry(b.list).unwrap().scroll_top, 287.);
    }
}

#[test]
fn a_later_preview_keeps_an_earlier_drops_anchor_and_deadline() {
    let source = SOURCE.replace("send changed = move(item, before)", "send changed = unchanged()")
        .replace("  view", "  action move_now(item: string, before: option<string>)\n    send changed = move(item, before)\n  view");
    for grouped in [false, true] {
        let source = source.replace(
            "virtualized=true",
            if grouped {
                "virtualized=true reorderGroup=\"cards\""
            } else {
                "virtualized=true"
            },
        );
        for cancel in [false, true] {
            for (delay, expected) in [(700., 187.), (1000., 287.)] {
                let (mut r, grip) = fixture_with(&source, 207., 10);
                let b = r.reorder_binding(grip).unwrap();
                let g = r.reorder_geometry(b.list).unwrap();
                let token = r.begin_reorder(b, g.clone()).unwrap().unwrap().token;
                r.preview_reorder(token, g.clone(), 300.).unwrap();
                r.drop_reorder(token, g).unwrap().unwrap();
                r.advance(600.).unwrap();
                r.finish_reorder(token).unwrap();

                let grip = r.kernel().find_by_test_id("grip-11")[0];
                report(&mut r, 207., Some(grip));
                let next = r.reorder_binding(grip).unwrap();
                let g = r.reorder_geometry(b.list).unwrap();
                let next = if grouped {
                    r.begin_group_reorder(next, g, false)
                } else {
                    r.begin_reorder(next, g)
                }
                .unwrap()
                .unwrap()
                .token;
                if cancel {
                    r.cancel_reorder(next).unwrap();
                    r.finish_reorder(next).unwrap();
                }
                r.advance(delay).unwrap();
                r.act(
                    "move_now",
                    vec![Value::str("10"), Value::some(Value::str("15"))],
                )
                .unwrap();
                assert_eq!(
                    r.reorder_geometry(b.list).unwrap().scroll_top,
                    expected,
                    "grouped={grouped}, cancel={cancel}, delay={delay}"
                );
            }
        }
    }
}

#[test]
fn a_second_drop_keeps_an_earlier_drops_anchor_and_deadline() {
    for (second, grouped) in [
        ("same_slot", false),
        ("same_slot", true),
        ("pending", false),
        ("cancelled", false),
        ("refused", false),
        ("refused", true),
        ("completed", false),
        ("completed", true),
    ] {
        let query = if second == "refused" {
            "refuse_second"
        } else {
            "unchanged"
        };
        let source = SOURCE.replace("move(item, before)", &format!("{query}(item, before)"))
            .replace("  view", "  action move_now(item: string, before: option<string>)\n    send changed = move(item, before)\n  view")
            .replace("virtualized=true", if grouped { "virtualized=true reorderGroup=\"cards\"" } else { "virtualized=true" });
        for (delay, expected) in [(700., 187.), (1000., 287.)] {
            let (mut r, grip) = fixture_with(&source, 207., 10);
            let b = r.reorder_binding(grip).unwrap();
            let g = r.reorder_geometry(b.list).unwrap();
            let first = r.begin_reorder(b, g.clone()).unwrap().unwrap().token;
            r.preview_reorder(first, g.clone(), 300.).unwrap();
            r.drop_reorder(first, g).unwrap().unwrap();
            r.advance(600.).unwrap();
            r.finish_reorder(first).unwrap();
            r.advance(650.).unwrap();
            let grip = r.kernel().find_by_test_id("grip-11")[0];
            report(&mut r, 207., Some(grip));
            let b = r.reorder_binding(grip).unwrap();
            let g = r.reorder_geometry(b.list).unwrap();
            let next = if grouped {
                r.begin_group_reorder(b, g.clone(), false)
            } else {
                r.begin_reorder(b, g.clone())
            }
            .unwrap()
            .unwrap()
            .token;
            let y = if second == "same_slot" { 220. } else { 300. };
            r.preview_reorder(next, g.clone(), y).unwrap();
            assert_eq!(r.drop_reorder(next, g).is_err(), second == "refused");
            if second == "cancelled" {
                r.cancel_reorder(next).unwrap();
            }
            if second == "completed" {
                r.act(
                    "move_now",
                    vec![Value::str("11"), Value::some(Value::str("15"))],
                )
                .unwrap();
                assert_eq!(r.reorder_geometry(b.list).unwrap().scroll_top, 207.);
            }
            r.finish_reorder(next).unwrap();
            r.advance(delay).unwrap();
            r.act(
                "move_now",
                vec![Value::str("10"), Value::some(Value::str("15"))],
            )
            .unwrap();
            assert_eq!(
                r.reorder_geometry(b.list).unwrap().scroll_top,
                expected,
                "second={second}, grouped={grouped}, delay={delay}"
            );
            if second == "pending" {
                report(&mut r, 207., None);
                r.advance(if delay == 700. { 1100. } else { 1650. })
                    .unwrap();
                r.act(
                    "move_now",
                    vec![Value::str("11"), Value::some(Value::str("15"))],
                )
                .unwrap();
                assert_eq!(r.reorder_geometry(b.list).unwrap().scroll_top, expected);
            }
        }
    }
}

#[test]
fn a_drop_after_the_return_spring_keeps_the_other_visible_row_in_place() {
    let source = SOURCE.replace("send changed = move(item, before)", "send changed = unchanged()")
        .replace("  view", "  action move_now(item: string, before: option<string>)\n    send changed = move(item, before)\n  view");
    for (delay, expected) in [(600., 187.), (999., 187.), (1000., 287.), (1001., 287.)] {
        for finish in [false, true] {
            let (mut r, grip) = fixture_with(&source, 207., 10);
            r.advance(3000.).unwrap();
            let b = r.reorder_binding(grip).unwrap();
            let g = r.reorder_geometry(b.list).unwrap();
            let token = r.begin_reorder(b, g.clone()).unwrap().unwrap().token;
            r.preview_reorder(token, g.clone(), 300.).unwrap();
            r.drop_reorder(token, g).unwrap().unwrap();
            r.advance(3600.).unwrap();
            if finish {
                r.finish_reorder(token).unwrap();
                assert!(r.reorder_frame(token).is_none());
            }
            r.act(
                "move_now",
                vec![Value::str("0"), Value::some(Value::str("2"))],
            )
            .unwrap();
            assert_eq!(r.reorder_geometry(b.list).unwrap().scroll_top, 207.);
            r.advance(3000. + delay).unwrap();
            r.act(
                "move_now",
                vec![Value::str("10"), Value::some(Value::str("15"))],
            )
            .unwrap();
            assert_eq!(
                r.reorder_geometry(b.list).unwrap().scroll_top,
                expected,
                "delay={delay}, finish={finish}"
            );
        }
    }
}

#[test]
fn delayed_drops_landing_in_one_update_anchor_a_row_neither_moved() {
    let source = SOURCE.replace("send changed = move(item, before)", "send changed = unchanged()")
        .replace("  view", "  action move_both(a: string, a_before: option<string>, b: string, b_before: option<string>)\n    send changed = move2(a, a_before, b, b_before)\n  view");
    let (mut r, grip) = fixture_with(&source, 207., 10);
    let b = r.reorder_binding(grip).unwrap();
    let g = r.reorder_geometry(b.list).unwrap();
    let first = r.begin_reorder(b, g.clone()).unwrap().unwrap().token;
    r.preview_reorder(first, g.clone(), 300.).unwrap();
    r.drop_reorder(first, g).unwrap().unwrap();
    r.advance(600.).unwrap();
    r.finish_reorder(first).unwrap();
    r.advance(650.).unwrap();
    let grip = r.kernel().find_by_test_id("grip-11")[0];
    report(&mut r, 207., Some(grip));
    let b = r.reorder_binding(grip).unwrap();
    let g = r.reorder_geometry(b.list).unwrap();
    let next = r.begin_reorder(b, g.clone()).unwrap().unwrap().token;
    r.preview_reorder(next, g.clone(), 280.).unwrap();
    r.drop_reorder(next, g).unwrap().unwrap();
    r.finish_reorder(next).unwrap();
    r.advance(700.).unwrap();
    let both = ["10", "15", "11", "14"].map(Value::str);
    let [a, a_before, b_item, b_before] = both;
    r.act(
        "move_both",
        vec![a, Value::some(a_before), b_item, Value::some(b_before)],
    )
    .unwrap();
    assert_eq!(&r.data_ref().0[10..15], ["12", "13", "11", "14", "10"]);
    // Row 12, 33 px into the port before, stays there.
    assert_eq!(r.reorder_geometry(b.list).unwrap().scroll_top, 167.);
}

/// Rebinding a retiring wrapper that grouped reorder settling hid must restore
/// kernel `visibility: visible` so the newly bound row is shown.
#[test]
fn rebinding_a_hidden_settled_row_restores_wrapper_visibility() {
    struct TwoLists {
        left: Vec<String>,
        right: Vec<String>,
    }
    impl DataSource for TwoLists {
        fn query(&mut self, name: &str, args: &[Value]) -> Result<Value, DataError> {
            if name == "transfer" {
                let item = args[0].as_str().unwrap().to_string();
                self.left.retain(|k| k != &item);
                let before = match &args[1] {
                    Value::Option(Some(v)) => v.as_str(),
                    _ => None,
                };
                let to = self
                    .right
                    .iter()
                    .position(|k| Some(k.as_str()) == before)
                    .unwrap_or(self.right.len());
                self.right.insert(to, item);
            }
            let list = |xs: &[String]| Value::list(xs.iter().map(|k| Value::str(k)).collect());
            Ok(Value::record(vec![list(&self.left), list(&self.right)]))
        }
    }
    let source = r#"shape Pair
  left: string[]
  right: string[]

component App
  resource initial = lists() as shape Pair
  mutation moved as shape Pair
  derive pair = match moved { case some(v) => v, case none => initial }
  action drop_right(item: string, before: option<string>)
    send moved = transfer(item, before)
  view
    column
      list id="source" testId="source" virtualized=true estimated-item-height=20 height=100 reorderGroup="cards" reorderdrop=drop_right
        for x in pair.left key=x
          column reorderFor="source" testId=`grip-${x}` height=20
            text x
      list id="target" testId="target" virtualized=true estimated-item-height=20 height=100 reorderGroup="cards" reorderdrop=drop_right
        for x in pair.right key=x
          column reorderFor="target" testId=`target-${x}` height=20
            text x
"#;
    let mut r = Runner::boot(
        contract::compile(source).unwrap(),
        TwoLists {
            left: (0..10).map(|i| format!("a{i}")).collect(),
            right: (0..30).map(|i| format!("b{i}")).collect(),
        },
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    r.set_row_reuse(true);
    let feed = |r: &mut Runner<TwoLists>, right_offset: f64, pin: Option<NodeKey>| {
        let collections = r.collections();
        let source_view = collections[0].view;
        for c in collections {
            let is_source = c.view == source_view;
            r.collection_feedback(CollectionFeedback {
                view: c.view,
                revision: c.revision,
                scroll_sequence: c.scroll_sequence + 1,
                offset: if is_source { 0. } else { right_offset },
                port_cross: 320.,
                port_main: 100.,
                cross: 320.,
                padding: [0.; 2],
                focus_view: None,
                interaction_view: pin
                    .filter(|_| is_source)
                    .map(|k| r.kernel().node_by_key(k).unwrap().id),
                measurements: c
                    .rows
                    .iter()
                    .map(|row| RowMeasurement {
                        view: row.view,
                        epoch: row.epoch,
                        size: 20.,
                    })
                    .collect(),
            })
            .unwrap();
        }
    };
    for _ in 0..3 {
        feed(&mut r, 0., None);
    }
    let grip = r.kernel().find_by_test_id("grip-a0")[0];
    feed(&mut r, 0., Some(grip));
    let b = r.reorder_binding(grip).unwrap();
    let g = r.reorder_geometry(b.list).unwrap();
    let token = r.begin_group_reorder(b, g, true).unwrap().unwrap().token;
    let target = r.kernel().find_by_test_id("target")[0];
    let tg = r.reorder_geometry(target).unwrap();
    assert!(matches!(
        r.preview_reorder_into(token, target, tg.clone(), 0.)
            .unwrap(),
        ReorderProgress::Accepted { .. }
    ));
    r.drop_reorder(token, tg).unwrap().unwrap();
    assert_eq!(
        r.reorder_frame(token).unwrap().phase,
        crate::ReorderPhase::Settling
    );
    let hidden_wrapper = r.collections()[1]
        .rows
        .iter()
        .find(|row| row.index == 0)
        .unwrap()
        .view;
    assert_eq!(
        r.kernel().node(hidden_wrapper).unwrap().style.visibility,
        exact_kernel::Visibility::Hidden
    );
    // Scroll the target list while settling so `a0` leaves the window and its
    // hidden wrapper is rebound to a newly realized row.
    feed(&mut r, 500., None);
    let reused_index = r.collections()[1]
        .rows
        .iter()
        .find(|row| row.view == hidden_wrapper)
        .expect("settling wrapper should be reused by Collection::rebind")
        .index;
    assert_ne!(reused_index, 0);
    assert_eq!(
        r.kernel().node(hidden_wrapper).unwrap().style.visibility,
        exact_kernel::Visibility::Visible
    );
    r.finish_reorder(token).unwrap();
    assert_eq!(
        r.kernel().node(hidden_wrapper).unwrap().style.visibility,
        exact_kernel::Visibility::Visible
    );
}
