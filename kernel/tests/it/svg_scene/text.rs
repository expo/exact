use super::*;

/// The strings of a `text` whose `tspan`s carry `runs`.
fn resolved(runs: &[&str]) -> Vec<String> {
    use NodeType::{SvgTSpan, SvgText};
    let mut d = Doc::new();
    let svg = d.node(
        NodeType::Svg,
        &[(StyleId::Width, "100"), (StyleId::Height, "100")],
        &[],
    );
    let text = d.node(SvgText, &[], &[]);
    let spans: Vec<u32> = runs
        .iter()
        .map(|run| d.node(SvgTSpan, &[], &[(PropId::Text, run)]))
        .collect();
    d.children(text, &spans);
    d.children(svg, &[text]);
    let k = d.kernel(&[svg], 400.0);
    let s = resolve(&k, svg);
    let Kind::Text(t) = &find(&s.items, text).unwrap().kind else {
        panic!("a text item")
    };
    t.chunks
        .iter()
        .flat_map(|c| c.runs.iter().map(|r| r.text.clone()))
        .collect()
}

/// CSS Text 3 §4.1.1: only spaces, tabs and line ends collapse.
#[test]
fn no_break_and_other_spaces_are_text() {
    for space in ['\u{a0}', '\u{202f}', '\u{2009}', '\u{3000}'] {
        let written = format!("{space}72{space}{space}F{space}");
        assert_eq!(resolved(&[&written]), [written.as_str()], "{space:?}");
        // Across `tspan`s: one at a run's edge is kept beside a collapsed one.
        let (a, b) = (format!("a \t{space}"), format!("{space} \r\n b "));
        let (a_kept, b_kept) = (format!("a {space}"), format!("{space} b"));
        assert_eq!(resolved(&[&a, &b]), [a_kept, b_kept], "{space:?}");
    }
}

#[test]
fn spaces_tabs_and_line_ends_collapse() {
    for space in [" ", "\t", "\n", "\r", " \t\r\n"] {
        let a = format!("{space}a{space}{space}b{space}");
        let b = format!("{space}c{space}");
        assert_eq!(resolved(&[&a, &b]), ["a b ", "c"], "{space:?}");
    }
}
