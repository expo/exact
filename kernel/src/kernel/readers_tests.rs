//! The reader index (LLP 1123 D9 "Cost", D5 "Captures"): a change reaches
//! the boxes that read it, through captures and the declarations that read
//! them, and the values the readers share follow what they depend on.
use super::super::owner::tests::{children, create, style};
use super::*;
use crate::id::Offer;
use crate::wire::Op;
use crate::{Kernel, StyleId, ViewId};

const TOP: StyleId = StyleId::PaddingTop;
const PROPERTIES: StyleId = StyleId::SafeAreaProperties;

/// `ops` applied to a new tree rooted at 1, 844 points tall.
fn kernel(ops: Vec<Op>) -> Kernel {
    let mut k = Kernel::with_monospace();
    let mut ops = ops;
    ops.push(style(1, &[(StyleId::Height, "844")]));
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    k
}

fn y(k: &mut Kernel, id: ViewId) -> f32 {
    k.compute_layout(1, Offer::definite(390.0, 844.0)).unwrap();
    k.node(id).unwrap().frame.y
}

#[test]
fn a_moved_declaration_reads_its_new_parents() {
    // 2 declares --a 8, 3 declares --a 20; 4, in 2, declares --b as twice
    // --a; 5, in 4, pads by --b; 6 in 5
    let mut ops = create(1..=6);
    ops.push(style(2, &[(PROPERTIES, "--a: 8px")]));
    ops.push(style(3, &[(PROPERTIES, "--a: 20px")]));
    ops.push(style(4, &[(PROPERTIES, "--b: calc(var(--a) * 2)")]));
    ops.push(style(5, &[(TOP, "var(--b)")]));
    ops.push(style(6, &[(StyleId::Height, "10")]));
    ops.extend([
        children(1, &[2, 3]),
        children(2, &[4]),
        children(4, &[5]),
        children(5, &[6]),
    ]);
    let mut k = kernel(ops);
    assert_eq!(y(&mut k, 6), 16.0);
    k.apply(0, 2, &[children(2, &[]), children(3, &[4])])
        .unwrap();
    assert_eq!(y(&mut k, 6), 40.0, "--b is resolved where it is declared");
}

#[test]
fn a_capture_follows_an_override_above_it() {
    // 2 an owner (inset 100); 3 overrides its top; 4 captures it; 5 pads by
    // the capture; 6 in 5
    let mut ops = create(1..=6);
    ops.push(style(3, &[(StyleId::LocalInsetTop, "40")]));
    ops.push(style(
        4,
        &[(PROPERTIES, "--top: var(--exact-safe-area-inset-top)")],
    ));
    ops.push(style(5, &[(TOP, "var(--top)")]));
    ops.push(style(6, &[(StyleId::Height, "10")]));
    ops.extend([
        children(1, &[2]),
        children(2, &[3]),
        children(3, &[4]),
        children(4, &[5]),
        children(5, &[6]),
    ]);
    let mut k = kernel(ops);
    k.set_local_insets(2, Some([100.0, 0.0, 0.0, 0.0])).unwrap();
    assert_eq!(y(&mut k, 6), 40.0);
    k.apply(0, 2, &[style(3, &[(StyleId::LocalInsetTop, "25")])])
        .unwrap();
    assert_eq!(y(&mut k, 6), 25.0);
}

#[test]
fn an_inset_reaches_a_reader_through_a_declaration_below_its_capture() {
    // 2 an owner that captures its top; 3 declares twice the capture; 4
    // pads by that; 5 in 4
    let mut ops = create(1..=5);
    ops.push(style(
        2,
        &[(PROPERTIES, "--top: var(--exact-safe-area-inset-top)")],
    ));
    ops.push(style(3, &[(PROPERTIES, "--twice: calc(var(--top) * 2)")]));
    ops.push(style(4, &[(TOP, "var(--twice)")]));
    ops.push(style(5, &[(StyleId::Height, "10")]));
    ops.extend([
        children(1, &[2]),
        children(2, &[3]),
        children(3, &[4]),
        children(4, &[5]),
    ]);
    let mut k = kernel(ops);
    k.set_local_insets(2, Some([10.0, 0.0, 0.0, 0.0])).unwrap();
    assert_eq!(y(&mut k, 5), 20.0);
    assert!(k.set_local_insets(2, Some([15.0, 0.0, 0.0, 0.0])).unwrap());
    assert_eq!(y(&mut k, 5), 30.0);
    // The capture removed: --twice reads nothing defined, and no inset reaches 4
    let clear = Op::ClearStyle {
        id: 2,
        mask: crate::StyleMask::of(PROPERTIES),
        written: false,
    };
    k.apply(0, 2, &[clear]).unwrap();
    assert_eq!(y(&mut k, 5), 0.0);
    assert!(!k.set_local_insets(2, Some([25.0, 0.0, 0.0, 0.0])).unwrap());
}

#[test]
fn an_inset_reaches_no_reader_under_a_nested_owner_with_its_own() {
    // 2 an owner; 3 an owner in it; 4, in 3, pads by its local inset; 5 in 4
    let mut ops = create(1..=5);
    ops.push(style(4, &[(TOP, "var(--exact-safe-area-inset-top)")]));
    ops.push(style(5, &[(StyleId::Height, "10")]));
    ops.extend([
        children(1, &[2]),
        children(2, &[3]),
        children(3, &[4]),
        children(4, &[5]),
    ]);
    let mut k = kernel(ops);
    k.set_local_insets(3, Some([30.0, 0.0, 0.0, 0.0])).unwrap();
    assert!(
        !k.set_local_insets(2, Some([100.0, 0.0, 0.0, 0.0])).unwrap(),
        "4 reads 3's insets, so nothing reads 2's"
    );
    assert_eq!(y(&mut k, 5), 30.0);
    // Without its own, 3's subtree reads 2's
    assert!(k.set_local_insets(3, None).unwrap());
    assert_eq!(y(&mut k, 5), 100.0);
}

#[test]
fn an_override_change_reaches_a_captures_reader_below_another_override() {
    // 2 overrides its top with 40; 3 captures it; 4 overrides its own top
    // with 5; 5 pads by the capture; 6 in 5
    let mut ops = create(1..=6);
    ops.push(style(2, &[(StyleId::LocalInsetTop, "40")]));
    ops.push(style(
        3,
        &[(PROPERTIES, "--p: var(--exact-safe-area-inset-top)")],
    ));
    ops.push(style(4, &[(StyleId::LocalInsetTop, "5")]));
    ops.push(style(5, &[(TOP, "var(--p)")]));
    ops.push(style(6, &[(StyleId::Height, "10")]));
    ops.extend([
        children(1, &[2]),
        children(2, &[3]),
        children(3, &[4]),
        children(4, &[5]),
        children(5, &[6]),
    ]);
    let mut k = kernel(ops);
    assert_eq!(y(&mut k, 6), 40.0);
    let receipt = k
        .apply(0, 2, &[style(2, &[(StyleId::LocalInsetTop, "25")])])
        .unwrap();
    let key = |k: &Kernel, id| k.arena.key(k.arena.slot_of(id).unwrap());
    assert!(
        receipt.touched.contains(&key(&k, 5)),
        "a host restyles what the receipt names"
    );
    assert_eq!(
        y(&mut k, 6),
        25.0,
        "4's override stops inheritance, not 3's value"
    );
}

#[test]
fn an_override_in_em_reaches_a_captures_reader_when_its_font_size_changes() {
    // 2, 10 points to an em, overrides its top with 2em; 3 captures it; 4
    // overrides its own top with 5; 5 pads by the capture; 6 in 5
    let mut ops = create(1..=6);
    ops.push(style(
        2,
        &[(StyleId::FontSize, "10px"), (StyleId::LocalInsetTop, "2em")],
    ));
    ops.push(style(
        3,
        &[(PROPERTIES, "--p: var(--exact-safe-area-inset-top)")],
    ));
    ops.push(style(4, &[(StyleId::LocalInsetTop, "5")]));
    ops.push(style(5, &[(TOP, "var(--p)")]));
    ops.push(style(6, &[(StyleId::Height, "10")]));
    ops.extend([
        children(1, &[2]),
        children(2, &[3]),
        children(3, &[4]),
        children(4, &[5]),
        children(5, &[6]),
    ]);
    let mut k = kernel(ops);
    assert_eq!(y(&mut k, 6), 20.0);
    k.apply(0, 2, &[style(2, &[(StyleId::FontSize, "15px")])])
        .unwrap();
    assert_eq!(y(&mut k, 6), 30.0);
}

#[test]
fn an_override_change_derives_no_reader_behind_a_nearer_override() {
    // 2 overrides its top with 10; 3 overrides its own with 7 and pads by
    // it; 4 in 3
    let mut ops = create(1..=4);
    ops.push(style(2, &[(StyleId::LocalInsetTop, "10")]));
    ops.push(style(
        3,
        &[
            (StyleId::LocalInsetTop, "7"),
            (TOP, "var(--exact-safe-area-inset-top)"),
        ],
    ));
    ops.push(style(4, &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
    let mut k = kernel(ops);
    assert_eq!(y(&mut k, 4), 7.0);
    let receipt = k
        .apply(0, 2, &[style(2, &[(StyleId::LocalInsetTop, "20")])])
        .unwrap();
    let three = k.arena.key(k.arena.slot_of(3).unwrap());
    assert!(!receipt.touched.contains(&three), "3's input stays 7");
    assert_eq!(y(&mut k, 4), 7.0);
}

#[test]
fn a_reparent_between_overrides_reads_the_new_parents_insets() {
    // 2 and 3 each override their top with their left, 8 and 20; 4, in 2,
    // overrides its left with 1 and pads by its top; 5 in 4. No host sets
    // an inset, and 4 moves to 3 without leaving 2 first.
    let mut ops = create(1..=5);
    let top = "var(--exact-safe-area-inset-left)";
    ops.push(style(
        2,
        &[
            (StyleId::LocalInsetTop, top),
            (StyleId::LocalInsetLeft, "8"),
        ],
    ));
    ops.push(style(
        3,
        &[
            (StyleId::LocalInsetTop, top),
            (StyleId::LocalInsetLeft, "20"),
        ],
    ));
    ops.push(style(
        4,
        &[
            (StyleId::LocalInsetLeft, "1"),
            (TOP, "var(--exact-safe-area-inset-top)"),
        ],
    ));
    ops.push(style(5, &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2, 3]), children(2, &[4]), children(4, &[5])]);
    let mut k = kernel(ops);
    assert_eq!(y(&mut k, 5), 8.0);
    k.apply(0, 2, &[children(3, &[4])]).unwrap();
    assert_eq!(y(&mut k, 5), 20.0);
}

#[test]
fn a_root_font_change_reaches_a_reader_below_an_override_in_rem() {
    // The root overrides its top with 1rem; 2, whose own font size is
    // fixed, pads by its top; 3 in 2
    let mut ops = create(1..=3);
    ops.push(style(
        2,
        &[
            (StyleId::FontSize, "16px"),
            (TOP, "var(--exact-safe-area-inset-top)"),
        ],
    ));
    ops.push(style(3, &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    ops.push(style(1, &[(StyleId::LocalInsetTop, "calc(1rem + 0px)")]));
    let mut k = kernel(ops);
    assert_eq!(y(&mut k, 3), 16.0);
    k.set_root_font_size(20.0).unwrap();
    k.apply(0, 2, &[]).unwrap();
    assert_eq!(y(&mut k, 3), 20.0);
}

#[test]
fn an_inset_change_derives_no_reader_whose_input_stays() {
    // 2 an owner; 3 overrides its top with 7 and pads by it; 4 pads by its
    // bottom; 5 pads by a declared property whose fallback reads the top; 6
    // declares a capture whose top is in a fallback not taken, which 7 pads
    // by; 8 pads by a declared property whose fallback is a capture, and 9
    // by a declaration whose only read of the capture is such a fallback;
    // 10 reads no capture beside an `env()` with a fallback; 11 and 12 read
    // one only in a fallback their text leaves unused; 13 to 15 read
    // declarations of the capture's name that are invalid: one reading
    // itself, two reading each other and one reading a missing property,
    // and 16 reads a capture that is invalid itself
    let mut ops = create(1..=16);
    let reads = |edge| format!("var(--exact-safe-area-inset-{edge})");
    ops.push(style(2, &[(PROPERTIES, "--x: 3px")]));
    ops.push(style(
        3,
        &[(StyleId::LocalInsetTop, "7"), (TOP, &reads("top"))],
    ));
    ops.push(style(4, &[(TOP, &reads("bottom"))]));
    ops.push(style(5, &[(TOP, &format!("var(--x, {})", reads("top")))]));
    ops.push(style(
        6,
        &[(
            PROPERTIES,
            "--fixed: 3px; --held: var(--fixed, var(--exact-safe-area-inset-top)); --top: var(--exact-safe-area-inset-top); --alias: var(--fixed, var(--top)); --own: var(--own, var(--top)); --a: var(--b); --b: var(--a, var(--top)); --gone: calc(var(--missing) + var(--top)); --bad: calc(var(--exact-safe-area-inset-top) + var(--missing))",
        )],
    ));
    ops.push(style(7, &[(TOP, "var(--held)")]));
    ops.push(style(8, &[(TOP, "var(--fixed, var(--top))")]));
    ops.push(style(9, &[(TOP, "var(--alias)")]));
    ops.push(style(
        10,
        &[
            (TOP, "var(--fixed)"),
            (StyleId::Width, "env(safe-area-inset-left, 100px)"),
        ],
    ));
    ops.push(style(
        11,
        &[(
            TOP,
            "calc(var(--fixed, var(--top)) + env(safe-area-inset-left, 100px))",
        )],
    ));
    ops.push(style(
        12,
        &[(StyleId::Width, "env(safe-area-inset-left, var(--top))")],
    ));
    for (id, name) in [(13, "own"), (14, "a"), (15, "gone"), (16, "bad")] {
        ops.push(style(id, &[(TOP, &format!("var(--{name}, 7px)"))]));
    }
    ops.extend([
        children(1, &[2]),
        children(2, &[3, 4, 5, 6]),
        children(6, &[7, 8, 9, 10, 11, 12, 13, 14, 15, 16]),
    ]);
    let mut k = kernel(ops);
    k.set_local_insets(2, Some([10.0, 0.0, 0.0, 0.0])).unwrap();
    y(&mut k, 3);
    assert!(
        !k.set_local_insets(2, Some([20.0, 0.0, 0.0, 0.0])).unwrap(),
        "an override, another edge and fallbacks not taken, in a reader or a capture"
    );
    assert!(k.set_local_insets(2, Some([20.0, 0.0, 6.0, 0.0])).unwrap());
    y(&mut k, 4);
    assert_eq!(k.node(4).unwrap().frame.height, 6.0);
}

#[test]
fn a_capture_below_a_capture_follows_each_owner() {
    // 2 an owner (inset 100) captures --a; 3 an owner in it (inset 30)
    // declares --b as --a plus its own top; 4 pads by --b; 5 in 4
    let mut ops = create(1..=5);
    ops.push(style(
        2,
        &[(PROPERTIES, "--a: var(--exact-safe-area-inset-top)")],
    ));
    ops.push(style(
        3,
        &[(
            PROPERTIES,
            "--b: calc(var(--a) + var(--exact-safe-area-inset-top))",
        )],
    ));
    ops.push(style(4, &[(TOP, "var(--b)")]));
    ops.push(style(5, &[(StyleId::Height, "10")]));
    ops.extend([
        children(1, &[2]),
        children(2, &[3]),
        children(3, &[4]),
        children(4, &[5]),
    ]);
    let mut k = kernel(ops);
    k.set_local_insets(2, Some([100.0, 0.0, 0.0, 0.0])).unwrap();
    k.set_local_insets(3, Some([30.0, 0.0, 0.0, 0.0])).unwrap();
    assert_eq!(y(&mut k, 5), 130.0);
    assert!(k.set_local_insets(2, Some([90.0, 0.0, 0.0, 0.0])).unwrap());
    assert_eq!(y(&mut k, 5), 120.0);
    assert!(k.set_local_insets(3, Some([10.0, 0.0, 0.0, 0.0])).unwrap());
    assert_eq!(y(&mut k, 5), 100.0);
}

#[test]
fn kept_values_follow_a_reader_and_a_declaration_through_their_lives() {
    // 2 declares --a; 3 in it reads it as a length, a local inset or a
    // literal in turn; 4 in 3
    let mut ops = create(1..=4);
    ops.push(style(2, &[(PROPERTIES, "--a: 8px")]));
    ops.push(style(3, &[(TOP, "var(--a)")]));
    ops.push(style(4, &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
    let mut k = kernel(ops);
    assert_eq!(y(&mut k, 4), 8.0);
    k.apply(
        0,
        2,
        &[style(3, &[(TOP, "var(--exact-safe-area-inset-top)")])],
    )
    .unwrap();
    k.set_local_insets(2, Some([12.0, 0.0, 0.0, 0.0])).unwrap();
    assert_eq!(y(&mut k, 4), 12.0);
    k.apply(0, 3, &[style(3, &[(TOP, "5px")])]).unwrap();
    assert_eq!(y(&mut k, 4), 5.0);
    k.apply(0, 4, &[style(3, &[(TOP, "var(--a)")])]).unwrap();
    assert_eq!(y(&mut k, 4), 8.0);
    // The reader's row cleared, then read again
    let clear = |id, row| Op::ClearStyle {
        id,
        mask: crate::StyleMask::of(row),
        written: false,
    };
    k.apply(0, 40, &[clear(3, TOP)]).unwrap();
    assert_eq!(y(&mut k, 4), 0.0);
    k.apply(0, 41, &[style(3, &[(TOP, "var(--a, 2px)")])])
        .unwrap();
    assert_eq!(y(&mut k, 4), 8.0);
    // The declaration removed, so the fallback, and declared again
    k.apply(0, 42, &[clear(2, PROPERTIES)]).unwrap();
    assert_eq!(y(&mut k, 4), 2.0);
    k.apply(0, 43, &[style(2, &[(PROPERTIES, "--a: 8px")])])
        .unwrap();
    k.apply(0, 44, &[style(3, &[(TOP, "var(--a)")])]).unwrap();
    assert_eq!(y(&mut k, 4), 8.0);
    // A cycle warmed, broken and formed again
    k.apply(
        0,
        5,
        &[style(2, &[(PROPERTIES, "--a: calc(var(--a) + 1px)")])],
    )
    .unwrap();
    assert_eq!(y(&mut k, 4), 0.0);
    k.apply(0, 6, &[style(2, &[(PROPERTIES, "--a: 9px")])])
        .unwrap();
    assert_eq!(y(&mut k, 4), 9.0);
    k.apply(0, 7, &[style(2, &[(PROPERTIES, "--a: var(--a)")])])
        .unwrap();
    assert_eq!(y(&mut k, 4), 0.0);
    // Destroyed, then a new declaring node in the freed slot
    let slot = k.arena.slot_of(2).unwrap();
    k.apply(0, 8, &[children(1, &[]), Op::DestroyView { id: 2 }])
        .unwrap();
    let mut ops = create([12, 13, 14]);
    ops.push(style(12, &[(PROPERTIES, "--a: 20px")]));
    ops.push(style(13, &[(TOP, "var(--a)")]));
    ops.push(style(14, &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[12]), children(12, &[13]), children(13, &[14])]);
    k.apply(0, 9, &ops).unwrap();
    assert!(
        k.arena.slot_of(14) == Some(slot)
            || k.arena.slot_of(12) == Some(slot)
            || k.arena.slot_of(13) == Some(slot),
        "a freed slot is reused"
    );
    assert_eq!(y(&mut k, 14), 20.0);
    // A reset, and the tree built again
    k.reset();
    let mut ops = create(1..=3);
    ops.push(style(
        1,
        &[(StyleId::Height, "844"), (PROPERTIES, "--a: 6px")],
    ));
    ops.push(style(2, &[(TOP, "var(--a)")]));
    ops.push(style(3, &[(StyleId::Height, "10")]));
    ops.extend([
        children(1, &[2]),
        children(2, &[3]),
        Op::AttachRoot { id: 1 },
    ]);
    k.apply(0, 1, &ops).unwrap();
    assert_eq!(y(&mut k, 3), 6.0);
}

#[test]
fn a_clone_keeps_values_of_its_own() {
    let mut ops = create(1..=3);
    ops.push(style(2, &[(PROPERTIES, "--a: 8px")]));
    ops.push(style(3, &[(TOP, "var(--a)")]));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    let mut k = kernel(ops);
    y(&mut k, 3);
    let mut copy = k.arena.clone();
    let (two, three) = (k.arena.slot_of(2).unwrap(), k.arena.slot_of(3).unwrap());
    let mut patch = StyleProps::default();
    patch
        .set_dynamic(PROPERTIES, &crate::StyleValue::Text("--a: 20px".into()))
        .unwrap();
    copy.patch_style(two, &patch);
    let key = (two, Arc::from("--a"));
    assert!(
        copy.readers.value(&key).is_none(),
        "the copy's declaration changed"
    );
    assert!(
        k.arena.readers.value(&key).is_some(),
        "the original keeps its own"
    );
    let read = |arena: &NodeArena| {
        let resolved = owner::properties_at(arena, three, arena.style(three));
        resolved.get("--a").cloned()
    };
    assert_ne!(read(&copy), read(&k.arena));
}

/// The walk down from a changed declaration and the walk up from each
/// reader find the same readers, over generated trees: declarations that
/// read one another, shadow one another or form cycles, and readers with
/// and without fallbacks.
#[test]
fn the_walks_down_and_up_find_the_same_readers() {
    const NAMES: [&str; 4] = ["a", "b", "c", "d"];
    let mut seed = 0x9e37_79b9_u64;
    let mut next = |n: u64| {
        seed = seed
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        (seed >> 33) % n
    };
    let (mut found, mut members) = (0, 0);
    for _ in 0..40 {
        let count = 30;
        let mut ops = create(1..=count);
        let mut kids: Vec<Vec<ViewId>> = vec![Vec::new(); count as usize + 1];
        for id in 2..=count {
            let parent = 1 + next(u64::from(id - 1)) as ViewId;
            kids[parent as usize].push(id);
        }
        for id in 1..=count {
            if next(3) == 0 {
                let mut declared = Vec::new();
                for _ in 0..=next(2) {
                    let name = NAMES[next(4) as usize];
                    let value = match next(3) {
                        0 => format!("{}px", next(9)),
                        _ => format!("calc(var(--{}) + 1px)", NAMES[next(4) as usize]),
                    };
                    declared.push(format!("--{name}: {value}"));
                }
                ops.push(style(id, &[(PROPERTIES, &declared.join("; "))]));
            }
            if next(3) == 0 {
                let read = match next(2) {
                    0 => format!("var(--{})", NAMES[next(4) as usize]),
                    _ => format!("var(--{}, 2px)", NAMES[next(4) as usize]),
                };
                ops.push(style(id, &[(StyleId::MarginTop, &read)]));
            }
        }
        for (id, list) in kids.iter().enumerate().skip(1) {
            if !list.is_empty() {
                ops.push(children(id as ViewId, list));
            }
        }
        // Some nodes read a local inset or override one, and some are owners
        for id in 2..=count {
            match next(6) {
                0 => ops.push(style(
                    id,
                    &[(StyleId::MarginLeft, "var(--exact-safe-area-inset-top)")],
                )),
                1 => ops.push(style(
                    id,
                    &[(StyleId::LocalInsetTop, "var(--exact-safe-area-inset-left)")],
                )),
                _ => {}
            }
        }
        let mut k = Kernel::with_monospace_on_demand();
        ops.push(Op::AttachRoot { id: 1 });
        k.apply(0, 1, &ops).unwrap();
        for id in 2..=count {
            if next(5) == 0 {
                k.set_local_insets(id, Some([1.0, 2.0, 3.0, 4.0])).unwrap();
            }
        }
        let arena = &k.arena;
        for id in 1..=count {
            let slot = arena.slot_of(id).unwrap();
            let mut down = region_within(arena, slot, usize::MAX);
            let up = region_within(arena, slot, 0);
            down.0.sort_unstable();
            down.1.sort_unstable();
            assert_eq!(down, up, "the region at node {id}");
            members += up.0.len() + up.1.len();
        }
        for id in 1..=count {
            let slot = arena.slot_of(id).unwrap();
            let changed: Vec<Arc<str>> = (NAMES.iter())
                .filter(|_| next(2) == 0)
                .map(|n| Arc::from(format!("--{n}")))
                .collect();
            if changed.is_empty() {
                continue;
            }
            let down = declared_at(arena, slot, &changed).down(arena, usize::MAX);
            let up = declared_at(arena, slot, &changed).up(arena);
            let mut down = down.expect("an unbounded walk ends");
            down.sort_unstable();
            assert_eq!(down, up, "node {id}, {changed:?}");
            found += up.len();
            let mut chosen = of_declarations(arena, slot, changed);
            chosen.sort_unstable();
            assert_eq!(chosen, up);
        }
    }
    assert!(
        found > 100,
        "the trees hold readers of what changes: {found}"
    );
    assert!(members > 100, "the regions hold readers: {members}");
}
