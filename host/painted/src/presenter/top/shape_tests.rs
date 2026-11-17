//! The top layer's reading of authored titles and confirmation rows.
use super::*;

#[test]
fn projected_title_direction_follows_its_text_or_its_own_label() {
    let source = r#"component App
  state blank = ""
  view
    main direction="rtl"
      button id="child" aria-label="Accessible label"
        text "Visible title" direction="ltr"
      button id="empty" aria-label="Fallback label" width=100 height=40
        text blank direction="ltr"
      button id="label" aria-label="Own label" direction="ltr" width=100 height=40
"#;
    let (p, error) = crate::presenter::Presenter::boot_with(
        &crate::test_plan(source),
        (),
        (300., 600.),
        1.,
        std::path::PathBuf::new(),
        crate::presenter::PainterChoice::Cpu,
    )
    .unwrap();
    assert!(error.is_none(), "{error:?}");
    let kernel = p.host.kernel();
    for (id, title, direction) in [
        ("child", "Visible title", Direction::Ltr),
        ("empty", "Fallback label", Direction::Rtl),
        ("label", "Own label", Direction::Ltr),
    ] {
        let node = kernel.node_by_key(kernel.find_by_id(id)[0]).unwrap();
        let row = row(kernel, &node, false);
        assert_eq!(row.title, title, "{id}");
        assert_eq!(row.direction, direction, "{id}");
    }
}

/// The reader crosses the boxes the compiler groups an alert into
/// (`alertPart`) only when each holds what its part does: a `message` box of
/// texts, an `actions` box of buttons.
#[test]
fn the_reader_crosses_an_alerts_part_boxes_only_when_they_hold_their_parts() {
    use exact_kernel::{Kernel, NodeType, Op, PropId, PropValue};
    let set = |id, prop, value: &str| Op::SetProp {
        id,
        prop,
        value: PropValue::Str(value.into()),
    };
    let create = |id, node_type| Op::CreateView { id, node_type };
    // A dialog `d` (1) with `rows`, each a (id, type, alertPart, children);
    // 9 is the cancel, a button that closes it, with its word in 10
    let read = |rows: &[(u32, NodeType, Option<&str>, Vec<u32>)]| {
        let mut ops = vec![create(1, NodeType::View)];
        ops.extend([
            set(1, PropId::SemanticTag, "dialog"),
            set(1, PropId::Id, "d"),
            set(1, PropId::AccessibilityRole, "alertdialog"),
            set(1, PropId::AccessibilityLabel, "Couldn’t Save"),
            create(9, NodeType::Pressable),
            set(9, PropId::Commandfor, "d"),
            set(9, PropId::Command, "close"),
            create(10, NodeType::Text),
            set(10, PropId::Text, "OK"),
            Op::SetChildren {
                id: 9,
                children: vec![10],
            },
        ]);
        for (id, node_type, part, _) in rows {
            ops.push(create(*id, *node_type));
            if *node_type == NodeType::Text {
                ops.push(set(*id, PropId::Text, "The disk is full."));
            }
            if let Some(part) = part {
                ops.push(set(*id, PropId::AlertPart, part));
            }
        }
        for (id, _, _, children) in rows {
            if !children.is_empty() {
                ops.push(Op::SetChildren {
                    id: *id,
                    children: children.clone(),
                });
            }
        }
        let tops: Vec<u32> = rows
            .iter()
            .map(|r| r.0)
            .filter(|id| !rows.iter().any(|r| r.3.contains(id)))
            .collect();
        ops.extend([
            Op::SetChildren {
                id: 1,
                children: tops,
            },
            Op::AttachRoot { id: 1 },
        ]);
        let mut k = Kernel::with_monospace();
        k.apply(0, 1, &ops).unwrap();
        alert(&k, 1, &|_| false, true).map(|plan| match plan {
            Plan::Alert {
                title,
                message,
                rows,
                ..
            } => (
                title,
                message,
                rows.into_iter().map(|r| r.title).collect::<Vec<_>>(),
            ),
            Plan::Menu(_) => unreachable!("an alertdialog is no menu"),
        })
    };
    use NodeType::{Pressable, Text, View};
    let alert = Some((
        Some("Couldn’t Save".into()),
        Some("The disk is full.".into()),
        vec!["OK".to_string()],
    ));
    // The compiler's boxes
    assert_eq!(
        read(&[
            (2, View, Some("message"), vec![3]),
            (3, Text, None, vec![]),
            (4, View, Some("actions"), vec![9])
        ]),
        alert
    );
    // Flat, as an author writes it
    assert!(
        read(&[(3, Text, None, vec![]), (4, View, None, vec![9])]).is_none(),
        "an unmarked box is a row of its own"
    );
    assert_eq!(
        read(&[(3, Text, None, vec![]), (9, Pressable, None, vec![])]),
        alert
    );
    // A button marked `message` is a button, not a box of its words
    assert_eq!(
        read(&[
            (3, Text, None, vec![]),
            (9, Pressable, Some("message"), vec![])
        ]),
        alert
    );
    // A message box holding a box, or an actions box holding a text, is no part
    assert_eq!(
        read(&[
            (2, View, Some("message"), vec![3, 5]),
            (3, Text, None, vec![]),
            (5, View, None, vec![]),
            (4, View, Some("actions"), vec![9])
        ]),
        None
    );
    assert_eq!(
        read(&[
            (3, Text, None, vec![]),
            (4, View, Some("actions"), vec![9, 6]),
            (6, Text, None, vec![])
        ]),
        None
    );
}
