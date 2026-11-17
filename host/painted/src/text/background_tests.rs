use super::{
    backgrounds::{InlineBackground, InlinePaint},
    *,
};
use crate::paint::Shape;
use exact_kernel::{Dimension, Direction, Env, StyleProps, WhiteSpace};

const GRAY: [u8; 4] = [242, 242, 247, 255];
const ALL: [bool; 4] = [true; 4];
const LEFT: [bool; 4] = [true, false, false, true];
const RIGHT: [bool; 4] = [false, true, true, false];

fn engine() -> TextEngine {
    TextEngine::with_catalog(catalog::Catalog::from_bytes(
        &[
            include_bytes!("../../../../scripts/fixtures/fonts/assets/DejaVuSans.ttf").as_slice(),
            include_bytes!("../../tests/fonts/NotoSansArabic.ttf").as_slice(),
        ],
        "DejaVu Sans",
    ))
}

fn spec(texts: &[&str]) -> Spec {
    let style = StyleProps::default().with_rows(|rows| {
        rows.font_size = 16.0;
    });
    let mut spec = crate::paint::text_spec(&style, "");
    spec.runs = texts
        .iter()
        .map(|t| Run::from_style(t, exact_kernel::TextStyle::from_style(&style)))
        .collect();
    spec
}

fn background(id: u32) -> Option<InlineBackground> {
    Some(InlineBackground {
        color: GRAY,
        id,
        radii: [const { Dimension::Points(3.0) }; 4],
        reach: exact_kernel::InlineEdges::default(),
        above: 0.0,
        below: 0.0,
        border: None,
        outer: Vec::new(),
    })
}

fn fills(p: &Paragraph, backgrounds: &[Option<InlineBackground>]) -> Vec<Shape> {
    p.inline_backgrounds(backgrounds, &Env::default())
        .into_iter()
        .map(|paint| match paint {
            InlinePaint::Fill(s, c) => {
                assert_eq!(c, GRAY);
                s
            }
            InlinePaint::Border(..) => panic!("no border: {paint:?}"),
        })
        .collect()
}

/// Each paint as (shape, colour), a border's in its top colour.
fn paints(p: &Paragraph, backgrounds: &[Option<InlineBackground>]) -> Vec<(Shape, [u8; 4])> {
    p.inline_backgrounds(backgrounds, &Env::default())
        .into_iter()
        .map(|paint| match paint {
            InlinePaint::Fill(s, c) => (s, c),
            InlinePaint::Border(s, b) => (s, b.colors[0]),
        })
        .collect()
}

fn corners(shape: &Shape) -> [bool; 4] {
    shape.radii.map(|(x, y)| x > 0.0 && y > 0.0)
}

#[test]
fn inline_background_rounds_one_box_and_keeps_same_color_neighbors_separate() {
    let mut e = engine();
    let s = spec(&["left ", "marked", " right"]);
    let p = e.paragraph(&s, Some(400.0));
    let one = fills(&p, &[None, background(1), None]);
    assert_eq!(one.len(), 1);
    assert_eq!(corners(&one[0]), ALL);
    assert!(one[0].rect.0 > 0.0);
    let pair = e.paragraph(&spec(&["one", "two"]), None);
    let two = fills(&pair, &[background(1), background(2)]);
    assert_eq!(two.len(), 2);
    assert!(two.iter().all(|s| corners(s) == ALL));
    assert!((two[0].rect.0 + two[0].rect.2 - two[1].rect.0).abs() < 0.01);
    assert!(
        (two.iter().map(|s| s.rect.2).sum::<f32>() - pair.layout_runs().next().unwrap().line_w)
            .abs()
            < 0.01
    );
    let same = fills(&pair, &[background(1), background(1)]);
    assert_eq!(same.len(), 1, "canonical runs within one box join");
    assert_eq!(corners(&same[0]), ALL);
    assert!(
        Rc::ptr_eq(&p, &e.paragraph(&s, Some(400.0))),
        "paint must preserve the shape cache"
    );
}

#[test]
fn inline_background_slices_wrapped_and_hard_lines_in_css_direction() {
    let mut e = engine();
    for direction in [Direction::Ltr, Direction::Rtl] {
        for (text, width, white_space) in [
            ("one two three four", 90.0, WhiteSpace::Normal),
            ("one two\nthree four", 400.0, WhiteSpace::PreWrap),
        ] {
            let mut s = spec(&[text]);
            s.direction = direction;
            s.white_space = white_space;
            let p = e.paragraph(&s, Some(width));
            let pieces = fills(&p, &[background(1)]);
            assert_eq!(pieces.len(), 2, "{text:?} {direction:?}");
            let (first, last) = if direction == Direction::Ltr {
                (LEFT, RIGHT)
            } else {
                (RIGHT, LEFT)
            };
            assert_eq!(corners(&pieces[0]), first, "{text:?} {direction:?}");
            assert_eq!(corners(&pieces[1]), last, "{text:?} {direction:?}");
            assert!(pieces[1].rect.1 > pieces[0].rect.1);
        }
    }
}

#[test]
fn inline_background_shares_cross_box_ligatures_by_characters_in_glyph_direction() {
    let mut e = engine();
    for (parts, direction) in [(["f", "i"], Direction::Ltr), (["ل", "ا"], Direction::Rtl)] {
        let mut s = spec(&parts);
        s.direction = direction;
        let p = e.paragraph(&s, None);
        let line = p.layout_runs().next().unwrap();
        let advancing: Vec<_> = line.glyphs.iter().filter(|g| g.w > 0.0).collect();
        assert_eq!(
            advancing.len(),
            1,
            "fixture must shape {parts:?} as one ligature: {:?}",
            line.glyphs
        );
        let g = *advancing[0];
        assert_eq!(g.range(), 0..parts.concat().len());
        let pieces = fills(&p, &[background(1), background(2)]);
        assert_eq!(pieces.len(), 2);
        for piece in &pieces {
            assert_eq!(corners(piece), ALL);
            assert!((piece.rect.2 - g.w / 2.0).abs() < 0.01);
        }
        // Leave only the first logical character rounded to expose which half
        // of the ligature owns it; Arabic's first character is on the right.
        let single = fills(&p, &[background(1), None]);
        assert_eq!(single.len(), 1);
        let expected = g.x
            + if direction == Direction::Rtl {
                g.w / 2.0
            } else {
                0.0
            };
        assert!((single[0].rect.0 - expected).abs() < 0.01, "{parts:?}");
    }
}

#[test]
fn inline_background_rounds_visual_outer_edges_of_mixed_bidi_pieces() {
    let mut e = engine();
    for direction in [Direction::Ltr, Direction::Rtl] {
        let mut s = spec(&["abc ", "ابج 12", "3 دهو xyz"]);
        s.direction = direction;
        let p = e.paragraph(&s, Some(400.0));
        assert!(p
            .layout_runs()
            .flat_map(|l| l.glyphs)
            .all(|g| g.glyph_id != 0));
        let pieces = fills(&p, &[None, background(1), None]);
        assert_eq!(
            pieces.len(),
            2,
            "fixture separates the box's bidi pieces: {direction:?} {:?}",
            p.layout_runs().next().unwrap().glyphs
        );
        assert_eq!(corners(&pieces[0]), LEFT);
        assert_eq!(corners(&pieces[1]), RIGHT);
        assert!(pieces[0].rect.0 + pieces[0].rect.2 < pieces[1].rect.0);
    }
}

#[test]
fn inline_background_counts_zero_width_and_trimmed_endpoints() {
    let mut e = engine();
    let p = e.paragraph(&spec(&["\u{200b}hello"]), None);
    assert_eq!(corners(&fills(&p, &[background(1)])[0]), ALL);
    let p = e.paragraph(&spec(&["abc ", "restless"]), Some(50.0));
    assert_eq!(p.layout_runs().count(), 2);
    let pieces = fills(&p, &[background(1), None]);
    assert_eq!(pieces.len(), 1);
    assert_eq!(corners(&pieces[0]), ALL);
    let visible = e.paragraph(&spec(&["abc"]), None);
    assert!(
        (pieces[0].rect.2 - visible.layout_runs().next().unwrap().line_w).abs() < 0.01,
        "the trimmed endpoint must not add space to the background"
    );
}

#[test]
fn inline_background_ellipsis_never_supplies_the_box_end() {
    let mut e = engine();
    for direction in [Direction::Ltr, Direction::Rtl] {
        let mut s = spec(&["abcdefghijklmnopqrstuvwxyz"]);
        s.direction = direction;
        s.white_space = WhiteSpace::Nowrap;
        let p = e.paragraph(&s, Some(400.0));
        // A cut of one character still must not give the token that endpoint.
        for width in [80.0, p.width - 1.0] {
            let cut = p.ellipsized(width).unwrap();
            assert!(cut
                .layout_runs()
                .next()
                .unwrap()
                .glyphs
                .iter()
                .any(|g| g.range().is_empty()));
            let pieces = fills(&cut, &[background(1)]);
            assert_eq!(pieces.len(), 1);
            // Latin under RTL is truncated at the visual left, which removes
            // its first logical character. CSS direction puts the end there.
            assert_eq!(corners(&pieces[0]), LEFT);
        }
    }
    let mut s = spec(&["one two three four five"]);
    s.line_clamp = 1;
    let p = e.paragraph(&s, Some(90.0));
    assert_eq!(corners(&fills(&p, &[background(1)])[0]), LEFT);
    let flowed = e.paragraph_flow(
        &s,
        90.0,
        &[exact_textflow::FlowShape::Circle {
            cx: 80.0,
            cy: 10.0,
            r: 10.0,
        }],
        None,
    );
    assert!(!flowed.flow_incomplete());
    assert!(!flowed.fragments().is_empty());
    assert!(flowed
        .layout_runs()
        .next()
        .unwrap()
        .glyphs
        .last()
        .unwrap()
        .range()
        .is_empty());
    assert_eq!(corners(&fills(&flowed, &[background(1)])[0]), LEFT);
}

#[test]
fn inline_background_resolves_percentage_and_calc_radii_against_each_piece() {
    let mut e = engine();
    let p = e.paragraph(&spec(&["one two three four"]), Some(90.0));
    for radius in [
        Dimension::Percent(25.0),
        Dimension::Calc(10.0, 2.0),
        Dimension::parse_calc("calc(25% + 1px)").unwrap(),
    ] {
        let mut bg = background(1).unwrap();
        bg.radii = std::array::from_fn(|_| radius.clone());
        let pieces = fills(&p, &[Some(bg)]);
        assert_eq!(pieces.len(), 2);
        for (piece, corner) in [(&pieces[0], 0), (&pieces[1], 1)] {
            let expected = (
                radius.at_basis(&Env::default(), piece.rect.2),
                radius.at_basis(&Env::default(), piece.rect.3),
            );
            assert!((piece.radii[corner].0 - expected.0).abs() < 0.01);
            assert!((piece.radii[corner].1 - expected.1).abs() < 0.01);
        }
    }
    let mut bg = background(1).unwrap();
    bg.radii = [const { Dimension::Percent(100.0) }; 4];
    let p = e.paragraph(&spec(&["round"]), None);
    let piece = fills(&p, &[Some(bg)])[0];
    assert_eq!(
        piece.radii,
        [(piece.rect.2 / 2.0, piece.rect.3 / 2.0); 4],
        "CSS reduces all radii by one factor"
    );
}

#[test]
fn inline_background_zero_radius_keeps_the_square_path() {
    let mut e = engine();
    let p = e.paragraph(&spec(&["one", "two"]), None);
    let backgrounds = [1, 2].map(|id| {
        Some(InlineBackground {
            radii: [const { Dimension::Points(0.0) }; 4],
            ..background(id).unwrap()
        })
    });
    let actual = p.inline_backgrounds(&backgrounds, &Env::default());
    let expected: Vec<_> = p
        .run_backgrounds(&[Some(GRAY), Some(GRAY)])
        .into_iter()
        .map(|(r, c)| InlinePaint::Fill(Shape::rect(r), c))
        .collect();
    assert_eq!(actual, expected);
    assert_eq!(actual.len(), 1);
}

fn exclusion() -> [exact_textflow::FlowShape; 1] {
    [exact_textflow::FlowShape::RoundRect {
        x: 64.0,
        y: 0.0,
        width: 160.0,
        height: 100.0,
        radius: 0.0,
    }]
}

#[test]
fn flowed_background_generated_hyphen_has_source_owner() {
    let mut e = engine();
    let p = e.paragraph_flow(&spec(&["é", "ab\u{ad}cdefghij"]), 400.0, &exclusion(), None);
    assert!(!p.flow_incomplete());
    assert!(p.fragments()[0].hyphenated, "{:?}", p.fragments());
    let pieces = fills(&p, &[None, background(1)]);
    assert_eq!(pieces.len(), 2);
    assert_eq!(corners(&pieces[0]), LEFT);
    assert_eq!(corners(&pieces[1]), RIGHT);
    let line = p.layout_runs().next().unwrap();
    let hyphen = line.glyphs.last().unwrap();
    assert_eq!(hyphen.run(), 1);
    assert!((pieces[0].rect.0 + pieces[0].rect.2 - hyphen.x - hyphen.w).abs() < 0.01);
}

#[test]
fn flowed_background_generated_spaces_keep_breaks_open() {
    let mut e = engine();
    for text in ["Eab cd ef\ngh ij kl", "Éab cd ef\ngh ij kl"] {
        let p = e.paragraph_flow(&spec(&[text]), 300.0, &exclusion(), None);
        assert!(!p.flow_incomplete());
        assert!(p.fragments().len() > 1);
        let pieces = fills(&p, &[background(1)]);
        assert_eq!(corners(&pieces[0]), LEFT, "{text:?}");
        assert_eq!(corners(pieces.last().unwrap()), RIGHT, "{text:?}");
        for piece in &pieces[1..pieces.len() - 1] {
            assert_eq!(corners(piece), [false; 4], "{text:?}");
        }
    }
}

#[test]
fn flowed_background_counts_consumed_zero_width_endpoint() {
    let mut e = engine();
    let p = e.paragraph_flow(&spec(&["\u{200b}hello"]), 400.0, &exclusion(), None);
    assert!(!p.flow_incomplete());
    let pieces = fills(&p, &[background(1)]);
    assert_eq!(pieces.len(), 1);
    assert_eq!(corners(&pieces[0]), ALL);
}

#[test]
fn flowed_background_counts_consumed_space_endpoint() {
    let mut e = engine();
    let p = e.paragraph_flow(&spec(&["abc ", "restless"]), 400.0, &exclusion(), None);
    assert!(!p.flow_incomplete());
    assert_eq!(p.fragments().len(), 2);
    let pieces = fills(&p, &[background(1), None]);
    assert_eq!(pieces.len(), 1);
    assert_eq!(corners(&pieces[0]), ALL);
    let visible = e.paragraph(&spec(&["abc"]), None);
    assert!((pieces[0].rect.2 - visible.layout_runs().next().unwrap().line_w).abs() < 0.01);
}

#[test]
fn inline_background_many_runs() {
    let mut e = engine();
    for count in [2_500, 6_000] {
        let p = e.paragraph(&spec(&vec!["abcdefghi "; count]), Some(1000.0));
        for all in [false, true] {
            let backgrounds: Vec<_> = (0..count)
                .map(|i| {
                    if all || i + 1 == count {
                        background(i as u32)
                    } else {
                        None
                    }
                })
                .collect();
            let start = std::time::Instant::now();
            let pieces = fills(&p, &backgrounds);
            eprintln!(
                "backgrounds: {count} runs, all={all}: {:?}",
                start.elapsed()
            );
            assert_eq!(pieces.len(), if all { count } else { 1 });
            assert!(pieces.iter().all(|p| corners(p) == ALL));
        }
    }
}

#[test]
fn flowed_background_clamp_excludes_consumed_but_truncated_endpoint() {
    let mut e = engine();
    let mut s = spec(&["one two ", "three four"]);
    s.line_clamp = 1;
    let p = e.paragraph_flow(
        &s,
        90.0,
        &[exact_textflow::FlowShape::Circle {
            cx: 80.0,
            cy: 10.0,
            r: 10.0,
        }],
        None,
    );
    assert_eq!(p.fragments().len(), 1);
    assert_eq!(p.fragments()[0].end, s.runs[0].text.len());
    assert!(
        p.layout_runs()
            .flat_map(|l| l.glyphs)
            .map(|g| g.end as usize)
            .max()
            .unwrap()
            < s.runs[0].text.len()
    );
    let pieces = fills(&p, &[background(1), None]);
    assert_eq!(pieces.len(), 1);
    assert_eq!(corners(&pieces[0]), LEFT);
}

#[test]
fn flowed_background_spaces_and_later_hard_lines_keep_run_ownership() {
    let mut e = engine();
    let s = spec(&["Éab", " cd\nef", " gh ij kl"]);
    let p = e.paragraph_flow(&s, 300.0, &exclusion(), None);
    assert!(!p.flow_incomplete());
    let mut ends = Vec::new();
    let mut end = 0;
    for run in &s.runs {
        end += run.text.len();
        ends.push(end);
    }
    for line in p.layout_runs() {
        for g in line.glyphs {
            assert!(!line.text[g.range()].is_empty());
            assert_eq!(
                g.run(),
                ends.partition_point(|end| *end <= g.start as usize)
            );
        }
    }
    let pieces = fills(&p, &[None, background(1), None]);
    assert_eq!(pieces.len(), 2);
    assert_eq!(corners(&pieces[0]), LEFT);
    assert_eq!(corners(&pieces[1]), RIGHT);
}

#[test]
fn flowed_background_clamp_keeps_consumed_space_before_cut() {
    let mut e = engine();
    let mut s = spec(&["one ", "twothreefour"]);
    s.line_clamp = 1;
    let p = e.paragraph_flow(
        &s,
        90.0,
        &[exact_textflow::FlowShape::Circle {
            cx: 80.0,
            cy: 10.0,
            r: 10.0,
        }],
        None,
    );
    assert_eq!(p.fragments().len(), 1);
    assert_eq!(p.fragments()[0].end, s.runs[0].text.len());
    let pieces = fills(&p, &[background(1), None]);
    assert_eq!(pieces.len(), 1);
    assert_eq!(corners(&pieces[0]), ALL);
}

/// A box's background covers its padding and border (CSS 2 §14.2), from
/// its first glyph back by its start and from its last glyph's own end on
/// by its end, never its margin, as Chrome paints
/// `a <span style="padding:0 4px;margin-right:3px;background:#ddd">code</span> b`.
#[test]
fn a_background_covers_its_padding_and_not_its_margin() {
    let mut e = engine();
    let plain = e.paragraph(&spec(&["a ", "code", " b"]), None);
    let bare = fills(&plain, &[None, background(1), None]);
    let mut s = spec(&["a ", "code", " b"]);
    s.runs[1].edges = exact_kernel::InlineEdges {
        start: 4.0,
        end: 7.0,
    };
    let reach = exact_kernel::InlineEdges {
        start: 4.0,
        end: 4.0,
    };
    let reaching = background(1).map(|b| InlineBackground { reach, ..b });
    let padded = fills(&e.paragraph(&s, None), &[None, reaching, None]);
    let (b, p) = (bare[0].rect, padded[0].rect);
    assert!(
        (p.0 - b.0).abs() < 0.05,
        "it starts where the bare box did: {b:?} {p:?}"
    );
    assert!(
        (p.2 - b.2 - 8.0).abs() < 0.05,
        "4 each side, not the 3 of margin: {b:?} {p:?}"
    );
    assert_eq!(corners(&padded[0]), ALL, "its corners are the padded box's");
    // A square box with a margin alone paints its text and not the margin.
    let mut s = spec(&["a ", "code", " b"]);
    s.runs[1].edges = exact_kernel::InlineEdges {
        start: 3.0,
        end: 3.0,
    };
    let square = background(1).map(|b| InlineBackground {
        radii: [const { Dimension::Points(0.0) }; 4],
        ..b
    });
    let margined = fills(&e.paragraph(&s, None), &[None, square, None]);
    let m = margined[0].rect;
    assert!((m.0 - b.0 - 3.0).abs() < 0.05, "{b:?} {m:?}");
    assert!((m.2 - b.2).abs() < 0.05, "{b:?} {m:?}");
}

/// Above and below its text a box's background covers its padding and
/// border too, which move no line (CSS 2 §10.6.1): Chrome paints
/// `<span style="padding:3px 0 12px;background:#ddd">code</span>` 15 px
/// taller than its font's content area, from 3 px above it. A square box
/// with block padding alone still paints its padded rect.
#[test]
fn a_background_covers_its_block_padding() {
    let mut e = engine();
    let p = e.paragraph(&spec(&["a ", "code", " b"]), None);
    let bare = fills(&p, &[None, background(1), None])[0].rect;
    let padded = background(1).map(|b| InlineBackground {
        above: 3.0,
        below: 12.0,
        ..b
    });
    let tall = fills(&p, &[None, padded.clone(), None]);
    assert_eq!(tall.len(), 1);
    let t = tall[0].rect;
    assert!((t.1 - (bare.1 - 3.0)).abs() < 0.05, "{bare:?} {t:?}");
    assert!((t.3 - (bare.3 + 15.0)).abs() < 0.05, "{bare:?} {t:?}");
    assert_eq!((t.0, t.2), (bare.0, bare.2), "no wider: {bare:?} {t:?}");
    let square = padded.map(|b| InlineBackground {
        radii: [const { Dimension::Points(0.0) }; 4],
        ..b
    });
    let s = fills(&p, &[None, square, None])[0].rect;
    assert_eq!(s, t, "{s:?} {t:?}");
}

/// Boxes inside one another each paint their own background into their own
/// padding, the inner over the outer (CSS 2 Appendix E), as Chrome 154
/// paints `a <span style="padding:0 2px;background:red;border-radius:4px">p<span
/// style="padding:0 4px;background:blue">x</span></span> b`: the outer from
/// "p"'s unpadded start to 12 past "x"'s unpadded end, the inner from 2 past
/// "x"'s unpadded start to 10 past its end.
#[test]
fn nested_boxes_each_paint_their_own_padding() {
    let mut e = engine();
    let bare = e.paragraph(&spec(&["a ", "p", "x", " b"]), None);
    let at = |p: &Paragraph, byte: u32| {
        p.layout_runs()
            .flat_map(|r| {
                r.glyphs
                    .iter()
                    .map(|g| (g.start, g.x, g.w))
                    .collect::<Vec<_>>()
            })
            .find(|g| g.0 == byte)
            .expect("its glyph")
    };
    let (p0, x0) = (at(&bare, 2), at(&bare, 3));
    let mut s = spec(&["a ", "p", "x", " b"]);
    s.runs[1].edges = exact_kernel::InlineEdges {
        start: 2.0,
        end: 0.0,
    };
    s.runs[2].edges = exact_kernel::InlineEdges {
        start: 4.0,
        end: 6.0,
    };
    let edges = |start, end| exact_kernel::InlineEdges { start, end };
    let outer = InlineBackground {
        color: GRAY,
        reach: edges(2.0, 6.0),
        ..background(8).unwrap()
    };
    let inner = InlineBackground {
        color: [0, 0, 255, 255],
        radii: [const { Dimension::Points(0.0) }; 4],
        reach: edges(4.0, 4.0),
        outer: vec![outer.clone()],
        ..background(9).unwrap()
    };
    let shapes = paints(
        &e.paragraph(&s, None),
        &[None, Some(outer), Some(inner), None],
    );
    assert_eq!(shapes.len(), 2, "{shapes:?}");
    let (o, i) = (shapes[0].0.rect, shapes[1].0.rect);
    assert_eq!(shapes[0].1, GRAY, "the outer box paints first");
    assert!((o.0 - p0.1).abs() < 0.05, "{o:?} {p0:?}");
    assert!(
        (o.0 + o.2 - (x0.1 + x0.2 + 12.0)).abs() < 0.05,
        "{o:?} {x0:?}"
    );
    assert_eq!(corners(&shapes[0].0), ALL);
    assert!((i.0 - (x0.1 + 2.0)).abs() < 0.05, "{i:?} {x0:?}");
    assert!(
        (i.0 + i.2 - (x0.1 + x0.2 + 10.0)).abs() < 0.05,
        "{i:?} {x0:?}"
    );
}

/// An inline box's room separates the characters either side of it: "f"
/// and a padded "i" are two glyphs with the room between them, not the "fi"
/// ligature they shape as unpadded (above), widened by it.
#[test]
fn a_boxs_room_breaks_a_ligature_across_it() {
    let mut e = engine();
    let mut s = spec(&["f", "i"]);
    s.runs[1].edges = exact_kernel::InlineEdges {
        start: 3.0,
        end: 0.0,
    };
    let p = e.paragraph(&s, None);
    let line = p.layout_runs().next().unwrap();
    let advancing: Vec<_> = line.glyphs.iter().filter(|g| g.w > 0.0).collect();
    assert_eq!(advancing.len(), 2, "{:?}", line.glyphs);
    let (f, i) = (advancing[0], advancing[1]);
    assert!((i.x - (f.x + f.w) - 3.0).abs() < 0.05, "{f:?} {i:?}");
}

/// A paragraph of `texts` in `direction`, its second run with `room` (its
/// start, its end) beside its text.
fn boxed(
    e: &mut TextEngine,
    texts: &[&str],
    direction: Direction,
    width: Option<f32>,
    room: (f32, f32),
) -> std::rc::Rc<Paragraph> {
    let mut s = spec(texts);
    s.direction = direction;
    s.runs[1].edges = exact_kernel::InlineEdges {
        start: room.0,
        end: room.1,
    };
    e.paragraph(&s, width)
}

/// An inline box's start and end follow its own direction, its
/// paragraph's, whatever the direction of its text, and its first and last
/// fragments are in visual order (CSS 2 §8.6, §9.10; CSS Writing Modes 3
/// §2.1): in a right-to-left paragraph its start room is right of its text
/// and its end room left of it, as Chrome draws `<p dir=rtl>אב <span
/// style="padding:0 3px 0 7px">גדה</span> וז</p>` and the same span
/// holding "abc"; in a left-to-right one the other way about, a Hebrew
/// span's start left of its last letter, its leftmost.
#[test]
fn a_boxs_rooms_are_on_its_own_sides_whatever_its_texts_direction() {
    let mut e = engine();
    // Each case: its texts, its direction, and the run's glyphs (by byte in
    // the line) that have its start room and its end room beside them.
    for (texts, direction, start_at, end_at) in [
        (["אב ", "גדה", " וז"], Direction::Rtl, 5, 9),
        (["אב ", "abc", " גד"], Direction::Rtl, 7, 5),
        (["ab ", "אבג", " cd"], Direction::Ltr, 7, 3),
        (["ab ", "abc", " cd"], Direction::Ltr, 3, 5),
    ] {
        let p = boxed(&mut e, &texts, direction, None, (3.0, 7.0));
        let line = p.layout_runs().next().unwrap();
        let glyphs: Vec<_> = line.glyphs.iter().filter(|g| g.w > 0.0).collect();
        let at = |byte: u32| glyphs.iter().position(|g| g.start == byte).unwrap();
        let gap = |left: usize| glyphs[left + 1].x - (glyphs[left].x + glyphs[left].w);
        let rtl = direction == Direction::Rtl;
        let (s, n) = (at(start_at), at(end_at));
        let (start_gap, end_gap) = if rtl {
            (gap(s), gap(n - 1))
        } else {
            (gap(s - 1), gap(n))
        };
        let case = format!("{texts:?} {direction:?}: {glyphs:?}");
        assert!((start_gap - 3.0).abs() < 0.05, "{case}: start {start_gap}");
        assert!((end_gap - 7.0).abs() < 0.05, "{case}: end {end_gap}");
        // And nowhere else.
        let inside: f32 = (0..glyphs.len() - 1).map(gap).sum();
        assert!((inside - 10.0).abs() < 0.05, "{case}: {inside}");
    }
}

/// Each fragment of a wrapped box holds its glyphs, its padding past them
/// on its own start and end sides: a right-to-left paragraph's Hebrew,
/// Arabic and Latin boxes keep their last letters inside them.
#[test]
fn a_wrapped_boxs_fragments_hold_its_glyphs_in_a_right_to_left_paragraph() {
    let mut e = engine();
    let reach = exact_kernel::InlineEdges {
        start: 6.0,
        end: 6.0,
    };
    for texts in [
        ["מילה ", "טקסט ארוך שעובר בין שורות", " סוף"],
        ["كلمة ", "نص طويل يمتد عبر السطور", " نهاية"],
        ["Wrap ", "a run that wraps across lines", " end"],
    ] {
        let p = boxed(&mut e, &texts, Direction::Rtl, Some(180.0), (9.0, 9.0));
        let box_ = background(1).map(|b| InlineBackground {
            reach,
            radii: [const { Dimension::Points(0.0) }; 4],
            ..b
        });
        let rects: Vec<_> = fills(&p, &[None, box_, None])
            .into_iter()
            .map(|s| s.rect)
            .collect();
        let lines: Vec<_> = p.layout_runs().collect();
        let first = lines
            .iter()
            .position(|l| l.glyphs.iter().any(|g| g.run() == 1));
        let last = lines
            .iter()
            .rposition(|l| l.glyphs.iter().any(|g| g.run() == 1));
        let (first, last) = (first.unwrap(), last.unwrap());
        assert!(last > first, "{texts:?} wraps");
        for (i, line) in lines
            .iter()
            .enumerate()
            .filter(|(_, l)| l.glyphs.iter().any(|g| g.run() == 1))
        {
            let ink: Vec<_> = line
                .glyphs
                .iter()
                .filter(|g| g.run() == 1 && g.w > 0.0 && !line.text[g.range()].trim().is_empty())
                .collect();
            let lo = ink.iter().map(|g| g.x).fold(f32::INFINITY, f32::min);
            let hi = ink
                .iter()
                .map(|g| g.x + g.w)
                .fold(f32::NEG_INFINITY, f32::max);
            let mine: Vec<_> = rects
                .iter()
                .filter(|r| r.1 < line.line_y && line.line_y < r.1 + r.3)
                .collect();
            let case = format!("{texts:?} line {i}: {lo}..{hi} in {mine:?}");
            assert_eq!(mine.len(), 1, "{case}");
            let r = mine[0];
            // Its start on its first line's right, its end on its last's left.
            let right = if i == first { hi + 6.0 } else { hi };
            let left = if i == last { lo - 6.0 } else { lo };
            assert!((r.0 - left).abs() < 0.05, "{case}: left");
            assert!((r.0 + r.2 - right).abs() < 0.05, "{case}: right");
        }
    }
}
