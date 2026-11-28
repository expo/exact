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
}
