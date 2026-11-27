//! LLP 1123 D16: the window's aspect-fit guide lays the root out in its rectangle, at its origin,
//! the viewport and its units staying the offer's.

use super::*;
use crate::StyleValue;

const OFFER: Offer = Offer::definite(400.0, 800.0);

fn style(id: ViewId, rows: &[(StyleId, &str)]) -> Op {
    let mut patch = StyleProps::default();
    for (row, value) in rows {
        patch
            .set_dynamic(*row, &StyleValue::Text((*value).into()))
            .unwrap();
    }
    Op::SetStyle {
        id,
        patch: Box::new(patch),
    }
}

fn app() -> Kernel {
    let mut k = Kernel::new(Box::new(MonospaceMeasurer::default()));
    let ops = [
        Op::CreateView {
            id: 1,
            node_type: NodeType::View,
        },
        style(1, &[(StyleId::Width, "100%"), (StyleId::Height, "100%")]),
        Op::CreateView {
            id: 2,
            node_type: NodeType::View,
        },
        style(2, &[(StyleId::Width, "50%"), (StyleId::Height, "10px")]),
        Op::CreateView {
            id: 3,
            node_type: NodeType::View,
        },
        style(3, &[(StyleId::Width, "50vw"), (StyleId::Height, "10px")]),
        Op::SetChildren {
            id: 1,
            children: vec![2, 3],
        },
        Op::AttachRoot { id: 1 },
    ];
    k.apply(0, 1, &ops).unwrap();
    k
}

fn rect(k: &Kernel, id: ViewId) -> [f32; 4] {
    let f = k.node(id).unwrap().frame;
    [f.x, f.y, f.width, f.height]
}

#[test]
fn a_fitted_root_is_laid_out_in_the_guide_at_its_origin() {
    let mut k = app();
    k.compute_layout(1, OFFER).unwrap();
    assert_eq!(rect(&k, 1), [0.0, 0.0, 400.0, 800.0]);
    assert!(k.set_root_fit(1, Some([30.0, 40.0, 300.0, 200.0])).unwrap());
    assert!(
        !k.set_root_fit(1, Some([30.0, 40.0, 300.0, 200.0])).unwrap(),
        "unchanged"
    );
    let receipt = k.compute_layout(1, OFFER).unwrap();
    assert_eq!(
        rect(&k, 1),
        [30.0, 40.0, 300.0, 200.0],
        "its border box the guide's rectangle"
    );
    assert_eq!(rect(&k, 2)[2], 150.0, "a percentage of the guide");
    assert_eq!(
        rect(&k, 3)[2],
        200.0,
        "a viewport unit of the offer's viewport"
    );
    assert!(receipt.changed.contains(&k.node(1).unwrap().key));
    // The origin alone moves: the root is published there again, its children whole
    assert!(k.set_root_fit(1, Some([50.0, 60.0, 300.0, 200.0])).unwrap());
    let receipt = k.compute_layout(1, OFFER).unwrap();
    assert_eq!(rect(&k, 1), [50.0, 60.0, 300.0, 200.0]);
    assert!(
        receipt.changed.contains(&k.node(1).unwrap().key)
            || receipt.updated.contains(&k.node(1).unwrap().key)
    );
    // Unfitted, the root takes the offer again
    assert!(k.set_root_fit(1, None).unwrap());
    k.compute_layout(1, OFFER).unwrap();
    assert_eq!(rect(&k, 1), [0.0, 0.0, 400.0, 800.0]);
}

#[test]
fn only_a_root_takes_a_finite_positive_guide_and_a_replaced_root_none() {
    let mut k = app();
    for bad in [
        [0.0, 0.0, 0.0, 10.0],
        [0.0, 0.0, 10.0, -1.0],
        [f32::NAN, 0.0, 10.0, 10.0],
        [0.0, f32::INFINITY, 10.0, 10.0],
    ] {
        assert!(k.set_root_fit(1, Some(bad)).is_err(), "{bad:?}");
    }
    assert!(
        k.set_root_fit(2, Some([0.0, 0.0, 10.0, 10.0])).is_err(),
        "not a root"
    );
    k.set_root_fit(1, Some([30.0, 40.0, 300.0, 200.0])).unwrap();
    // A root made again under the same id is another, which the fit does not reach
    k.apply(
        0,
        2,
        &[
            Op::DestroyView { id: 1 },
            Op::CreateView {
                id: 1,
                node_type: NodeType::View,
            },
            style(1, &[(StyleId::Width, "100%"), (StyleId::Height, "100%")]),
            Op::AttachRoot { id: 1 },
        ],
    )
    .unwrap();
    k.compute_layout(1, OFFER).unwrap();
    assert_eq!(rect(&k, 1), [0.0, 0.0, 400.0, 800.0]);
}

#[test]
fn an_unsized_roots_border_box_is_the_guide() {
    // A root with no size of its own fills its offer's width and takes its content's height; fitted, its
    // border box is the guide's rectangle whatever it says
    let mut k = Kernel::new(Box::new(MonospaceMeasurer::default()));
    k.apply(
        0,
        1,
        &[
            Op::CreateView {
                id: 1,
                node_type: NodeType::View,
            },
            Op::CreateView {
                id: 2,
                node_type: NodeType::View,
            },
            style(2, &[(StyleId::Width, "20px"), (StyleId::Height, "10px")]),
            Op::SetChildren {
                id: 1,
                children: vec![2],
            },
            Op::AttachRoot { id: 1 },
        ],
    )
    .unwrap();
    k.compute_layout(1, OFFER).unwrap();
    assert_eq!(rect(&k, 1), [0.0, 0.0, 400.0, 10.0]);
    k.set_root_fit(1, Some([30.0, 40.0, 300.0, 200.0])).unwrap();
    k.compute_layout(1, OFFER).unwrap();
    assert_eq!(rect(&k, 1), [30.0, 40.0, 300.0, 200.0]);
    k.set_root_fit(1, None).unwrap();
    k.compute_layout(1, OFFER).unwrap();
    assert_eq!(rect(&k, 1), [0.0, 0.0, 400.0, 10.0], "its own again");
}
