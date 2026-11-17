//! A layout rebuilt after a derived fault keeps a paragraph's stamp.
use super::*;
#[test]
fn derived_layout_fault_rebuild_keeps_authored_paragraph_stamp() {
    let mut k = Kernel::with_monospace();
    k.apply(
        0,
        0,
        &[
            Op::CreateView {
                id: 1,
                node_type: NodeType::Text,
            },
            Op::SetProp {
                id: 1,
                prop: crate::PropId::Text,
                value: "preserve identity".into(),
            },
            Op::AttachRoot { id: 1 },
        ],
    )
    .unwrap();
    let before = k.node(1).unwrap().paragraph_stamp().unwrap();
    k.arena.set_taffy(before.owner().index, None);
    k.compute_layout(1, Offer::definite(300.0, 200.0)).unwrap();
    assert_eq!(before, k.node(1).unwrap().paragraph_stamp().unwrap());
}
