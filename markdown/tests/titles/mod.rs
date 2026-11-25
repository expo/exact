//! A link's or image's title.

use super::*;

#[test]
fn titles_keep_parentheses_and_escapes() {
    for (destination, target) in [
        ("https://x \"b :) c\"", "https://x"),
        ("https://x \"b ( c\"", "https://x"),
        ("https://x 'b :) c'", "https://x"),
        ("https://x 'b ( c'", "https://x"),
        ("https://x (b ' c)", "https://x"),
        (r"https://x (b \) c)", "https://x"),
        (r"https://x (b \( c)", "https://x"),
        (r#"https://x "b \") c" "#, "https://x"),
        (r"https://x 'b \') c'", "https://x"),
        ("\t/a(b)\t \":) c\" ", "/a(b)"),
        (r"a\(b '日本) 😀'", "a(b"),
    ] {
        for (prefix, kind) in [("", LINK), ("!", IMAGE)] {
            let source = format!("[before](u) {prefix}[a]({destination}) [after](e)");
            let styled = style(&source, None);
            let targets: Vec<_> = styled
                .spans
                .iter()
                .map(|span| (span.href.as_str(), span.style))
                .collect();
            assert_eq!(
                targets,
                [("u", LINK), (target, kind), ("e", LINK)],
                "{source}"
            );
            assert_eq!(plain(&source), "before a after", "{source}");
        }
    }
    // What is not a title keeps its parentheses as text.
    for (source, text) in [
        ("[a](b \"c) d", "a d"),
        ("[a](b \"c) \"d\")", "a \"d\")"),
        ("[a](b (c) d)", "a"),
        ("[a](b) \"c)\"", "a \"c)\""),
        ("[a](b c)", "a"),
    ] {
        assert_eq!(plain(source), text, "{source}");
        assert_eq!(style(source, None).spans[0].href, "b", "{source}");
    }
    for source in ["![a](b.png \":) c\")", "![a](b.png 'c ( d')"] {
        let image = Segment::Image {
            caption: "a",
            src: "b.png".into(),
        };
        assert_eq!(segments(source, 0), [image], "{source}");
    }
    for source in [
        "![a](b(c).png \"d\")",
        "![a](b.png \"c\") d)",
        "![a](b.png \"c\" (d))",
    ] {
        assert_eq!(segments(source, 0), [Segment::Text(source)], "{source}");
    }
}

#[test]
fn unclosed_titles_do_not_rescan_the_suffix() {
    for opener in ["[a](b \"", "[a](b '", "[a](b (", "[a](b"] {
        for count in [4_000, 16_000] {
            let source = opener.repeat(count);
            assert!(style(&source, None).spans.is_empty());
        }
    }
}
