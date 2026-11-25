//! Bare URLs and the links they sit in.

use super::*;

#[test]
fn a_bare_url_in_a_link_label_opens_the_links_destination() {
    let to = "https://y.com/z";
    for (source, expected) in [
        (
            "[https://x.com](https://y.com/z)",
            &[("https://x.com", LINK, to)][..],
        ),
        (
            "[see https://x.com now](https://y.com/z)",
            &[("see https://x.com now", LINK, to)],
        ),
        (
            "[**see https://x.com** (https://w.com)](https://y.com/z)",
            &[
                ("see https://x.com", BOLD | LINK, to),
                (" (https://w.com)", LINK, to),
            ],
        ),
        // Brackets that are no link, and the text after one that is.
        (
            "[see https://x.com now] [a](https://y.com/z) https://w.com",
            &[
                ("https://x.com", LINK, "https://x.com"),
                ("a", LINK, to),
                ("https://w.com", LINK, "https://w.com"),
            ],
        ),
    ] {
        let styled: Vec<_> = style(source, None)
            .spans
            .into_iter()
            .map(|s| (utf16(source, s.range), s.style, s.href))
            .collect();
        let drawn: Vec<_> = pieces(source)
            .into_iter()
            .filter(|p| !p.href.is_empty())
            .map(|p| (p.text, p.href))
            .collect();
        let spans: Vec<_> = expected
            .iter()
            .map(|(text, flags, href)| (text.to_string(), *flags, href.to_string()))
            .collect();
        let links: Vec<_> = expected
            .iter()
            .map(|(text, _, href)| (text.to_string(), href.to_string()))
            .collect();
        assert_eq!(styled, spans, "{source}");
        assert_eq!(drawn, links, "{source}");
    }
}

#[test]
fn a_bare_url_just_inside_emphasis_is_a_link() {
    let (x, bold) = ("https://x.dev", BOLD | LINK);
    for (source, expected) in [
        ("**https://x.dev** z", &[(x, bold, x)][..]),
        ("_https://x.dev_", &[(x, ITALIC | LINK, x)]),
        ("~~https://x.dev~~", &[(x, STRIKE | LINK, x)]),
        ("***https://x.dev***", &[(x, BOLD | ITALIC | LINK, x)]),
        ("(**https://x.dev**)", &[(x, bold, x)]),
        (r"\*https://x.dev", &[(x, LINK, x)]),
        (
            "_https://x.dev/a_b_c~d_",
            &[(
                "https://x.dev/a_b_c~d",
                ITALIC | LINK,
                "https://x.dev/a_b_c~d",
            )],
        ),
        // A URL runs to white space, but not over the closer of the emphasis it starts.
        ("**https://x.dev**more", &[(x, bold, x)]),
        (
            "***https://x.dev**more*",
            &[(x, BOLD | ITALIC | LINK, x), ("more", ITALIC, "")],
        ),
        (
            "_**https://x.dev**more_",
            &[(x, BOLD | ITALIC | LINK, x), ("more", ITALIC, "")],
        ),
        (
            "~~https://x.dev/a~/b~~",
            &[("https://x.dev/a~/b", STRIKE | LINK, "https://x.dev/a~/b")],
        ),
        (
            "**https://x.dev/a*b**more",
            &[("https://x.dev/a*b", bold, "https://x.dev/a*b")],
        ),
        // Emphasis adds no link inside a link's label.
        (
            "[see **https://x.dev**](u)",
            &[("see ", LINK, "u"), (x, bold, "u")],
        ),
    ] {
        assert_spans(source, expected);
    }
}

#[test]
fn a_url_after_a_delimiter_stops_at_a_closer_of_an_earlier_opener() {
    let x = "https://x.dev";
    for (source, prefix, flags) in [
        ("**foo_https://x.dev**bar", "foo_", BOLD),
        ("*foo_https://x.dev*bar", "foo_", ITALIC),
        ("~~foo~https://x.dev~~bar", "foo~", STRIKE),
        ("**foo*https://x.dev**bar", "foo*", BOLD),
        ("**_https://x.dev**more_", "_", BOLD),
    ] {
        assert_spans(source, &[(prefix, flags, ""), (x, flags | LINK, x)]);
    }
}

#[test]
fn only_unused_openers_can_shorten_a_url() {
    for (source, text, flags, url) in [
        (
            "*before*https://x.dev/a*b",
            "before",
            ITALIC,
            "https://x.dev/a*b",
        ),
        (
            "**foo**https://x.dev/a**b",
            "foo",
            BOLD,
            "https://x.dev/a**b",
        ),
        (
            "**foo**https://x.dev**bar**",
            "foo",
            BOLD,
            "https://x.dev**bar",
        ),
        (
            "**_foo**https://x.dev/a_/b",
            "_foo",
            BOLD,
            "https://x.dev/a_/b",
        ),
    ] {
        assert_spans(source, &[(text, flags, ""), (url, LINK, url)]);
    }
    assert_spans(
        "***before**https://x.dev/a*b",
        &[
            ("before", BOLD | ITALIC, ""),
            ("https://x.dev/a", ITALIC | LINK, "https://x.dev/a"),
        ],
    );
}

#[test]
fn an_escaped_delimiter_inside_a_url_cannot_close_emphasis() {
    for (source, url, flags) in [
        (
            r"*https://x.dev/a\*/b*",
            r"https://x.dev/a\*/b",
            ITALIC | LINK,
        ),
        (
            r"*https://x.dev/a\\*/b*",
            r"https://x.dev/a\\*/b",
            ITALIC | LINK,
        ),
        (r"*https://x.dev\*", r"https://x.dev\*", LINK),
    ] {
        assert_spans(source, &[(url, flags, url)]);
    }
}

#[test]
fn a_url_only_checks_for_closers_after_a_delimiter() {
    let x = "https://x.dev";
    for (source, visible, url, flags) in [
        (
            "~~https://x.dev~~more",
            "https://x.devmore",
            x,
            STRIKE | LINK,
        ),
        ("foo*https://x.dev", "foo*https://x.dev", x, LINK),
        (
            "*https://x.dev/c*d*",
            "https://x.dev/cd*",
            "https://x.dev/c",
            ITALIC | LINK,
        ),
        (
            "**see https://x.dev**more",
            "**see https://x.dev**more",
            "https://x.dev**more",
            LINK,
        ),
    ] {
        assert_spans(source, &[(url, flags, url)]);
        assert_eq!(plain(source), visible, "{source}");
    }
}

fn assert_spans(source: &str, expected: &[(&str, u8, &str)]) {
    let styled = style(source, None);
    let targets: Vec<_> = styled
        .spans
        .iter()
        .map(|s| (utf16(source, s.range), s.style, s.href.as_str()))
        .collect();
    let expected: Vec<_> = expected
        .iter()
        .map(|(text, style, href)| (text.to_string(), *style, *href))
        .collect();
    assert_eq!(targets, expected, "{source}");
}
