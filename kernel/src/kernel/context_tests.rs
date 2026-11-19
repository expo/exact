//! The inset rule (`kernel::context`): `env(safe-area-inset-*)` reads what
//! its context leaves, and a scroller's padding and content read 0 on the
//! edges of the axes it scrolls that it does not ignore.
use crate::id::Offer;
use crate::kernel::owner::tests::{children, create, style};
use crate::wire::Op;
use crate::{Env, Kernel, StyleId, ViewId};

const ALL: [StyleId; 4] = [
    StyleId::PaddingTop,
    StyleId::PaddingRight,
    StyleId::PaddingBottom,
    StyleId::PaddingLeft,
];
const ENV: [&str; 4] = [
    "env(safe-area-inset-top)",
    "env(safe-area-inset-right)",
    "env(safe-area-inset-bottom)",
    "env(safe-area-inset-left)",
];

fn padding(k: &Kernel, id: ViewId) -> [f32; 4] {
    let key = k.node(id).unwrap().key;
    let (left, top, right, bottom) = k.resolved_padding(key).unwrap();
    [top, right, bottom, left]
}

fn page(ops: Vec<Op>, env: Env) -> Kernel {
    let mut k = Kernel::with_monospace();
    k.apply(0, 1, &ops).unwrap();
    k.set_env(env).unwrap();
    k.compute_layout(1, Offer::definite(390.0, 844.0)).unwrap();
    k
}

fn pads_env(id: ViewId) -> Op {
    let rows: Vec<_> = ALL.into_iter().zip(ENV).collect();
    style(id, &rows)
}

/// 1 the root; 2 a route, which its host has laid out in what it clears;
/// 3 in it and 4 outside it, each padded by `env()` on every edge.
fn route() -> Kernel {
    let mut ops = create(1..=4);
    ops.extend([pads_env(3), pads_env(4)]);
    ops.extend([children(1, &[2, 4]), children(2, &[3])]);
    ops.push(Op::AttachRoot { id: 1 });
    let mut k = page(ops, Env::new(62.0, 0.0, 34.0, 0.0));
    k.set_local_insets(2, Some([0.0; 4])).unwrap();
    k.compute_layout(1, Offer::definite(390.0, 844.0)).unwrap();
    k
}

#[test]
fn env_in_an_owner_is_what_its_root_leaves() {
    let mut k = route();
    assert_eq!(
        padding(&k, 3),
        [0.0; 4],
        "the route's root clears every edge"
    );
    assert_eq!(
        padding(&k, 4),
        [62.0, 0.0, 34.0, 0.0],
        "the document's, outside the route"
    );
    // A root that takes the top: its host reports the bar's whole band there
    k.set_local_insets(2, Some([116.0, 0.0, 0.0, 0.0])).unwrap();
    k.compute_layout(1, Offer::definite(390.0, 844.0)).unwrap();
    assert_eq!(padding(&k, 3), [116.0, 0.0, 0.0, 0.0]);
}

#[test]
fn a_nested_owner_reads_its_own() {
    let mut ops = create(1..=3);
    ops.push(pads_env(3));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    ops.push(Op::AttachRoot { id: 1 });
    let mut k = page(ops, Env::new(62.0, 0.0, 34.0, 0.0));
    k.set_local_insets(2, Some([0.0; 4])).unwrap();
    k.set_local_insets(3, Some([0.0, 0.0, 20.0, 0.0])).unwrap();
    k.compute_layout(1, Offer::definite(390.0, 844.0)).unwrap();
    assert_eq!(padding(&k, 3), [0.0, 0.0, 20.0, 0.0]);
}

/// Under `viewport-fit=cover` and no owner: 2 a full-screen scroller and 3 a
/// box in it, padded as `rows` say; 4 a box beside the scroller that pads
/// its top by `env()`.
fn scroller(scroller: &[(StyleId, &str)], content: &[(StyleId, &str)], env: Env) -> Kernel {
    let mut ops = create(1..=4);
    let mut rows = vec![(StyleId::OverflowY, "scroll"), (StyleId::Height, "844")];
    rows.extend_from_slice(scroller);
    ops.extend([
        style(2, &rows),
        style(3, content),
        style(4, &[(StyleId::PaddingTop, "env(safe-area-inset-top)")]),
    ]);
    ops.extend([children(1, &[2, 4]), children(2, &[3])]);
    ops.push(Op::AttachRoot { id: 1 });
    page(ops, env)
}

#[test]
fn a_scrollers_padding_and_content_read_0_on_the_edges_it_insets() {
    let env = Env::new(62.0, 0.0, 34.0, 0.0);
    let top = (StyleId::PaddingTop, "env(safe-area-inset-top)");
    let bottom = (StyleId::PaddingBottom, "env(safe-area-inset-bottom)");
    let k = scroller(&[top, bottom], &[top, bottom], env.clone());
    assert_eq!(
        padding(&k, 2),
        [0.0; 4],
        "its padding lies inside its content inset"
    );
    assert_eq!(padding(&k, 3), [0.0; 4], "and its content");
    assert_eq!(padding(&k, 4)[0], 62.0, "the document's, beside it");
    // `var()` alike: the session root's local inset is the document's here
    let local = (StyleId::PaddingTop, "var(--exact-safe-area-inset-top)");
    let k = scroller(&[local], &[local], env);
    assert_eq!((padding(&k, 2)[0], padding(&k, 3)[0]), (0.0, 0.0));
}

#[test]
fn at_least_a_length_in_a_scroller_is_the_length() {
    let max = (StyleId::PaddingTop, "max(16px, env(safe-area-inset-top))");
    let k = scroller(&[max], &[], Env::new(62.0, 0.0, 34.0, 0.0));
    assert_eq!(
        padding(&k, 2)[0],
        16.0,
        "16 past the inset the platform gives"
    );
}

#[test]
fn a_scroller_that_takes_an_edge_leaves_it_to_its_padding() {
    let take = (StyleId::SafeAreaIgnore, "container top");
    let top = (StyleId::PaddingTop, "env(safe-area-inset-top)");
    let bottom = (StyleId::PaddingBottom, "env(safe-area-inset-bottom)");
    let k = scroller(&[take, top, bottom], &[top], Env::new(62.0, 0.0, 34.0, 0.0));
    assert_eq!(
        padding(&k, 2),
        [62.0, 0.0, 0.0, 0.0],
        "the top is the author's; the bottom still inset"
    );
    assert_eq!(padding(&k, 3)[0], 62.0);
}

#[test]
fn a_vertical_scroller_leaves_its_sides_to_its_content() {
    // Landscape: UIKit insets the vertical axis only, so content pads the side bands itself
    let sides = [
        (StyleId::PaddingLeft, "env(safe-area-inset-left)"),
        (StyleId::PaddingRight, "env(safe-area-inset-right)"),
    ];
    let k = scroller(&[], &sides, Env::new(0.0, 47.0, 21.0, 47.0));
    assert_eq!(padding(&k, 3), [0.0, 47.0, 0.0, 47.0]);
}

#[test]
fn a_local_inset_declared_inside_a_scroller_stands() {
    let local = (StyleId::PaddingTop, "var(--exact-safe-area-inset-top)");
    let own = (StyleId::LocalInsetTop, "7");
    let k = scroller(&[], &[own, local], Env::new(62.0, 0.0, 34.0, 0.0));
    assert_eq!(
        padding(&k, 3)[0],
        7.0,
        "the author's, below the scroller that takes the top"
    );
}
