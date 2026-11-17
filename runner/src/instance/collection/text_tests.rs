use super::*;

#[test]
fn clearing_typography_remounts_zero_height_rows() {
    check_clearing_typography(false);
}

#[test]
fn clearing_typography_remounts_zero_height_rows_without_a_kernel() {
    check_clearing_typography(true);
}

fn check_clearing_typography(detached: bool) {
    for property in ["font-size", "line-height"] {
        for plain in ["opacity=1".to_owned(), format!("{property}=\"unset\"")] {
            let source = format!(
                r#"style Shut
  {property}=0
style Plain
  {plain}
component App
  state shut = false
  action flip
    shut = not shut
  view
    column
      button press=flip testId="flip"
        text "flip"
      list class=(shut ? Shut : Plain) virtualized=true estimated-item-height=20 height=100
        for x in range(200) key=x
          text "row"
"#
            );
            let kernel = if detached {
                Kernel::detached()
            } else {
                Kernel::with_monospace()
            };
            let mut r = crate::Runner::boot(
                contract::compile(&source).unwrap(),
                NoData,
                kernel,
                Default::default(),
                "/",
            )
            .unwrap();
            let open = settle_text_rows(&mut r, detached.then_some(20.0));
            assert!(open.total_extent > 0.0 && !open.rows.is_empty());
            r.act("flip", vec![]).unwrap();
            let shut = settle_text_rows(&mut r, detached.then_some(0.0));
            assert_eq!(shut.total_extent, 0.0, "{property}, {plain}");
            assert!(shut.rows.is_empty());
            r.act("flip", vec![]).unwrap();
            let restored = &r.collections()[0];
            assert!(
                !restored.rows.is_empty(),
                "clearing {property} with {plain} must remount a measurable window"
            );
            assert!(restored.rows.iter().all(|row| !row.measured));
            let reopened = settle_text_rows(&mut r, detached.then_some(20.0));
            assert_eq!(reopened.total_extent, open.total_extent);
            assert_eq!(reopened.rows.len(), open.rows.len());
            assert!(reopened
                .rows
                .iter()
                .all(|row| row.measured && row.size > 0.0));
        }
    }
}

fn settle_text_rows(
    r: &mut crate::Runner<NoData>,
    reported_size: Option<f64>,
) -> CollectionSnapshot {
    for _ in 0..100 {
        let snapshot = r.collections()[0].clone();
        if reported_size.is_none() {
            let root = r.kernel().roots()[0];
            r.kernel_mut()
                .compute_layout(root, exact_kernel::Offer::definite(640.0, 320.0))
                .unwrap();
        }
        let mut feedback = runner_feedback(r);
        feedback.port_main = 100.0;
        feedback.measurements = snapshot
            .rows
            .iter()
            .map(|row| RowMeasurement {
                view: row.view,
                epoch: row.epoch,
                size: reported_size
                    .unwrap_or_else(|| r.kernel().node(row.view).unwrap().frame.height as f64),
            })
            .collect();
        r.collection_feedback(feedback).unwrap();
        let after = r.collections()[0].clone();
        if after.revision == snapshot.revision {
            return after;
        }
    }
    panic!("text row measurements did not settle");
}

#[test]
fn logical_copy_omits_control_labels_in_unmounted_rows() {
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let enabled = b.constant(&Value::Bool(true));
    let root = b.node(
        NodeType::List as u8,
        None,
        None,
        0,
        &[binding(
            BindingKind::Prop,
            PropId::Virtualized as u16,
            enabled,
        )],
        &[],
        None,
    );
    let subject = b.constant(&values(128));
    let key = code(&mut b, |a| {
        a.load_item(0);
    });
    let (_, arms) = b.region(RegionKind::Each, Some(root), None, 0, subject, key, 1);
    let row = b.node(NodeType::View as u8, None, Some(arms[0]), 0, &[], &[], None);
    let button = b.constant(&Value::str("button"));
    let control = b.node(
        NodeType::Control as u8,
        Some(row),
        Some(arms[0]),
        0,
        &[binding(BindingKind::Prop, PropId::Type as u16, button)],
        &[],
        None,
    );
    let label = b.constant(&Value::str("Button label"));
    b.node(
        NodeType::Text as u8,
        Some(control),
        Some(arms[0]),
        0,
        &[binding(BindingKind::Prop, PropId::Text as u16, label)],
        &[],
        None,
    );
    let role = b.constant(&Value::str("button"));
    let pressable = b.node(
        NodeType::View as u8,
        Some(row),
        Some(arms[0]),
        1,
        &[binding(
            BindingKind::Prop,
            PropId::AccessibilityRole as u16,
            role,
        )],
        &[],
        None,
    );
    let label = b.constant(&Value::str("Pressable label"));
    b.node(
        NodeType::Text as u8,
        Some(pressable),
        Some(arms[0]),
        0,
        &[binding(BindingKind::Prop, PropId::Text as u16, label)],
        &[],
        None,
    );
    // A text a press acts on is a control; a text's run a press acts on is
    // its paragraph's, as the page selects it.
    let tap = b.code(Asm::new());
    let tap = b.action("tap", &[], &[], tap);
    let pressed = [(exact_plan::EventKind::Press, tap, &[][..])];
    let label = b.constant(&Value::str("Tap to retry"));
    b.node(
        NodeType::Text as u8,
        Some(row),
        Some(arms[0]),
        2,
        &[binding(BindingKind::Prop, PropId::Text as u16, label)],
        &pressed,
        None,
    );
    let text = code(&mut b, |a| {
        a.load_item(0).call(exact_plan::Stdlib::ToString);
    });
    let paragraph = b.node(
        NodeType::Text as u8,
        Some(row),
        Some(arms[0]),
        3,
        &[binding(BindingKind::Prop, PropId::Text as u16, text)],
        &[],
        None,
    );
    let run = b.constant(&Value::str(" more"));
    b.node(
        NodeType::Text as u8,
        Some(paragraph),
        Some(arms[0]),
        0,
        &[binding(BindingKind::Prop, PropId::Text as u16, run)],
        &pressed,
        None,
    );
    let h = Harness::from_parts(b.finish().unwrap(), vec![]);
    let view = h.snapshot().view;
    let mounted = h.snapshot().rows.len();
    assert!(mounted < 128);
    let mut ids = Ids::default();
    let mut u = Update::new(env(&h.plan, &h.slots), &h.sites, &mut ids);
    for policy in [crate::ListTextPolicy::Web, crate::ListTextPolicy::MacOS] {
        let copied = h.tree.list_text(&mut u, view, None, policy).unwrap();
        let rows = |range: std::ops::Range<usize>| {
            range
                .map(|i| format!("{i} more"))
                .collect::<Vec<_>>()
                .join("\n\n")
        };
        assert_eq!(copied, rows(0..128));
        // A native button's title is a paragraph on the web, none on the Mac.
        let web = policy == crate::ListTextPolicy::Web;
        let at = |key, offset| crate::ListTextPosition {
            fixed: None,
            key,
            paragraph: if web { 3 } else { 2 },
            offset,
        };
        let copied = h
            .tree
            .list_text(&mut u, view, Some((at("n:1", 0), at("n:127", 2))), policy)
            .unwrap();
        assert_eq!(copied, format!("1 more\n\n{}\n\n12", rows(2..127)));
    }
    assert_eq!(h.snapshot().rows.len(), mounted, "copy mounts no rows");
    assert!(u.ops.is_empty());
}

/// Text the reader can neither see nor reach is no part of a list's copy,
/// mounted rows or not, as Chrome and AppKit leave it out: a row's text
/// under `visibility: hidden` (a hover card), here or above, unless it
/// computes `visible` again, and `inert` text. Each still counts toward an
/// endpoint's paragraph ordinal, as the host's paragraphs do.
#[test]
fn logical_copy_omits_hidden_and_inert_text() {
    use exact_kernel::StyleId;
    let mut b = PlanBuilder::new(exact_kernel::SCHEMA_DIGEST, 1);
    let enabled = b.constant(&Value::Bool(true));
    let root = b.node(
        NodeType::List as u8,
        None,
        None,
        0,
        &[binding(
            BindingKind::Prop,
            PropId::Virtualized as u16,
            enabled,
        )],
        &[],
        None,
    );
    let subject = b.constant(&values(128));
    let key = code(&mut b, |a| {
        a.load_item(0);
    });
    let (_, arms) = b.region(RegionKind::Each, Some(root), None, 0, subject, key, 1);
    let row = b.node(NodeType::View as u8, None, Some(arms[0]), 0, &[], &[], None);
    let hidden = b.constant(&Value::str("hidden"));
    let visible = b.constant(&Value::str("visible"));
    let visibility = |v| binding(BindingKind::Style, StyleId::Visibility as u16, v);
    let text = |b: &mut PlanBuilder, parent, order, value: &str, style: &[BindingsRow]| {
        let value = b.constant(&Value::str(value));
        let mut bindings = vec![binding(BindingKind::Prop, PropId::Text as u16, value)];
        bindings.extend_from_slice(style);
        b.node(
            NodeType::Text as u8,
            Some(parent),
            Some(arms[0]),
            order,
            &bindings,
            &[],
            None,
        );
    };
    let number = code(&mut b, |a| {
        a.load_item(0).call(exact_plan::Stdlib::ToString);
    });
    b.node(
        NodeType::Text as u8,
        Some(row),
        Some(arms[0]),
        0,
        &[binding(BindingKind::Prop, PropId::Text as u16, number)],
        &[],
        None,
    );
    text(&mut b, row, 1, "Card", &[visibility(hidden)]);
    let card = b.node(
        NodeType::View as u8,
        Some(row),
        Some(arms[0]),
        2,
        &[visibility(hidden)],
        &[],
        None,
    );
    text(&mut b, card, 0, "Again", &[visibility(visible)]);
    text(&mut b, card, 1, "Gone", &[]);
    let inert = b.constant(&Value::Bool(true));
    let blocked = b.node(
        NodeType::View as u8,
        Some(row),
        Some(arms[0]),
        3,
        &[binding(BindingKind::Prop, PropId::Inert as u16, inert)],
        &[],
        None,
    );
    text(&mut b, blocked, 0, "Inert", &[]);
    let h = Harness::from_parts(b.finish().unwrap(), vec![]);
    let view = h.snapshot().view;
    let mounted = h.snapshot().rows.len();
    assert!(mounted < 128);
    let mut ids = Ids::default();
    let mut u = Update::new(env(&h.plan, &h.slots), &h.sites, &mut ids);
    let rows = |range: std::ops::Range<usize>| {
        range
            .map(|i| format!("{i}\n\nAgain"))
            .collect::<Vec<_>>()
            .join("\n\n")
    };
    for policy in [crate::ListTextPolicy::Web, crate::ListTextPolicy::MacOS] {
        let copied = h.tree.list_text(&mut u, view, None, policy).unwrap();
        assert_eq!(copied, rows(0..128), "{policy:?}");
        // "Again" is each row's third paragraph: after the number and the card.
        let at = |key, paragraph, offset| crate::ListTextPosition {
            fixed: None,
            key,
            paragraph,
            offset,
        };
        let copied = h
            .tree
            .list_text(
                &mut u,
                view,
                Some((at("n:1", 0, 0), at("n:127", 2, 3))),
                policy,
            )
            .unwrap();
        assert_eq!(
            copied,
            format!("{}\n\n127\n\nAga", rows(1..127)),
            "{policy:?}"
        );
    }
    assert_eq!(h.snapshot().rows.len(), mounted, "copy mounts no rows");
    assert!(u.ops.is_empty());
}

#[test]
fn logical_text_reads_unmounted_rows_and_wrappers_publish_their_place() {
    let mut h = key_reuse_harness(false);
    let mut items = key_reuse_items(&h);
    for (i, item) in items.iter_mut().enumerate() {
        *item = key_reuse_row(i as f64, &format!("row {i}"));
    }
    h.slots[0] = Value::list(items);
    h.update().unwrap();
    let view = h.snapshot().view;
    let mounted = h.snapshot().rows.len();
    assert!(mounted < 128, "most rows are not mounted");
    let first = h.snapshot().rows[0].clone();
    let wrapper = h.kernel.node(first.view).unwrap();
    assert_eq!(wrapper.props.str(PropId::ListItemKey), Some("n:0"));
    assert_eq!(
        wrapper.props.get(PropId::AccessibilityPosInSet),
        Some(&exact_kernel::PropValue::Int(1))
    );
    assert_eq!(
        wrapper.props.get(PropId::AccessibilitySetSize),
        Some(&exact_kernel::PropValue::Int(128))
    );
    assert_eq!(h.tree.list_index(view, "n:127"), Some(127));
    assert_eq!(h.tree.list_index(view, "n:128"), None);
    let mut ids = Ids::default();
    let mut u = Update::new(env(&h.plan, &h.slots), &h.sites, &mut ids);
    let all = h
        .tree
        .list_text(&mut u, view, None, crate::ListTextPolicy::Web)
        .unwrap();
    assert!(all.starts_with("row 0\n\nrow 1\n\n"), "{}", &all[..40]);
    assert!(all.ends_with("row 127"));
    let position = |key, offset| crate::ListTextPosition {
        fixed: None,
        key,
        paragraph: 0,
        offset,
    };
    let range = h
        .tree
        .list_text(
            &mut u,
            view,
            Some((position("n:126", 2), position("n:1", 3))),
            crate::ListTextPolicy::Web,
        )
        .unwrap();
    // UTF-16 offsets within the first and last paragraphs, in either order.
    assert!(range.starts_with(" 1\n\nrow 2"), "{range}");
    assert!(range.ends_with("row 125\n\nro"), "{range}");
    assert!(u.ops.is_empty(), "reading commits nothing");
    assert_eq!(h.snapshot().rows.len(), mounted);
}
