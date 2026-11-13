//! What a native container covers of a box (LLP 1075.003 §3.5, §3.7): a
//! navigation bar over a route's top edge adds to its padding, the header
//! whose heading the bar shows takes no space, and clearing either lays the
//! route out as authored again.

use exact_kernel::{Env, HostCover, Kernel, NodeType, Offer, Op, StyleId, StyleProps, StyleValue};

fn style(rows: &[(StyleId, StyleValue)]) -> StyleProps {
    let mut s = StyleProps::default();
    for (id, value) in rows {
        s.set_dynamic(*id, value).unwrap();
    }
    s
}

/// A root, one route filling it (a flex column padded by the top inset as
/// an app under `viewport-fit=cover` pads it), a 52-point header and a list.
fn route() -> Kernel {
    let mut kernel = Kernel::with_monospace();
    let column = |extra: &[(StyleId, StyleValue)]| {
        let mut rows = vec![
            (StyleId::Display, StyleValue::Text("flex".into())),
            (StyleId::FlexDirection, StyleValue::Text("column".into())),
        ];
        rows.extend_from_slice(extra);
        style(&rows)
    };
    let ops = vec![
        Op::CreateView {
            id: 1,
            node_type: NodeType::View,
        },
        Op::SetStyle {
            id: 1,
            patch: Box::new(style(&[(StyleId::Height, StyleValue::Percent(100.0))])),
        },
        Op::CreateView {
            id: 2,
            node_type: NodeType::View,
        },
        Op::SetStyle {
            id: 2,
            patch: Box::new(column(&[
                (StyleId::PositionType, StyleValue::Text("absolute".into())),
                (StyleId::Top, StyleValue::Number(0.0)),
                (StyleId::Bottom, StyleValue::Number(0.0)),
                (StyleId::Left, StyleValue::Number(0.0)),
                (StyleId::Right, StyleValue::Number(0.0)),
                (
                    StyleId::PaddingTop,
                    StyleValue::Text("env(safe-area-inset-top)".into()),
                ),
            ])),
        },
        Op::CreateView {
            id: 3,
            node_type: NodeType::View,
        },
        Op::SetStyle {
            id: 3,
            patch: Box::new(style(&[
                (StyleId::Height, StyleValue::Number(52.0)),
                (StyleId::FlexShrink, StyleValue::Number(0.0)),
            ])),
        },
        Op::CreateView {
            id: 4,
            node_type: NodeType::View,
        },
        Op::SetStyle {
            id: 4,
            patch: Box::new(style(&[(StyleId::FlexGrow, StyleValue::Number(1.0))])),
        },
        Op::SetChildren {
            id: 2,
            children: vec![3, 4],
        },
        Op::SetChildren {
            id: 1,
            children: vec![2],
        },
        Op::AttachRoot { id: 1 },
    ];
    kernel.apply(0, 1, &ops).unwrap();
    kernel
}

fn frame(kernel: &mut Kernel, id: u32) -> (f32, f32, f32, f32) {
    kernel
        .compute_layout(1, Offer::definite(390.0, 844.0))
        .unwrap();
    let f = kernel.node(id).unwrap().frame;
    (f.x, f.y, f.width, f.height)
}

/// LLP 1123 D3: the route pads `env(safe-area-inset-top)` itself, so its
/// padding is credited: the edge is the larger of the two, not their sum.
#[test]
fn a_padding_that_reads_the_inset_is_credited_not_added() {
    let mut kernel = route();
    kernel.set_env(Env::new(62.0, 0.0, 34.0, 0.0)).unwrap();
    assert_eq!(
        frame(&mut kernel, 3),
        (0.0, 62.0, 390.0, 52.0),
        "as authored"
    );
    kernel
        .set_host_cover(2, Some(HostCover::Edges([106.0, 0.0, 0.0, 0.0])))
        .unwrap();
    assert_eq!(
        frame(&mut kernel, 3),
        (0.0, 106.0, 390.0, 52.0),
        "the status bar's 62 and the bar's 44, once"
    );
    assert_eq!(frame(&mut kernel, 4), (0.0, 158.0, 390.0, 686.0));
    // The padding read back is the author's 62, the clearance's 44 apart
    let key = kernel.node(2).unwrap().key;
    assert_eq!(kernel.resolved_padding(key).unwrap().1, 62.0);
    let pad = |kernel: &mut Kernel, batch, value: StyleValue| {
        let op = Op::SetStyle {
            id: 2,
            patch: Box::new(style(&[(StyleId::PaddingTop, value)])),
        };
        kernel.apply(1, batch, &[op]).unwrap();
        frame(kernel, 3).1
    };
    // A plain padding clears nothing: the clearance adds to it, as SwiftUI's
    // safe area adds to `padding()`.
    assert_eq!(pad(&mut kernel, 2, StyleValue::Number(16.0)), 122.0);
    // `max(16px, env())`: at least 16, or the inset (D3's table, row 9).
    let max = StyleValue::Text("max(16px, env(safe-area-inset-top))".into());
    assert_eq!(pad(&mut kernel, 3, max), 106.0);
    // More than the inset: the padding's own.
    let more = StyleValue::Text("calc(2 * env(safe-area-inset-top))".into());
    assert_eq!(pad(&mut kernel, 4, more), 124.0);
    // A percentage resolves at layout, credited the same (10% of 390 is 39).
    let pct = StyleValue::Text("max(10%, env(safe-area-inset-top))".into());
    assert_eq!(pad(&mut kernel, 5, pct), 106.0);
    let pct = StyleValue::Text("max(40%, env(safe-area-inset-top))".into());
    assert_eq!(pad(&mut kernel, 6, pct), 156.0);
    // Another edge's inset clears nothing of this one.
    let bottom = StyleValue::Text("env(safe-area-inset-bottom)".into());
    assert_eq!(pad(&mut kernel, 7, bottom), 34.0 + 106.0);
}

/// LLP 1123 D4: `-exact-safe-area` turns an edge's clearance off, and only
/// that edge's; the padding stays as authored, and the owner reads the
/// inset through `var(--exact-safe-area-inset-*)` to pad with it itself.
#[test]
fn an_owner_opts_an_edge_out_of_its_clearance() {
    let mut kernel = route();
    let unpadded = style(&[(StyleId::PaddingTop, StyleValue::Number(0.0))]);
    let op = Op::SetStyle {
        id: 2,
        patch: Box::new(unpadded),
    };
    kernel.apply(1, 2, &[op]).unwrap();
    kernel
        .set_host_cover(2, Some(HostCover::Edges([106.0, 0.0, 34.0, 0.0])))
        .unwrap();
    assert_eq!(frame(&mut kernel, 3).1, 106.0);
    assert_eq!(frame(&mut kernel, 4).3, 844.0 - 106.0 - 52.0 - 34.0);
    let off = |kernel: &mut Kernel, batch, value: &str| {
        let op = Op::SetStyle {
            id: 2,
            patch: Box::new(style(&[(
                StyleId::SafeArea,
                StyleValue::Text(value.into()),
            )])),
        };
        kernel.apply(1, batch, &[op]).unwrap();
        (frame(kernel, 3).1, frame(kernel, 4).3)
    };
    let top = (0.0, 844.0 - 52.0 - 34.0);
    assert_eq!(off(&mut kernel, 3, "top"), top, "the top only");
    assert_eq!(
        off(&mut kernel, 4, "none"),
        (0.0, 844.0 - 52.0),
        "every edge"
    );
    let bottom = (106.0, 844.0 - 106.0 - 52.0);
    assert_eq!(off(&mut kernel, 5, "bottom"), bottom);
    let auto = (106.0, 844.0 - 106.0 - 52.0 - 34.0);
    assert_eq!(off(&mut kernel, 6, "auto"), auto);
    // Off at the top, the route pads the inset it reads itself.
    kernel
        .set_local_insets(2, Some([106.0, 0.0, 34.0, 0.0]))
        .unwrap();
    let op = Op::SetStyle {
        id: 2,
        patch: Box::new(style(&[
            (StyleId::SafeArea, StyleValue::Text("top".into())),
            (
                StyleId::PaddingTop,
                StyleValue::Text("calc(var(--exact-safe-area-inset-top) + 8px)".into()),
            ),
        ])),
    };
    kernel.apply(1, 7, &[op]).unwrap();
    assert_eq!(frame(&mut kernel, 3).1, 114.0);
    for refused in ["top top", "none top", "middle"] {
        let mut props = StyleProps::default();
        let value = StyleValue::Text(refused.into());
        let set = props.set_dynamic(StyleId::SafeArea, &value);
        assert!(set.is_err(), "{refused}");
    }
}

#[test]
fn an_absolute_footer_in_a_covered_route_clears_the_bar_it_rests_on() {
    let mut kernel = route();
    // A footer pinned to the route's bottom edge, as a cart's Undo button.
    let footer = vec![
        Op::CreateView {
            id: 5,
            node_type: NodeType::View,
        },
        Op::SetStyle {
            id: 5,
            patch: Box::new(style(&[
                (StyleId::PositionType, StyleValue::Text("absolute".into())),
                (StyleId::Bottom, StyleValue::Number(0.0)),
                (StyleId::Left, StyleValue::Number(0.0)),
                (StyleId::Right, StyleValue::Number(0.0)),
                (StyleId::Height, StyleValue::Number(40.0)),
            ])),
        },
        Op::SetChildren {
            id: 2,
            children: vec![3, 4, 5],
        },
    ];
    kernel.apply(0, 1, &footer).unwrap();
    assert_eq!(
        frame(&mut kernel, 5),
        (0.0, 804.0, 390.0, 40.0),
        "as authored"
    );
    // The tab bar and home indicator, 83, cover the route's bottom: the
    // in-flow content pads past them and the footer rests above them.
    kernel
        .set_host_cover(2, Some(HostCover::Edges([0.0, 0.0, 83.0, 0.0])))
        .unwrap();
    assert_eq!(
        frame(&mut kernel, 4),
        (0.0, 52.0, 390.0, 709.0),
        "in-flow content ends above the bar"
    );
    assert_eq!(
        frame(&mut kernel, 5),
        (0.0, 721.0, 390.0, 40.0),
        "the footer rests on the bar, not under it"
    );
    // The route's last in-flow child is a scroller the platform insets at
    // the bottom edge: the route pads nothing (the scroller reaches under
    // the bar), and the footer still clears it.
    kernel
        .set_host_cover(4, Some(HostCover::Inset([0.0, 83.0])))
        .unwrap();
    assert_eq!(
        frame(&mut kernel, 4),
        (0.0, 52.0, 390.0, 792.0),
        "the scroller reaches the bottom edge"
    );
    assert_eq!(frame(&mut kernel, 5), (0.0, 721.0, 390.0, 40.0));
    // A percentage inset is the author's to resolve; it stays as written.
    kernel
        .apply(
            0,
            1,
            &[Op::SetStyle {
                id: 5,
                patch: Box::new(style(&[(StyleId::Bottom, StyleValue::Percent(10.0))])),
            }],
        )
        .unwrap();
    kernel.set_host_cover(4, None).unwrap();
    let (_, y, _, _) = frame(&mut kernel, 5);
    assert!((y - (844.0 - 84.4 - 40.0)).abs() < 0.5, "{y}");
}

#[test]
fn a_header_the_bar_replaces_takes_no_space_until_the_cover_is_cleared() {
    let mut kernel = route();
    kernel
        .set_host_cover(2, Some(HostCover::Edges([44.0, 0.0, 0.0, 0.0])))
        .unwrap();
    kernel.set_host_cover(3, Some(HostCover::Whole)).unwrap();
    assert_eq!(frame(&mut kernel, 3), (0.0, 0.0, 0.0, 0.0));
    assert_eq!(
        frame(&mut kernel, 4),
        (0.0, 44.0, 390.0, 800.0),
        "the list starts under the bar"
    );
    kernel.set_host_cover(3, None).unwrap();
    kernel.set_host_cover(2, None).unwrap();
    assert_eq!(frame(&mut kernel, 4), (0.0, 52.0, 390.0, 792.0));
}

#[test]
fn a_cover_on_a_percent_padding_keeps_the_percent() {
    let mut kernel = route();
    let patch = style(&[(StyleId::PaddingTop, StyleValue::Percent(10.0))]);
    kernel
        .apply(
            1,
            2,
            &[Op::SetStyle {
                id: 2,
                patch: Box::new(patch),
            }],
        )
        .unwrap();
    kernel
        .set_host_cover(2, Some(HostCover::Edges([44.0, 0.0, 0.0, 0.0])))
        .unwrap();
    // Padding percentages resolve against the containing block's width.
    assert_eq!(frame(&mut kernel, 3), (0.0, 83.0, 390.0, 52.0));
    // The bar is the host's sizing, not the route's CSS padding
    let route = kernel.node(2).unwrap().key;
    assert_eq!(kernel.resolved_padding(route), Some((0.0, 39.0, 0.0, 0.0)));
    assert_eq!(kernel.layout_padding(route), Some((0.0, 83.0, 0.0, 0.0)));
}

#[test]
fn a_negative_or_nonfinite_edge_is_refused() {
    let mut kernel = route();
    assert!(kernel
        .set_host_cover(2, Some(HostCover::Edges([-1.0, 0.0, 0.0, 0.0])))
        .is_err());
    assert!(kernel
        .set_host_cover(2, Some(HostCover::Edges([f32::NAN, 0.0, 0.0, 0.0])))
        .is_err());
    assert!(kernel.set_host_cover(99, Some(HostCover::Whole)).is_err());
}

/// The route as `route()` builds it, but with the status bar cleared by
/// the header (`padding-top: env(safe-area-inset-top)`), as a web page under
/// `viewport-fit=cover` usually pads it, rather than by the route.
fn header_padded() -> Kernel {
    let mut kernel = route();
    let route = style(&[(StyleId::PaddingTop, StyleValue::Number(0.0))]);
    let header = style(&[(
        StyleId::PaddingTop,
        StyleValue::Text("env(safe-area-inset-top)".into()),
    )]);
    let ops = [
        Op::SetStyle {
            id: 2,
            patch: Box::new(route),
        },
        Op::SetStyle {
            id: 3,
            patch: Box::new(header),
        },
    ];
    kernel.apply(1, 2, &ops).unwrap();
    kernel.set_env(Env::new(62.0, 0.0, 34.0, 0.0)).unwrap();
    kernel
}

/// LLP 1123 D3: a header the bar replaces is `display: none`, so its
/// padding neither clears nor is credited, and the route's top clearance is
/// its whole local top, whatever the header pads.
#[test]
fn a_replaced_header_neither_clears_nor_is_credited() {
    let mut kernel = header_padded();
    assert_eq!(frame(&mut kernel, 4).1, 114.0, "as authored: 62 + 52");
    kernel
        .set_host_cover(2, Some(HostCover::Edges([106.0, 0.0, 0.0, 0.0])))
        .unwrap();
    kernel.set_host_cover(3, Some(HostCover::Whole)).unwrap();
    assert_eq!(frame(&mut kernel, 4).1, 106.0, "the route's whole top");
    let paddings = [(2, "0px"), (3, "calc(env(safe-area-inset-top) + 40px)")];
    for (batch, padding) in paddings {
        let op = Op::SetStyle {
            id: 3,
            patch: Box::new(style(&[(
                StyleId::PaddingTop,
                StyleValue::Text(padding.into()),
            )])),
        };
        kernel.apply(2, batch, &[op]).unwrap();
        assert_eq!(frame(&mut kernel, 4).1, 106.0, "{padding}");
    }
    let op = Op::SetChildren {
        id: 2,
        children: vec![4],
    };
    kernel.apply(2, 4, &[op]).unwrap();
    assert_eq!(frame(&mut kernel, 4).1, 106.0, "and gone");
}

#[test]
fn a_scroller_under_the_bar_takes_no_inset_from_the_header() {
    let mut kernel = header_padded();
    // The bar covers no top edge (a scroller goes under it, inset by the
    // platform); only the sides are covered.
    kernel
        .set_host_cover(2, Some(HostCover::Edges([0.0, 8.0, 0.0, 0.0])))
        .unwrap();
    kernel.set_host_cover(3, Some(HostCover::Whole)).unwrap();
    assert_eq!(
        frame(&mut kernel, 4).1,
        0.0,
        "the list starts at the top, under the bar"
    );
}

/// The route's list (4) as a scroller padded by 16 holding a column (5)
/// that fills its height (`flex: 1` in its flex column) and, when `long`,
/// a 2000-point box after it: trivia-quiz's results and a filled-height
/// screen (LLP 1116.000 D1.2).
fn scroller(long: bool) -> Kernel {
    let mut kernel = route();
    let flex_column = [
        (StyleId::Display, StyleValue::Text("flex".into())),
        (StyleId::FlexDirection, StyleValue::Text("column".into())),
        (StyleId::OverflowY, StyleValue::Text("scroll".into())),
        (StyleId::MinHeight, StyleValue::Number(0.0)),
        (StyleId::PaddingTop, StyleValue::Number(16.0)),
        (StyleId::PaddingBottom, StyleValue::Number(16.0)),
    ];
    let mut ops = vec![
        Op::SetStyle {
            id: 4,
            patch: Box::new(style(&flex_column)),
        },
        Op::CreateView {
            id: 5,
            node_type: NodeType::View,
        },
        Op::SetStyle {
            id: 5,
            patch: Box::new(style(&[(StyleId::FlexGrow, StyleValue::Number(1.0))])),
        },
        Op::CreateView {
            id: 6,
            node_type: NodeType::View,
        },
        Op::SetStyle {
            id: 6,
            patch: Box::new(style(&[
                (StyleId::Height, StyleValue::Number(2000.0)),
                (StyleId::FlexShrink, StyleValue::Number(0.0)),
            ])),
        },
    ];
    ops.push(Op::SetChildren {
        id: 4,
        children: if long { vec![5, 6] } else { vec![5] },
    });
    kernel.apply(1, 2, &ops).unwrap();
    // The bar replaces the header and the list runs under it, as a route's
    // collapsing scroller does on iOS.
    kernel.set_host_cover(3, Some(HostCover::Whole)).unwrap();
    kernel
}

#[test]
fn a_scroller_the_platform_insets_lays_out_its_content_between_the_insets() {
    let n = StyleValue::Number;
    let text = |t: &'static str| StyleValue::Text(t.into());
    // The list (4) holds a filled column (5) and a footer pinned to its bottom (7)
    let mut kernel = scroller(false);
    let footer = [
        (StyleId::PositionType, text("absolute")),
        (StyleId::Bottom, n(0.0)),
        (StyleId::Left, n(0.0)),
        (StyleId::Right, n(0.0)),
        (StyleId::Height, n(40.0)),
    ];
    let ops = [
        Op::SetStyle {
            id: 4,
            patch: Box::new(style(&[(StyleId::PositionType, text("relative"))])),
        },
        Op::CreateView {
            id: 7,
            node_type: NodeType::View,
        },
        Op::SetStyle {
            id: 7,
            patch: Box::new(style(&footer)),
        },
        Op::SetChildren {
            id: 4,
            children: vec![5, 7],
        },
    ];
    kernel.apply(2, 3, &ops).unwrap();
    assert_eq!(frame(&mut kernel, 4), (0.0, 0.0, 390.0, 844.0));
    assert_eq!(
        frame(&mut kernel, 5),
        (0.0, 16.0, 390.0, 812.0),
        "as authored"
    );
    assert_eq!(frame(&mut kernel, 7).1, 804.0, "as authored");
    // UIKit insets the scroller by the large title's 140 and the home
    // indicator's 34: the filled column and the footer end where the
    // indicator begins once the title's inset moves the content down, and
    // the column keeps its start.
    kernel
        .set_host_cover(4, Some(HostCover::Inset([140.0, 34.0])))
        .unwrap();
    assert_eq!(
        frame(&mut kernel, 4),
        (0.0, 0.0, 390.0, 844.0),
        "the scroller keeps its box"
    );
    assert_eq!(frame(&mut kernel, 5), (0.0, 16.0, 390.0, 638.0));
    assert_eq!(frame(&mut kernel, 7).1, 630.0, "844 - 174 - 40");
    let list = kernel.node(4).unwrap().key;
    assert_eq!(
        kernel.resolved_padding(list),
        Some((0.0, 16.0, 0.0, 16.0)),
        "CSS's"
    );
    assert_eq!(
        kernel.layout_padding(list),
        Some((0.0, 16.0, 0.0, 16.0)),
        "the insets are a gutter, not padding"
    );
    assert_eq!(
        kernel.node(4).unwrap().content.1,
        670.0,
        "the extent the platform adds its insets to, where the column's end padding and the footer end"
    );
    // A scroller written `overflow-x: hidden` lays out the same
    let ops = [Op::SetStyle {
        id: 4,
        patch: Box::new(style(&[(StyleId::OverflowX, text("hidden"))])),
    }];
    kernel.apply(3, 4, &ops).unwrap();
    assert_eq!(frame(&mut kernel, 5).3, 638.0, "overflow-x: hidden");
    assert_eq!(
        kernel.node(4).unwrap().content.1,
        670.0,
        "overflow-x: hidden"
    );
    kernel.set_host_cover(4, None).unwrap();
    assert_eq!(frame(&mut kernel, 5).3, 812.0, "cleared, as authored");
}

#[test]
fn scrolling_content_keeps_its_extent_under_an_inset() {
    let mut kernel = scroller(true);
    kernel
        .set_host_cover(4, Some(HostCover::Inset([140.0, 34.0])))
        .unwrap();
    assert_eq!(frame(&mut kernel, 6), (0.0, 16.0, 390.0, 2000.0));
    assert_eq!(kernel.node(4).unwrap().content.1, 2032.0, "16 + 2000 + 16");
    assert!(kernel
        .set_host_cover(4, Some(HostCover::Inset([f32::NAN, 0.0])))
        .is_err());
}

#[test]
fn a_sticky_box_in_an_inset_scroller_sticks_until_the_content_ends() {
    let text = |t: &'static str| StyleValue::Text(t.into());
    // The scroller's first child (5) sticks to its top; its containing block
    // is the content the scroller scrolls, which ends after the 2000-point
    // child (6) as with no inset, wherever the insets end the content box
    let mut kernel = scroller(true);
    let sticky = [
        (StyleId::PositionType, text("sticky")),
        (StyleId::Top, StyleValue::Number(0.0)),
    ];
    let ops = [Op::SetStyle {
        id: 5,
        patch: Box::new(style(&sticky)),
    }];
    kernel.apply(2, 3, &ops).unwrap();
    kernel
        .set_host_cover(4, Some(HostCover::Inset([140.0, 34.0])))
        .unwrap();
    frame(&mut kernel, 4);
    let key = kernel.arena().key_of(5).unwrap();
    let c = kernel.sticky_constraint(key).unwrap();
    assert_eq!(c.limit[3], 2016.0, "16 + 2000, where the content ends");
}

#[test]
fn an_inset_scroller_keeps_the_box_its_sizes_give_it() {
    let text = |t: &'static str| StyleValue::Text(t.into());
    let n = StyleValue::Number;
    let share = [
        (StyleId::FlexGrow, n(1.0)),
        (StyleId::FlexShrink, n(1.0)),
        (StyleId::FlexBasis, StyleValue::Percent(0.0)),
    ];
    let pinned = [
        (StyleId::Height, n(100.0)),
        (StyleId::AspectRatio, n(2.0)),
        (StyleId::AlignSelf, text("flex-start")),
    ];
    // A scroller beside a sibling (7, `flex: 1 1 0%`) keeps the box its
    // definite sizes give it. Under `content-box`: a height (an arranged pane
    // fills its allocation with `height: 100%`), a minimum, a maximum, a flex
    // basis, and a share of the column, which Taffy floors at the box's
    // padding and border (insets laid out as padding take 525, not 438).
    // Under both sizings: a height too small for the padding and the insets,
    // whose box stays with an empty content box, and the width an aspect
    // ratio gives.
    let cases: [(&str, &[(StyleId, StyleValue)]); 9] = [
        (
            "content-box",
            &[(StyleId::Height, StyleValue::Percent(100.0))],
        ),
        (
            "content-box",
            &[(StyleId::Height, n(10.0)), (StyleId::MinHeight, n(600.0))],
        ),
        (
            "content-box",
            &[
                (StyleId::Height, StyleValue::Percent(100.0)),
                (StyleId::MaxHeight, n(300.0)),
            ],
        ),
        ("content-box", &[(StyleId::FlexBasis, n(400.0))]),
        ("content-box", &share),
        ("content-box", &[(StyleId::Height, n(100.0))]),
        ("border-box", &[(StyleId::Height, n(100.0))]),
        ("content-box", &pinned),
        ("border-box", &pinned),
    ];
    for (sizing, size) in cases {
        let mut kernel = scroller(false);
        let mut rows = vec![
            (StyleId::FlexGrow, n(0.0)),
            (StyleId::FlexShrink, n(0.0)),
            (StyleId::BoxSizing, text(sizing)),
        ];
        rows.extend_from_slice(size);
        let ops = [
            Op::SetStyle {
                id: 4,
                patch: Box::new(style(&rows)),
            },
            Op::CreateView {
                id: 7,
                node_type: NodeType::View,
            },
            Op::SetStyle {
                id: 7,
                patch: Box::new(style(&share)),
            },
            Op::SetChildren {
                id: 2,
                children: vec![3, 4, 7],
            },
        ];
        kernel.apply(2, 3, &ops).unwrap();
        let authored = frame(&mut kernel, 4);
        kernel
            .set_host_cover(4, Some(HostCover::Inset([140.0, 34.0])))
            .unwrap();
        assert_eq!(frame(&mut kernel, 4), authored, "{sizing} {size:?}");
        // The filled column rests between the padding and the insets
        assert_eq!(
            frame(&mut kernel, 5).3,
            (authored.3 - 32.0 - 174.0).max(0.0),
            "{sizing} {size:?}"
        );
    }
    // A size taken from content counts the insets, as a web scroller's
    // counts a horizontal scrollbar's gutter: a scroller growing from its
    // automatic basis takes more of the column under them
    let mut kernel = scroller(false);
    let ops = [
        Op::SetStyle {
            id: 4,
            patch: Box::new(style(&[(StyleId::FlexGrow, n(1.0))])),
        },
        Op::CreateView {
            id: 7,
            node_type: NodeType::View,
        },
        Op::SetStyle {
            id: 7,
            patch: Box::new(style(&share)),
        },
        Op::SetChildren {
            id: 2,
            children: vec![3, 4, 7],
        },
    ];
    kernel.apply(2, 3, &ops).unwrap();
    assert_eq!(
        (frame(&mut kernel, 4).3, frame(&mut kernel, 7).3),
        (438.0, 406.0)
    );
    kernel
        .set_host_cover(4, Some(HostCover::Inset([140.0, 34.0])))
        .unwrap();
    assert_eq!(
        (frame(&mut kernel, 4).3, frame(&mut kernel, 7).3),
        (525.0, 319.0),
        "the scroller and its sibling"
    );
}

#[test]
fn a_multi_column_scroller_lays_its_columns_out_inside_its_gutters() {
    let n = StyleValue::Number;
    let text = |t: &'static str| StyleValue::Text(t.into());
    // A two-column list (4) whose rows (5, then 20 to 25) are 100 and 20
    // points tall
    let columns = |extra: &[(StyleId, StyleValue)], rows: u32| {
        let mut kernel = scroller(false);
        let mut list = vec![
            (StyleId::Display, text("block")),
            (StyleId::ColumnCount, n(2.0)),
            (StyleId::BoxSizing, text("border-box")),
            (StyleId::FlexShrink, n(0.0)),
        ];
        list.extend_from_slice(extra);
        let mut ops = vec![
            Op::SetStyle {
                id: 4,
                patch: Box::new(style(&list)),
            },
            Op::SetStyle {
                id: 5,
                patch: Box::new(style(&[(StyleId::Height, n(100.0))])),
            },
        ];
        for id in 20..20 + rows {
            ops.push(Op::CreateView {
                id,
                node_type: NodeType::View,
            });
            ops.push(Op::SetStyle {
                id,
                patch: Box::new(style(&[(StyleId::Height, n(20.0))])),
            });
        }
        if rows > 0 {
            ops.push(Op::SetChildren {
                id: 4,
                children: (20..20 + rows).collect(),
            });
        }
        kernel.apply(2, 3, &ops).unwrap();
        kernel
    };
    // Its extent is at least its client box, which the platform's insets and
    // a native scrollbar's gutters are outside of (CSS 2.1 §11.1.1): a list
    // that fits does not scroll by them. A vertical scrollbar's gutter is at
    // the end edge, which is the left in right-to-left text, and the first
    // column starts at the client box's start edge. Points are from the
    // border box's left (AppKit's `kernelDocumentOrigin` takes a left gutter
    // off), so the client box's right edge is 375 left to right and 390
    // right to left.
    let full = [(StyleId::Height, StyleValue::Percent(100.0))];
    let mut kernel = columns(&full, 0);
    kernel
        .set_host_cover(4, Some(HostCover::Inset([140.0, 34.0])))
        .unwrap();
    frame(&mut kernel, 4);
    assert_eq!(kernel.node(4).unwrap().content.1, 670.0, "844 - 174");
    for (direction, start, right) in [("ltr", 0.0, 375.0), ("rtl", 390.0, 390.0)] {
        let mut kernel = columns(&[full[0].clone(), (StyleId::Direction, text(direction))], 0);
        kernel
            .set_host_cover(4, Some(HostCover::Gutter([15.0, 0.0])))
            .unwrap();
        let first = frame(&mut kernel, 5);
        let edge = if direction == "rtl" {
            first.0 + first.2
        } else {
            first.0
        };
        assert_eq!(edge, start, "{direction}: the first column's start");
        assert_eq!(
            kernel.node(4).unwrap().content.0,
            right,
            "{direction}: the client box's right edge"
        );
    }
    // Six 20-point rows under an 80-point cap: the columns end above the
    // bottom gutter, 33 points tall, so each holds one row and they run on
    // sideways
    let mut kernel = columns(&[(StyleId::MaxHeight, n(80.0))], 6);
    kernel
        .set_host_cover(4, Some(HostCover::Gutter([0.0, 15.0])))
        .unwrap();
    frame(&mut kernel, 4);
    assert_eq!(
        kernel.node(4).unwrap().content,
        (1202.0, 65.0),
        "six 187-point columns with their gaps, and the client box above the gutter"
    );
}

/// The route's list (4) as a column holding a 40-point header (5) and then
/// a scroller (6) filling the rest, with a footer (7) pinned to the route's
/// bottom edge: a sheet component's root column, the scroller two boxes
/// down from the route (keyboard-form's pinned tab).
fn nested() -> Kernel {
    let mut kernel = route();
    let grow = |extra: &[(StyleId, StyleValue)]| {
        let mut rows = vec![
            (StyleId::FlexGrow, StyleValue::Number(1.0)),
            (StyleId::MinHeight, StyleValue::Number(0.0)),
        ];
        rows.extend_from_slice(extra);
        style(&rows)
    };
    let view = |id| Op::CreateView {
        id,
        node_type: NodeType::View,
    };
    let ops = vec![
        Op::SetStyle {
            id: 4,
            patch: Box::new(grow(&[
                (StyleId::Display, StyleValue::Text("flex".into())),
                (StyleId::FlexDirection, StyleValue::Text("column".into())),
            ])),
        },
        view(5),
        Op::SetStyle {
            id: 5,
            patch: Box::new(style(&[
                (StyleId::Height, StyleValue::Number(40.0)),
                (StyleId::FlexShrink, StyleValue::Number(0.0)),
            ])),
        },
        view(6),
        Op::SetStyle {
            id: 6,
            patch: Box::new(grow(&[(
                StyleId::OverflowY,
                StyleValue::Text("scroll".into()),
            )])),
        },
        view(7),
        Op::SetStyle {
            id: 7,
            patch: Box::new(style(&[
                (StyleId::PositionType, StyleValue::Text("absolute".into())),
                (StyleId::Bottom, StyleValue::Number(0.0)),
                (StyleId::Left, StyleValue::Number(0.0)),
                (StyleId::Right, StyleValue::Number(0.0)),
                (StyleId::Height, StyleValue::Number(40.0)),
            ])),
        },
        Op::SetChildren {
            id: 4,
            children: vec![5, 6],
        },
        Op::SetChildren {
            id: 2,
            children: vec![3, 4, 7],
        },
    ];
    kernel.apply(0, 1, &ops).unwrap();
    // The tab bar and home indicator, 83, over the route's bottom.
    kernel
        .set_host_cover(2, Some(HostCover::Edges([0.0, 0.0, 83.0, 0.0])))
        .unwrap();
    kernel
}

#[test]
fn a_scroller_two_boxes_down_reaches_under_the_bar_and_the_footer_still_rests_on_it() {
    let mut kernel = nested();
    assert_eq!(
        frame(&mut kernel, 6),
        (0.0, 92.0, 390.0, 669.0),
        "padded above the bar while the platform insets nothing"
    );
    assert_eq!(frame(&mut kernel, 7), (0.0, 721.0, 390.0, 40.0));
    // The platform insets the scroller at the bottom edge by the bar's 83:
    // the route pads nothing there, however deep the scroller sits.
    kernel
        .set_host_cover(6, Some(HostCover::Inset([0.0, 83.0])))
        .unwrap();
    assert_eq!(
        frame(&mut kernel, 4),
        (0.0, 52.0, 390.0, 792.0),
        "the column reaches the bottom edge"
    );
    assert_eq!(frame(&mut kernel, 6), (0.0, 92.0, 390.0, 752.0));
    assert_eq!(
        frame(&mut kernel, 7),
        (0.0, 721.0, 390.0, 40.0),
        "the footer still rests on the bar"
    );
    kernel.set_host_cover(6, None).unwrap();
    assert_eq!(
        frame(&mut kernel, 6),
        (0.0, 92.0, 390.0, 669.0),
        "the padding comes back with the cover"
    );
    // Inset at its top alone (a keyboard ended the route above the bars),
    // then at its bottom again as the keyboard goes: the route re-derived
    // each time, though the scroller was inset throughout.
    kernel
        .set_host_cover(6, Some(HostCover::Inset([116.0, 0.0])))
        .unwrap();
    assert_eq!(frame(&mut kernel, 6).3, 669.0);
    kernel
        .set_host_cover(6, Some(HostCover::Inset([116.0, 83.0])))
        .unwrap();
    assert_eq!(
        frame(&mut kernel, 6),
        (0.0, 92.0, 390.0, 752.0),
        "under the bar again once it is inset at the bottom"
    );
}

/// theme2's Q1: a scroller the host marks off the kernel's chain counts for
/// nothing, so the route keeps its padding: an inset twice at worst, never
/// the scroller above the bar for good nor the footer under it.
#[test]
fn a_marked_scroller_off_the_chain_leaves_the_route_its_padding() {
    let mut kernel = nested();
    // A box after the scroller in its column: the chain ends there.
    let after = vec![
        Op::CreateView {
            id: 8,
            node_type: NodeType::View,
        },
        Op::SetStyle {
            id: 8,
            patch: Box::new(style(&[
                (StyleId::Height, StyleValue::Number(10.0)),
                (StyleId::FlexShrink, StyleValue::Number(0.0)),
            ])),
        },
        Op::SetChildren {
            id: 4,
            children: vec![5, 6, 8],
        },
    ];
    kernel.apply(1, 2, &after).unwrap();
    kernel
        .set_host_cover(6, Some(HostCover::Inset([0.0, 83.0])))
        .unwrap();
    assert_eq!(
        frame(&mut kernel, 4),
        (0.0, 52.0, 390.0, 709.0),
        "the route still pads above the bar"
    );
    assert_eq!(frame(&mut kernel, 7), (0.0, 721.0, 390.0, 40.0));
    // Taken out of the column, the scroller ends it again: the route,
    // re-derived for a change two boxes down, pads nothing.
    kernel
        .apply(
            2,
            3,
            &[Op::SetChildren {
                id: 4,
                children: vec![5, 6],
            }],
        )
        .unwrap();
    assert_eq!(frame(&mut kernel, 4), (0.0, 52.0, 390.0, 792.0));
    // A scroller inside the edge scroller is past where the chain stops.
    kernel.set_host_cover(6, None).unwrap();
    let inner = vec![
        Op::CreateView {
            id: 9,
            node_type: NodeType::View,
        },
        Op::SetStyle {
            id: 9,
            patch: Box::new(style(&[
                (StyleId::Height, StyleValue::Number(100.0)),
                (StyleId::OverflowY, StyleValue::Text("scroll".into())),
            ])),
        },
        Op::SetChildren {
            id: 6,
            children: vec![9],
        },
    ];
    kernel.apply(3, 4, &inner).unwrap();
    kernel
        .set_host_cover(9, Some(HostCover::Inset([0.0, 83.0])))
        .unwrap();
    assert_eq!(frame(&mut kernel, 4), (0.0, 52.0, 390.0, 709.0));
}

/// Astra's and Grok's round-1 finding: a box's `position` or `display` moves
/// it onto the chain or off it with the scroller's inset unchanged, and the
/// route is derived again.
#[test]
fn a_box_moved_onto_the_chain_or_off_it_re_derives_the_route() {
    let mut kernel = nested();
    kernel
        .set_host_cover(6, Some(HostCover::Inset([0.0, 83.0])))
        .unwrap();
    assert_eq!(frame(&mut kernel, 4).3, 792.0);
    // The footer taken into the flow ends the route: it pads above the bar.
    let flow = |kernel: &mut Kernel, batch: u64, position: &str| {
        kernel
            .apply(
                1,
                batch,
                &[Op::SetStyle {
                    id: 7,
                    patch: Box::new(style(&[(
                        StyleId::PositionType,
                        StyleValue::Text(position.into()),
                    )])),
                }],
            )
            .unwrap();
    };
    flow(&mut kernel, 2, "relative");
    assert_eq!(
        frame(&mut kernel, 4).3,
        669.0,
        "the footer in the flow, the route padded"
    );
    flow(&mut kernel, 3, "absolute");
    assert_eq!(frame(&mut kernel, 4).3, 792.0);
    // The scroller hidden, its inset still said: the chain ends at the header.
    kernel
        .apply(
            1,
            4,
            &[Op::SetStyle {
                id: 6,
                patch: Box::new(style(&[(
                    StyleId::Display,
                    StyleValue::Text("none".into()),
                )])),
            }],
        )
        .unwrap();
    assert_eq!(frame(&mut kernel, 4).3, 709.0, "the route padded again");
}

/// Round 2 (Astra, Grok): a footer attached, or moved away, after the
/// route's cover has settled is placed past it, or no longer; a box that
/// clips across (`overflow-x: hidden`, so its `visible` height scrolls as
/// the engine computes it) is a scroller the chain stops at; and the
/// padding the route reports is what layout pads.
#[test]
fn a_footer_attached_late_a_clipping_box_and_the_reported_padding() {
    let mut kernel = nested();
    kernel
        .set_host_cover(6, Some(HostCover::Inset([0.0, 83.0])))
        .unwrap();
    assert_eq!(frame(&mut kernel, 4).3, 792.0);
    let late = vec![
        Op::CreateView {
            id: 8,
            node_type: NodeType::View,
        },
        Op::SetStyle {
            id: 8,
            patch: Box::new(style(&[
                (StyleId::PositionType, StyleValue::Text("absolute".into())),
                (StyleId::Bottom, StyleValue::Number(0.0)),
                (StyleId::Left, StyleValue::Number(0.0)),
                (StyleId::Height, StyleValue::Number(20.0)),
                (StyleId::Width, StyleValue::Number(20.0)),
            ])),
        },
        Op::SetChildren {
            id: 2,
            children: vec![3, 4, 7, 8],
        },
    ];
    kernel.apply(1, 2, &late).unwrap();
    assert_eq!(
        frame(&mut kernel, 8).1,
        844.0 - 83.0 - 20.0,
        "attached late, on the bar"
    );
    // Moved into the header, which no bar covers: placed as written, at the
    // foot of its containing block (the positioned route), under the bar.
    let moved = [
        Op::SetChildren {
            id: 2,
            children: vec![3, 4, 7],
        },
        Op::SetChildren {
            id: 3,
            children: vec![8],
        },
    ];
    kernel.apply(1, 3, &moved).unwrap();
    assert_eq!(
        frame(&mut kernel, 8).1,
        844.0 - 20.0,
        "no longer past a cover"
    );
    // The route's own padding at its bottom, as layout pads it: the
    // authored 16, none of the bar's.
    kernel
        .apply(
            1,
            4,
            &[Op::SetStyle {
                id: 2,
                patch: Box::new(style(&[(StyleId::PaddingBottom, StyleValue::Number(16.0))])),
            }],
        )
        .unwrap();
    frame(&mut kernel, 4);
    let key = kernel.node(2).unwrap().key;
    assert_eq!(kernel.layout_padding(key).unwrap().3, 16.0);
    assert_eq!(kernel.resolved_padding(key).unwrap().3, 16.0);
    assert_eq!(kernel.cover_bottom(key), 0.0);
    // The column clipping across scrolls as the engine computes it: the
    // inset below it is past the chain's end, and the route pads again.
    kernel
        .apply(
            1,
            5,
            &[Op::SetStyle {
                id: 4,
                patch: Box::new(style(&[(
                    StyleId::OverflowX,
                    StyleValue::Text("hidden".into()),
                )])),
            }],
        )
        .unwrap();
    assert_eq!(frame(&mut kernel, 4).3, 844.0 - 83.0 - 16.0 - 52.0);
    assert_eq!(kernel.cover_bottom(key), 83.0);
}

/// An absolute child at `top: 0` of a covered route is placed past the
/// route's clearance (the bar ends there), credited or not, and at the
/// route's own top where the route opts its top out (LLP 1123 D3, D4).
#[test]
fn an_absolute_child_is_placed_past_the_clearance_its_route_keeps() {
    let mut kernel = header_padded();
    let badge = [
        Op::CreateView {
            id: 5,
            node_type: NodeType::View,
        },
        Op::SetStyle {
            id: 5,
            patch: Box::new(style(&[
                (StyleId::PositionType, StyleValue::Text("absolute".into())),
                (StyleId::Top, StyleValue::Number(0.0)),
                (StyleId::Left, StyleValue::Number(0.0)),
                (StyleId::Width, StyleValue::Number(10.0)),
                (StyleId::Height, StyleValue::Number(10.0)),
            ])),
        },
        Op::SetChildren {
            id: 2,
            children: vec![3, 4, 5],
        },
    ];
    kernel.apply(1, 3, &badge).unwrap();
    kernel
        .set_host_cover(2, Some(HostCover::Edges([106.0, 0.0, 0.0, 0.0])))
        .unwrap();
    kernel.set_host_cover(3, Some(HostCover::Whole)).unwrap();
    assert_eq!(frame(&mut kernel, 5).1, 106.0, "the bar's bottom");
    let route = |kernel: &mut Kernel, batch, row, value: &str| {
        let op = Op::SetStyle {
            id: 2,
            patch: Box::new(style(&[(row, StyleValue::Text(value.into()))])),
        };
        kernel.apply(1, batch, &[op]).unwrap();
        frame(kernel, 5).1
    };
    let env = "env(safe-area-inset-top)";
    let credited = route(&mut kernel, 4, StyleId::PaddingTop, env);
    assert_eq!(credited, 106.0, "credited, the bar still ends there");
    let off = route(&mut kernel, 5, StyleId::SafeArea, "top");
    assert_eq!(off, 0.0, "opted out");
    assert_eq!(route(&mut kernel, 6, StyleId::SafeArea, "auto"), 106.0);
    kernel.set_host_cover(2, None).unwrap();
    assert_eq!(frame(&mut kernel, 5).1, 0.0, "placed as written");
}

/// Round 3 (Astra): an inset at its top alone on a box down the chain stops
/// the walk there; said or cleared, in either order with the scroller's,
/// the route is derived again.
#[test]
fn a_top_only_inset_down_the_chain_said_or_cleared_re_derives_the_route() {
    for scroller_first in [true, false] {
        let mut kernel = nested();
        let top = Some(HostCover::Inset([20.0, 0.0]));
        let edge = Some(HostCover::Inset([0.0, 83.0]));
        if scroller_first {
            kernel.set_host_cover(6, edge).unwrap();
            kernel.set_host_cover(4, top).unwrap();
        } else {
            kernel.set_host_cover(4, top).unwrap();
            kernel.set_host_cover(6, edge).unwrap();
        }
        assert_eq!(
            frame(&mut kernel, 4).3,
            709.0,
            "the walk stops at the column: {scroller_first}"
        );
        kernel.set_host_cover(4, None).unwrap();
        assert_eq!(
            frame(&mut kernel, 4).3,
            792.0,
            "and reaches the scroller: {scroller_first}"
        );
    }
}

#[test]
fn a_host_block_does_not_inherit_its_parents_covered_edges() {
    use exact_kernel::{BlockAxis, HostBlock};
    let mut k = route();
    k.apply(
        0,
        2,
        &[
            Op::CreateView {
                id: 5,
                node_type: NodeType::View,
            },
            Op::SetStyle {
                id: 5,
                patch: Box::new(style(&[
                    (StyleId::PositionType, StyleValue::Text("absolute".into())),
                    (StyleId::Top, StyleValue::Number(0.)),
                    (StyleId::Right, StyleValue::Number(0.)),
                    (StyleId::Bottom, StyleValue::Number(0.)),
                    (StyleId::Left, StyleValue::Number(0.)),
                ])),
            },
            Op::SetChildren {
                id: 2,
                children: vec![3, 4, 5],
            },
        ],
    )
    .unwrap();
    k.set_host_cover(2, Some(HostCover::Edges([20., 250., 0., 0.])))
        .unwrap();
    assert_eq!(frame(&mut k, 5), (0., 20., 140., 824.));
    k.set_host_block(
        5,
        Some(HostBlock {
            width: BlockAxis::Is(600.),
            height: BlockAxis::Is(500.),
        }),
    )
    .unwrap();
    assert_eq!(frame(&mut k, 5), (0., 0., 600., 500.));
    k.set_host_cover(2, Some(HostCover::Edges([30., 200., 0., 0.])))
        .unwrap();
    assert_eq!(frame(&mut k, 5), (0., 0., 600., 500.));
    k.set_host_block(5, None).unwrap();
    assert_eq!(frame(&mut k, 5), (0., 30., 190., 814.));
}
