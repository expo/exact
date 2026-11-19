//! An observation's `start` and `end` (LLP 1123 D9), which resolve against the reading box's
//! `direction`: the left and right edges under `ltr`, the right and left under `rtl`.
use super::*;
use crate::style::targets::{ChainEntry, NativeTarget, NativeTargets};

/// `id` across the owner, `border-box`, padding its top and bottom by `family`'s left and right
/// edges and its left and right by its `start` and `end`, with `rows`.
fn sides(id: ViewId, family: &str, rows: &[(StyleId, &'static str)]) -> Op {
    let read = |side: &str| {
        Box::leak(format!("var(--exact-{family}-inset-{side}, 3px)").into_boxed_str())
            as &'static str
    };
    let mut all = vec![
        (StyleId::PositionType, "absolute"),
        (StyleId::Left, "0"),
        (StyleId::Top, "0"),
        (StyleId::Width, "402"),
        (StyleId::Height, "874"),
        (StyleId::BoxSizing, "border-box"),
        (StyleId::PaddingTop, read("left")),
        (StyleId::PaddingBottom, read("right")),
        (StyleId::PaddingLeft, read("start")),
        (StyleId::PaddingRight, read("end")),
    ];
    all.extend_from_slice(rows);
    style(id, vec![], &all)
}

#[test]
fn a_boxs_own_start_and_end_take_its_direction() {
    // Under a 40-point left band a plain box's margins are 48 on the left and 8 on the right:
    // 3, `ltr`, reads them as its start and end, and 4, `rtl`, as its end and start; when 4
    // turns `ltr`, its start is the left again; 5, `rtl`, reads its start alone, the right
    let rtl = [(StyleId::Direction, "rtl")];
    let mut ops = create(3..=5);
    ops.extend([
        sides(3, "layout-margins", &[]),
        sides(4, "layout-margins", &rtl),
    ]);
    let start = [
        (
            StyleId::PaddingTop,
            "var(--exact-layout-margins-inset-start, 3px)",
        ),
        (StyleId::Direction, "rtl"),
    ];
    ops.push(style(5, vec![], &start));
    ops.push(children(2, &[3, 4, 5]));
    let mut k = pro(ops);
    k.set_owner_band(2, Some([62.0, 0.0, 34.0, 40.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(read(&k, 3), [48.0, 8.0, 8.0, 48.0]);
    assert_eq!(read(&k, 4), [48.0, 48.0, 8.0, 8.0]);
    assert_eq!(top(&k, 5), 8.0);
    k.apply(
        0,
        2,
        &[sides(4, "layout-margins", &[(StyleId::Direction, "ltr")])],
    )
    .unwrap();
    lay_out(&mut k);
    assert_eq!(read(&k, 4), [48.0, 8.0, 8.0, 48.0]);
}

#[test]
fn a_targets_start_takes_the_readers_direction_and_a_captures_its_declarers() {
    // The owner's controller has 10, 20, 30 and 40 on its four edges. 3, `rtl`, reads its
    // start as the right, 20; 5 declares `--s` as its own start, and 6 below it, `ltr` of its
    // own, reads `--s` as 5 resolved it: 40 under `ltr`, 20 once 5 turns `rtl`
    let target = NativeTarget {
        id: 1,
        generation: 1,
        insets: Some([10.0, 20.0, 30.0, 40.0]),
    };
    let report = NativeTargets {
        chain: vec![ChainEntry { target, kinds: 0 }],
        relations: Vec::new(),
    };
    let start = "var(--exact-owner-safe-area-inset-start, 7px)";
    let mut ops = create(3..=6);
    ops.push(style(
        3,
        vec![],
        &[(StyleId::PaddingTop, start), (StyleId::Direction, "rtl")],
    ));
    let declared = "--s: var(--exact-owner-safe-area-inset-start, 3px)";
    ops.push(style(5, vec![], &[(StyleId::SafeAreaProperties, declared)]));
    let own = [
        (StyleId::PaddingTop, "var(--s)"),
        (StyleId::Direction, "ltr"),
    ];
    ops.push(style(6, vec![], &own));
    ops.extend([children(2, &[3, 5]), children(5, &[6])]);
    let mut k = pro(ops);
    k.set_owner_targets(2, report).unwrap();
    lay_out(&mut k);
    assert_eq!((top(&k, 3), top(&k, 6)), (20.0, 40.0));
    let turned = [
        (StyleId::SafeAreaProperties, declared),
        (StyleId::Direction, "rtl"),
    ];
    let receipt = k.apply(0, 2, &[style(5, vec![], &turned)]).unwrap();
    // The commit names 6, whose lengths it derived again, for the host to restyle
    assert!(receipt.touched.contains(&k.arena.key(slot(&k, 6))));
    lay_out(&mut k);
    assert_eq!(top(&k, 6), 20.0);
}

#[test]
fn a_capture_follows_an_inherited_direction() {
    // 5, under 3, declares `--s` as its own layout margins' start, and 6 below it, `ltr` of its
    // own, reads it: a full-width box's left margin under the 40-point left band, 48, until 3
    // turns `rtl`, which 5 inherits, and its right, 8
    let mut ops = create(3..=6);
    let declared = "--s: var(--exact-layout-margins-inset-start, 3px)";
    let full = [
        (StyleId::PositionType, "absolute"),
        (StyleId::Left, "0"),
        (StyleId::Top, "0"),
        (StyleId::Width, "402"),
        (StyleId::Height, "874"),
    ];
    let mut five = full.to_vec();
    five.push((StyleId::SafeAreaProperties, declared));
    ops.extend([style(3, vec![], &full), style(5, vec![], &five)]);
    let own = [
        (StyleId::PaddingTop, "var(--s)"),
        (StyleId::Direction, "ltr"),
    ];
    ops.push(style(6, vec![], &own));
    ops.extend([children(2, &[3]), children(3, &[5]), children(5, &[6])]);
    let mut k = pro(ops);
    k.set_owner_band(2, Some([62.0, 0.0, 34.0, 40.0]), &[])
        .unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 6), 48.0);
    let mut turned = full.to_vec();
    turned.push((StyleId::Direction, "rtl"));
    k.apply(0, 2, &[style(3, vec![], &turned)]).unwrap();
    lay_out(&mut k);
    assert_eq!(top(&k, 6), 8.0);
}

#[test]
fn start_and_end_read_back_and_round_trip_on_the_wire() {
    for name in [
        "--exact-keyboard-inset-start",
        "--exact-native-2-safe-area-inset-end",
    ] {
        let text = format!("var({name}, 2px)");
        let parsed = crate::style::Dimension::parse_calc(&text).expect(name);
        let crate::style::Dimension::Compare(c) = parsed else {
            panic!("a tree: {parsed:?}");
        };
        let mut w = crate::wire::codec::Writer::new();
        c.encode(&mut w);
        let bytes = w.into_vec();
        let mut r = crate::wire::codec::Reader::new(&bytes);
        let read = crate::style::calc::Comparison::decode(&mut r, StyleId::PaddingTop);
        assert_eq!(read.ok(), Some(c), "{name}");
    }
    // `middle` is no side, and the box's own safe area takes no `start` or `end`: its name is
    // the local inset, by edge, until activation
    for name in [
        "--exact-keyboard-inset-middle",
        "--exact-safe-area-inset-start",
    ] {
        let text = format!("var({name}, 2px)");
        assert_eq!(crate::style::Dimension::parse_calc(&text), None, "{name}");
    }
}
