use super::*;

fn strip(count: usize, body: &str) -> Harness {
    let source = format!(
        r#"component App
  state width = 0
  view
    list virtualized=true display="flex" width=320 height=100 gap=14 estimated-item-width=100
      for n in range({count}) key=n
        {body}
"#
    );
    Harness::from_parts(
        contract::compile(&source).unwrap(),
        vec![Value::Number(0.0)],
    )
}

fn measured(h: &Harness, offset: f64, size: f64) -> CollectionFeedback {
    let mut f = h.feedback(offset);
    f.measurements = h
        .snapshot()
        .rows
        .iter()
        .map(|r| RowMeasurement {
            view: r.view,
            epoch: r.epoch,
            size,
        })
        .collect();
    f
}

#[test]
fn a_gapped_zero_card_reappears_after_content_invalidation() {
    let mut h = strip(1, "view width=width height=100");
    h.send(h.feedback(0.0));
    h.send(measured(&h, 0.0, 0.0));
    assert!(h.snapshot().rows.is_empty());
    h.slots[0] = Value::Number(100.0);
    h.update().unwrap();
    assert_eq!(h.snapshot().total_extent, 100.0);
    assert_eq!(h.snapshot().rows.len(), 1);
}

#[test]
fn gapped_zero_cards_regain_estimates_after_cross_size_invalidation() {
    let mut h = strip(1000, "view width=0 height=100");
    h.send(h.feedback(0.0));
    // Measure each row through feedback, including rows that later leave the window.
    for p in 0..1000 {
        let offset = h.collection().index.prefix(p).unwrap();
        h.send(h.feedback(offset));
        h.send(measured(&h, offset, 0.0));
    }
    assert_eq!(h.snapshot().total_extent, 999.0 * 14.0);
    let mut f = h.feedback(0.0);
    f.cross = 200.0;
    h.send(f);
    assert_eq!(h.snapshot().total_extent, 1000.0 * 114.0 - 14.0);
    assert_eq!(h.collection().index.prefix(800).unwrap(), 800.0 * 114.0);
}

#[test]
fn a_gapped_cards_distance_ends_at_its_own_edge() {
    let h = strip(10, "view width=100 height=100");
    assert_eq!(h.collection().distance(0, 101.0, 421.0), (true, 1.0));
}

#[test]
fn a_gapped_cards_trailing_gap_does_not_reveal_it() {
    for deferred in [false, true] {
        let mut h = strip(10, "view width=100 height=100");
        h.send(h.feedback(101.0));
        let mut u = Update::new(env_with(&h.plan, &h.slots, h.strings), &h.sites, &mut h.ids);
        let Child::Node(root) = &mut h.tree.children[0] else {
            panic!()
        };
        let c = root.collection_mut().unwrap();
        let row = c.mounted.iter_mut().find(|r| r.position == 0).unwrap();
        let wrapper = row.wrapper;
        row.awaiting = true;
        c.any_awaiting = true;
        if deferred {
            c.reveal_at(&mut u, 101.0);
        } else {
            c.reveal_shown(&mut u);
        }
        assert!(!u.shown.revealed.contains(&wrapper), "deferred={deferred}");
        c.reveal_at(&mut u, 99.0);
        assert!(u.shown.revealed.contains(&wrapper));
    }
}

#[test]
fn hidden_roots_match_eager_collection_layout() {
    for horizontal in [false, true] {
        let attrs = if horizontal {
            r#"display="flex" gap=14 estimated-item-width=100"#
        } else {
            "estimated-item-height=100"
        };
        let source = format!(
            r#"component App
  view
    list virtualized=true width=320 height=300 {attrs}
      for n in range(3) key=n
        view width=100 height=100 display=(n == 1 ? "none" : "block")
"#
        );
        let mut h = Harness::from_parts(contract::compile(&source).unwrap(), vec![]);
        let c = h.snapshot();
        h.kernel
            .compute_layout(c.view, exact_kernel::Offer::definite(320.0, 300.0))
            .unwrap();
        let frames: Vec<_> = h
            .collection()
            .mounted
            .iter()
            .map(|r| h.kernel.node(r.wrapper).unwrap().frame)
            .collect();
        if horizontal {
            assert_eq!(frames[1].width, 0.0);
            assert_eq!(frames[2].x, 114.0);
        } else {
            assert_eq!(frames[1].height, 0.0);
            assert_eq!(frames[2].y, 100.0);
        }
    }
}

#[test]
fn gapped_measurement_noise_preserves_zero_to_positive_changes() {
    let mut h = strip(2, "view width=100 height=100");
    h.send(h.feedback(0.0));
    for size in [0.0, 0.005, 0.0] {
        h.send(measured(&h, 0.0, size));
        assert_eq!(h.collection().index.height(0), Some(14.0 + size));
    }
}

#[test]
fn a_gapped_scroll_anchor_skips_cards_before_the_port() {
    let mut h = strip(10, "view width=100 height=100");
    let anchor =
        h.tree
            .collection_scroll_anchor(&h.kernel, h.snapshot().view, 105.0, 0.0, 320.0, 100.0);
    assert_eq!(anchor, Some((Value::str("1"), -9.0)));
    assert_eq!(
        h.tree
            .collection_scroll_anchor(&h.kernel, h.snapshot().view, 105.0, 0.0, 3.0, 100.0),
        None
    );
    assert_eq!(
        h.tree
            .collection_scroll_anchor(&h.kernel, h.snapshot().view, 1128.0, 0.0, 10.0, 100.0),
        None
    );
    h.send(h.feedback(0.0));
    let row = h
        .snapshot()
        .rows
        .into_iter()
        .find(|r| r.index == 1)
        .unwrap();
    let mut f = h.feedback(0.0);
    f.measurements.push(RowMeasurement {
        view: row.view,
        epoch: row.epoch,
        size: 0.0,
    });
    h.send(f);
    assert_eq!(
        h.tree
            .collection_scroll_anchor(&h.kernel, h.snapshot().view, 105.0, 0.0, 320.0, 100.0),
        Some((Value::str("2"), -23.0))
    );
}

#[test]
fn a_gapped_glimpse_ranks_card_centres() {
    let h = strip(10, "view width=100 height=100");
    assert_eq!(
        h.collection().glimpsed(0..2, 113.0, 1, &BTreeMap::new()),
        vec![1..2]
    );
}

#[test]
fn hidden_horizontal_roots_take_no_extent_but_visible_zero_cards_take_gaps() {
    for (body, extent, third) in [
        (
            r#"view width=100 height=100 display=(n == 1 ? "none" : "block")"#,
            214.0,
            114.0,
        ),
        (
            r#"view width=100 height=100 display=(n == 2 ? "none" : "block")"#,
            214.0,
            0.0,
        ),
        (r#"view width=100 height=100 display="none""#, 0.0, 0.0),
        ("view width=(n == 1 ? 0 : 100) height=100", 228.0, 128.0),
    ] {
        let mut h = strip(3, body);
        h.send(h.feedback(0.0));
        let c = h.snapshot();
        h.kernel
            .compute_layout(c.view, exact_kernel::Offer::definite(320.0, 100.0))
            .unwrap();
        let mut f = h.feedback(0.0);
        f.measurements = c
            .rows
            .iter()
            .map(|r| RowMeasurement {
                view: r.view,
                epoch: r.epoch,
                size: f64::from(h.kernel.node(r.view).unwrap().frame.width),
            })
            .collect();
        h.send(f);
        assert_eq!(h.snapshot().total_extent, extent, "{body}");
        if let Some(row) = h.collection().mounted.iter().find(|r| r.position == 2) {
            assert_eq!(
                f64::from(h.kernel.node(row.wrapper).unwrap().frame.x),
                third,
                "{body}"
            );
        }
    }
}

#[test]
fn hidden_horizontal_roots_reappear_on_content_change() {
    let mut h = strip(
        3,
        r#"view width=100 height=100 display=(n == width ? "none" : "block")"#,
    );
    h.send(h.feedback(0.0));
    assert_eq!(h.snapshot().total_extent, 214.0);
    h.slots[0] = Value::Number(-1.0);
    h.update().unwrap();
    h.send(h.feedback(0.0));
    assert_eq!(h.snapshot().total_extent, 328.0);
    assert_eq!(h.snapshot().rows.len(), 3);
}

#[test]
fn invalidating_a_zero_card_keeps_the_scrolled_anchor() {
    let mut h = strip(100, "view width=(n == 3 ? 0 : 100) height=100 background-color=(width == 0 ? \"red\" : \"blue\")");
    h.send(h.feedback(0.0));
    let mut f = measured(&h, 500.0, 100.0);
    for m in &mut f.measurements {
        if h.snapshot()
            .rows
            .iter()
            .any(|r| r.view == m.view && r.index == 3)
        {
            m.size = 0.0;
        }
    }
    h.send(f);
    h.send(h.feedback(500.0));
    h.slots[0] = Value::Number(1.0);
    h.update().unwrap();
    let at = h.collection().geometry.as_ref().unwrap().offset;
    let mut f = measured(&h, at, 100.0);
    for m in &mut f.measurements {
        if h.snapshot()
            .rows
            .iter()
            .any(|r| r.view == m.view && r.index == 3)
        {
            m.size = 0.0;
        }
    }
    h.send(f);
    assert_eq!(h.collection().geometry.as_ref().unwrap().offset, 500.0);
}

#[test]
fn hidden_off_window_cards_regain_estimates_on_invalidation() {
    let mut h = strip(
        100,
        "view width=100 height=100 display=(n == 80 and width == 0 ? \"none\" : \"block\")",
    );
    h.send(h.feedback(9000.0));
    assert_eq!(h.collection().index.height(80), Some(0.0));
    h.send(h.feedback(0.0));
    assert!(!h.collection().mounted.iter().any(|r| r.position == 80));
    h.slots[0] = Value::Number(1.0);
    h.update().unwrap();
    assert_eq!(h.collection().index.height(80), Some(114.0));
    assert_eq!(h.snapshot().total_extent, 11386.0);
    assert!(!h.collection().mounted.iter().any(|r| r.position == 80));
}

#[test]
fn first_hidden_card_before_the_port_preserves_its_anchor() {
    let mut h = strip(
        100,
        "view width=100 height=100 display=(n == 10 ? \"none\" : \"block\")",
    );
    h.send(h.feedback(0.0));
    h.send(h.feedback(1300.0));
    assert_eq!(h.collection().index.height(10), Some(0.0));
    assert_eq!(h.collection().geometry.as_ref().unwrap().offset, 1186.0);
}

#[test]
fn first_measurement_anchors_the_card_after_a_gap() {
    let mut h = strip(100, "view width=100 height=100");
    h.send(h.feedback(0.0));
    let row = h
        .snapshot()
        .rows
        .into_iter()
        .find(|r| r.index == 0)
        .unwrap();
    let mut f = h.feedback(105.0);
    f.measurements.push(RowMeasurement {
        view: row.view,
        epoch: row.epoch,
        size: 50.0,
    });
    h.send(f);
    assert_eq!(h.collection().geometry.as_ref().unwrap().offset, 55.0);
}

#[test]
fn hidden_boundaries_are_transparent_to_reach_events() {
    for hidden in [0, 2, 3] {
        let mut h = strip(3, &format!("view width=100 height=100 display=(n == {hidden} or {hidden} == 3 ? \"none\" : \"block\")"));
        h.send(h.feedback(0.0));
        let Child::Node(root) = &mut h.tree.children[0] else {
            panic!()
        };
        assert_eq!(
            root.collection_mut().unwrap().geometric_edges(0.0).unwrap(),
            [hidden != 3; 2],
            "hidden={hidden}"
        );
    }
}

#[test]
fn hidden_tail_keeps_requesting_reports_until_the_window_is_filled() {
    let mut h = strip(
        1000,
        "view width=100 height=100 display=(n < 3 or width > 0 ? \"block\" : \"none\")",
    );
    for _ in 0..1000 {
        h.send(h.feedback(0.0));
        if !h.snapshot().pending {
            break;
        }
    }
    assert!(!h.snapshot().pending);
    assert_eq!(h.snapshot().total_extent, 328.0);
    assert!(h.collection().mounted.len() <= 16);
    h.slots[0] = Value::Number(1.0);
    h.update().unwrap();
    assert_eq!(h.snapshot().total_extent, 113986.0);
    let mut u = Update::new(env_with(&h.plan, &h.slots, h.strings), &h.sites, &mut h.ids);
    let Child::Node(root) = &mut h.tree.children[0] else {
        panic!()
    };
    let c = root.collection_mut().unwrap();
    c.begin_into_view(&mut u, &[], "n:500", Align::Start, false, None)
        .unwrap();
    assert_eq!(c.geometry.as_ref().unwrap().offset, 57000.0);
}

#[test]
fn hidden_rows_are_neither_glimpses_nor_scroll_anchors() {
    let mut h = strip(
        5,
        "view width=100 height=100 display=(n == 1 ? \"none\" : \"block\")",
    );
    h.send(h.feedback(0.0));
    assert_eq!(
        h.collection().glimpsed(0..5, 114.0, 5, &BTreeMap::new()),
        vec![2..3, 0..1, 3..4, 4..5]
    );
    assert_eq!(
        h.tree
            .collection_scroll_anchor(&h.kernel, h.snapshot().view, 105.0, 0.0, 320.0, 100.0),
        Some((Value::str("2"), -9.0))
    );
}
