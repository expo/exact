use super::*;

fn query(h: &Harness, proposed: f64) -> Option<f64> {
    query_work(h, proposed).0
}

fn query_work(h: &Harness, proposed: f64) -> (Option<f64>, usize) {
    query_bounded(h, proposed, f64::INFINITY)
}

fn query_bounded(h: &Harness, proposed: f64, distance: f64) -> (Option<f64>, usize) {
    let mut ids = Ids::default();
    let mut u = Update::new(env_with(&h.plan, &h.slots, h.strings), &h.sites, &mut ids);
    let result = h
        .tree
        .collection_snap(
            &mut u,
            &h.kernel,
            h.snapshot().view,
            [
                proposed,
                300.0,
                (h.snapshot().total_extent - 300.0).max(0.0),
                distance,
            ],
        )
        .unwrap();
    (result, u.work.nodes_visited)
}

#[test]
fn snap_uses_each_rows_alignment_and_index_size_beyond_the_mounted_window() {
    for gap in [0.0, 14.0] {
        let source = format!(
            r#"component App
  view
    list virtualized=true display="flex" width=300 height=100 gap={gap} estimated-item-width=150
      for n in range(400) key=n
        view width=(100 + n % 5 * 25) height=100 scroll-snap-align=(n % 3 == 0 ? "none" : n % 3 == 1 ? "start" : "end")
"#
        );
        let mut h = Harness::from_parts(contract::compile(&source).unwrap(), vec![]);
        let opening = h.snapshot();
        assert!(opening.rows.iter().all(|r| r.index < 10));
        // Supply the measurements a traversal would leave in the index; they
        // remain available after their native views have retired.
        let Child::Node(root) = &mut h.tree.children[0] else {
            panic!("list root")
        };
        let c = root.collection_mut().unwrap();
        for p in 0..400 {
            let token = c.index.measurement_token_at(p).unwrap();
            c.index
                .set_measured_height_at(p, token, (100 + p % 5 * 25) as f64 + gap)
                .unwrap();
        }
        let before = h.snapshot();
        let mut start = 0.0;
        let mut positions = Vec::new();
        for p in 0..400 {
            let size = (100 + p % 5 * 25) as f64;
            if p % 3 != 0 && !before.rows.iter().any(|r| r.index == p) {
                positions.push(
                    (if p % 3 == 1 {
                        start
                    } else {
                        start + size - 300.0
                    })
                    .clamp(0.0, before.total_extent - 300.0),
                );
            }
            start += size + gap;
        }
        for proposed in [1380.0, 30020.0, 59990.0] {
            let expected = positions.iter().copied().min_by(|a, b| {
                (a - proposed)
                    .abs()
                    .total_cmp(&(b - proposed).abs())
                    .then(a.total_cmp(b))
            });
            assert_eq!(query(&h, proposed), expected, "projection {proposed}");
        }
        assert_eq!(
            h.snapshot(),
            before,
            "a snap read changes no collection state"
        );
    }
}

#[test]
fn snap_uses_estimates_and_current_data_and_styles_within_the_local_window() {
    let source = r#"component App
  state reverse = false
  state count = 400
  action flip
    reverse = not reverse
  action trim
    count = 200
  view
    list virtualized=true display="flex" width=300 height=100 estimated-item-width=150
      for n in range(count) key=n
        view width=150 height=100 scroll-snap-align=(n == (reverse ? 199 : 350) ? "end" : "none")
"#;
    let mut r = crate::Runner::boot(
        contract::compile(source).unwrap(),
        NoData,
        Kernel::with_monospace(),
        Default::default(),
        "/",
    )
    .unwrap();
    let id = r.collections()[0].view;
    assert_eq!(
        r.collection_snap(id, 30020.0, 300.0, 59700.0, f64::INFINITY),
        None
    );
    r.act("flip", vec![]).unwrap();
    assert_eq!(
        r.collection_snap(id, 30020.0, 300.0, 59700.0, f64::INFINITY),
        Some(29700.0)
    );
    r.act("trim", vec![]).unwrap();
    assert_eq!(
        r.collection_snap(id, 30020.0, 300.0, 29700.0, f64::INFINITY),
        Some(29700.0)
    );
    r.act("flip", vec![]).unwrap();
    assert_eq!(
        r.collection_snap(id, 30020.0, 300.0, 29700.0, f64::INFINITY),
        None
    );
    assert_eq!(
        r.collection_snap(id, f64::NAN, 300.0, 29700.0, f64::INFINITY),
        None
    );
}

#[test]
fn oversized_unmounted_rows_keep_their_free_scroll_interval_and_margins() {
    let source = r#"component App
  view
    list virtualized=true display="flex" width=300 height=100 estimated-item-width=620
      for n in range(400) key=n
        view width=600 height=100 margin-left=10 margin-right=10 scroll-snap-align="end"
"#;
    let h = Harness::from_parts(contract::compile(source).unwrap(), vec![]);
    assert_eq!(query(&h, 124100.0), Some(124100.0));
    assert_eq!(query(&h, 124500.0), Some(124630.0));
}

#[test]
fn snap_sparse_alignment_evaluates_at_most_64_rows_in_a_100000_row_list() {
    let mut counts = Vec::new();
    for body in [
        "view width=150 scroll-snap-align=(n == 0 ? \"start\" : \"none\")",
        "view width=150\n          view scroll-snap-align=\"start\"",
    ] {
        let source = format!(
            r#"component App
  state items = range(1)
  view
    list virtualized=true display="flex" width=300 height=100 estimated-item-width=150
      for n in items key=n
        {body}
"#
        );
        for (width, projection) in [(150, 7500020.0), (1, 50000.25)] {
            let source = source.replace("150", &width.to_string());
            let h = Harness::from_parts(contract::compile(&source).unwrap(), vec![values(100000)]);
            let (target, nodes) = query_work(&h, projection);
            assert_eq!(target, None);
            let nodes_per_row = if body.contains('\n') { 2 } else { 1 };
            counts.push(nodes / nodes_per_row);
            assert_eq!(
                query_bounded(&h, 0.0, 0.0),
                (None, 0),
                "an exact mounted match evaluates no row"
            );
        }
    }
    assert!(counts.iter().all(|&n| n <= 64), "evaluated {counts:?} rows");
    assert_eq!(counts[1], 64, "tiny rows exercise the fixed cap");
    assert_eq!(counts[3], 64);
}

#[test]
fn snap_negative_margin_can_put_a_later_row_nearer_than_the_slot_bound() {
    let source = r#"component App
  view
    list virtualized=true display="flex" width=300 height=100 estimated-item-width=150
      for n in range(1000) key=n
        view width=150 height=100 margin-left=(n == 20 ? -400 : 0) scroll-snap-align="start"
"#;
    let h = Harness::from_parts(contract::compile(source).unwrap(), vec![]);
    assert_eq!(query(&h, 2600.0), Some(2600.0));
    assert_eq!(
        [query(&h, 2800.0), query(&h, 2620.0)],
        [Some(2850.0), Some(2600.0)]
    );
}

#[test]
fn snap_font_relative_and_percentage_offsets_need_mounted_layout() {
    for property in ["left", "right", "margin-left", "margin-right"] {
        for value in ["1em", "1rem", "10%"] {
            let source = format!(
                r#"component App
  view
    list virtualized=true display="flex" width=300 height=100 estimated-item-width=150
      for n in range(1000) key=n
        view width=150 height=100 font-size=30 position="relative" {property}="{value}" scroll-snap-align=(n == 200 ? "start" : "none")
"#
            );
            let mut h = Harness::from_parts(contract::compile(&source).unwrap(), vec![]);
            h.kernel
                .compute_layout(
                    h.snapshot().view,
                    exact_kernel::Offer::definite(300.0, 300.0),
                )
                .unwrap();
            assert_eq!(query(&h, 30020.0), None, "{property}={value}");
        }
    }
}

#[test]
fn snap_relative_root_insets_move_the_unmounted_target() {
    for (inset, expected) in [
        ("left=30", 30030.0),
        ("right=30", 29970.0),
        ("left=30 right=70", 30030.0),
    ] {
        let source = format!(
            r#"component App
  view
    list virtualized=true display="flex" width=300 height=100 estimated-item-width=150
      for n in range(1000) key=n
        view width=150 height=100 position="relative" {inset} scroll-snap-align="start"
"#
        );
        let h = Harness::from_parts(contract::compile(&source).unwrap(), vec![]);
        assert_eq!(query(&h, 30020.0), Some(expected), "{inset}");
    }
}

#[test]
fn snap_equal_distance_keeps_the_smaller_offset() {
    let source = r#"component App
  view
    list virtualized=true display="flex" width=300 height=100 estimated-item-width=150
      for n in range(1000) key=n
        view width=150 height=100 scroll-snap-align="start"
"#;
    let h = Harness::from_parts(contract::compile(source).unwrap(), vec![]);
    assert_eq!(query(&h, 2925.0), Some(2850.0));
}

#[test]
fn snap_unmounted_roots_contribute_their_own_area_not_their_descendants() {
    for overflow in ["hidden", "clip", "auto", "scroll"] {
        for (align, expected) in [("center", Some(29925.0)), ("none", None)] {
            let source = format!(
                r#"component App
  view
    list virtualized=true display="flex" width=300 height=100 estimated-item-width=150
      for n in range(400) key=n
        view width=150 height=100 overflow-x="{overflow}" overflow-y="{overflow}" scroll-snap-align="{align}"
          view width=600 height=100 scroll-snap-align="start"
"#
            );
            let h = Harness::from_parts(contract::compile(&source).unwrap(), vec![]);
            assert_eq!(query(&h, 29930.0), expected, "{overflow}, {align}");
        }
    }
}

#[test]
fn snap_failed_row_keeps_a_target_already_found() {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let number = b.primitive(TypeKind::Number);
    let enabled = b.constant(&Value::Bool(true));
    let flex = b.constant(&Value::str("flex"));
    let estimate = b.constant(&Value::Number(150.0));
    let root = b.node(
        NodeType::List as u8,
        None,
        None,
        0,
        &[
            binding(BindingKind::Prop, PropId::Virtualized as u16, enabled),
            binding(
                BindingKind::Prop,
                PropId::EstimatedItemWidth as u16,
                estimate,
            ),
            binding(
                BindingKind::Style,
                exact_kernel::StyleId::Display as u16,
                flex,
            ),
        ],
        &[],
        None,
    );
    let items = b.constant(&values(1000));
    let key = code(&mut b, |a| {
        a.load_item(0);
    });
    let (_, arms) = b.region(RegionKind::Each, Some(root), None, 0, items, key, 1);
    let state = b.slot("value", number, key);
    b.set_slot_owner(state, arms[0]);
    let start = b.constant(&Value::str("start"));
    b.node(
        NodeType::View as u8,
        None,
        Some(arms[0]),
        0,
        &[binding(
            BindingKind::Style,
            exact_kernel::StyleId::ScrollSnapAlign as u16,
            start,
        )],
        &[],
        None,
    );
    let mut h = Harness::from_parts(b.finish().unwrap(), vec![]);
    let Child::Node(root) = &mut h.tree.children[0] else {
        panic!("list")
    };
    let c = root.collection_mut().unwrap();
    let mut items = c.items.to_vec();
    items[201] = Value::Bool(false); // This row's numeric slot cannot initialize.
    c.items = items.into();
    assert_eq!(query(&h, 30020.0), Some(30000.0));
}

#[test]
fn snap_mounted_distance_bounds_candidates_after_their_style_is_read() {
    let source = r#"component App
  view
    list virtualized=true display="flex" width=300 height=100 estimated-item-width=150
      for n in range(1000) key=n
        view width=150 scroll-snap-align="start"
"#;
    let h = Harness::from_parts(contract::compile(source).unwrap(), vec![]);
    assert_eq!(query_bounded(&h, 30020.0, 19.0).0, None);
    assert_eq!(query_bounded(&h, 30020.0, 20.0).0, Some(30000.0));
    assert_eq!(query_bounded(&h, 30000.0, 0.0), (None, 0));
}

#[test]
fn snap_vertical_and_center_use_the_axis_insets_and_margins() {
    for horizontal in [false, true] {
        let list = if horizontal {
            "display=\"flex\" width=300 height=100 estimated-item-width=150 padding-left=10"
        } else {
            "width=100 height=300 estimated-item-height=150 padding-top=10"
        };
        let row = if horizontal {
            "width=130 height=100 margin-left=10 margin-right=10 left=30"
        } else {
            "width=100 height=130 margin-top=10 margin-bottom=10 top=30"
        };
        for (align, expected) in [("start", 30050.0), ("center", 29965.0), ("end", 29880.0)] {
            let source = format!("component App\n  view\n    list virtualized=true {list}\n      for n in range(400) key=n\n        view {row} position=\"relative\" scroll-snap-align=(n == 200 ? \"{align}\" : \"none\")\n");
            let mut h = Harness::from_parts(contract::compile(&source).unwrap(), vec![]);
            h.kernel
                .compute_layout(
                    h.snapshot().view,
                    exact_kernel::Offer::definite(300.0, 300.0),
                )
                .unwrap();
            assert_eq!(
                query(&h, 30020.0),
                Some(expected),
                "{align}, horizontal={horizontal}"
            );
        }
    }
}

#[test]
fn a_hidden_unmounted_root_is_not_a_snap_target() {
    let source = r#"component App
  state shown = false
  view
    list virtualized=true display="flex" width=300 height=100 gap=14 estimated-item-width=100
      for n in range(1000) key=n
        view width=100 height=100 display=(n == 80 and not shown ? "none" : "block") scroll-snap-align=(n == 80 ? "start" : "none")
"#;
    let mut h = Harness::from_parts(contract::compile(source).unwrap(), vec![Value::Bool(false)]);
    assert_eq!(query(&h, 9000.0), None);
    h.slots[0] = Value::Bool(true);
    h.update().unwrap();
    assert_eq!(query(&h, 9000.0), Some(9120.0));
}
