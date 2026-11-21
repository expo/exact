// The page's URL rule against the runner's: `plan/src/http_url.rs`'s cases, kept and refused with the same reasons,
// each read by WHATWG URL too (Bun's, which is WebKit's parser). Every URL kept parses; a URL refused that WHATWG
// parses is one refused on purpose (LLP 1094.000 D1), since Foundation reads it otherwise or not at all.
import { test, expect } from 'bun:test';
import { httpUrlRefusal } from './url.js';

const HOST = 'its host is not a name of letters, digits, `-`, `.` and `_`, four decimal octets or a bracketed IPv6 address of hex groups';
const SPACE = 'it holds whitespace or a control character; encode a space as `%20`', NO_HOST = 'it names no host';
const HOST_SPACE = 'its host holds whitespace or a control character; write the host without it';
const ASCII = 'its host is an internationalized name, which a browser and Foundation map by IDNA differently; use an ASCII host with no `xn--` label';
const ESCAPE = 'it holds a `%` that two hex digits do not follow; write `%25` for a percent sign';
const BACKSLASH = 'it holds a backslash, which a browser reads as `/` in a path and Foundation as `%5C`; write `/` for a separator or `%5C` for a backslash';
const PORT = 'its port is not a whole number up to 65535', PATH = "another app cannot open this app's path";
const PATH_CHARACTER = 'its path holds a character it must write percent-encoded, as a non-ASCII one or `|` must; write it as UTF-8 `%XX`';
const USERINFO = "its user name or password holds a character it must write percent-encoded, as `'`, `;`, `=` or a second `:` must; write it as `%XX`";
const IPV6 = 'its IPv6 address is not in the form a browser writes it in, as `[2001:db8::1]` is; write that form';
const EDGE = 'it begins or ends with whitespace or a control character; remove it';
const DOTS = 'its path holds a `.` or `..` segment, which a browser resolves and Foundation keeps; write the path it stands for';
const KEPT = ['https://example.com', 'HTTP://example.com/a?b#c', 'https://user:pass@example.com:8080/x', 'http://example.com:8080/',
  'https://[::1]:8080/x', 'https://[2001:DB8::A]', 'https://[::ffff:c000:201]/', 'https://[::]/', 'https://[1:2:3:4:5:6:7:0]/',
  'https://192.168.0.1/', 'https://example.com./', 'https://example.com/a/.b/..c/?x=/../#/./',
  'https://a_b.example/', 'https://example.com/%E2%82%AC?q=%2F', "https://user:pa%3Ass@example.com/it's?q=%27#it's"];
// Each refused URL, its reason, and whether WHATWG URL parses it anyway
const REFUSED = [
  ['/landmark/1016', PATH, false], ['/landmark 1016', PATH, false], ['ftp://example.com', 'its scheme is not `http:` or `https:`', true],
  ['https:example.com', 'it names no host; write `https://` and the host', true],
  ['https://', NO_HOST, false], ['https:///a', NO_HOST, true], ['https://?q', NO_HOST, false], ['https://user@/x', NO_HOST, false],
  ['https://[]/', NO_HOST, false], ['https://[broken', HOST, false], ['https://[::1', HOST, false], ['https://[:::]/', HOST, false],
  ['https://[::ffff:192.0.2.999]/', HOST, false], ['https://[::ffff:192.0.2.1]/', HOST, true], ['https://[1:2:3:4::192.0.2.1]/', HOST, false],
  ['https://[::1:]/', HOST, true], ['https://[1::2::3]/', HOST, false], ['https://[1:2:3:4:5:6:7:8:9]/', HOST, false], ['https://[12345::]/', HOST, false], ['https://[fe80::a%25en1]/', HOST, false], ['https://[example.com]/', HOST, false],
  ['https://[::1]x/', HOST, false], ['https://2001:db8::1/', 'an IPv6 host goes in brackets, as `https://[2001:db8::1]/`', false],
  ['https://example.com:443:444/', PORT, false],
  ['https://exa<mple.com', HOST, false], ['https://a"b.example/', HOST, true], ['https://a{b.example/', HOST, true],
  ['https://exa%41mple.com', HOST, true], ['https://example.123', HOST, false], ['https://1.2.3.999', HOST, false],
  ['https://1.2.3.09/', HOST, false], ['https://1.2.3.010/', HOST, true], ['https://1.2.3.4./', HOST, true], ['https://127.1/', HOST, true],
  ['https://b\u00fccher.example/', ASCII, true], ['https://xn--bcher-kva.example/', ASCII, true], ['https://xn--/', ASCII, false], ['https://a.XN--b.example/', ASCII, false], ['https://example.\uff11\uff12\uff13/', ASCII, false],
  ['http://\uff10\uff38\uff43\uff10\uff0e\uff10\uff12\uff15\uff10\uff0e\uff10\uff11', ASCII, true],
  ['https://example.com:99999', PORT, false], ['https://example.com:8a', PORT, false],
  ['https://exa mple.com', HOST_SPACE, false], ['https://example.com: 80/', PORT, false], ['https://example.com:443: 444/', PORT, false],
  ['https://example.com/\ufeff', PATH_CHARACTER, true], ['https://example.com/caf\u00e9%20menu', PATH_CHARACTER, true], ['https://example.com/a|b', PATH_CHARACTER, true],
  ["https://example.com/?q='", "its query holds a character it must write percent-encoded, as a non-ASCII one or `'` must; write it as UTF-8 `%XX`", true],
  ['https://example.com/#caf\u00e9', 'its fragment holds a character it must write percent-encoded, as a non-ASCII one must; write it as UTF-8 `%XX`', true],
  ['https://user;name@example.com/', USERINFO, true], ['https://user:pass:word@example.com/', USERINFO, true],
  ["https://o'brien@example.com/", USERINFO, true],
  ['https://:pass@example.com/', 'it has a password or a `:` with no user name; give a user name, or remove what is before the `@` and the `@`', true],
  ['https://:@example.com/', 'it has a password or a `:` with no user name; give a user name, or remove what is before the `@` and the `@`', true],
  ['https://example.com:065535/', 'its port has a leading zero, which a browser drops; write the port without it', true],
  ['ht\ttps://example.com/', 'it holds a tab or a line break, which a browser drops; remove it', true],
  ['https://example.com/a\nb', 'it holds a tab or a line break, which a browser drops; remove it', true],
  [`https://example.com/${'a'.repeat(2 ** 20)}`, 'it is too long for every browser to read, 1 MiB or more; shorten it', true],
  [`https://example.com?${'a'.repeat(2 ** 20 - 22)}`, 'it is too long for every browser to read, 1 MiB or more; shorten it', true],
  ['https://a*b.example/a', HOST, true], ['https://a~b.example/a', HOST, true],
  ['https://example.com:443/a', "its port is its scheme's default, which a browser drops; remove it", true],
  ['http://example.com:80/', "its port is its scheme's default, which a browser drops; remove it", true],
  ['https://example.com:0443/', 'its port has a leading zero, which a browser drops; write the port without it', true],
  ['https://example.com:/', 'its port is empty, which a browser drops with its `:`; remove the `:`', true],
  ['https://user:@example.com/a', 'its password is empty, which a browser drops with its `:`; remove the `:`', true],
  ['https://@example.com/a', 'it has an `@` with no user name before it, which a browser drops; remove the `@`', true],
  ['https://[2001:0db8:0:0:0:0:0:1]/a', IPV6, true], ['https://[1:2:3:4:5:6:7::]/', IPV6, true], ['https://[1:0:0:2:0:0:0:3]/', IPV6, true],
  [' https://example.com/a', EDGE, true], ['https://example.com/a ', EDGE, true], ['https://example.com/a{b', PATH_CHARACTER, true],
  ['https://user name@example.com/', 'its user name or password holds whitespace or a control character; encode a space as `%20`', true], ['https://example.com/a b', SPACE, true], ['https://example.com/\u0085a', SPACE, true],
  ['https://example.com/a%zz', ESCAPE, true], ['https://example.com/a/../b', DOTS, true], ['https://example.com/a/%2e%2E/b', DOTS, true],
  ['https://example.com/./a', DOTS, true], ['https://example.com/a/.%2e', DOTS, true], ['https://example.com/100%', ESCAPE, true],
  ['https://example.com/a\\b', BACKSLASH, true], ['https://example.com/#a\\b', BACKSLASH, true],
];

test('a URL is kept as the runner keeps it, and refused with the runner\'s reason', () => {
  for (const url of KEPT) expect([url, httpUrlRefusal(url), URL.canParse(url)]).toEqual([url, null, true]);
  for (const [url, why, parses] of REFUSED) expect([url, httpUrlRefusal(url), URL.canParse(url)]).toEqual([url, why, parses]);
});

// A URL kept is one a browser writes back as it was written, once its scheme and host are lowercased and an empty path is
// `/`, which is what D1's "alike" allows.
test('a URL kept is written back as it was written, but for case and an empty path', () => {
  const written = (url) => {
    const [, scheme, userinfo = '', host, rest] = /^(https?:\/\/)([^@/?#]*@)?([^/?#]*)(.*)$/i.exec(url);
    return scheme.toLowerCase() + userinfo + host.toLowerCase() + (/^[?#]|^$/.test(rest) ? '/' + rest : rest);
  };
  for (const url of KEPT) expect([url, new URL(url).href]).toEqual([url, written(url)]);
});
