//! Which symbols the neutral look draws at the bar's size (LLP 1115.000
//! D4.15): a binary of its own, because the metrics column is the
//! process's. Whether a header is a bar is decided from what hears `press`,
//! as the web marks `data-exact-bar`; within a bar, a `button` with no
//! handler is an item too, as the neutral sheet selects it.
use exact_kernel::style::metrics::{self, Column};
use exact_kernel::{Kernel, NodeType, Op, PropId, PropValue, ViewId};

/// A route (1) whose header (2) holds a handler-free button (10 + n)
/// around a heading text (20 + n) for each of `headings`, then a pressable
/// (3) whose face is a symbol (4) written 22 by 20.
fn route(headings: u32) -> Kernel {
    let mut k = Kernel::with_monospace();
    let prop = |id, prop, value: &str| Op::SetProp {
        id,
        prop,
        value: PropValue::Str(value.into()),
    };
    let mut ops = vec![];
    let mut header = vec![];
    for n in 0..headings {
        ops.push(Op::CreateView {
            id: 10 + n,
            node_type: NodeType::Pressable,
        });
        ops.push(Op::CreateView {
            id: 20 + n,
            node_type: NodeType::Text,
        });
        ops.push(prop(20 + n, PropId::AccessibilityRole, "heading"));
        ops.push(Op::SetChildren {
            id: 10 + n,
            children: vec![20 + n],
        });
        header.push(10 + n);
    }
    for (id, node_type) in [
        (1, NodeType::View),
        (2, NodeType::View),
        (3, NodeType::Pressable),
        (4, NodeType::Image),
    ] {
        ops.push(Op::CreateView { id, node_type });
    }
    header.push(3);
    ops.extend([
        prop(1, PropId::NavigationKey, "route"),
        prop(2, PropId::SemanticTag, "header"),
        prop(4, PropId::ImageSource, "symbol:circle"),
        Op::SetChildren {
            id: 3,
            children: vec![4],
        },
        Op::SetChildren {
            id: 2,
            children: header,
        },
        Op::SetChildren {
            id: 1,
            children: vec![2],
        },
        Op::AttachRoot { id: 1 },
    ]);
    k.apply(0, 1, &ops).unwrap();
    k
}

#[test]
fn a_header_is_a_bar_by_what_hears_press() {
    metrics::set_column(Column::Neutral);
    let pressed = |id: ViewId| id == 3;
    // One heading in a button no handler presses: a bar, whose pressable's
    // symbol is the bar's.
    let mut k = route(1);
    k.project_bar_symbols(&pressed);
    assert!(k.node(4).unwrap().bar_symbol_size().is_some());
    // Two: no bar, though counting each button as pressed would make the
    // first the title's tap, leave the second's heading unread and see one.
    let mut k = route(2);
    k.project_bar_symbols(&pressed);
    assert_eq!(k.node(4).unwrap().bar_symbol_size(), None);
}

#[test]
fn a_file_backed_icon_keeps_its_authored_geometry_in_a_neutral_bar() {
    metrics::set_column(Column::Neutral);
    let mut k = route(1);
    k.apply(
        0,
        2,
        &[Op::SetProp {
            id: 4,
            prop: PropId::ImageSource,
            value: PropValue::Str("symbol:icon||10 20 40 20||two.svg".into()),
        }],
    )
    .unwrap();
    k.project_bar_symbols(&|id| id == 3);
    assert_eq!(k.node(4).unwrap().bar_symbol_size(), None);
}
