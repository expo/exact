// The page's port of `exact_plan::http_url_refusal` (LLP 1094.000 D1), which a drag's `url` (drag.js) and a share's
// (files.js) are admitted by, as the runner admits them on every other host: the same inputs refused, each with the
// same reason.

const HOST = 'its host is not a name of letters, digits, `-`, `.` and `_`, four decimal octets or a bracketed IPv6 address of hex groups';
const PORT = 'its port is not a whole number up to 65535';
const USERINFO_CHARACTER = "its user name or password holds a character it must write percent-encoded, as `'`, `;`, `=` or a second `:` must; write it as `%XX`";
const BLANK = /[\p{White_Space}\p{Cc}]/u;

/** Why `url` is no absolute `http:` or `https:` URL that WHATWG URL and Foundation read alike, as written once its
 * scheme and host are lowercased and an empty path is `/`, or null. */
export function httpUrlRefusal(url) {
  if (/^[\p{White_Space}\p{Cc}]|[\p{White_Space}\p{Cc}]$/u.test(url)) return 'it begins or ends with whitespace or a control character; remove it';
  if (/[\t\n\r]/.test(url)) return 'it holds a tab or a line break, which a browser drops; remove it';
  const scheme = /^https?:\/\//i.exec(url);
  if (!scheme) {
    if (/^https?:/i.test(url)) return 'it names no host; write `https://` and the host';
    return /^[a-z][a-z0-9+.-]*:/i.test(url) ? 'its scheme is not `http:` or `https:`' : "another app cannot open this app's path";
  }
  const rest = url.slice(scheme[0].length), end = rest.search(/[/?#]|$/);
  const authority = rest.slice(0, end), after = rest.slice(end), at = authority.lastIndexOf('@');
  const userinfo = at < 0 ? '' : authority.slice(0, at), hostPort = authority.slice(at + 1);
  if (BLANK.test(userinfo)) return 'its user name or password holds whitespace or a control character; encode a space as `%20`';
  const colons = hostPort.split(':').length - 1;
  const portAt = hostPort.startsWith('[') ? (hostPort.indexOf(']:') < 0 ? -1 : hostPort.indexOf(']:') + 1) : ipv6(hostPort) ? -1 : hostPort.indexOf(':');
  const [hostPart, portPart] = portAt < 0 ? [hostPort, ''] : [hostPort.slice(0, portAt), hostPort.slice(portAt)];
  if (BLANK.test(hostPart)) return 'its host holds whitespace or a control character; write the host without it';
  if (BLANK.test(portPart)) return PORT;
  if (BLANK.test(after)) return 'it holds whitespace or a control character; encode a space as `%20`';
  if (url.includes('\\')) return 'it holds a backslash, which a browser reads as `/` in a path and Foundation as `%5C`; write `/` for a separator or `%5C` for a backslash';
  if (!url.split('%').slice(1).every((piece) => /^[0-9a-f]{2}/i.test(piece))) return 'it holds a `%` that two hex digits do not follow; write `%25` for a percent sign';
  const hash = after.indexOf('#'), beforeFragment = hash < 0 ? after : after.slice(0, hash), fragment = hash < 0 ? '' : after.slice(hash + 1);
  const mark = beforeFragment.indexOf('?'), path = mark < 0 ? beforeFragment : beforeFragment.slice(0, mark), query = mark < 0 ? '' : beforeFragment.slice(mark + 1);
  // Each part keeps only what both parsers leave as written: RFC 3986's characters for it, which Foundation keeps, less
  // those a browser escapes there (`'` in a special scheme's query; `'`, `;`, `=` and a second `:` in userinfo), as
  // measured in Chromium, Firefox, WebKit and Foundation.
  const colon = userinfo.indexOf(':'), user = colon < 0 ? userinfo : userinfo.slice(0, colon), password = colon < 0 ? '' : userinfo.slice(colon + 1);
  for (const [text, marks, why] of [[user, '!$&()*+,', USERINFO_CHARACTER], [password, '!$&()*+,', USERINFO_CHARACTER],
    [path, "!$&'()*+,;=:@/", 'its path holds a character it must write percent-encoded, as a non-ASCII one or `|` must; write it as UTF-8 `%XX`'],
    [query, '!$&()*+,;=:@/?', "its query holds a character it must write percent-encoded, as a non-ASCII one or `'` must; write it as UTF-8 `%XX`"],
    [fragment, "!$&'()*+,;=:@/?", 'its fragment holds a character it must write percent-encoded, as a non-ASCII one must; write it as UTF-8 `%XX`']]) {
    if (![...text].every((c) => /[A-Za-z0-9\-._~%]/.test(c) || marks.includes(c))) return why;
  }
  if (at >= 0 && !userinfo) return 'it has an `@` with no user name before it, which a browser drops; remove the `@`';
  if (at >= 0 && !user) return 'it has a password or a `:` with no user name; give a user name, or remove what is before the `@` and the `@`';
  if (userinfo.endsWith(':')) return 'its password is empty, which a browser drops with its `:`; remove the `:`';
  if (path.split('/').some((segment) => ['.', '..'].includes(segment.toLowerCase().replaceAll('%2e', '.')))) {
    return 'its path holds a `.` or `..` segment, which a browser resolves and Foundation keeps; write the path it stands for';
  }
  let host, port;
  const bracketed = hostPort.startsWith('[');
  if (bracketed) {
    const close = hostPort.indexOf(']'), tail = hostPort.slice(close + 1);
    if (close < 0 || (tail && !tail.startsWith(':'))) return HOST;
    [host, port] = [hostPort.slice(1, close), tail ? tail.slice(1) : undefined];
  } else {
    if (colons > 1) return ipv6(hostPort) ? 'an IPv6 host goes in brackets, as `https://[2001:db8::1]/`' : PORT;
    [host, port] = colons ? [hostPort.slice(0, hostPort.indexOf(':')), hostPort.slice(hostPort.indexOf(':') + 1)] : [hostPort, undefined];
  }
  if (!host) return 'it names no host';
  // IDNA, which WHATWG and Foundation apply differently, maps a non-ASCII host and checks an `xn--` label.
  if (/[^\x00-\x7f]/.test(host) || host.split('.').some((label) => /^xn--/i.test(label))) {
    return 'its host is an internationalized name, which a browser and Foundation map by IDNA differently; use an ASCII host with no `xn--` label';
  }
  // Firefox reads no URL of 1 MiB or more, counting the `/` a browser gives an empty path and the two bytes its parser
  // adds (an `@` it budgets and a terminating NUL).
  if (new TextEncoder().encode(url).length + (path ? 0 : 1) + 2 >= 2 ** 20) return 'it is too long for every browser to read, 1 MiB or more; shorten it';
  if (!(bracketed ? ipv6(host) : hostName(host))) return HOST;
  if (bracketed && host.toLowerCase() !== whatwgIpv6(host)) return 'its IPv6 address is not in the form a browser writes it in, as `[2001:db8::1]` is; write that form';
  if (port === undefined) return null;
  if (!/^\d*$/.test(port) || port.replace(/^0+/, '').length > 5 || Number(port) > 65535) return PORT;
  if (port === '') return 'its port is empty, which a browser drops with its `:`; remove the `:`';
  if (port.length > 1 && port.startsWith('0')) return 'its port has a leading zero, which a browser drops; write the port without it';
  return port === (/^https:/i.test(url) ? '443' : '80') ? "its port is its scheme's default, which a browser drops; remove it" : null;
}

/** `text`, an IPv6 address of hex groups, as WHATWG URL's host serializer writes it: lowercase groups with no leading
 * zeros, and the first longest run of two or more zero groups written `::`. */
function whatwgIpv6(text) {
  const groups = (part) => part.split(':').filter(Boolean).map((group) => parseInt(group, 16));
  const at = text.indexOf('::');
  const pieces = at < 0 ? groups(text) : (() => {
    const head = groups(text.slice(0, at)), tail = groups(text.slice(at + 2));
    return [...head, ...Array(8 - head.length - tail.length).fill(0), ...tail];
  })();
  let compress = -1, longest = 1;
  for (let i = 0; i < 8;) {
    const start = i;
    while (i < 8 && pieces[i] === 0) i++;
    if (i - start > longest) [compress, longest] = [start, i - start];
    i = Math.max(i, start + 1);
  }
  let out = '', skipping = false;
  pieces.forEach((piece, index) => {
    if (skipping && piece === 0) return;
    skipping = false;
    if (index === compress) { out += index === 0 ? '::' : ':'; skipping = true; return; }
    out += piece.toString(16) + (index === 7 ? '' : ':');
  });
  return out;
}

/** An IPv6 address of hex groups: eight of one to four digits, or fewer around one `::`, with no dotted IPv4 part. */
function ipv6(text) {
  if (!/^[0-9a-f:]+$/i.test(text)) return false;
  const groups = (part) => part === '' ? 0 : part.split(':').every((group) => group.length >= 1 && group.length <= 4) ? part.split(':').length : null;
  const at = text.indexOf('::');
  if (at < 0) return groups(text) === 8;
  const head = groups(text.slice(0, at)), tail = text.slice(at + 2);
  if (tail.includes('::')) return false;
  const after = groups(tail);
  return head != null && after != null && head + after <= 7;
}

/** A name of ASCII letters, digits, `-`, `.` and `_`, and four decimal octets with no leading zero or trailing dot when
 * its last label is a number. */
function hostName(name) {
  if (!/^[A-Za-z0-9._-]+$/.test(name)) return false;
  const labels = name.endsWith('.') ? name.slice(0, -1) : name, last = labels.slice(labels.lastIndexOf('.') + 1);
  if (!/^(?:\d+|0x[0-9a-f]*)$/i.test(last)) return true;
  const octets = name.split('.');
  return octets.length === 4 && octets.every((octet) => /^(?:0|[1-9]\d{0,2})$/.test(octet) && Number(octet) <= 255);
}
