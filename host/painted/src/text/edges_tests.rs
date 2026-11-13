//! A `text` inside a paragraph is an inline box whose padding, border and
//! margin take room in the line beside its text (`Run::edges`), as a
//! `<span>`'s do in Chrome (#389). The same room on Linux, Android and
//! Windows, which shape with this crate.
use super::*;
use exact_kernel::{InlineEdges, StyleProps, WhiteSpace};

const INTER: &[u8] = include_bytes!("../../tests/fonts/Inter-Regular.ttf");

fn engine() -> TextEngine {
    TextEngine::with_catalog(catalog::Catalog::from_bytes(&[INTER], "Inter"))
}

/// A paragraph of `runs`, each its text and its room before and after.
fn spec(runs: &[(&str, f32, f32)]) -> Spec {
    let mut s = crate::paint::text_spec(
        &StyleProps {
            font_size: 16.0,
            white_space: WhiteSpace::Normal,
            ..Default::default()
        },
        "",
    );
    let style = s.strut.clone();
    s.runs = runs
        .iter()
        .map(|&(text, start, end)| Run {
            edges: InlineEdges { start, end },
            text: text.into(),
            ..style.clone()
        })
        .collect();
    s
}

/// Each line's glyphs' starts, as (line, byte in its text, x): lines in
/// order down the paragraph.
fn glyphs(e: &mut TextEngine, s: &Spec, width: Option<f32>) -> Vec<(usize, u32, f32)> {
    let p = e.paragraph(s, width);
    let mut tops: Vec<f32> = p.layout_runs().map(|r| r.line_top).collect();
    tops.sort_by(f32::total_cmp);
    tops.dedup();
    p.layout_runs()
        .flat_map(|r| {
            let line = tops
                .iter()
                .position(|t| *t == r.line_top)
                .expect("its line");
            r.glyphs
                .iter()
                .map(move |g| (line, g.start, g.x))
                .collect::<Vec<_>>()
        })
        .collect()
}

fn x(glyphs: &[(usize, u32, f32)], line: usize, at: u32) -> f32 {
    glyphs
        .iter()
        .find(|g| g.0 == line && g.1 == at)
        .map(|g| g.2)
        .expect("a glyph there")
}

#[test]
fn a_runs_edges_take_room_beside_its_text() {
    let mut e = engine();
    let plain = spec(&[("1", 0.0, 0.0), ("The", 0.0, 0.0)]);
    let padded = spec(&[("1", 0.0, 4.8), ("The", 0.0, 0.0)]);
    let width = |e: &mut TextEngine, s: &Spec| e.measure(s, AxisOffer::MaxContent).width;
    let (w0, w1) = (width(&mut e, &plain), width(&mut e, &padded));
    // (A measure is whole layout units of the line's advance.)
    assert!((w1 - w0 - 4.8).abs() < 1.0, "{w0} {w1}");
    let (a, b) = (glyphs(&mut e, &plain, None), glyphs(&mut e, &padded, None));
    assert!((x(&b, 0, 1) - x(&a, 0, 1) - 4.8).abs() < 0.05);
    // A later run's start goes after the glyph before it; the first run's,
    // with none before it, is the line's start.
    let started = glyphs(&mut e, &spec(&[("1", 0.0, 0.0), ("The", 3.0, 0.0)]), None);
    assert!((x(&started, 0, 1) - x(&a, 0, 1) - 3.0).abs() < 0.05);
    // The glyphs after it move by the room once, not again.
    assert!((x(&started, 0, 2) - x(&a, 0, 2) - 3.0).abs() < 0.05);
    let first = glyphs(&mut e, &spec(&[("1", 6.0, 0.0), ("The", 0.0, 0.0)]), None);
    assert!((x(&first, 0, 0) - x(&a, 0, 0) - 6.0).abs() < 0.05);
}

/// Where a line breaks right before a run with a start, its room starts the
/// next line with the run's text, as Chrome keeps a `<span>`'s padding with
/// its text; the space before it hangs at the end of the line before.
#[test]
fn a_start_at_a_break_starts_the_next_line() {
    let mut e = engine();
    let plain = spec(&[("aaaa ", 0.0, 0.0), ("bbbb", 0.0, 0.0)]);
    let one = e.measure(&plain, AxisOffer::MaxContent).width;
    let padded = spec(&[("aaaa ", 0.0, 0.0), ("bbbb", 20.0, 0.0)]);
    // Narrower than the line with the room: "bbbb" goes to line 2.
    let g = glyphs(&mut e, &padded, Some(one + 10.0));
    assert!(g.iter().any(|g| g.0 == 1), "{g:?}");
    let bare = glyphs(
        &mut e,
        &spec(&[("aaaa ", 0.0, 0.0), ("bbbb", 0.0, 0.0)]),
        Some(1.0),
    );
    assert!(
        (x(&g, 1, 5) - x(&bare, 1, 5) - 20.0).abs() < 0.05,
        "{g:?} {bare:?}"
    );
    // On one line the room is between the space and the run.
    let wide = glyphs(&mut e, &padded, None);
    let flat = glyphs(&mut e, &plain, None);
    assert!((x(&wide, 0, 5) - x(&flat, 0, 5) - 20.0).abs() < 0.05);
    // Min-content: the run's word carries its room, the space hangs.
    let min = e.measure(&padded, AxisOffer::MinContent).width;
    let word = e
        .measure(&spec(&[("bbbb", 0.0, 0.0)]), AxisOffer::MaxContent)
        .width;
    assert!((min - (word + 20.0)).abs() < 0.05, "{min} {word}");
}

/// Between two CJK characters, where a line can break with no space before
/// the run, the room still goes to the next line with the run's text, as in
/// Chrome 154: `中文<span style="padding-left:20px">中文</span>` at 2.5em
/// puts "中文" on the first line and the span's text 20 px into the second.
#[test]
fn a_start_after_a_character_that_is_no_space_moves_with_its_text() {
    let mut e = engine();
    let pair = e
        .measure(&spec(&[("中文", 0.0, 0.0)]), AxisOffer::MaxContent)
        .width;
    let padded = spec(&[("中文", 0.0, 0.0), ("中文", 20.0, 0.0)]);
    let g = glyphs(&mut e, &padded, Some(pair + 4.0));
    assert_eq!(
        g.iter().filter(|g| g.0 == 0).map(|g| g.1).max(),
        Some(3),
        "the first line keeps both of its characters: {g:?}"
    );
    let bare = glyphs(
        &mut e,
        &spec(&[("中文", 0.0, 0.0), ("中文", 0.0, 0.0)]),
        Some(pair + 4.0),
    );
    assert!(
        (x(&g, 1, 6) - x(&bare, 1, 6) - 20.0).abs() < 0.05,
        "{g:?} {bare:?}"
    );
}
