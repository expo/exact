//! `var(--exact-safe-area-inset-*)`: a content owner's local insets, read
//! per node (LLP 1123 D5, D9).
use super::*;
use crate::id::Offer;
use crate::wire::Op;
use crate::{HostCover, NodeType, StyleId, StyleValue};

fn style(id: ViewId, rows: &[(StyleId, &str)]) -> Op {
    let mut patch = StyleProps::default();
    for (row, value) in rows {
        let value = match value.parse::<f64>() {
            Ok(n) => StyleValue::Number(n),
            Err(_) => StyleValue::Text((*value).into()),
        };
        patch.set_dynamic(*row, &value).unwrap();
    }
    Op::SetStyle {
        id,
        patch: Box::new(patch),
    }
}

fn create(ids: impl IntoIterator<Item = ViewId>) -> Vec<Op> {
    (ids.into_iter())
        .map(|id| Op::CreateView {
            id,
            node_type: NodeType::View,
        })
        .collect()
}

fn children(id: ViewId, children: &[ViewId]) -> Op {
    Op::SetChildren {
        id,
        children: children.to_vec(),
    }
}

const TOP: StyleId = StyleId::PaddingTop;

/// 1 the root, 2 a route, 3 a box in the route that pads its top with the
/// local inset, 4 a box in 3; 5 a box outside the route that does the same.
fn page() -> Kernel {
    let mut k = Kernel::with_monospace();
    let mut ops = create(1..=5);
    ops.push(style(1, &[(StyleId::Height, "844")]));
    ops.push(style(3, &[(TOP, "var(--exact-safe-area-inset-top)")]));
    ops.push(style(4, &[(StyleId::Height, "10")]));
    ops.push(style(5, &[(TOP, "var(--exact-safe-area-inset-top)")]));
    ops.extend([children(1, &[2, 5]), children(2, &[3]), children(3, &[4])]);
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    k
}

fn layout(k: &mut Kernel) {
    k.compute_layout(1, Offer::definite(390.0, 844.0)).unwrap();
}

fn y(k: &Kernel, id: ViewId) -> f32 {
    k.node(id).unwrap().frame.y
}

#[test]
fn outside_every_owner_the_inset_is_the_session_roots() {
    let mut k = page();
    k.set_env(Env::new(47.0, 0.0, 34.0, 0.0)).unwrap();
    layout(&mut k);
    assert_eq!(y(&k, 4), 47.0, "env()'s, unless the host says otherwise");
    assert_eq!(k.local_insets(5), Some([47.0, 0.0, 34.0, 0.0]));
    // The document covers the view while env() is 0 (LLP 1123 D1, D9)
    k.set_env(Env::new(0.0, 0.0, 0.0, 0.0)).unwrap();
    assert!(k
        .set_root_local_insets(Some([59.0, 0.0, 34.0, 0.0]))
        .unwrap());
    layout(&mut k);
    assert_eq!(y(&k, 4), 59.0);
    assert_eq!(k.local_insets(1), Some([59.0, 0.0, 34.0, 0.0]));
    assert!(k
        .set_root_local_insets(Some([-1.0, 0.0, 0.0, 0.0]))
        .is_err());
}

#[test]
fn an_owners_insets_reach_its_subtree_and_nothing_else() {
    let mut k = page();
    k.set_root_local_insets(Some([59.0, 0.0, 34.0, 0.0]))
        .unwrap();
    assert!(k
        .set_local_insets(2, Some([106.0, 0.0, 83.0, 0.0]))
        .unwrap());
    layout(&mut k);
    assert_eq!(y(&k, 4), 106.0, "the route's, under its bar");
    assert_eq!(k.local_insets(4), Some([106.0, 0.0, 83.0, 0.0]));
    assert_eq!(k.local_insets(5), Some([59.0, 0.0, 34.0, 0.0]));
    let five = k.node(5).unwrap().frame;
    assert_eq!(five.height, 59.0, "the root's, outside the route");
    // An owner inside another clears its own (a sheet over a route)
    k.set_local_insets(3, Some([12.0, 0.0, 0.0, 0.0])).unwrap();
    layout(&mut k);
    assert_eq!(y(&k, 4), 12.0);
    // A change re-derives the readers: the bar shrinks to an inline title
    k.set_local_insets(3, None).unwrap();
    assert!(k.set_local_insets(2, Some([54.0, 0.0, 83.0, 0.0])).unwrap());
    layout(&mut k);
    assert_eq!(y(&k, 4), 54.0);
    assert!(!k.set_local_insets(2, Some([54.0, 0.0, 83.0, 0.0])).unwrap());
    assert!(k.set_local_insets(2, Some([f32::NAN; 4])).is_err());
    // Taken back, the route reads the root's again
    k.set_local_insets(2, None).unwrap();
    layout(&mut k);
    assert_eq!(y(&k, 4), 59.0);
}

#[test]
fn a_box_reaches_under_its_routes_bar_and_the_routes_content_stays() {
    // The issue's sidebar (expo/exact#36): a route the host pads below its
    // bar (its cover, 106) holds a sidebar whose background reaches the
    // route's top, under the bar, with its content back below it, and a
    // column beside it that already clears the bar.
    let mut k = Kernel::with_monospace();
    let mut ops = create(1..=6);
    ops.push(style(1, &[(StyleId::Height, "844")]));
    ops.push(style(2, &[(StyleId::Display, "flex")]));
    ops.push(style(
        3,
        &[
            (StyleId::Width, "290"),
            (
                StyleId::MarginTop,
                "calc(-1 * var(--exact-safe-area-inset-top))",
            ),
            (TOP, "var(--exact-safe-area-inset-top)"),
        ],
    ));
    ops.push(style(4, &[(StyleId::Height, "20")]));
    ops.push(style(5, &[(StyleId::FlexGrow, "1")]));
    ops.push(style(6, &[(StyleId::Height, "20")]));
    ops.extend([
        children(1, &[2]),
        children(2, &[3, 5]),
        children(3, &[4]),
        children(5, &[6]),
    ]);
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    k.set_host_cover(2, Some(HostCover::Edges([106.0, 0.0, 0.0, 0.0])))
        .unwrap();
    k.set_local_insets(2, Some([106.0, 0.0, 34.0, 0.0]))
        .unwrap();
    layout(&mut k);
    let sidebar = k.node(3).unwrap().frame;
    assert_eq!(sidebar.y, 0.0, "its background starts under the bar");
    assert_eq!(y(&k, 4), 106.0, "its content below it");
    assert_eq!(y(&k, 5), 106.0, "the column beside it does not move");
}

#[test]
fn a_percentage_beside_a_local_inset_is_resolved_at_layout_with_the_owners() {
    let mut k = Kernel::with_monospace();
    let mut ops = create(1..=3);
    ops.push(style(
        1,
        &[(StyleId::Width, "200"), (StyleId::Height, "100")],
    ));
    ops.push(style(
        3,
        &[(
            StyleId::Width,
            "calc(50% + var(--exact-safe-area-inset-left))",
        )],
    ));
    ops.extend([children(1, &[2]), children(2, &[3])]);
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    k.set_env(Env::new(0.0, 0.0, 0.0, 7.0)).unwrap();
    k.set_local_insets(2, Some([0.0, 0.0, 0.0, 20.0])).unwrap();
    layout(&mut k);
    assert_eq!(k.node(3).unwrap().frame.width, 120.0);
    let min = "min(10%, var(--exact-safe-area-inset-left))";
    k.apply(0, 2, &[style(3, &[(StyleId::Width, min)])])
        .unwrap();
    layout(&mut k);
    assert_eq!(k.node(3).unwrap().frame.width, 20.0);
}

#[test]
fn the_term_parses_writes_back_and_crosses_the_wire() {
    use crate::style::Comparison;
    for text in [
        "var(--exact-safe-area-inset-top)",
        "max(16px, var(--exact-safe-area-inset-bottom))",
        "calc(-1 * var(--exact-safe-area-inset-left))",
        "calc(env(safe-area-inset-right) + var(--exact-safe-area-inset-right))",
    ] {
        let Some(Dimension::Compare(c)) = Dimension::parse_calc(text) else {
            panic!("{text}");
        };
        let mut css = String::new();
        c.css(&mut css);
        assert_eq!(Comparison::from_css(&css), Some(c.clone()), "{css}");
        let mut w = crate::wire::codec::Writer::new();
        c.encode(&mut w);
        let bytes = w.into_vec();
        let mut r = crate::wire::codec::Reader::new(&bytes);
        assert_eq!(Comparison::decode(&mut r, TOP).unwrap(), c);
    }
    let mut env = Env::new(1.0, 2.0, 3.0, 4.0);
    let at = |text: &str, env: &Env| Dimension::parse_calc(text).unwrap().at_basis(env, 0.0);
    env.local = Some([10.0, 20.0, 30.0, 40.0]);
    assert_eq!(at("var(--exact-safe-area-inset-left)", &env), 40.0);
    assert_eq!(
        at("max(16px, var(--exact-safe-area-inset-top))", &env),
        16.0
    );
    for refused in [
        "var(--exact-safe-area-inset-top, 4px)",
        "var(--exact-accent)",
        "var(--exact-safe-area-inset-middle)",
        "calc(var(--Exact-safe-area-inset-top) + 1px)",
    ] {
        let mut props = StyleProps::default();
        let text = StyleValue::Text(refused.into());
        assert!(props.set_dynamic(TOP, &text).is_err(), "{refused}");
    }
}

#[test]
fn a_box_moved_or_mounted_into_an_owner_reads_its_insets() {
    let mut k = page();
    k.set_local_insets(2, Some([106.0, 0.0, 83.0, 0.0]))
        .unwrap();
    // 5 moves from the root into the route
    k.apply(0, 2, &[children(1, &[2]), children(2, &[3, 5])])
        .unwrap();
    layout(&mut k);
    assert_eq!(k.node(5).unwrap().frame.height, 106.0);
    // A subtree made and styled before it is mounted in the route
    let mut ops = create(6..=7);
    ops.push(style(7, &[(TOP, "var(--exact-safe-area-inset-bottom)")]));
    ops.extend([children(6, &[7]), children(2, &[3, 5, 6])]);
    k.apply(0, 3, &ops).unwrap();
    layout(&mut k);
    assert_eq!(k.node(7).unwrap().frame.height, 83.0);
}

/// D9: overrides on one node that read one another resolve in dependency
/// order, a chain through two edges included; a cycle across edges is
/// invalid, so its members are unset, and an override that reads a member
/// reads what the member inherits.
#[test]
fn overrides_resolve_their_dependencies_and_reject_cycles() {
    let mut k = page();
    k.set_local_insets(2, Some([106.0, 7.0, 34.0, 9.0]))
        .unwrap();
    let [top, right, bottom, left] = OVERRIDES;
    let var = |edge: &str| format!("var(--exact-safe-area-inset-{edge})");
    let (to_bottom, to_right) = (var("bottom"), var("right"));
    let chain = [
        (bottom, "20"),
        (right, &to_bottom[..]),
        (top, &to_right[..]),
    ];
    k.apply(0, 2, &[style(3, &chain)]).unwrap();
    assert_eq!(k.local_insets(3), Some([20.0, 20.0, 20.0, 9.0]), "a chain");
    let (to_left, to_top) = (var("left"), var("top"));
    let cycle = [
        (top, &to_left[..]),
        (left, &to_top[..]),
        (bottom, &to_left[..]),
    ];
    k.apply(0, 3, &[style(5, &cycle)]).unwrap();
    let root = Edge::ALL.map(|e| k.env().root_inset(e));
    let got = k.local_insets(5).unwrap();
    assert_eq!(got[0], root[0], "top and left read each other: unset");
    assert_eq!(got[3], root[3]);
    assert_eq!(
        got[2], root[3],
        "bottom reads left as the node has it, unset"
    );
}

/// A sticky box's inset that reads the local inset sticks below its route's
/// bar, not at the document's 0: the constraint resolves where the box is.
#[test]
fn a_sticky_inset_reads_its_owners_inset() {
    let mut k = Kernel::with_monospace();
    let mut ops = create(1..=5);
    ops.push(style(1, &[(StyleId::Height, "844")]));
    ops.push(style(2, &[(StyleId::Height, "844")]));
    ops.push(style(
        3,
        &[(StyleId::Height, "844"), (StyleId::OverflowY, "scroll")],
    ));
    ops.push(style(
        4,
        &[
            (StyleId::PositionType, "sticky"),
            (StyleId::Top, "var(--exact-safe-area-inset-top)"),
            (StyleId::Height, "20"),
        ],
    ));
    ops.push(style(5, &[(StyleId::Height, "2000")]));
    ops.extend([children(1, &[2]), children(2, &[3]), children(3, &[4, 5])]);
    ops.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &ops).unwrap();
    k.set_local_insets(2, Some([106.0, 0.0, 0.0, 0.0])).unwrap();
    layout(&mut k);
    let key = k.node(4).unwrap().key;
    let constraint = k.sticky_constraint(key).expect("a sticky box");
    assert_eq!(constraint.insets[0], Some(106.0));
}

#[test]
fn absolute_insets_read_their_owner_instead_of_the_document() {
    let mut k = Kernel::with_monospace();
    let mut ops = create(1..=4);
    ops.push(style(1, &[(StyleId::Height, "844")]));
    ops.push(style(
        2,
        &[
            (StyleId::PositionType, "relative"),
            (StyleId::BoxSizing, "border-box"),
            (StyleId::Width, "300"),
            (StyleId::Height, "300"),
        ],
    ));
    for (id, sides) in [
        (3, [(StyleId::Top, "top"), (StyleId::Left, "left")]),
        (4, [(StyleId::Bottom, "bottom"), (StyleId::Right, "right")]),
    ] {
        let mut rows = vec![
            (StyleId::PositionType, "absolute"),
            (StyleId::Width, "20"),
            (StyleId::Height, "30"),
        ];
        let values = sides.map(|(_, edge)| format!("var(--exact-safe-area-inset-{edge})"));
        rows.extend(
            sides
                .iter()
                .zip(&values)
                .map(|((id, _), value)| (*id, value.as_str())),
        );
        ops.push(style(id, &rows));
    }
    ops.extend([
        children(1, &[2]),
        children(2, &[3, 4]),
        Op::AttachRoot { id: 1 },
    ]);
    k.apply(0, 1, &ops).unwrap();
    k.set_root_local_insets(Some([62.0, 2.0, 34.0, 3.0]))
        .unwrap();
    k.set_local_insets(2, Some([116.0, 7.0, 34.0, 9.0]))
        .unwrap();
    layout(&mut k);
    let frame = k.node(3).unwrap().frame;
    assert_eq!((frame.x, frame.y), (9.0, 116.0));
    let frame = k.node(4).unwrap().frame;
    assert_eq!((frame.x, frame.y), (273.0, 236.0));
    k.set_host_cover(2, Some(HostCover::Edges([116.0, 7.0, 34.0, 9.0])))
        .unwrap();
    layout(&mut k);
    let frame = k.node(3).unwrap().frame;
    assert_eq!(
        (frame.x, frame.y),
        (9.0, 116.0),
        "the inset clears the cover once"
    );
    let frame = k.node(4).unwrap().frame;
    assert_eq!((frame.x, frame.y), (273.0, 236.0));
    k.set_local_insets(2, Some([54.0, 12.0, 40.0, 15.0]))
        .unwrap();
    k.set_host_cover(2, Some(HostCover::Edges([54.0, 12.0, 40.0, 15.0])))
        .unwrap();
    layout(&mut k);
    let frame = k.node(3).unwrap().frame;
    assert_eq!((frame.x, frame.y), (15.0, 54.0));
    let frame = k.node(4).unwrap().frame;
    assert_eq!((frame.x, frame.y), (268.0, 230.0));
    k.apply(
        0,
        2,
        &[
            style(3, &[(StyleId::Top, "0"), (StyleId::Left, "8")]),
            style(4, &[(StyleId::Bottom, "8"), (StyleId::Right, "0")]),
        ],
    )
    .unwrap();
    layout(&mut k);
    let frame = k.node(3).unwrap().frame;
    assert_eq!(
        (frame.x, frame.y),
        (23.0, 54.0),
        "plain lengths still add the cover"
    );
    let frame = k.node(4).unwrap().frame;
    assert_eq!((frame.x, frame.y), (268.0, 222.0));
    k.apply(
        0,
        3,
        &[
            style(3, &[(StyleId::Top, "var(--exact-safe-area-inset-left)")]),
            style(
                4,
                &[(
                    StyleId::Bottom,
                    "calc(var(--exact-safe-area-inset-bottom) + 8px)",
                )],
            ),
        ],
    )
    .unwrap();
    layout(&mut k);
    assert_eq!(
        y(&k, 3),
        69.0,
        "another edge's inset does not clear the top"
    );
    assert_eq!(
        y(&k, 4),
        222.0,
        "a matching inset in calc clears the cover once"
    );
    k.set_env(Env::default()).unwrap();
    k.set_local_insets(2, Some([116.0, 40.0, 34.0, 50.0]))
        .unwrap();
    k.set_host_cover(2, Some(HostCover::Edges([116.0, 40.0, 34.0, 50.0])))
        .unwrap();
    let mut frames = Vec::new();
    for (revision, source) in [(4, "env"), (5, "min")] {
        let offset = |edge| match source {
            "env" => format!("env(safe-area-inset-{edge})"),
            _ => format!("min(20px, var(--exact-safe-area-inset-{edge}))"),
        };
        k.apply(
            0,
            revision,
            &[
                style(
                    3,
                    &[
                        (StyleId::Top, &offset("top")),
                        (StyleId::Left, &offset("left")),
                    ],
                ),
                style(
                    4,
                    &[
                        (StyleId::Bottom, &offset("bottom")),
                        (StyleId::Right, &offset("right")),
                    ],
                ),
            ],
        )
        .unwrap();
        layout(&mut k);
        let top_left = k.node(3).unwrap().frame;
        let bottom_right = k.node(4).unwrap().frame;
        frames.push(((top_left.x, top_left.y), (bottom_right.x, bottom_right.y)));
    }
    assert_eq!(
        frames,
        vec![((50.0, 116.0), (240.0, 236.0)); 2],
        "zero and shorter inset references still clear every covered edge"
    );
}

/// D9: an author's `--exact-safe-area-inset-*` on a node is what its
/// subtree reads, over the host's value on an owner's root, as an inherited
/// custom property; a change reaches the readers below it.
#[test]
fn an_authored_inset_is_what_its_subtree_reads() {
    let mut k = page();
    k.set_local_insets(2, Some([106.0, 0.0, 83.0, 0.0]))
        .unwrap();
    let top = StyleId::LocalInsetTop;
    k.apply(0, 2, &[style(3, &[(top, "20")])]).unwrap();
    layout(&mut k);
    assert_eq!(y(&k, 4), 20.0, "the box's own, which it reads too");
    assert_eq!(k.local_insets(4), Some([20.0, 0.0, 83.0, 0.0]), "inherited");
    k.apply(0, 3, &[style(3, &[(top, "30")])]).unwrap();
    layout(&mut k);
    assert_eq!(y(&k, 4), 30.0, "a change reaches the reader");
    // On the owner's root, over the host's value
    k.apply(0, 4, &[style(2, &[(top, "12")])]).unwrap();
    assert_eq!(k.local_insets(2), Some([12.0, 0.0, 83.0, 0.0]));
    // An override reads another edge as the node has it: the node's own
    // where it declares one (30), its parent's otherwise
    let bottom = StyleId::LocalInsetBottom;
    let plus = "calc(var(--exact-safe-area-inset-top) + 5px)";
    k.apply(0, 5, &[style(3, &[(bottom, plus)])]).unwrap();
    assert_eq!(k.local_insets(3).unwrap()[2], 35.0, "the node's own top");
    k.apply(0, 6, &[style(5, &[(bottom, plus)])]).unwrap();
    let root = k.env().root_inset(Edge::Top);
    assert_eq!(k.local_insets(5).unwrap()[2], root + 5.0, "the parent's");
    // One that reads itself is a cycle: unset, so inherited
    let cycle = "calc(var(--exact-safe-area-inset-top) + 1px)";
    k.apply(0, 7, &[style(5, &[(top, cycle)])]).unwrap();
    assert_eq!(k.local_insets(5).unwrap()[0], k.env().root_inset(Edge::Top));
    // A percentage is no length a custom property of `<length>` takes
    let mut props = StyleProps::default();
    let pct = StyleValue::Text("10%".into());
    assert!(props.set_dynamic(top, &pct).is_err());
}

/// The document's environment has no local insets: a reader that resolves
/// a box's length against it, not the box's own (`NodeArena::env_of`),
/// stops a test instead of reading the session root's under a route's bar.
#[test]
#[should_panic(expected = "resolved against the document's environment")]
fn a_local_inset_read_against_the_documents_environment_stops() {
    let k = page();
    let length = Dimension::parse_calc("var(--exact-safe-area-inset-top)").unwrap();
    let _ = length.at_basis(k.arena.env(), 0.0);
}

/// The per-node resolver, for a box whose lengths read none, is the
/// document's, and for one that reads one carries its insets.
#[test]
fn the_node_resolver_carries_a_boxs_insets_only_where_it_reads_them() {
    let mut k = page();
    k.set_local_insets(2, Some([106.0, 0.0, 0.0, 0.0])).unwrap();
    let slot = |k: &Kernel, id| k.arena.slot_of(id).unwrap();
    let route = k.arena.env_of(slot(&k, 2)).local;
    assert!(route.is_none(), "the route reads none");
    let reader = k.arena.env_of(slot(&k, 3)).local;
    assert_eq!(reader, Some([106.0, 0.0, 0.0, 0.0]));
    assert_eq!(
        k.env_of(2).local,
        Some([106.0, 0.0, 0.0, 0.0]),
        "a host's always does"
    );
}

/// A route (2) under the root (1) whose local top inset is 106 and left 20,
/// holding what `ops` makes (`ids`), laid out.
fn in_route(k: &mut Kernel, ids: impl IntoIterator<Item = ViewId>, ops: Vec<Op>) {
    let mut all = create(1..=2);
    all.extend(create(ids));
    all.push(style(1, &[(StyleId::Height, "844")]));
    all.push(children(1, &[2]));
    all.extend(ops);
    all.push(Op::AttachRoot { id: 1 });
    k.apply(0, 1, &all).unwrap();
    k.set_local_insets(2, Some([106.0, 0.0, 0.0, 20.0]))
        .unwrap();
    layout(k);
}

fn prop(id: ViewId, prop: crate::PropId, value: &str) -> Op {
    Op::SetProp {
        id,
        prop,
        value: crate::PropValue::Str(value.into()),
    }
}

/// [`style`], with the rows marked as the author's.
fn written(id: ViewId, rows: &[(StyleId, &str)]) -> Op {
    let Op::SetStyle { id, mut patch } = style(id, rows) else {
        unreachable!()
    };
    for (row, _) in rows {
        patch.written.set(*row);
    }
    Op::SetStyle { id, patch }
}

/// A native button's padding reads its own inset in what it asks the host
/// to measure and in the face the host draws.
#[test]
fn a_buttons_face_reads_its_own_inset() {
    use std::sync::{Arc, Mutex};
    struct Host(Arc<Mutex<Vec<crate::Dimension>>>);
    impl crate::TextMeasurer for Host {
        fn measure(&mut self, _: &crate::TextMeasureRequest<'_>) -> crate::TextMetrics {
            crate::TextMetrics::default()
        }
        fn button_measure(
            &mut self,
            r: &crate::ButtonMeasureRequest,
        ) -> Option<crate::ButtonMeasure> {
            (self.0.lock().unwrap()).push(r.style.button.padding_top.clone());
            Some(crate::ButtonMeasure {
                width: 100.0,
                height: 30.0,
                provisional: false,
            })
        }
    }
    let asked = Arc::new(Mutex::new(Vec::new()));
    let mut k = Kernel::new(Box::new(Host(asked.clone())));
    let ops = vec![
        Op::CreateView {
            id: 3,
            node_type: NodeType::Control,
        },
        prop(3, crate::PropId::Type, "button"),
        written(3, &[(TOP, "var(--exact-safe-area-inset-top)")]),
        children(2, &[3]),
    ];
    in_route(&mut k, [], ops);
    let top = crate::Dimension::Points(106.0);
    assert!(!asked.lock().unwrap().is_empty(), "the host measured it");
    assert!(asked.lock().unwrap().iter().all(|d| *d == top));
    let slot = k.arena.slot_of(3).unwrap();
    let face = k.arena.button_face_style(slot).unwrap();
    assert_eq!(face.button.padding_top, top);
}

/// A grouped list's section margins are the section's lengths, read where
/// the section is: the list itself reads no inset.
#[test]
fn a_sections_margin_reads_its_own_inset() {
    let mut k = Kernel::with_monospace();
    let ops = vec![
        prop(3, crate::PropId::ListStyle, "inset-grouped"),
        prop(4, crate::PropId::SemanticTag, "section"),
        written(
            4,
            &[(StyleId::MarginTop, "var(--exact-safe-area-inset-top)")],
        ),
        children(2, &[3]),
        children(3, &[4]),
    ];
    in_route(&mut k, 3..=4, ops);
    let list = k.grouped_list(3).unwrap();
    assert_eq!(list.sections[0].space_above, Some(106.0));
}

/// A caption spans its table's padding box, whose padding is the table's
/// length, read where the table is: as a written 20 would place it.
#[test]
fn a_caption_takes_its_tables_padding_in_the_tables_environment() {
    let caption = |padding: &str| {
        let mut k = Kernel::with_monospace();
        let ops = vec![
            prop(3, crate::PropId::SemanticTag, "table"),
            written(3, &[(StyleId::PaddingLeft, padding)]),
            prop(4, crate::PropId::SemanticTag, "caption"),
            written(5, &[(StyleId::Width, "10"), (StyleId::Height, "10")]),
            prop(6, crate::PropId::SemanticTag, "tr"),
            prop(7, crate::PropId::SemanticTag, "td"),
            written(8, &[(StyleId::Width, "60"), (StyleId::Height, "10")]),
            children(2, &[3]),
            children(3, &[4, 6]),
            children(4, &[5]),
            children(6, &[7]),
            children(7, &[8]),
        ];
        in_route(&mut k, 3..=8, ops);
        k.node(4).unwrap().frame
    };
    let written = caption("20");
    assert_ne!(caption("0"), written, "the padding places the caption");
    assert_eq!(caption("var(--exact-safe-area-inset-left)"), written);
}
