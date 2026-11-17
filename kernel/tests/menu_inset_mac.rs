//! A menu's content inset on the Mac (LLP 1021 D4, expo/exact#551): a
//! `role="menu"` popover whose author wrote no padding is padded as an
//! `NSMenu` insets its items, so a background its author wrote shows round
//! its rows. A binary of its own, because the metrics column is the
//! process's.
use exact_kernel::style::metrics::{self, Column};
use exact_kernel::{
    Kernel, NodeType, Offer, Op, PropId, PropValue, StyleId, StyleMask, StyleProps, StyleValue,
    ViewId,
};

fn prop(id: ViewId, prop: PropId, value: &str) -> Op {
    Op::SetProp {
        id,
        prop,
        value: PropValue::Str(value.into()),
    }
}

/// `rows` as their author wrote them.
fn written(id: ViewId, rows: &[(StyleId, f64)]) -> Op {
    let mut patch = StyleProps::default();
    for (row, value) in rows {
        patch
            .set_dynamic(*row, &StyleValue::Number(*value))
            .unwrap();
    }
    patch.written = patch.mask;
    Op::SetStyle {
        id,
        patch: Box::new(patch),
    }
}

#[test]
fn a_menu_is_inset_as_an_nsmenu_where_its_author_wrote_no_padding() {
    metrics::set_column(Column::Mac);
    let mut k = Kernel::with_monospace();
    let mut ops: Vec<Op> = (1..=3)
        .map(|id| Op::CreateView {
            id,
            node_type: NodeType::View,
        })
        .collect();
    ops.extend([
        prop(2, PropId::Popover, "auto"),
        written(3, &[(StyleId::Width, 100.), (StyleId::Height, 24.)]),
        Op::SetChildren {
            id: 1,
            children: vec![2],
        },
        Op::SetChildren {
            id: 2,
            children: vec![3],
        },
        Op::AttachRoot { id: 1 },
    ]);
    k.apply(0, 1, &ops).unwrap();
    let padding = |k: &mut Kernel| {
        k.compute_layout(1, Offer::definite(480., 640.)).unwrap();
        k.resolved_padding(k.node(2).unwrap().key).unwrap()
    };
    assert_eq!(
        padding(&mut k),
        (0., 0., 0., 0.),
        "a popover that is no menu"
    );

    k.apply(0, 2, &[prop(2, PropId::AccessibilityRole, "menu")])
        .unwrap();
    assert_eq!(padding(&mut k), (5., 5., 5., 5.), "an NSMenu's inset");
    let row = k.node(3).unwrap().frame;
    let menu = k.node(2).unwrap().frame;
    assert_eq!((row.x - menu.x, row.y - menu.y), (5., 5.));
    assert_eq!(
        (menu.width, menu.height),
        (110., 34.),
        "the menu's box holds the inset"
    );

    k.apply(0, 3, &[written(2, &[(StyleId::PaddingLeft, 0.)])])
        .unwrap();
    assert_eq!(
        padding(&mut k),
        (0., 5., 5., 5.),
        "the left side, which its author wrote, is theirs"
    );
    k.apply(
        0,
        4,
        &[Op::ClearStyle {
            id: 2,
            mask: StyleMask::of(StyleId::PaddingLeft),
            written: false,
        }],
    )
    .unwrap();
    assert_eq!(padding(&mut k), (5., 5., 5., 5.));

    k.apply(0, 5, &[prop(2, PropId::AccessibilityRole, "dialog")])
        .unwrap();
    assert_eq!(padding(&mut k), (0., 0., 0., 0.), "no menu, no inset");
}
