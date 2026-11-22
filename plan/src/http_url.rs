//! A drag's and a share's `url`: the rule Exact admits one by (LLP 1094.000
//! D1), which `host/web-js/url.js` ports for the page. It checks what Exact
//! owns: that the author meant a web link, not the app's own path or another
//! scheme, and that the text holds nothing that breaks how a host frames it
//! (`text/uri-list` lines, the props' newline-separated lists). Whether a
//! link opens is the receiving app's parser's to decide; Exact carries the
//! URL as written.

/// Why `url` is no web link a drag or a share carries (LLP 1094.000 D1), or
/// `None` when it is one: it starts with `http://` or `https://`, in any
/// case, a host follows, and it holds no space or ASCII control character.
pub fn http_url_refusal(url: &str) -> Option<&'static str> {
    let blank = |b: u8| b <= b' ' || b == 0x7f;
    if url.bytes().next().is_some_and(blank) || url.bytes().next_back().is_some_and(blank) {
        return Some("it begins or ends with a space or a control character; remove it");
    }
    let lower = url.get(..8).unwrap_or(url).to_ascii_lowercase();
    let Some(rest) = ["https://", "http://"]
        .into_iter()
        .find(|scheme| lower.starts_with(scheme))
        .map(|scheme| &url[scheme.len()..])
    else {
        let scheme = url.split_once(':').is_some_and(|(scheme, _)| {
            scheme.starts_with(|c: char| c.is_ascii_alphabetic())
                && scheme
                    .chars()
                    .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '-' | '.'))
        });
        return Some(match scheme {
            _ if lower.starts_with("http:") || lower.starts_with("https:") => NO_HOST,
            true => "its scheme is not `http:` or `https:`",
            false => "another app cannot open this app's path",
        });
    };
    // The host, read only far enough to see that there is one: after any
    // user information, before any port, inside brackets for an IPv6 address.
    let authority = &rest[..rest.find(['/', '?', '#']).unwrap_or(rest.len())];
    let host_port = authority
        .rsplit_once('@')
        .map_or(authority, |(_, after)| after);
    let host = match host_port.strip_prefix('[') {
        Some(inside) => inside.split(']').next(),
        None => host_port.split(':').next(),
    };
    if host.is_none_or(str::is_empty) {
        return Some(NO_HOST);
    }
    if url.bytes().any(blank) {
        return Some("it holds a space, a tab, a line break or another control character; write a space as `%20` and remove the rest");
    }
    None
}

const NO_HOST: &str = "it names no host; write the host after `https://`";

#[cfg(test)]
mod tests {
    use super::{http_url_refusal, NO_HOST};

    /// The cases `host/web-js/url.test.mjs` holds its port to: kept, and
    /// refused with each reason.
    #[test]
    fn a_web_link_names_a_host_and_holds_no_control_character() {
        for ok in [
            "https://example.com",
            "HTTP://example.com/a?b#c",
            "https://user:pass@example.com:8080/x",
            "https://[::1]:8080/x",
            "https://example.com/caf\u{e9}?q='#\u{85}",
            "https://example.com:99999",
            "https://[broken",
            "https://@example.com/",
        ] {
            assert_eq!(http_url_refusal(ok), None, "{ok}");
        }
        let edge = "it begins or ends with a space or a control character; remove it";
        let inside = "it holds a space, a tab, a line break or another control character; write a space as `%20` and remove the rest";
        for (no, why) in [
            ("/landmark/1016", "another app cannot open this app's path"),
            ("/landmark 1016", "another app cannot open this app's path"),
            ("ftp://example.com", "its scheme is not `http:` or `https:`"),
            ("https:example.com", NO_HOST),
            ("https://", NO_HOST),
            ("https:///a", NO_HOST),
            ("https://?q", NO_HOST),
            ("https://#a", NO_HOST),
            ("https://:443/", NO_HOST),
            ("https://user@/x", NO_HOST),
            ("https://[]/", NO_HOST),
            (" https://example.com/a", edge),
            ("https://example.com/a\n", edge),
            ("https://example.com/a b", inside),
            ("https://example.com/a\r\nb", inside),
            ("https://example.com/\u{7f}", edge),
            ("https://exa\u{7f}mple.com/", inside),
        ] {
            assert_eq!(http_url_refusal(no), Some(why), "{no:?}");
        }
    }
}
