//! LLP 1060.000 D3: on a host whose text is natural, a text leaf whose
//! direction no node from it up to the root declares takes each
//! paragraph's base from its text. Whether a `direction` is declared above
//! can change while every computed direction stays `ltr`: writing the value
//! it already had, or moving the subtree under or out of a node that says
//! it. Each re-measures the paragraph. Markdown text is never natural.
use exact_kernel::*;

fn patch(id: u32, row: StyleId, value: &str) -> Op {
    let mut s = StyleProps::default();
    s.set_dynamic(row, &StyleValue::Text(value.into())).unwrap();
    Op::SetStyle {
        id,
        patch: Box::new(s),
    }
}

fn children(id: u32, children: &[u32]) -> Op {
    Op::SetChildren {
        id,
        children: children.to_vec(),
    }
}

fn view(id: u32, node_type: NodeType) -> Op {
    Op::CreateView { id, node_type }
}

/// Root 1 holds 2, which says `direction: ltr`, and 3, which says none;
/// 4 under 3 holds the Arabic text 5.
fn tree() -> Kernel {
    let mut k = Kernel::with_monospace();
    assert!(k.set_natural_text(true));
    k.apply(
        0,
        0,
        &[
            view(1, NodeType::View),
            view(2, NodeType::View),
            view(3, NodeType::View),
            view(4, NodeType::View),
            view(5, NodeType::Text),
            patch(2, StyleId::Direction, "ltr"),
            Op::SetProp {
                id: 5,
                prop: PropId::Text,
                value: PropValue::Str("مرحبا بالعالم.".into()),
            },
            children(1, &[2, 3]),
            children(3, &[4]),
            children(4, &[5]),
            Op::AttachRoot { id: 1 },
        ],
    )
    .unwrap();
    k.compute_layout(1, Offer::definite(400., 400.)).unwrap();
    k
}

fn natural(k: &Kernel) -> bool {
    let node = k.node(5).unwrap();
    assert_eq!(node.paragraph().direction, Direction::Ltr);
    assert_eq!(node.is_natural_text(), node.paragraph().natural);
    node.paragraph().natural
}

/// `step` turns natural text off or on and re-measures the paragraph.
fn remeasures(k: &mut Kernel, ops: &[Op], natural_after: bool) {
    let before = k.node(5).unwrap().paragraph_stamp().unwrap();
    k.apply(0, 1, ops).unwrap();
    assert_eq!(natural(k), natural_after);
    let after = k.node(5).unwrap().paragraph_stamp().unwrap();
    assert!(
        !before.same_metrics(&after),
        "the paragraph is measured again"
    );
}

#[test]
fn a_same_value_move_turns_natural_text_off_and_on() {
    let mut k = tree();
    assert!(natural(&k));
    remeasures(&mut k, &[children(3, &[]), children(2, &[4])], false);
    remeasures(&mut k, &[children(2, &[]), children(3, &[4])], true);
}

#[test]
fn writing_the_direction_already_computed_turns_natural_text_off() {
    let mut k = tree();
    remeasures(&mut k, &[patch(3, StyleId::Direction, "ltr")], false);
    remeasures(
        &mut k,
        &[Op::ClearStyle {
            id: 3,
            mask: StyleMask::of(StyleId::Direction),
            written: true,
        }],
        true,
    );
}

#[test]
fn markdown_text_and_a_host_that_did_not_say_are_not_natural() {
    let mut k = tree();
    k.apply(
        0,
        1,
        &[Op::SetProp {
            id: 5,
            prop: PropId::Markup,
            value: PropValue::Str("markdown".into()),
        }],
    )
    .unwrap();
    assert!(!k.node(5).unwrap().paragraph().natural);
    assert!(k.set_natural_text(false));
    k.apply(
        0,
        2,
        &[Op::ClearProp {
            id: 5,
            prop: PropId::Markup,
        }],
    )
    .unwrap();
    assert!(!k.node(5).unwrap().paragraph().natural);
}
