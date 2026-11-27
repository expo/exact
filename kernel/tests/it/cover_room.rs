//! A box a native bar keeps the room of (`HostCover::Room`, LLP 1083.000 D4): a root tablist
//! with rows between the panels and it, laid out as the bar's height while what it holds hides.
use crate::cover::{frame, route, style};
use exact_kernel::{HostCover, NodeType, Op, StyleId, StyleValue};

#[test]
fn a_tablist_whose_room_the_bar_keeps_is_the_bars_height_with_the_row_above_it() {
    let mut kernel = route();
    let ops = vec![
        Op::CreateView {
            id: 5,
            node_type: NodeType::View,
        },
        Op::SetStyle {
            id: 5,
            patch: Box::new(style(&[
                (StyleId::Height, StyleValue::Number(60.0)),
                (StyleId::FlexShrink, StyleValue::Number(0.0)),
            ])),
        },
        Op::CreateView {
            id: 6,
            node_type: NodeType::View,
        },
        Op::SetStyle {
            id: 6,
            patch: Box::new(style(&[
                (StyleId::PaddingTop, StyleValue::Number(8.0)),
                (StyleId::BorderStyleTop, StyleValue::Text("solid".into())),
                (StyleId::BorderWidthTop, StyleValue::Number(1.0)),
                (StyleId::MarginTop, StyleValue::Number(4.0)),
            ])),
        },
        Op::CreateView {
            id: 7,
            node_type: NodeType::View,
        },
        Op::SetStyle {
            id: 7,
            patch: Box::new(style(&[(StyleId::Height, StyleValue::Number(49.0))])),
        },
        Op::SetChildren {
            id: 6,
            children: vec![7],
        },
        Op::SetChildren {
            id: 2,
            children: vec![3, 4, 5, 6],
        },
    ];
    kernel.apply(1, 2, &ops).unwrap();
    assert_eq!(frame(&mut kernel, 6), (0.0, 786.0, 390.0, 58.0));
    kernel
        .set_host_cover(6, Some(HostCover::Room(83.0)))
        .unwrap();
    // The bar's height, without the tablist's padding or border, after its margin
    assert_eq!(frame(&mut kernel, 6), (0.0, 761.0, 390.0, 83.0));
    let (_, _, width, height) = frame(&mut kernel, 7);
    assert_eq!(
        (width, height),
        (0.0, 0.0),
        "what the tablist holds takes no space"
    );
    assert_eq!(
        frame(&mut kernel, 5),
        (0.0, 697.0, 390.0, 60.0),
        "the row ends at the room's margin"
    );
    assert_eq!(
        frame(&mut kernel, 4),
        (0.0, 52.0, 390.0, 645.0),
        "and the panels at the row"
    );
    // A box moved in takes no space there, and one moved out takes its space back
    let ops = vec![
        Op::CreateView {
            id: 8,
            node_type: NodeType::View,
        },
        Op::SetStyle {
            id: 8,
            patch: Box::new(style(&[
                (StyleId::Height, StyleValue::Number(30.0)),
                (StyleId::FlexShrink, StyleValue::Number(0.0)),
            ])),
        },
        Op::SetChildren {
            id: 2,
            children: vec![3, 4, 8, 5, 6],
        },
    ];
    kernel.apply(2, 2, &ops).unwrap();
    assert_eq!(frame(&mut kernel, 8).3, 30.0, "laid out beside the room");
    let ops = vec![
        Op::SetChildren {
            id: 6,
            children: vec![8],
        },
        Op::SetChildren {
            id: 2,
            children: vec![3, 4, 7, 5, 6],
        },
    ];
    kernel.apply(3, 2, &ops).unwrap();
    assert_eq!(frame(&mut kernel, 8).3, 0.0, "moved into the room");
    assert_eq!(frame(&mut kernel, 7).3, 49.0, "moved out of it");
    let ops = vec![
        Op::SetChildren {
            id: 6,
            children: vec![7],
        },
        Op::SetChildren {
            id: 2,
            children: vec![3, 4, 5, 6],
        },
    ];
    kernel.apply(4, 2, &ops).unwrap();
    assert!(kernel
        .set_host_cover(6, Some(HostCover::Room(-1.0)))
        .is_err());
    assert!(kernel
        .set_host_cover(6, Some(HostCover::Room(f32::NAN)))
        .is_err());
    kernel.set_host_cover(6, None).unwrap();
    assert_eq!(frame(&mut kernel, 6), (0.0, 786.0, 390.0, 58.0));
    assert_eq!(
        frame(&mut kernel, 7),
        (0.0, 795.0, 390.0, 49.0),
        "and takes it back with the room"
    );
}
