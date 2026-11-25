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
