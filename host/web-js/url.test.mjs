// The page's URL rule against the runner's: `plan/src/http_url.rs`'s cases, kept and refused with the same reasons.
import { test, expect } from 'bun:test';
import { httpUrlRefusal } from './url.js';

const NO_HOST = 'it names no host; write the host after `https://`';
const EDGE = 'it begins or ends with a space or a control character; remove it';
const INSIDE = 'it holds a space, a tab, a line break or another control character; write a space as `%20` and remove the rest';
const KEPT = ['https://example.com', 'HTTP://example.com/a?b#c', 'https://user:pass@example.com:8080/x', 'https://[::1]:8080/x',
  "https://example.com/café?q='#\u0085", 'https://example.com:99999', 'https://[broken', 'https://@example.com/'];
const REFUSED = [
  ['/landmark/1016', "another app cannot open this app's path"], ['/landmark 1016', "another app cannot open this app's path"],
  ['ftp://example.com', 'its scheme is not `http:` or `https:`'], ['https:example.com', NO_HOST], ['https://', NO_HOST],
  ['https:///a', NO_HOST], ['https://?q', NO_HOST], ['https://#a', NO_HOST], ['https://:443/', NO_HOST], ['https://user@/x', NO_HOST], ['https://[]/', NO_HOST], [' https://example.com/a', EDGE],
  ['https://example.com/a\n', EDGE], ['https://example.com/a b', INSIDE], ['https://example.com/a\r\nb', INSIDE],
  ['https://example.com/\u007f', EDGE], ['https://exa\u007fmple.com/', INSIDE],
];

test("a URL is kept as the runner keeps it, and refused with the runner's reason", () => {
  for (const url of KEPT) expect([url, httpUrlRefusal(url)]).toEqual([url, null]);
  for (const [url, why] of REFUSED) expect([url, httpUrlRefusal(url)]).toEqual([url, why]);
});
