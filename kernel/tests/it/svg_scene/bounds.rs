//! LLP 1011.001 D2: an inline `svg` is bounded as an SVG image is. A few
//! elements that name each other can expand past what a host can hold;
//! the scene then draws nothing and names the limit it passed.
use super::*;
use exact_kernel::svg::scene::Limit;

/// An `svg` of 100 × 100 holding `body`'s elements, and its id.
fn svg(d: &mut Doc, body: &[u32]) -> u32 {
    let svg = d.node(
        NodeType::Svg,
        &[(StyleId::Width, "100"), (StyleId::Height, "100")],
        &[],
    );
    d.children(svg, body);
    svg
}

#[test]
fn use_doubling_stops_at_the_expansion_limit() {
    use NodeType::*;
    let mut d = Doc::new();
    let defs = d.node(SvgDefs, &[], &[]);
    let zigzag: String = (0..100).map(|i| format!("l1 {}", i % 2)).collect();
    let below = d.node(
        SvgPath,
        &[],
        &[(PropId::Id, "l0"), (PropId::D, &format!("M0 0{zigzag}"))],
    );
    let mut levels = vec![below];
    // Each level uses the one below twice: 2^16 copies of the path.
    for k in 1..=16 {
        let g = d.node(SvgGroup, &[], &[(PropId::Id, &format!("l{k}"))]);
        let href = format!("#l{}", k - 1);
        let a = d.node(SvgUse, &[], &[(PropId::Href, &href)]);
        let b = d.node(SvgUse, &[], &[(PropId::Href, &href)]);
        d.children(g, &[a, b]);
        levels.push(g);
    }
    d.children(defs, &levels);
    let top = d.node(SvgUse, &[], &[(PropId::Href, "#l16")]);
    let svg = svg(&mut d, &[defs, top]);
    let k = d.kernel(&[svg], 800.0);
    let started = std::time::Instant::now();
    let s = resolve(&k, svg);
    assert_eq!(s.over, Some(Limit::Expansion));
    assert!(s.items.is_empty(), "a scene over its limit draws nothing");
    assert!(started.elapsed().as_secs() < 5, "{:?}", started.elapsed());
}

#[test]
fn a_use_box_stops_at_the_expansion_limit() {
    use NodeType::*;
    let mut d = Doc::new();
    let defs = d.node(SvgDefs, &[], &[]);
    let below = d.node(
        SvgRect,
        &[(StyleId::Width, "1"), (StyleId::Height, "1")],
        &[(PropId::Id, "l0")],
    );
    let mut levels = vec![below];
    // Each level uses the one below four times: 4^12 copies of the rect.
    for k in 1..=12 {
        let g = d.node(SvgGroup, &[], &[(PropId::Id, &format!("l{k}"))]);
        let href = format!("#l{}", k - 1);
        let uses: Vec<u32> = (0..4)
            .map(|_| d.node(SvgUse, &[], &[(PropId::Href, &href)]))
            .collect();
        d.children(g, &uses);
        levels.push(g);
    }
    d.children(defs, &levels);
    // Its box is asked for before its instance is walked.
    let top = d.node(
        SvgUse,
        &[
            (StyleId::TransformBox, "fill-box"),
            (StyleId::Rotate, "10"),
            (StyleId::Filter, "blur(1px)"),
        ],
        &[(PropId::Href, "#l12")],
    );
    let svg = svg(&mut d, &[defs, top]);
    let k = d.kernel(&[svg], 800.0);
    let started = std::time::Instant::now();
    let s = resolve(&k, svg);
    assert_eq!(s.over, Some(Limit::Expansion));
    assert!(started.elapsed().as_secs() < 5, "{:?}", started.elapsed());
}

#[test]
fn a_collapsed_calibrated_path_keeps_its_svg_scene() {
    for path in ["M20 30Z", "M20 30L20 30", "M20 30L100 30"] {
        let mut d = Doc::new();
        let line = d.node(
            NodeType::SvgPath,
            &[
                (StyleId::Fill, "none"),
                (StyleId::Stroke, "#ff0000"),
                (StyleId::StrokeDasharray, "0.2 0.2"),
            ],
            &[(PropId::D, path)],
        );
        d.ops.push(Op::SetProp {
            id: line,
            prop: PropId::PathLength,
            value: PropValue::Float(1.0),
        });
        let sibling = d.node(
            NodeType::SvgRect,
            &[(StyleId::Width, "10"), (StyleId::Height, "10")],
            &[],
        );
        let svg = svg(&mut d, &[line, sibling]);
        let k = d.kernel(&[svg], 100.0);
        let scene = resolve(&k, svg);
        assert_eq!(scene.over, None, "{path}");
        assert!(find(&scene.items, sibling).is_some(), "{path}");
        let Kind::Shape(shape) = &find(&scene.items, line).unwrap().kind else {
            panic!("expected a path");
        };
        let expected = if path.ends_with("100 30") {
            vec![16.0, 16.0]
        } else {
            Vec::new()
        };
        assert_eq!(shape.dash, expected, "{path}");
    }
}

#[test]
fn calibrated_dash_underflow_still_exceeds_the_scene_limit() {
    for (path, path_length, dashes, over) in [
        ("M0 0H100", 1e48, "1 1", Some(Limit::Dashes)),
        ("M0 0H100", 1e32, "1e-30 1e-30", Some(Limit::Dashes)),
        ("M0 0H100H0", 1e48, "1 1", Some(Limit::Dashes)),
        ("M0 0H1e-30", 1e30, "1 1", Some(Limit::Dashes)),
        ("M0 0H100", 1e48, "0 0", None),
    ] {
        let mut d = Doc::new();
        let line = d.node(
            NodeType::SvgPath,
            &[
                (StyleId::Stroke, "#000000"),
                (StyleId::StrokeDasharray, dashes),
            ],
            &[(PropId::D, path)],
        );
        d.ops.push(Op::SetProp {
            id: line,
            prop: PropId::PathLength,
            value: PropValue::Float(path_length),
        });
        let sibling = d.node(
            NodeType::SvgRect,
            &[(StyleId::Width, "10"), (StyleId::Height, "10")],
            &[],
        );
        let svg = svg(&mut d, &[line, sibling]);
        let k = d.kernel(&[svg], 100.0);
        let scene = resolve(&k, svg);
        assert_eq!(scene.over, over, "{path}, {path_length}, {dashes}");
        if over.is_some() {
            assert!(scene.items.is_empty(), "an over-budget scene draws nothing");
        } else {
            assert!(find(&scene.items, sibling).is_some());
            let Kind::Shape(shape) = &find(&scene.items, line).unwrap().kind else {
                panic!("expected a path");
            };
            assert!(shape.dash.is_empty(), "authored all-zero dashes stay solid");
        }
    }
}

#[test]
fn dashes_and_surfaces_have_limits_and_an_ordinary_svg_passes() {
    use NodeType::*;
    let dashed = |dasharray: &str| {
        let mut d = Doc::new();
        let line = d.node(
            SvgPath,
            &[
                (StyleId::Stroke, "#000000"),
                (StyleId::StrokeDasharray, dasharray),
            ],
            &[(PropId::D, "M0 0H100000")],
        );
        let svg = svg(&mut d, &[line]);
        let k = d.kernel(&[svg], 800.0);
        resolve(&k, svg)
    };
    // 100,000 units dashed 0.25 on, 0.25 off: 200,000 dashes.
    let s = dashed("0.25");
    assert_eq!(s.over, Some(Limit::Dashes));
    assert!(s.items.is_empty());
    let s = dashed("4 2");
    assert_eq!((s.over, s.items.len()), (None, 1));

    // `count` shapes under a filter whose region is `width` units a side.
    // Two million is an island of the largest size, with the filter's
    // working images 872 MB: three shapes draw, four do not.
    let filtered = |x: &str, width: &str, count: usize| {
        let mut d = Doc::new();
        let defs = d.node(SvgDefs, &[], &[]);
        let filter = d.node(
            SvgFilter,
            &[
                (StyleId::X, x),
                (StyleId::Y, x),
                (StyleId::Width, width),
                (StyleId::Height, width),
            ],
            &[(PropId::Id, "f"), (PropId::FilterUnits, "userSpaceOnUse")],
        );
        let flood = d.node(SvgFe, &[], &[(PropId::Fe, "feFlood")]);
        d.children(filter, &[flood]);
        d.children(defs, &[filter]);
        let mut body = vec![defs];
        for _ in 0..count {
            body.push(d.node(
                SvgRect,
                &[
                    (StyleId::Width, "50"),
                    (StyleId::Height, "50"),
                    (StyleId::Filter, "url(#f)"),
                ],
                &[],
            ));
        }
        let svg = svg(&mut d, &body);
        let k = d.kernel(&[svg], 800.0);
        resolve(&k, svg)
    };
    let s = filtered("-1000000", "2000000", 3);
    assert_eq!((s.over, s.items.len()), (None, 3));
    let s = filtered("-1000000", "2000000", 4);
    assert_eq!(s.over, Some(Limit::Surfaces));
    let s = filtered("-10", "120", 100);
    assert_eq!((s.over, s.items.len()), (None, 100));
}
