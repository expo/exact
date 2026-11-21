//! A drag's and a share's `url`: the rule every host admits one by (LLP
//! 1094.000 D1), which `host/web-js/url.js` ports for the page.

/// Whether `url` is an absolute `http:` or `https:` URL with a host, as a
/// drag's and a share's `url=` must be (LLP 1094.000 D1).
pub fn is_absolute_http_url(url: &str) -> bool {
    http_url_refusal(url).is_none()
}

/// Why `url` is no absolute `http:` or `https:` URL that WHATWG URL (`new
/// URL`) and Foundation's `URL(string:)` read alike (LLP 1094.000 D1), or
/// `None` when it is one: as written, once its scheme and host are
/// lowercased and an empty path is `/`. Past `http://` or `https://`, the
/// host after any userinfo is a bracketed IPv6 address of hex groups in the
/// form WHATWG writes, four decimal octets with no leading zero, or a name
/// of ASCII letters, digits, `-`, `.` and `_`; a port is a decimal number up
/// to 65535 that is not the scheme's default; each part holds only what
/// neither parser escapes there; each `%` begins an escape; and no path
/// segment is `.` or `..`, written or escaped.
pub fn http_url_refusal(url: &str) -> Option<&'static str> {
    let blank = |c: char| c.is_whitespace() || c.is_control();
    if url.starts_with(blank) || url.ends_with(blank) {
        return Some("it begins or ends with whitespace or a control character; remove it");
    }
    if url.contains(['\t', '\n', '\r']) {
        return Some("it holds a tab or a line break, which a browser drops; remove it");
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
            _ if lower.starts_with("http:") || lower.starts_with("https:") => {
                "it names no host; write `https://` and the host"
            }
            true => "its scheme is not `http:` or `https:`",
            false => "another app cannot open this app's path",
        });
    };
    let end = rest.find(['/', '?', '#']).unwrap_or(rest.len());
    let (authority, after) = rest.split_at(end);
    let (userinfo, host_port) = authority.rsplit_once('@').unwrap_or(("", authority));
    if userinfo.chars().any(blank) {
        return Some("its user name or password holds whitespace or a control character; encode a space as `%20`");
    }
    let port_at = if host_port.starts_with('[') {
        host_port.find("]:").map(|at| at + 1)
    } else if is_ipv6_text(host_port) {
        None
    } else {
        host_port.find(':')
    };
    let (host_part, port_part) = host_port.split_at(port_at.unwrap_or(host_port.len()));
    if host_part.chars().any(blank) {
        return Some("its host holds whitespace or a control character; write the host without it");
    }
    if port_part.chars().any(blank) {
        return Some(PORT);
    }
    if after.chars().any(blank) {
        return Some("it holds whitespace or a control character; encode a space as `%20`");
    }
    if url.contains('\\') {
        return Some(
            "it holds a backslash, which a browser reads as `/` in a path and Foundation as `%5C`; write `/` for a separator or `%5C` for a backslash",
        );
    }
    let escapes = url
        .split('%')
        .skip(1)
        .all(|after| after.len() >= 2 && after.as_bytes()[..2].iter().all(u8::is_ascii_hexdigit));
    if !escapes {
        return Some(
            "it holds a `%` that two hex digits do not follow; write `%25` for a percent sign",
        );
    }
    let (before_fragment, fragment) = after.split_once('#').unwrap_or((after, ""));
    let (path, query) = before_fragment
        .split_once('?')
        .unwrap_or((before_fragment, ""));
    // Each part keeps only what both parsers leave as written: RFC 3986's characters for it, which Foundation keeps,
    // less those a browser escapes there (`'` in a special scheme's query; `'`, `;`, `=` and a second `:` in userinfo),
    // as measured in Chromium, Firefox, WebKit and Foundation.
    let (user, password) = userinfo.split_once(':').unwrap_or((userinfo, ""));
    let parts = [
        (user, "!$&()*+,", USERINFO_CHARACTER),
        (password, "!$&()*+,", USERINFO_CHARACTER),
        (path, "!$&'()*+,;=:@/", "its path holds a character it must write percent-encoded, as a non-ASCII one or `|` must; write it as UTF-8 `%XX`"),
        (query, "!$&()*+,;=:@/?", "its query holds a character it must write percent-encoded, as a non-ASCII one or `'` must; write it as UTF-8 `%XX`"),
        (fragment, "!$&'()*+,;=:@/?", "its fragment holds a character it must write percent-encoded, as a non-ASCII one must; write it as UTF-8 `%XX`"),
    ];
    for (text, marks, why) in parts {
        if !text
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || "-._~%".contains(c) || marks.contains(c))
        {
            return Some(why);
        }
    }
    if authority.contains('@') && userinfo.is_empty() {
        return Some(
            "it has an `@` with no user name before it, which a browser drops; remove the `@`",
        );
    }
    if authority.contains('@') && user.is_empty() {
        return Some(
            "it has a password or a `:` with no user name; give a user name, or remove what is before the `@` and the `@`",
        );
    }
    if userinfo.ends_with(':') {
        return Some("its password is empty, which a browser drops with its `:`; remove the `:`");
    }
    if path.split('/').any(is_dot_segment) {
        return Some("its path holds a `.` or `..` segment, which a browser resolves and Foundation keeps; write the path it stands for");
    }
    let (host, port, bracketed) = match host_port.strip_prefix('[') {
        Some(inside) => match inside.split_once(']') {
            Some((address, "")) => (address, None, true),
            Some((address, after)) => match after.strip_prefix(':') {
                Some(port) => (address, Some(port), true),
                None => return Some(HOST),
            },
            None => return Some(HOST),
        },
        None if host_port.matches(':').count() > 1 => {
            return Some(if is_ipv6_text(host_port) {
                "an IPv6 host goes in brackets, as `https://[2001:db8::1]/`"
            } else {
                PORT
            });
        }
        None => match host_port.split_once(':') {
            Some((name, port)) => (name, Some(port), false),
            None => (host_port, None, false),
        },
    };
    if host.is_empty() {
        return Some("it names no host");
    }
    // IDNA, which WHATWG and Foundation apply differently, maps a non-ASCII host and checks an `xn--` label.
    let idn = host.split('.').any(|label| {
        label
            .get(..4)
            .is_some_and(|prefix| prefix.eq_ignore_ascii_case("xn--"))
    });
    if !host.is_ascii() || idn {
        return Some(
            "its host is an internationalized name, which a browser and Foundation map by IDNA differently; use an ASCII host with no `xn--` label",
        );
    }
    // Firefox reads no URL of 1 MiB or more, counting the `/` a browser gives an empty path and the two bytes its
    // parser adds (an `@` it budgets and a terminating NUL).
    if url.len() + usize::from(path.is_empty()) + 2 >= 1 << 20 {
        return Some("it is too long for every browser to read, 1 MiB or more; shorten it");
    }
    let host_ok = if bracketed {
        is_ipv6_text(host)
    } else {
        is_http_host_name(host)
    };
    if !host_ok {
        return Some(HOST);
    }
    if bracketed && host.to_ascii_lowercase() != whatwg_ipv6(host) {
        return Some("its IPv6 address is not in the form a browser writes it in, as `[2001:db8::1]` is; write that form");
    }
    let Some(port) = port else {
        return None;
    };
    let value = port.trim_start_matches('0');
    if !port.bytes().all(|b| b.is_ascii_digit())
        || value.len() > 5
        || value.parse::<u32>().is_ok_and(|n| n > 65535)
    {
        return Some(PORT);
    }
    let default = if lower.starts_with("https://") {
        "443"
    } else {
        "80"
    };
    Some(if port.is_empty() {
        "its port is empty, which a browser drops with its `:`; remove the `:`"
    } else if port.len() > 1 && port.starts_with('0') {
        "its port has a leading zero, which a browser drops; write the port without it"
    } else if port == default {
        "its port is its scheme's default, which a browser drops; remove it"
    } else {
        return None;
    })
}

const HOST: &str =
    "its host is not a name of letters, digits, `-`, `.` and `_`, four decimal octets or a bracketed IPv6 address of hex groups";
const PORT: &str = "its port is not a whole number up to 65535";
const USERINFO_CHARACTER: &str = "its user name or password holds a character it must write percent-encoded, as `'`, `;`, `=` or a second `:` must; write it as `%XX`";

/// `text`, an IPv6 address of hex groups, as WHATWG URL's host serializer
/// writes it: lowercase groups with no leading zeros, and the first longest
/// run of two or more zero groups written `::`.
fn whatwg_ipv6(text: &str) -> String {
    let groups = |part: &str| -> Vec<u16> {
        part.split(':')
            .filter(|group| !group.is_empty())
            .map(|group| u16::from_str_radix(group, 16).unwrap_or(0))
            .collect()
    };
    let pieces: Vec<u16> = match text.split_once("::") {
        None => groups(text),
        Some((head, tail)) => {
            let (head, tail) = (groups(head), groups(tail));
            let zeros = 8 - head.len() - tail.len();
            head.into_iter()
                .chain(std::iter::repeat_n(0, zeros))
                .chain(tail)
                .collect()
        }
    };
    let (mut compress, mut longest, mut at) = (None, 1, 0);
    while at < 8 {
        let start = at;
        while at < 8 && pieces[at] == 0 {
            at += 1;
        }
        if at - start > longest {
            (compress, longest) = (Some(start), at - start);
        }
        at = at.max(start + 1);
    }
    let mut out = String::new();
    let mut skipping = false;
    for (index, piece) in pieces.iter().enumerate() {
        if skipping && *piece == 0 {
            continue;
        }
        skipping = false;
        if compress == Some(index) {
            out.push_str(if index == 0 { "::" } else { ":" });
            skipping = true;
            continue;
        }
        out.push_str(&format!("{piece:x}"));
        if index != 7 {
            out.push(':');
        }
    }
    out
}

/// Whether `segment` of a path is `.` or `..`, a dot written or escaped as
/// `%2e`, which WHATWG resolves as it parses.
fn is_dot_segment(segment: &str) -> bool {
    let dots = segment.to_ascii_lowercase().replace("%2e", ".");
    dots == "." || dots == ".."
}

/// Whether `text` is an IPv6 address of hex groups: eight of one to four
/// digits, or fewer around one `::`, with no dotted IPv4 part, the spelling
/// WHATWG, WebKit and Foundation all read alike.
fn is_ipv6_text(text: &str) -> bool {
    if !text.bytes().all(|b| b.is_ascii_hexdigit() || b == b':') {
        return false;
    }
    let groups = |part: &str| -> Option<usize> {
        if part.is_empty() {
            return Some(0);
        }
        part.split(':')
            .all(|group| (1..=4).contains(&group.len()))
            .then(|| part.split(':').count())
    };
    match text.split_once("::") {
        None => groups(text) == Some(8),
        Some((_, tail)) if tail.contains("::") => false,
        Some((head, tail)) => match (groups(head), groups(tail)) {
            (Some(head), Some(tail)) => head + tail <= 7,
            _ => false,
        },
    }
}

/// Whether `name` is a host name of ASCII letters, digits, `-`, `.` and
/// `_`, which every browser and Foundation write as they read (Chromium
/// escapes some sub-delimiters, such as `*`, that WebKit keeps), and is four
/// decimal octets with no leading zero or trailing dot when its last label
/// is a number, which WHATWG reads as IPv4.
fn is_http_host_name(name: &str) -> bool {
    if !name
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || "-._".contains(c))
    {
        return false;
    }
    let labels = name.strip_suffix('.').unwrap_or(name);
    let last = labels.rsplit('.').next().unwrap_or(labels);
    let numeric = !last.is_empty()
        && (last.bytes().all(|b| b.is_ascii_digit())
            || last
                .strip_prefix("0x")
                .or_else(|| last.strip_prefix("0X"))
                .is_some_and(|hex| hex.bytes().all(|b| b.is_ascii_hexdigit())));
    !numeric || name.parse::<core::net::Ipv4Addr>().is_ok()
}

#[cfg(test)]
mod tests {
    use super::{http_url_refusal, HOST, PORT};

    /// The cases `host/web-js/url.test.mjs` holds its port to, which it also
    /// reads with WHATWG URL: kept, and refused with each reason.
    #[test]
    fn an_absolute_http_url_has_a_host_both_parsers_read_alike() {
        for ok in [
            "https://example.com",
            "HTTP://example.com/a?b#c",
            "https://user:pass@example.com:8080/x",
            "https://[::1]:8080/x",
            "https://[2001:DB8::A]",
            "https://[1:2:3:4:5:6:7:0]/",
            "http://example.com:8080/",
            "https://[::ffff:c000:201]/",
            "https://[::]/",
            "https://192.168.0.1/",
            "https://example.com./",
            "https://a_b.example/",
            "https://example.com/%E2%82%AC?q=%2F",
            "https://example.com/a/.b/..c/?x=/../#/./",
            "https://user:pa%3Ass@example.com/it's?q=%27#it's",
        ] {
            assert_eq!(http_url_refusal(ok), None, "{ok}");
        }
        let space = "it holds whitespace or a control character; encode a space as `%20`";
        let host_space =
            "its host holds whitespace or a control character; write the host without it";
        let ascii = "its host is an internationalized name, which a browser and Foundation map by IDNA differently; use an ASCII host with no `xn--` label";
        let escape =
            "it holds a `%` that two hex digits do not follow; write `%25` for a percent sign";
        let backslash = "it holds a backslash, which a browser reads as `/` in a path and Foundation as `%5C`; write `/` for a separator or `%5C` for a backslash";
        let port = PORT;
        let path_character = "its path holds a character it must write percent-encoded, as a non-ASCII one or `|` must; write it as UTF-8 `%XX`";
        let ipv6 = "its IPv6 address is not in the form a browser writes it in, as `[2001:db8::1]` is; write that form";
        let dots = "its path holds a `.` or `..` segment, which a browser resolves and Foundation keeps; write the path it stands for";
        let no_host = "it names no host";
        for (no, why) in [
            ("/landmark/1016", "another app cannot open this app's path"),
            ("/landmark 1016", "another app cannot open this app's path"),
            ("ftp://example.com", "its scheme is not `http:` or `https:`"),
            ("https:example.com", "it names no host; write `https://` and the host"),
            ("https://", no_host),
            ("https:///a", no_host),
            ("https://?q", no_host),
            ("https://user@/x", no_host),
            ("https://[]/", no_host),
            ("https://[broken", HOST),
            ("https://[::1", HOST),
            ("https://[:::]/", HOST),
            ("https://[::ffff:192.0.2.999]/", HOST),
            ("https://[::ffff:192.0.2.1]/", HOST),
            ("https://[1:2:3:4::192.0.2.1]/", HOST),
            ("https://[::1:]/", HOST),
            ("https://[1::2::3]/", HOST),
            ("https://[1:2:3:4:5:6:7:8:9]/", HOST),
            ("https://[12345::]/", HOST),
            ("https://[fe80::a%25en1]/", HOST),
            ("https://[example.com]/", HOST),
            ("https://[::1]x/", HOST),
            ("https://2001:db8::1/", "an IPv6 host goes in brackets, as `https://[2001:db8::1]/`"),
            ("https://example.com:443:444/", port),
            ("https://exa<mple.com", HOST),
            ("https://a\"b.example/", HOST),
            ("https://a{b.example/", HOST),
            ("https://exa%41mple.com", HOST),
            ("https://example.123", HOST),
            ("https://1.2.3.999", HOST),
            ("https://1.2.3.09/", HOST),
            ("https://1.2.3.010/", HOST),
            ("https://1.2.3.4./", HOST),
            ("https://127.1/", HOST),
            ("https://b\u{fc}cher.example/", ascii),
            ("https://xn--bcher-kva.example/", ascii),
            ("https://xn--/", ascii),
            ("https://a.XN--b.example/", ascii),
            ("https://example.\u{ff11}\u{ff12}\u{ff13}/", ascii),
            ("http://\u{ff10}\u{ff38}\u{ff43}\u{ff10}\u{ff0e}\u{ff10}\u{ff12}\u{ff15}\u{ff10}\u{ff0e}\u{ff10}\u{ff11}", ascii),
            ("https://example.com:99999", port),
            ("https://example.com:8a", port),
            ("https://exa mple.com", host_space),
            (
                "https://user name@example.com/",
                "its user name or password holds whitespace or a control character; encode a space as `%20`",
            ),
            ("https://example.com: 80/", port),
            ("https://example.com:443: 444/", port),
            ("https://example.com/\u{feff}", path_character),
            ("https://example.com/caf\u{e9}%20menu", path_character),
            ("https://example.com/a|b", path_character),
            (
                "https://example.com/?q='",
                "its query holds a character it must write percent-encoded, as a non-ASCII one or `'` must; write it as UTF-8 `%XX`",
            ),
            (
                "https://example.com/#caf\u{e9}",
                "its fragment holds a character it must write percent-encoded, as a non-ASCII one must; write it as UTF-8 `%XX`",
            ),
            ("https://user;name@example.com/", super::USERINFO_CHARACTER),
            ("https://user:pass:word@example.com/", super::USERINFO_CHARACTER),
            ("https://o'brien@example.com/", super::USERINFO_CHARACTER),
            ("https://:pass@example.com/", "it has a password or a `:` with no user name; give a user name, or remove what is before the `@` and the `@`"),
            ("https://:@example.com/", "it has a password or a `:` with no user name; give a user name, or remove what is before the `@` and the `@`"),
            ("https://example.com:065535/", "its port has a leading zero, which a browser drops; write the port without it"),
            ("ht\ttps://example.com/", "it holds a tab or a line break, which a browser drops; remove it"),
            ("https://example.com/a\nb", "it holds a tab or a line break, which a browser drops; remove it"),
            ("https://a*b.example/a", HOST),
            ("https://a~b.example/a", HOST),
            ("https://example.com:443/a", "its port is its scheme's default, which a browser drops; remove it"),
            ("http://example.com:80/", "its port is its scheme's default, which a browser drops; remove it"),
            ("https://example.com:0443/", "its port has a leading zero, which a browser drops; write the port without it"),
            ("https://example.com:/", "its port is empty, which a browser drops with its `:`; remove the `:`"),
            ("https://user:@example.com/a", "its password is empty, which a browser drops with its `:`; remove the `:`"),
            ("https://@example.com/a", "it has an `@` with no user name before it, which a browser drops; remove the `@`"),
            ("https://[2001:0db8:0:0:0:0:0:1]/a", ipv6),
            ("https://[1:2:3:4:5:6:7::]/", ipv6),
            ("https://[1:0:0:2:0:0:0:3]/", ipv6),
            (" https://example.com/a", "it begins or ends with whitespace or a control character; remove it"),
            ("https://example.com/a ", "it begins or ends with whitespace or a control character; remove it"),
            ("https://example.com/a{b", path_character),
            ("https://example.com/a b", space),
            ("https://example.com/\u{85}a", space),
            (
                &format!("https://example.com/{}", "a".repeat(1 << 20)),
                "it is too long for every browser to read, 1 MiB or more; shorten it",
            ),
            (
                &format!("https://example.com?{}", "a".repeat((1 << 20) - 22)),
                "it is too long for every browser to read, 1 MiB or more; shorten it",
            ),
            ("https://example.com/a%zz", escape),
            ("https://example.com/a/../b", dots),
            ("https://example.com/a/%2e%2E/b", dots),
            ("https://example.com/./a", dots),
            ("https://example.com/a/.%2e", dots),
            ("https://example.com/100%", escape),
            ("https://example.com/a\\b", backslash),
            ("https://example.com/#a\\b", backslash),
        ] {
            assert_eq!(http_url_refusal(no), Some(why), "{no}");
        }
    }
}
