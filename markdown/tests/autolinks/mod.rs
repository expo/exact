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
fn url_suffix_markers_can_close_emphasis() {
    let x = "https://x.dev";
    let wiki = "https://en.wikipedia.org/wiki/Rust_(programming_language)";
    for (source, url, flags) in [
        ("**https://x.dev** z", x, BOLD),
        ("_https://x.dev_", x, ITALIC),
        ("~~https://x.dev~~", x, STRIKE),
        ("***https://x.dev***", x, BOLD | ITALIC),
        ("(**https://x.dev**)", x, BOLD),
        ("~https://x.dev~", x, 0),
        ("foo*https://x.dev", x, 0),
        (r"\*https://x.dev", x, 0),
        ("_https://x.dev/a_b_c~d_", "https://x.dev/a_b_c~d", ITALIC),
        ("~~https://x.dev/a~/b~~", "https://x.dev/a~/b", STRIKE),
        ("*https://x.dev/c*d*", "https://x.dev/c*d", ITALIC),
        (
            "_https://en.wikipedia.org/wiki/Rust_(programming_language)_",
            wiki,
            ITALIC,
        ),
    ] {
        assert_spans(source, &[(url, flags | LINK, url)]);
    }
    assert_spans(
        "Call _init then **https://en.wikipedia.org/wiki/Rust_(programming_language)**",
        &[(wiki, BOLD | LINK, wiki)],
    );
    assert_spans(
        "[see **https://x.dev**](u)",
        &[("see ", LINK, "u"), (x, BOLD | LINK, "u")],
    );
}

#[test]
fn interior_markers_are_part_of_the_url() {
    for (source, visible, url, flags) in [
        (
            "**https://x.dev**more",
            "**https://x.dev**more",
            "https://x.dev**more",
            0,
        ),
        (
            "***https://x.dev**more*",
            "**https://x.dev**more",
            "https://x.dev**more",
            ITALIC,
        ),
        (
            "**_https://x.dev**more_",
            "**https://x.dev**more",
            "https://x.dev**more",
            ITALIC,
        ),
        (
            "**foo_https://x.dev**bar",
            "**foo_https://x.dev**bar",
            "https://x.dev**bar",
            0,
        ),
        (
            "*foo_https://x.dev*bar",
            "*foo_https://x.dev*bar",
            "https://x.dev*bar",
            0,
        ),
        (
            "~~foo~https://x.dev~~bar",
            "~~foo~https://x.dev~~bar",
            "https://x.dev~~bar",
            0,
        ),
        (
            "**foo*https://x.dev**bar",
            "**foo*https://x.dev**bar",
            "https://x.dev**bar",
            0,
        ),
        (
            "~~https://x.dev~~more",
            "~~https://x.dev~~more",
            "https://x.dev~~more",
            0,
        ),
        (
            "**see https://x.dev**more",
            "**see https://x.dev**more",
            "https://x.dev**more",
            0,
        ),
        (
            "**https://x.dev/a*b**more",
            "**https://x.dev/a*b**more",
            "https://x.dev/a*b**more",
            0,
        ),
    ] {
        assert_spans(source, &[(url, flags | LINK, url)]);
        assert_eq!(plain(source), visible, "{source}");
    }
}

#[test]
fn urls_can_follow_closed_or_partly_closed_emphasis() {
    for (source, visible, text, flags, url) in [
        (
            "*before*https://x.dev/a*b",
            "beforehttps://x.dev/a*b",
            "before",
            ITALIC,
            "https://x.dev/a*b",
        ),
        (
            "***before**https://x.dev/a*b",
            "*beforehttps://x.dev/a*b",
            "before",
            BOLD,
            "https://x.dev/a*b",
        ),
        (
            "**foo**https://x.dev/a**b",
            "foohttps://x.dev/a**b",
            "foo",
            BOLD,
            "https://x.dev/a**b",
        ),
        (
            "**foo**https://x.dev**bar**",
            "foohttps://x.dev**bar**",
            "foo",
            BOLD,
            "https://x.dev**bar",
        ),
        (
            "**one**https://x.dev**two words**",
            "onehttps://x.dev**two words**",
            "one",
            BOLD,
            "https://x.dev**two",
        ),
        (
            "**_foo**https://x.dev/a_/b",
            "_foohttps://x.dev/a_/b",
            "_foo",
            BOLD,
            "https://x.dev/a_/b",
        ),
    ] {
        assert_spans(source, &[(text, flags, ""), (url, LINK, url)]);
        assert_eq!(plain(source), visible, "{source}");
    }
}

#[test]
fn url_backslashes_are_literal() {
    for (source, url, flags) in [
        (r"*https://x.dev/a\*/b*", r"https://x.dev/a\*/b", ITALIC),
        (r"*https://x.dev/a\\*/b*", r"https://x.dev/a\\*/b", ITALIC),
        (r"*https://x.dev/a\\* done", r"https://x.dev/a\\", ITALIC),
        (r"*https://x.dev\*", r"https://x.dev\", ITALIC),
        (r"**https://x.dev\\**.", r"https://x.dev\\", BOLD),
        (r"~~https://x.dev\\~~", r"https://x.dev\\", STRIKE),
    ] {
        assert_spans(source, &[(url, flags | LINK, url)]);
    }
}

#[test]
fn extended_urls_use_gfm_boundaries_and_trim() {
    for (source, url) in [
        ("https://x.dev/a>?<rest", "https://x.dev/a>"),
        ("(http://x.dev/a_(b))).!?,:", "http://x.dev/a_(b)"),
        ("https://x.dev/a)(b)/c", "https://x.dev/a)(b)/c"),
        ("https://x.dev/a&hl;?!", "https://x.dev/a"),
        ("https://x.dev/a)&a1;", "https://x.dev/a"),
        ("https://x.dev/a&;", "https://x.dev/a&;"),
        ("https://x.dev/a;", "https://x.dev/a;"),
        ("https://x.dev/a'", "https://x.dev/a'"),
        ("https://x.dev/a\"", "https://x.dev/a\""),
        ("https://a_b.x.dev/a", "https://a_b.x.dev/a"),
        ("https://x.dev_ next", "https://x.dev"),
        ("https://x.dev_?*https://y.dev/a", "https://y.dev/a"),
    ] {
        assert_spans(source, &[(url, LINK, url)]);
    }
    for source in [
        ")https://x.dev",
        r"x\https://x.dev",
        "ahttps://x.dev",
        "https://",
        "https://x_.dev/a",
        "https://x.dev_/a",
        "http://localhost/a",
    ] {
        assert_spans(source, &[]);
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
