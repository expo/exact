//! Lines around a lone carriage return, form feed, vertical tab, NEL, U+2028
//! and U+2029, against Chrome 157's: `10px monospace` text set through
//! `textContent`, in a box of the width given, where width 0 takes every
//! opportunity. Only a line feed forces a break (CSS Text 3 §4.1).
use exact_textflow::{Options, OverflowWrap, Prepared, WhiteSpace};
use WhiteSpace::{Normal, Pre, PreLine, PreWrap};

/// 6px a character, as that font advances. A carriage return and a form feed
/// are given a space's advance, as a canvas measures them, so a preserved
/// line that counted either would be too wide.
fn mono(text: &str, white_space: WhiteSpace) -> Prepared {
    wrapping(text, white_space, OverflowWrap::Normal)
}

fn wrapping(text: &str, white_space: WhiteSpace, overflow_wrap: OverflowWrap) -> Prepared {
    let options = Options {
        white_space,
        overflow_wrap,
        ..Options::default()
    };
    Prepared::new(text, options, &mut |r: std::ops::Range<usize>| {
        6.0 * text[r].chars().count() as f32
    })
}

/// Lines as source text and advance.
type Lines<'a> = &'a [(&'a str, f32)];

/// Each line's source text and advance.
fn split(text: &str, white_space: WhiteSpace, width: f32) -> Vec<(&str, f32)> {
    crate::support::lines(&mono(text, white_space), width)
        .iter()
        .map(|l| (&text[l.start.byte..l.end.byte], l.width))
        .collect()
}

fn texts(text: &str, white_space: WhiteSpace, width: f32) -> Vec<&str> {
    let lines = split(text, white_space, width);
    lines.iter().map(|l| l.0).collect()
}

/// Chrome lays a preserved carriage return or form feed out with no advance
/// and no break before or after it, whatever its neighbours: `aaa\fbbb` is one
/// overflowing line in a 24px box, as wide as `aaabbb`.
#[test]
fn a_preserved_carriage_return_or_form_feed_has_no_advance_and_no_break() {
    for control in ['\r', '\u{c}'] {
        let s = |shape: &str| shape.replace('|', &control.to_string());
        for white_space in [PreWrap, Pre] {
            let text = s("aaa|bbb");
            for width in [0.0, 24.0, 500.0] {
                assert_eq!(
                    split(&text, white_space, width),
                    [(&*text, 36.0)],
                    "{white_space:?} {control:?} at {width}px"
                );
            }
            let p = mono(&text, white_space);
            assert_eq!(p.natural_width(), 36.0, "{white_space:?} {control:?}");
            assert_eq!(p.min_content_width(), 36.0, "{white_space:?} {control:?}");
        }
        assert_eq!(mono(&s("aaa | bbb"), Pre).natural_width(), 48.0);
        // Neither an ideograph, a bracket nor a hyphen beside it opens one.
        for shape in ["あ|い", "a|(b", "a)|b", "a-|b", "a\u{a0}|b", "a||b"] {
            let text = s(shape);
            assert_eq!(texts(&text, PreWrap, 0.0), [&*text], "{control:?}");
        }
        // The breaks around it are its neighbours' own.
        assert_eq!(texts(&s("a|-b"), PreWrap, 0.0), [&*s("a|-"), "b"]);
        assert_eq!(
            texts(&s("a|\u{200b}b"), PreWrap, 0.0),
            [&*s("a|\u{200b}"), "b"]
        );
        // An emergency break counts it for nothing either, and gives it no
        // line: it goes with the letter after it, or the last one before.
        for overflow_wrap in [OverflowWrap::Anywhere, OverflowWrap::BreakWord] {
            let cases: [(&str, f32, Lines); 5] = [
                // Each letter overflows 3px.
                ("a|b", 3.0, &[("a", 6.0), ("|b", 6.0)]),
                ("a||b", 3.0, &[("a", 6.0), ("||b", 6.0)]),
                ("|ab", 3.0, &[("|a", 6.0), ("b", 6.0)]),
                ("a|\u{200b}b", 3.0, &[("a|\u{200b}", 6.0), ("b", 6.0)]),
                ("ab|cd", 12.0, &[("ab", 12.0), ("|cd", 12.0)]),
            ];
            for (shape, width, want) in cases {
                let text = s(shape);
                let want: Vec<(String, f32)> = want.iter().map(|(t, w)| (s(t), *w)).collect();
                let got: Vec<(String, f32)> =
                    crate::support::lines(&wrapping(&text, PreWrap, overflow_wrap), width)
                        .iter()
                        .map(|l| (text[l.start.byte..l.end.byte].to_string(), l.width))
                        .collect();
                assert_eq!(
                    got, want,
                    "{overflow_wrap:?} {control:?} {shape:?} at {width}px"
                );
            }
        }
        let anywhere = wrapping(&s("ab|cd"), PreWrap, OverflowWrap::Anywhere);
        assert_eq!(anywhere.min_content_width(), 6.0, "{control:?}");
    }
}

/// Beside preserved spaces it stays where it is: the spaces around it hang as
/// one run, and a line starts after a space before it only where text follows.
/// Chrome keeps the first of two after a space on the earlier line, which
/// paints the same.
#[test]
fn a_preserved_carriage_return_or_form_feed_beside_spaces_keeps_their_breaks() {
    for control in ['\r', '\u{c}'] {
        let s = |shape: &str| shape.replace('|', &control.to_string());
        let cases: [(&str, f32, Lines); 12] = [
            ("aaa |bbb", 0.0, &[("aaa ", 18.0), ("|bbb", 18.0)]),
            ("aaa ||bbb", 0.0, &[("aaa ", 18.0), ("||bbb", 18.0)]),
            ("aaa| bbb", 0.0, &[("aaa| ", 18.0), ("bbb", 18.0)]),
            ("aaa | bbb", 0.0, &[("aaa | ", 18.0), ("bbb", 18.0)]),
            ("aaa | bbb", 18.0, &[("aaa | ", 18.0), ("bbb", 18.0)]),
            ("aaa | bbb", 500.0, &[("aaa | bbb", 48.0)]),
            ("aaa\t|\tbbb", 0.0, &[("aaa\t|\t", 18.0), ("bbb", 18.0)]),
            ("aaa |", 0.0, &[("aaa |", 18.0)]),
            ("| bbb", 0.0, &[("| ", 0.0), ("bbb", 18.0)]),
            ("| bbb", 24.0, &[("| bbb", 24.0)]),
            ("a |\nb", 0.0, &[("a |\n", 6.0), ("b", 6.0)]),
            ("a\n| b", 0.0, &[("a\n", 6.0), ("| ", 0.0), ("b", 6.0)]),
        ];
        for (shape, width, want) in cases {
            let text = s(shape);
            let want: Vec<(String, f32)> = want.iter().map(|(t, w)| (s(t), *w)).collect();
            let got: Vec<(String, f32)> = split(&text, PreWrap, width)
                .iter()
                .map(|(t, w)| (t.to_string(), *w))
                .collect();
            assert_eq!(got, want, "{control:?} {shape:?} at {width}px");
        }
        // A line paints neither the spaces it hangs nor the control among them.
        let text = s("a| b");
        assert_eq!(mono(&text, PreWrap).paint_range(&text, 0..3), 0..1);
        // Alone after the last line feed it makes no line, where a space does.
        for white_space in [PreWrap, Pre] {
            assert_eq!(texts(&s("a\n|"), white_space, 500.0), ["a\n"]);
            assert_eq!(texts(&s("||"), white_space, 500.0), [""; 0]);
            assert_eq!(texts(&s("|\nb"), white_space, 500.0), [&*s("|\n"), "b"]);
            assert_eq!(texts("a\n ", white_space, 500.0), ["a\n", " "]);
        }
    }
}

/// NEL, U+2028 and U+2029 are text in every mode: measured and painted, with
/// a line allowed to start after one, whatever follows but a space, and
/// before one only after a space or another of them.
#[test]
fn nel_and_the_unicode_separators_are_text_a_line_may_wrap_after() {
    for separator in ['\u{85}', '\u{2028}', '\u{2029}'] {
        let s = |shape: &str| shape.replace('|', &separator.to_string());
        let cases: [(&str, &[&str]); 14] = [
            ("aaa|bbb", &["aaa|", "bbb"]),
            ("aaa |bbb", &["aaa ", "|", "bbb"]),
            ("aaa| bbb", &["aaa| ", "bbb"]),
            ("aaa | bbb", &["aaa ", "| ", "bbb"]),
            ("aaa||bbb", &["aaa|", "|", "bbb"]),
            ("あ|い", &["あ|", "い"]),
            ("a|(b", &["a|", "(b"]),
            ("a)|b", &["a)|", "b"]),
            ("a-|b", &["a-|", "b"]),
            ("a|\u{a0}b", &["a|", "\u{a0}b"]),
            ("a\u{a0}|b", &["a\u{a0}|", "b"]),
            ("a\u{200b}|b", &["a\u{200b}|", "b"]),
            ("|bbb", &["|", "bbb"]),
            ("aaa|", &["aaa|"]),
        ];
        for white_space in [PreWrap, Normal, PreLine] {
            for (shape, want) in cases {
                let text = s(shape);
                let want: Vec<String> = want.iter().map(|t| s(t)).collect();
                assert_eq!(
                    texts(&text, white_space, 0.0),
                    want,
                    "{white_space:?} {separator:?} {shape:?}"
                );
            }
        }
        // A preserved control after one closes the break; before one it opens none.
        for (shape, want) in [
            ("a|\rb", &["a|\rb"][..]),
            ("a|\u{c}b", &["a|\u{c}b"]),
            ("a\r|b", &["a\r|", "b"]),
            ("a|\nb", &["a|\n", "b"]),
        ] {
            let text = s(shape);
            let want: Vec<String> = want.iter().map(|t| s(t)).collect();
            assert_eq!(texts(&text, PreWrap, 0.0), want, "{separator:?} {shape:?}");
        }
        // The line it ends measures and paints it; only a hung space or the
        // line feed a line ends at carries no ink.
        let text = s("a|b");
        let p = mono(&text, PreWrap);
        let first = 0..text.len() - 1;
        assert_eq!(split(&text, PreWrap, 6.0)[0], (&text[first.clone()], 12.0));
        assert_eq!(p.paint_range(&text, first.clone()), first);
        assert_eq!(split(&text, Pre, 0.0), [(&*text, 18.0)]);
    }
    let text = "a \nb";
    assert_eq!(mono(text, PreWrap).paint_range(text, 0..3), 0..1);
}

/// Where white space collapses, a form feed is text with no break beside it,
/// and so is a vertical tab in every mode: `aaa\fbbb` is one overflowing line
/// under `pre-line`. A line still starts after a space before one.
#[test]
fn a_form_feed_or_vertical_tab_that_is_text_opens_no_break() {
    for (control, modes) in [
        ('\u{c}', &[Normal, PreLine][..]),
        ('\u{b}', &[Normal, PreLine, PreWrap]),
    ] {
        let s = |shape: &str| shape.replace('|', &control.to_string());
        for &white_space in modes {
            for (shape, want) in [
                ("aaa|bbb", &["aaa|bbb"][..]),
                ("あ|い", &["あ|い"]),
                ("a|(b", &["a|(b"]),
                ("a-|b", &["a-|b"]),
                ("a\u{2028}|b", &["a\u{2028}|b"]),
                ("aaa |bbb", &["aaa ", "|bbb"]),
                ("aaa ||bbb", &["aaa ", "||bbb"]),
                ("aaa| bbb", &["aaa| ", "bbb"]),
                ("aaa | bbb", &["aaa ", "| ", "bbb"]),
            ] {
                let text = s(shape);
                let want: Vec<String> = want.iter().map(|t| s(t)).collect();
                assert_eq!(
                    texts(&text, white_space, 0.0),
                    want,
                    "{white_space:?} {control:?} {shape:?}"
                );
            }
            // It is measured as the text it is.
            let text = s("aaa|bbb");
            assert_eq!(split(&text, white_space, 24.0), [(&*text, 42.0)]);
        }
        assert_eq!(
            exact_textflow::line_breaks(&s("aaa|bbb"), &[]),
            [7],
            "{control:?}"
        );
    }
}

/// A collapsed carriage return is a space: a line may start after it.
#[test]
fn a_collapsed_carriage_return_is_a_space() {
    for white_space in [Normal, PreLine] {
        assert_eq!(texts("aaa\rbbb", white_space, 0.0), ["aaa\r", "bbb"]);
        assert_eq!(texts("あ\rい", white_space, 0.0), ["あ\r", "い"]);
        assert_eq!(texts("aaa \r bbb", white_space, 0.0), ["aaa \r ", "bbb"]);
        assert_eq!(
            split("aaa\r\rbbb", white_space, 500.0),
            [("aaa\r\rbbb", 42.0)]
        );
    }
}

/// Chrome 157 starts a line at a combining mark, variation selector or joiner
/// after a carriage return: collapsed, the return is a space; preserved, the
/// line starts before the return, and only after a space. No-break glue
/// starts a line there too. The line counts are Chrome's.
#[test]
fn a_line_starts_at_a_mark_after_a_carriage_return() {
    for mark in ['\u{301}', '\u{fe0f}', '\u{200d}', '\u{a0}'] {
        let s = |shape: &str| shape.replace('^', &mark.to_string());
        for white_space in [Normal, PreLine] {
            for (shape, lines) in [
                ("a\r^b", ["a\r", "^b"]),
                ("a \r^b", ["a \r", "^b"]),
                ("a\r\r^b", ["a\r\r", "^b"]),
                ("a\n^b", ["a\n", "^b"]),
            ] {
                assert_eq!(
                    texts(&s(shape), white_space, 0.0),
                    lines.map(s),
                    "{white_space:?} {shape:?} {mark:?}"
                );
            }
        }
        for (shape, lines) in [
            ("x \r^b", ["x ", "\r^b"]),
            ("x \u{c}^b", ["x ", "\u{c}^b"]),
            ("x \r\r^b", ["x ", "\r\r^b"]),
            ("x\t\r^b", ["x\t", "\r^b"]),
            ("x\n^b", ["x\n", "^b"]),
        ] {
            assert_eq!(
                texts(&s(shape), PreWrap, 0.0),
                lines.map(s),
                "{shape:?} {mark:?}"
            );
        }
        for shape in ["x\r^b", "x\u{c}^b"] {
            assert_eq!(
                texts(&s(shape), PreWrap, 0.0),
                [s(shape)],
                "{shape:?} {mark:?}"
            );
        }
    }
}
