//! Custom properties in a tree (LLP 1123 D5): declared on a node, read in
//! its subtree's lengths, captured from an owner's inset.
use super::tests::{children, create, style};
use super::*;
use crate::id::Offer;
use crate::wire::Op;
use crate::{NodeType, StyleId, StyleMask};

const TOP: StyleId = StyleId::PaddingTop;
const PROPERTIES: StyleId = StyleId::SafeAreaProperties;

/// 1 the root, 2 a box, 3 a box in 2 that pads its top by `pad`, 4 a box in 3.
fn tree(declared: &str, pad: &str) -> Kernel {
    let mut k = Kernel::with_monospace();
    let mut ops = create(1..=4);
    ops.push(style(
        1,
        &[(StyleId::Height, "844"), (PROPERTIES, declared)],
    ));
    ops.push(style(3, &[(TOP, pad)]));
    ops.push(style(4, &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4])]);
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    k
}

fn y(k: &mut Kernel, id: ViewId) -> f32 {
    k.compute_layout(1, Offer::definite(390.0, 844.0)).unwrap();
    k.node(id).unwrap().frame.y
}

#[test]
fn a_descendant_pads_by_a_property_its_ancestor_declares() {
    let mut k = tree("--gap: 8px; --pad: calc(var(--gap) * 2)", "var(--pad)");
    assert_eq!(y(&mut k, 4), 16.0);
    // A change to the declaration reaches the reader below it
    k.apply(
        0,
        2,
        &[style(
            1,
            &[(PROPERTIES, "--gap: 10px; --pad: calc(var(--gap) * 3)")],
        )],
    )
    .unwrap();
    assert_eq!(y(&mut k, 4), 30.0);
    // A declaration between them adds to the inherited ones, and shadows its own name
    k.apply(0, 3, &[style(2, &[(PROPERTIES, "--pad: 4px")])])
        .unwrap();
    assert_eq!(y(&mut k, 4), 4.0);
}

#[test]
fn a_fallback_stands_in_for_an_undefined_or_invalid_property() {
    let mut k = tree("--a: var(--b); --b: var(--a)", "var(--a, 7px)");
    assert_eq!(y(&mut k, 4), 7.0, "a cycle is guaranteed-invalid");
    let mut k = tree("--gap: 8px", "calc(var(--missing, 2px) + var(--gap))");
    assert_eq!(y(&mut k, 4), 10.0);
    // Without one, the length is invalid at computed-value time: its initial value
    let mut k = tree("--gap: 8px", "var(--missing)");
    assert_eq!(y(&mut k, 4), 0.0);
}

#[test]
fn a_capture_hands_an_outer_owners_inset_to_a_nested_owner() {
    // 1 root, 2 an outer owner (inset 100) that captures its top, 3 an inner
    // owner (inset 30) in it, 4 a box in 3 that pads by the capture, 5 in 4
    let mut k = Kernel::with_monospace();
    let mut ops = create(1..=5);
    ops.push(style(1, &[(StyleId::Height, "844")]));
    ops.push(style(
        2,
        &[(PROPERTIES, "--outer-top: var(--exact-safe-area-inset-top)")],
    ));
    ops.push(style(4, &[(TOP, "var(--outer-top)")]));
    ops.push(style(5, &[(StyleId::Height, "10")]));
    ops.extend([
        children(1, &[2]),
        children(2, &[3]),
        children(3, &[4]),
        children(4, &[5]),
    ]);
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    k.set_local_insets(2, Some([100.0, 0.0, 0.0, 0.0])).unwrap();
    k.set_local_insets(3, Some([30.0, 0.0, 0.0, 0.0])).unwrap();
    assert_eq!(
        y(&mut k, 5),
        100.0,
        "the outer owner's, not the nearest one's 30"
    );
    // The capture follows the outer owner's inset
    assert!(k.set_local_insets(2, Some([64.0, 0.0, 0.0, 0.0])).unwrap());
    assert_eq!(y(&mut k, 5), 64.0);
}

#[test]
fn a_property_with_a_value_is_defined_whatever_its_fallback_reads() {
    // The fallback reads a title-bar area the document does not have
    let mut k = tree("--gap: 8px", "var(--gap, env(titlebar-area-height))");
    assert_eq!(y(&mut k, 4), 8.0);
    let mut k = tree("--other: 8px", "var(--gap, env(titlebar-area-height))");
    assert_eq!(y(&mut k, 4), 0.0, "the fallback taken is undefined");
}

#[test]
fn an_env_with_a_fallback_reads_a_property_beside_it() {
    // `env()` with a fallback is held as text, substituted per node
    let mut k = tree(
        "--gap: 8px",
        "calc(env(safe-area-inset-top, 3px) + var(--gap))",
    );
    assert_eq!(y(&mut k, 4), 8.0);
    k.apply(0, 2, &[style(1, &[(PROPERTIES, "--gap: 5px")])])
        .unwrap();
    assert_eq!(y(&mut k, 4), 5.0);
}

#[test]
fn a_property_of_rem_follows_the_root_font_size() {
    let mut k = tree("--pad: 2rem", "var(--pad)");
    assert_eq!(y(&mut k, 4), 32.0);
    k.set_root_font_size(20.0).unwrap();
    k.apply(0, 2, &[]).unwrap();
    assert_eq!(y(&mut k, 4), 40.0);
}

#[test]
fn every_member_of_a_cycle_is_invalid_whatever_a_fallback_reaches() {
    // A fallback's reference is an edge too, used or not (CSS Variables 1 §2.3)
    let cases = [
        ("--a: var(--b, var(--a)); --b: 1px", "var(--a, 7px)"),
        (
            "--a: calc(var(--b) + var(--c)); --b: var(--a); --c: var(--a, 3px)",
            "var(--c, 7px)",
        ),
        (
            "--ok: 1px; --a: var(--ok, var(--b)); --b: var(--a)",
            "var(--a, 7px)",
        ),
        (
            "--ok: 1px; --a: var(--ok, var(--b)); --b: var(--a)",
            "var(--b, 7px)",
        ),
        (
            "--a: calc(var(--a) + var(--b)); --b: var(--a, 5px)",
            "var(--b, 7px)",
        ),
    ];
    for (declared, pad) in cases {
        let mut k = tree(declared, pad);
        assert_eq!(y(&mut k, 4), 7.0, "{declared} read as {pad}");
    }
    // What a cycle's member reads outside it stays valid
    let mut k = tree("--ok: 2px; --a: var(--b); --b: var(--a)", "var(--ok, 7px)");
    assert_eq!(y(&mut k, 4), 2.0);
}

#[test]
fn an_expansion_past_the_author_limits_is_invalid() {
    // 64 values at --p5, past the 32 every consumer of a tree holds an author's to
    let mut declared = vec!["--p0: min(1%, 1px)".to_string()];
    for i in 1..=5 {
        declared.push(format!(
            "--p{i}: calc(var(--p{}) + var(--p{}))",
            i - 1,
            i - 1
        ));
    }
    let mut k = tree(&declared.join("; "), "var(--p5, 7px)");
    assert_eq!(y(&mut k, 4), 7.0);
    let mut k = tree(&declared.join("; "), "var(--p4, 7px)");
    assert_eq!(y(&mut k, 4), 16.0, "32 values hold: sixteen 1px");
}

#[test]
fn a_walk_past_the_declaration_bound_invalidates_every_value_in_either_order() {
    let mut declared = vec!["--ok: 1px; --p0: 1px".to_string()];
    for i in 1..1100 {
        declared.push(format!("--p{i}: var(--p{})", i - 1));
    }
    let declared = declared.join("; ");
    let mut a = tree(&declared, "calc(var(--ok, 2px) + var(--p1099, 7px))");
    let mut b = tree(&declared, "calc(var(--p1099, 7px) + var(--ok, 2px))");
    assert_eq!(y(&mut a, 4), 9.0, "both fallbacks");
    assert_eq!(y(&mut b, 4), 9.0);
}

#[test]
fn a_long_expansion_is_invalid() {
    // Each level doubles the one before (CSS Variables 1 §3.3)
    let mut declared = vec!["--p0: 1%".to_string()];
    for i in 1..=16 {
        declared.push(format!(
            "--p{i}: calc(var(--p{}) + env(safe-area-inset-top) + var(--p{}))",
            i - 1,
            i - 1
        ));
    }
    let mut k = tree(&declared.join("; "), "var(--p16, 7px)");
    assert_eq!(y(&mut k, 4), 7.0);
}

#[test]
fn an_undefined_environment_value_invalidates_its_declaration() {
    let mut k = tree("--pad: env(titlebar-area-height)", "var(--pad, 7px)");
    assert_eq!(y(&mut k, 4), 7.0, "no title-bar area");
    let area = crate::Rect {
        x: 0.0,
        y: 0.0,
        width: 390.0,
        height: 28.0,
    };
    k.set_env(k.env().with_titlebar_area(Some(area))).unwrap();
    assert_eq!(y(&mut k, 4), 28.0);
    k.set_env(k.env().with_titlebar_area(None)).unwrap();
    assert_eq!(y(&mut k, 4), 7.0);
}

#[test]
fn a_text_length_names_a_property_in_any_case() {
    let mut k = tree(
        "--gap: 8px",
        "calc(env(safe-area-inset-top, 3px) + VAR(--gap))",
    );
    assert_eq!(y(&mut k, 4), 8.0);
}

#[test]
fn a_declaration_removed_or_shadowed_shows_the_one_above() {
    // 2 declares its own --pad over the root's
    let mut k = tree("--pad: 8px", "var(--pad)");
    k.apply(0, 2, &[style(2, &[(PROPERTIES, "--pad: 4px")])])
        .unwrap();
    assert_eq!(y(&mut k, 4), 4.0);
    // A change above a nearer declaration reaches nothing below it
    k.apply(0, 3, &[style(1, &[(PROPERTIES, "--pad: 9px")])])
        .unwrap();
    assert_eq!(y(&mut k, 4), 4.0);
    // Removing the nearer one shows the root's
    k.apply(
        0,
        4,
        &[Op::ClearStyle {
            id: 2,
            mask: StyleMask::of(PROPERTIES),
            written: false,
        }],
    )
    .unwrap();
    assert_eq!(y(&mut k, 4), 9.0);
    // A declaration of a name that reads that name reads its own: a cycle
    k.apply(
        0,
        5,
        &[style(2, &[(PROPERTIES, "--pad: calc(var(--pad) * 2)")])],
    )
    .unwrap();
    assert_eq!(y(&mut k, 4), 0.0);
    // A declaration that reads a changed name carries the change below it
    k.apply(
        0,
        6,
        &[style(2, &[(PROPERTIES, "--pad: calc(var(--base) * 2)")])],
    )
    .unwrap();
    k.apply(
        0,
        7,
        &[style(1, &[(PROPERTIES, "--pad: 9px; --base: 5px")])],
    )
    .unwrap();
    assert_eq!(y(&mut k, 4), 10.0);
}

#[test]
fn a_moved_reader_reads_its_new_ancestors_declarations() {
    // 1 the root, 2 declares --pad 8, 3 declares --pad 20, 4 pads by it, in 2, 5 in 4
    let mut k = Kernel::with_monospace();
    let mut ops = create(1..=5);
    ops.push(style(1, &[(StyleId::Height, "844")]));
    ops.push(style(2, &[(PROPERTIES, "--pad: 8px")]));
    ops.push(style(3, &[(PROPERTIES, "--pad: 20px")]));
    ops.push(style(4, &[(TOP, "var(--pad, 3px)")]));
    ops.push(style(5, &[(StyleId::Height, "10")]));
    ops.extend([children(1, &[2, 3]), children(2, &[4]), children(4, &[5])]);
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    let top = |k: &mut Kernel| {
        k.compute_layout(1, Offer::definite(390.0, 844.0)).unwrap();
        k.node(5).unwrap().frame.y
    };
    assert_eq!(top(&mut k), 8.0);
    k.apply(0, 2, &[children(2, &[]), children(3, &[4])])
        .unwrap();
    assert_eq!(top(&mut k), 20.0, "no owner has local insets");
    // Under the root, which declares none, the fallback
    k.apply(0, 3, &[children(3, &[]), children(1, &[2, 3, 4])])
        .unwrap();
    assert_eq!(top(&mut k), 3.0);
}

#[test]
fn a_percentage_resolves_against_each_readers_own_basis() {
    // 1 the root declares --w 50%; 2 (200 wide) holds 4, 3 (300 wide) holds 5
    let mut k = Kernel::with_monospace();
    let mut ops = create(1..=5);
    ops.push(style(
        1,
        &[(StyleId::Height, "844"), (PROPERTIES, "--w: 50%")],
    ));
    ops.push(style(2, &[(StyleId::Width, "200")]));
    ops.push(style(3, &[(StyleId::Width, "300")]));
    ops.push(style(4, &[(StyleId::Width, "var(--w)")]));
    ops.push(style(5, &[(StyleId::Width, "var(--w)")]));
    ops.extend([children(1, &[2, 3]), children(2, &[4]), children(3, &[5])]);
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    k.compute_layout(1, Offer::definite(390.0, 844.0)).unwrap();
    assert_eq!(k.node(4).unwrap().frame.width, 100.0);
    assert_eq!(k.node(5).unwrap().frame.width, 150.0);
}

#[test]
fn an_image_in_a_paragraph_measures_it_again_when_its_property_changes() {
    // 1 declares --h; 2 a paragraph of 3 ("ab") and 4, an image --h tall
    let mut k = Kernel::with_monospace();
    let mut ops = vec![
        Op::CreateView {
            id: 1,
            node_type: NodeType::View,
        },
        Op::CreateView {
            id: 2,
            node_type: NodeType::Text,
        },
        Op::CreateView {
            id: 3,
            node_type: NodeType::Text,
        },
        Op::CreateView {
            id: 4,
            node_type: NodeType::Image,
        },
        Op::SetProp {
            id: 3,
            prop: crate::PropId::Text,
            value: "ab".into(),
        },
        Op::SetProp {
            id: 4,
            prop: crate::PropId::ImageSource,
            value: "app:/a.png".into(),
        },
    ];
    ops.push(style(
        1,
        &[(StyleId::Width, "400"), (PROPERTIES, "--h: 10px")],
    ));
    ops.push(style(2, &[(StyleId::FontSize, "16")]));
    ops.push(style(
        4,
        &[(StyleId::Width, "10"), (StyleId::Height, "var(--h)")],
    ));
    ops.extend([children(2, &[3, 4]), children(1, &[2])]);
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    let height = |k: &mut Kernel| {
        k.compute_layout(1, Offer::definite(400.0, 600.0)).unwrap();
        k.node(4).unwrap().frame.height
    };
    assert_eq!(height(&mut k), 10.0);
    k.apply(0, 2, &[style(1, &[(PROPERTIES, "--h: 30px")])])
        .unwrap();
    assert_eq!(height(&mut k), 30.0);
    assert!(
        k.node(2).unwrap().frame.height > 30.0,
        "the line grew to hold it"
    );
}
