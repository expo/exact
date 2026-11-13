//! Code spans: CommonMark 0.31.2 §6.1's edge spaces, hidden as syntax, and
//! the padding the editor writes beside a backtick (LLP 1045 D2).

use exact_markdown::*;

use super::commands::run;
use super::{hidden, spans, utf16};

/// Each code span's shown text, in reading.
fn code(source: &str) -> Vec<String> {
    style(source, None)
        .spans
        .iter()
        .filter(|s| s.style & CODE != 0)
        .map(|s| utf16(source, s.range))
        .collect()
}

#[test]
fn code_spans_follow_the_commonmark_examples_this_dialect_covers() {
    // Not covered: 335–337 convert line endings to spaces, which this dialect
    // does not, and 344 is HTML, which stays inert text here.
    for (example, source, shown) in [
        (328, "`foo`", &["foo"][..]),
        (329, "`` foo ` bar ``", &["foo ` bar"]),
        (330, "` `` `", &["``"]),
        (331, "`  ``  `", &[" `` "]),
        (332, "` a`", &[" a"]),
        (333, "`\u{a0}b\u{a0}`", &["\u{a0}b\u{a0}"]),
        (334, "` `\n`  `", &[" ", "  "]),
        (338, "`foo\\`bar`", &["foo\\"]),
        (339, "``foo`bar``", &["foo`bar"]),
        (340, "` foo `` bar `", &["foo `` bar"]),
        (341, "*foo`*`", &["*"]),
        (342, "[not a `link](/foo`)", &["link](/foo"]),
        (343, "`<a href=\"`\">`", &["<a href=\""]),
        (345, "`<https://foo.bar.`baz>`", &["<https://foo.bar."]),
        (346, "<https://foo.bar.`baz>`", &[]),
        (347, "```foo``", &[]),
        (348, "`foo", &[]),
        (349, "`foo``bar``", &["bar"]),
    ] {
        assert_eq!(code(source), shown, "example {example}: {source:?}");
    }
    assert_eq!(plain("*foo`*`"), "*foo*");
    assert_eq!(plain("<https://foo.bar.`baz>`"), "https://foo.bar.`baz`");
}

#[test]
fn stripped_edge_spaces_are_hidden_syntax_that_stays_in_the_source() {
    let source = "a ` b ` c";
    assert_eq!(code(source), ["b"]);
    assert_eq!(hidden(source, None), ["` ", " `"]);
    assert_eq!(plain(source), "a b c");
    // Revealed, they are drawn as markers with the backticks.
    assert_eq!(
        spans(source, Some(Range::caret(5))),
        [
            ("` ".to_string(), MARKER),
            ("b".to_string(), CODE),
            (" `".to_string(), MARKER),
        ]
    );
    // A line ending is not a space: it is neither stripped nor converted.
    assert_eq!(hidden("``\nfoo \n``", None), ["``", "``"]);
}

#[test]
fn an_all_spaces_code_span_keeps_its_spaces() {
    for source in ["` `", "`  `", "`   `", "``   ``"] {
        let inner = source.trim_matches('`');
        assert_eq!(code(source), [inner], "{source:?}");
        assert_eq!(plain(source), inner, "{source:?}");
    }
}

#[test]
fn return_beside_a_literal_backtick_keeps_both_halves_code() {
    // The issue's three rows: each half shows its text unchanged.
    for (before, after, shown) in [
        ("- ``a`|b``", "- `` a` ``\n- ``|b``", "• a`\n• b"),
        ("- ``a|`b``", "- ``a``\n- `` |`b ``", "• a\n• `b"),
        ("- ```a`|`b```", "- ``` a` ```\n- ``` |`b ```", "• a`\n• `b"),
    ] {
        assert_eq!(run(before, Command::Newline), after, "{before}");
        let source = after.replace('|', "");
        assert_eq!(plain(&source), shown, "{before}");
        assert_eq!(code(&source).len(), 2, "{before}");
    }
}

#[test]
fn return_in_a_padded_span_keeps_what_each_half_shows() {
    for (before, after, shown) in [
        // Content that begins and ends with a backtick.
        (
            "- `` ` fo|o ` ``",
            "- `` ` fo ``\n- `` |o ` ``",
            "• ` fo\n• o `",
        ),
        // Padding the author wrote stays on both halves.
        ("- ` a|b `", "- ` a `\n- ` |b `", "• a\n• b"),
        // A half that would lose its edge spaces is padded.
        ("- `a| b `", "- `a`\n- ` | b  `", "• a\n•  b "),
        ("- `  a |b  `", "- `  a  `\n- ` |b  `", "•  a \n• b "),
        // An all-spaces half is never padded: it shows its spaces as they are.
        ("- `  | a `", "- ` `\n- ` | a `", "•  \n•  a"),
    ] {
        assert_eq!(run(before, Command::Newline), after, "{before}");
        assert_eq!(plain(&after.replace('|', "")), shown, "{before}");
    }
}

#[test]
fn the_code_command_pads_content_at_a_backtick() {
    assert_eq!(run("say |`a|", Command::Code), "say `` |`a| ``");
    assert_eq!(run("say `` |`a| ``", Command::Code), "say |`a|");
    assert_eq!(run("|` foo|", Command::Code), "`` |` foo| ``");
    assert_eq!(run("|foo `|", Command::Code), "`` |foo `| ``");
    assert_eq!(plain("`` ` foo ` ``"), "` foo `");
    assert_eq!(run("|a`|", Command::Code), "`` |a`| ``");
    assert_eq!(run("|a`\nb|", Command::Code), "`` |a` ``\n``b|``");
}
