// The page's port of `exact_plan::http_url_refusal` (LLP 1094.000 D1), which a drag's `url` (drag.js) and a share's
// (files.js) are admitted by, as the runner admits them on every other host: the same inputs refused, each with the
// same reason. It checks that the author meant a web link and that nothing breaks how a host frames it; whether the
// link opens is the receiving app's to decide.

const BLANK = /[\x00-\x20\x7f]/;
const NO_HOST = 'it names no host; write the host after `https://`';

/** Why `url` is no web link a drag or a share carries, or null: `http://` or `https://` in any case, a host after it,
 * and no space or ASCII control character. */
export function httpUrlRefusal(url) {
  if (BLANK.test(url.at(0) ?? '') || BLANK.test(url.at(-1) ?? '')) return 'it begins or ends with a space or a control character; remove it';
  const scheme = /^https?:\/\//i.exec(url);
  if (!scheme) {
    if (/^https?:/i.test(url)) return NO_HOST;
    return /^[a-z][a-z0-9+.-]*:/i.test(url) ? 'its scheme is not `http:` or `https:`' : "another app cannot open this app's path";
  }
  // The host, read only far enough to see that there is one: after any user information, before any port, inside
  // brackets for an IPv6 address.
  const rest = url.slice(scheme[0].length), authority = rest.slice(0, rest.search(/[/?#]|$/));
  const hostPort = authority.slice(authority.lastIndexOf('@') + 1);
  if (!(hostPort.startsWith('[') ? hostPort.slice(1).split(']')[0] : hostPort.split(':')[0])) return NO_HOST;
  if (BLANK.test(url)) return 'it holds a space, a tab, a line break or another control character; write a space as `%20` and remove the rest';
  return null;
}
