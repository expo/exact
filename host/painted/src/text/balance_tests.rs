// CSS `text-wrap-style: balance` (LLP 1001 §6) against Chrome: the issue's
// sentence in 16px DejaVu Sans (the fixture face, loaded in Chrome as a web
// font) in a 240px box breaks greedily as [208.61, 230.39, 74.8] and
// balanced as [174.33, 170.72, 168.75]; centred, the balanced lines centre
// in the box (the first starts at 32.83), and justified all but the last
// fill it ([240, 240, 168.75]).
use super::*;
use exact_kernel::{StyleProps, TextAlign, TextWrapStyle};

const SENTENCE: &str = "Settle every thread in this section and keep the newest one open";

fn engine() -> TextEngine {
    TextEngine::with_catalog(catalog::Catalog::from_bytes(
        &[include_bytes!("../../../../scripts/fixtures/fonts/assets/DejaVuSans.ttf").as_slice()],
        "DejaVu Sans",
    ))
}

fn spec(text: &str, balance: bool, align: TextAlign) -> Spec {
    let mut spec = crate::paint::text_spec(&StyleProps::default(), text);
    assert_eq!(spec.strut.size, 16.0);
    spec.align = align;
    if balance {
        spec.text_wrap_style = TextWrapStyle::Balance;
    }
    spec
}

fn widths(engine: &mut TextEngine, spec: &Spec, width: f32) -> (Vec<f32>, Vec<f32>, f32) {
    let p = engine.paragraph(spec, Some(width));
    let lines = p.lines();
    let starts = lines
        .lines
        .iter()
        .map(|l| lines.glyphs_of(l)[0].x)
        .collect();
    (lines.lines.iter().map(|l| l.w).collect(), starts, p.width)
}

fn near(actual: &[f32], chrome: &[f32]) -> bool {
    actual.len() == chrome.len() && actual.iter().zip(chrome).all(|(a, c)| (a - c).abs() < 0.5)
}

#[test]
fn the_issues_sentence_balances_as_chromes_does() {
    let mut engine = engine();
    let (greedy, _, measured) = widths(&mut engine, &spec(SENTENCE, false, TextAlign::Left), 240.0);
    assert!(near(&greedy, &[208.61, 230.39, 74.8]), "greedy {greedy:?}");
    let (balanced, _, width) = widths(&mut engine, &spec(SENTENCE, true, TextAlign::Left), 240.0);
    assert!(
        near(&balanced, &[174.33, 170.72, 168.75]),
        "balanced {balanced:?}"
    );
    assert_eq!(width, measured, "balancing moves breaks, never the box");
    let (_, starts, _) = widths(&mut engine, &spec(SENTENCE, true, TextAlign::Center), 240.0);
    assert!(
        near(&starts, &[32.83, 34.64, 35.63]),
        "centred in the box: {starts:?}"
    );
    let (justified, _, _) = widths(
        &mut engine,
        &spec(SENTENCE, true, TextAlign::Justify),
        240.0,
    );
    assert!(
        near(&justified, &[240.0, 240.0, 168.75]),
        "justified, the balanced lines fill the box: {justified:?}"
    );
}

// The report's headings (expo/exact#437), which Chrome 157 scores rather
// than breaking at the narrowest width that keeps three lines: that width
// gives "Seven / studies in live / typography" and "A road / that ended / at
// the sea".
#[test]
fn the_reports_headings_break_where_chromes_scorer_breaks_them() {
    let mut engine = engine();
    for (text, width, chrome) in [
        // "Seven / studies in / live typography"
        (
            "Seven studies in live typography",
            130.0,
            [49.45, 77.2, 125.28],
        ),
        // "A road that / ended at / the sea"
        ("A road that ended at the sea", 120.0, [89.59, 71.31, 59.33]),
    ] {
        let (balanced, ..) = widths(&mut engine, &spec(text, true, TextAlign::Left), width);
        assert!(near(&balanced, &chrome), "{text}: {balanced:?}");
    }
}

// Where Blink's scorer gives nothing (Chrome 157): three words bisect the
// width; a word wider than the box, or broken inside under `break-word`,
// keeps every line greedy; and so does a short first paragraph before a
// forced break, which the bisection cannot take.
#[test]
fn short_paragraphs_bisect_and_what_blink_leaves_alone_stays_greedy() {
    let mut engine = engine();
    let short = "large under they";
    let (greedy, ..) = widths(&mut engine, &spec(short, false, TextAlign::Left), 107.6);
    assert!(near(&greedy, &[92.5, 35.73]), "greedy {greedy:?}");
    let (bisected, ..) = widths(&mut engine, &spec(short, true, TextAlign::Left), 107.6);
    assert!(near(&bisected, &[40.55, 87.67]), "bisected {bisected:?}");
    let long = "Supercalifragilisticexpialidocious is a very long word";
    let (overflowing, ..) = widths(&mut engine, &spec(long, true, TextAlign::Left), 120.0);
    assert!(
        near(&overflowing, &[261.16, 107.73, 39.33]),
        "overflowing {overflowing:?}"
    );
    let mut break_word = spec(long, false, TextAlign::Left);
    break_word.overflow_wrap = exact_kernel::OverflowWrap::BreakWord;
    let (greedy, ..) = widths(&mut engine, &break_word, 120.0);
    assert_eq!(greedy.len(), 4);
    break_word.text_wrap_style = TextWrapStyle::Balance;
    let (broken, ..) = widths(&mut engine, &break_word, 120.0);
    assert_eq!(broken, greedy);
    let pre_line = StyleProps::default().with_rows(|rows| {
        rows.white_space = exact_kernel::WhiteSpace::PreLine;
        rows.text_wrap_style = TextWrapStyle::Balance;
    });
    let forced = crate::paint::text_spec(&pre_line, "large under they\nnext");
    let (lines, ..) = widths(&mut engine, &forced, 107.6);
    assert!(near(&lines, &[92.5, 35.73, 35.45]), "forced {lines:?}");
    // A forced break that ends the text is one too.
    let trailing = crate::paint::text_spec(&pre_line, "large under they\n");
    let (lines, ..) = widths(&mut engine, &trailing, 107.6);
    assert!(near(&lines, &[92.5, 35.73]), "trailing {lines:?}");
}

// A line separator, which Parley breaks at inside a hard line, ends the
// balanced lines as a line feed does: the heading before it breaks as it
// does alone, the text after it breaks greedily at the box's width, and
// every character is drawn.
#[test]
fn a_line_separator_ends_the_balanced_lines() {
    let mut engine = engine();
    let glyphs = |engine: &mut TextEngine, spec: &Spec, width: f32| {
        let p = engine.paragraph(spec, Some(width));
        let lines = p.lines();
        let counts = lines.lines.iter().map(|l| lines.glyphs_of(l).len());
        counts.sum::<usize>()
    };
    // One line before the separator: nothing to balance, under `pre-wrap`.
    let pre_wrap = |balance| {
        StyleProps::default().with_rows(|rows| {
            rows.white_space = exact_kernel::WhiteSpace::PreWrap;
            if balance {
                rows.text_wrap_style = TextWrapStyle::Balance;
            }
        })
    };
    let short = "a a a a\u{2028}b b";
    let greedy = crate::paint::text_spec(&pre_wrap(false), short);
    let balanced = crate::paint::text_spec(&pre_wrap(true), short);
    let (lines, ..) = widths(&mut engine, &balanced, 60.0);
    assert!(near(&lines, &[54.48, 25.41]), "short {lines:?}");
    assert_eq!(
        glyphs(&mut engine, &balanced, 60.0),
        glyphs(&mut engine, &greedy, 60.0)
    );
    // The heading's three lines are Chrome 157's for it alone.
    for (rest, after) in [("xx", &[18.94][..]), ("in live typography", &[47.88, 92.0])] {
        let text = format!("Seven studies in live typography\u{2028}{rest}");
        let (greedy, ..) = widths(&mut engine, &spec(&text, false, TextAlign::Left), 130.0);
        assert!(
            near(&greedy, &[&[112.08, 47.88, 92.0], after].concat()),
            "greedy {greedy:?}"
        );
        let (lines, ..) = widths(&mut engine, &spec(&text, true, TextAlign::Left), 130.0);
        assert!(
            near(&lines, &[&[49.45, 77.2, 125.28], after].concat()),
            "{rest}: {lines:?}"
        );
        assert_eq!(
            glyphs(&mut engine, &spec(&text, true, TextAlign::Left), 130.0),
            glyphs(&mut engine, &spec(&text, false, TextAlign::Left), 130.0)
        );
    }
    // A tab keeps the scorer out only among the lines to balance: after
    // the separator the heading still balances, before it the lines stay
    // greedy.
    let heading = |engine: &mut TextEngine, text: &str, balance| {
        let spec = crate::paint::text_spec(&pre_wrap(balance), text);
        widths(engine, &spec, 130.0).0
    };
    let after = "Seven studies in live typography\u{2028}a\tb";
    let lines = heading(&mut engine, after, true);
    assert!(
        near(&lines, &[49.45, 77.2, 125.28, 50.84]),
        "a tab after the separator: {lines:?}"
    );
    let before = "Seven studies in\tlive typography\u{2028}xx";
    let greedy = heading(&mut engine, before, false);
    assert!(
        near(&greedy, &[112.08, 68.89, 92.0, 18.94]),
        "greedy {greedy:?}"
    );
    assert_eq!(heading(&mut engine, before, true), greedy);
}

// The first line's `text-indent` comes off its room alone, and a soft
// hyphen's break costs the scorer a penalty: Chrome 157's lines. Greedy,
// the indented lines are "one two / three four five six / seven eight nine /
// ten", and the third hyphenated line at 120px ends "un-".
#[test]
fn an_indent_and_soft_hyphens_balance_as_chromes_do() {
    let mut engine = engine();
    let mut indented = spec(
        "one two three four five six seven eight nine ten",
        true,
        TextAlign::Left,
    );
    indented.text_indent = 40.0;
    let (lines, ..) = widths(&mut engine, &indented, 150.0);
    assert!(
        near(&lines, &[40.0 + 64.02, 79.56, 109.45, 111.86]),
        "indented {lines:?}"
    );
    let soft = "extra\u{ad}ordinary circum\u{ad}stances pre\u{ad}vail in un\u{ad}usual times";
    let (greedy, ..) = widths(&mut engine, &spec(soft, false, TextAlign::Left), 120.0);
    assert!(
        near(&greedy[..3], &[108.38, 115.53, 105.22]) && greedy.len() == 4,
        "greedy {greedy:?}"
    );
    for width in [120.0, 150.0] {
        let (lines, ..) = widths(&mut engine, &spec(soft, true, TextAlign::Left), width);
        assert!(
            near(&lines, &[108.38, 115.53, 74.06, 112.72]),
            "soft hyphens at {width}: {lines:?}"
        );
    }
}

#[test]
fn seven_lines_and_a_clamp_stay_greedy() {
    let mut engine = engine();
    let seven = ["aa"; 20].join(" ");
    let (greedy, ..) = widths(&mut engine, &spec(&seven, false, TextAlign::Left), 60.0);
    assert!(greedy.len() > 6, "{greedy:?}");
    let (balanced, ..) = widths(&mut engine, &spec(&seven, true, TextAlign::Left), 60.0);
    assert_eq!(balanced, greedy);
    let mut clamped = spec(SENTENCE, true, TextAlign::Left);
    clamped.line_clamp = 3;
    let (lines, ..) = widths(&mut engine, &clamped, 240.0);
    assert!(near(&lines, &[208.61, 230.39, 74.8]), "clamped {lines:?}");
}
