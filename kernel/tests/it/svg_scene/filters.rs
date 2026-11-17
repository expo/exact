use super::*;
use exact_kernel::svg::filter::Op as FilterOp;

#[test]
fn drop_shadow_uses_the_cascaded_color() {
    use NodeType::*;
    for owner in [View, Svg, SvgGroup, SvgRect, SvgUse] {
        let mut d = Doc::new();
        let svg = d.node(
            Svg,
            &[(StyleId::Width, "100"), (StyleId::Height, "100")],
            &[],
        );
        let group = d.node(SvgGroup, &[], &[]);
        let rect = d.node(
            SvgRect,
            &[
                (StyleId::X, "10"),
                (StyleId::Y, "10"),
                (StyleId::Width, "20"),
                (StyleId::Height, "10"),
                (StyleId::Fill, "blue"),
                (StyleId::Filter, "drop-shadow(8px 8px 0px)"),
            ],
            &[],
        );
        let child = if owner == SvgUse {
            d.ops.push(prop(rect, PropId::Id, "shape"));
            let defs = d.node(SvgDefs, &[(StyleId::TextColor, "blue")], &[]);
            d.children(defs, &[rect]);
            d.children(svg, &[defs, group]);
            d.node(SvgUse, &[], &[(PropId::Href, "#shape")])
        } else {
            d.children(svg, &[group]);
            rect
        };
        let id = match owner {
            View => 1,
            Svg => svg,
            SvgGroup => group,
            _ => child,
        };
        d.ops.push(style(id, &[(StyleId::TextColor, "#ff0000")]));
        d.children(group, &[child]);
        let k = d.kernel(&[svg], 100.0);
        let s = resolve(&k, svg);
        let filter = find(&s.items, rect).unwrap().filter.as_ref().unwrap();
        assert_eq!(
            filter.primitives[0].op,
            FilterOp::DropShadow(0.0, 0.0, 8.0, 8.0, [1.0, 0.0, 0.0, 1.0]),
            "color on {owner:?}"
        );
    }
}

#[test]
fn filter_colors_follow_the_appearance() {
    use NodeType::*;
    for dark in [false, true] {
        for (scheme, host_dark) in [
            ("normal", dark),
            (if dark { "dark" } else { "light" }, !dark),
        ] {
            for color in ["light-dark(#00aa00, #ff00ff)", "currentcolor"] {
                let mut d = Doc::new();
                let svg = d.node(
                    Svg,
                    &[
                        (StyleId::Width, "100"),
                        (StyleId::Height, "100"),
                        (StyleId::ColorScheme, scheme),
                        (StyleId::TextColor, "light-dark(#00aa00, #ff00ff)"),
                    ],
                    &[],
                );
                let filter = d.node(SvgFilter, &[], &[(PropId::Id, "f")]);
                let mut primitives = Vec::new();
                for kind in [
                    "feFlood",
                    "feDropShadow",
                    "feDiffuseLighting",
                    "feSpecularLighting",
                ] {
                    let p = d.node(
                        SvgFe,
                        &[
                            (StyleId::FloodColor, color),
                            (StyleId::FloodOpacity, "0.5"),
                            (StyleId::LightingColor, color),
                        ],
                        &[(PropId::Fe, kind)],
                    );
                    if kind.ends_with("Lighting") {
                        let light = d.node(SvgFe, &[], &[(PropId::Fe, "feDistantLight")]);
                        d.children(p, &[light]);
                    }
                    primitives.push(p);
                }
                d.children(filter, &primitives);
                let chain = "url(#f) drop-shadow(8px 8px 0px) \
                    drop-shadow(8px 8px 0px light-dark(#00aa00, #ff00ff))";
                let rect = d.node(
                    SvgRect,
                    &[
                        (StyleId::Width, "20"),
                        (StyleId::Height, "10"),
                        (StyleId::Filter, chain),
                    ],
                    &[],
                );
                d.children(svg, &[filter, rect]);
                let k = d.kernel(&[svg], 100.0);
                let node = k.node(svg).unwrap();
                let s = scene::resolve(
                    &k,
                    &node,
                    scene::content_box(&k, &node),
                    &|_, _| None,
                    host_dark,
                );
                let filter = find(&s.items, rect).unwrap().filter.as_ref().unwrap();
                assert_eq!(filter.primitives.len(), 6);
                let rgb = if dark {
                    [1.0, 0.0, 1.0]
                } else {
                    [0.0, 170.0 / 255.0, 0.0]
                };
                for (i, p) in filter.primitives.iter().enumerate() {
                    match &p.op {
                        FilterOp::Flood(c) | FilterOp::DropShadow(.., c) => {
                            assert_eq!(c[..3], rgb, "{color}, dark={dark}, primitive {i}");
                            assert_eq!(c[3], if i < 2 { 0.5 } else { 1.0 });
                        }
                        FilterOp::Lighting(l) => assert_eq!(l.color, rgb, "{color}, dark={dark}"),
                        op => panic!("unexpected primitive: {op:?}"),
                    }
                }
            }
        }
    }
}

/// LLP 1055.000 D14: a filter's region against the box, primitive inputs by
/// name, and a list whose functions chain after the reference.
#[test]
fn filters_resolve_into_one_chain() {
    use exact_kernel::svg::filter::{Input, Op};
    use NodeType::*;
    let mut d = Doc::new();
    let svg = d.node(
        Svg,
        &[(StyleId::Width, "100"), (StyleId::Height, "100")],
        &[],
    );
    let filter = d.node(SvgFilter, &[], &[(PropId::Id, "f")]);
    let flood = d.node(
        SvgFe,
        &[(StyleId::FloodOpacity, "0.5")],
        &[(PropId::Fe, "feFlood"), (PropId::Result, "wash")],
    );
    let offset = d.node(
        SvgFe,
        &[],
        &[
            (PropId::Fe, "feOffset"),
            (PropId::In, "SourceAlpha"),
            (PropId::FeDx, "3"),
        ],
    );
    let comp = d.node(
        SvgFe,
        &[],
        &[
            (PropId::Fe, "feComposite"),
            (PropId::In, "wash"),
            (PropId::Operator, "in"),
        ],
    );
    d.children(filter, &[flood, offset, comp]);
    let rect = d.node(
        SvgRect,
        &[
            (StyleId::X, "10"),
            (StyleId::Y, "10"),
            (StyleId::Width, "50"),
            (StyleId::Height, "20"),
            (StyleId::Filter, "url(#f) blur(2px)"),
        ],
        &[],
    );
    d.children(svg, &[filter, rect]);
    let k = d.kernel(&[svg], 400.0);
    let s = resolve(&k, svg);
    let f = find(&s.items, rect)
        .unwrap()
        .filter
        .as_ref()
        .expect("filtered");
    let p = &f.primitives;
    assert_eq!(p.len(), 4, "three primitives and the blur");
    assert!(matches!(p[0].op, Op::Flood(c) if (c[3] - 0.5).abs() < 1e-6));
    assert_eq!(p[1].inputs[0], Input::SourceAlpha);
    assert!(matches!(p[1].op, Op::Offset(dx, _) if dx == 3.0));
    // `in="wash"` names the flood; `in2` is the previous result.
    assert_eq!(p[2].inputs, [Input::Result(0), Input::Result(1)]);
    // The blur's source is the reference's result.
    assert_eq!(p[3].inputs[0], Input::Result(2));
    assert!(p.iter().all(|q| q.linear == (q.op != Op::Blur(2.0, 2.0))));
    // −10%/−10%/120%/120% of the box, grown by the blur's reach.
    assert!(f.region.0 <= 5.0 && f.region.2 >= 60.0, "{:?}", f.region);
}

/// Filter Effects 1 §5: each function consumes the previous one's result,
/// so the region covers what the last function spreads from it.
#[test]
fn a_chains_region_grows_from_each_functions_result() {
    use NodeType::*;
    for (css, region) in [
        (
            "drop-shadow(10px 0 0 #000) drop-shadow(10px 0 0 #000)",
            (10.0, 10.0, 30.0, 2.0),
        ),
        (
            "drop-shadow(10px 0 0 #000) blur(2px)",
            (4.0, 4.0, 32.0, 14.0),
        ),
    ] {
        let mut d = Doc::new();
        let svg = d.node(
            Svg,
            &[(StyleId::Width, "100"), (StyleId::Height, "100")],
            &[],
        );
        let rect = d.node(
            SvgRect,
            &[
                (StyleId::X, "10"),
                (StyleId::Y, "10"),
                (StyleId::Width, "10"),
                (StyleId::Height, "2"),
                (StyleId::Filter, css),
            ],
            &[],
        );
        d.children(svg, &[rect]);
        let k = d.kernel(&[svg], 100.0);
        let s = resolve(&k, svg);
        let filter = find(&s.items, rect).unwrap().filter.as_ref().unwrap();
        assert_eq!(filter.region, region, "{css}");
    }
}
