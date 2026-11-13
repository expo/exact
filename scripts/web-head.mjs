// app.json's `host.web.head`: the tags the web builds write into every document's `<head>` (LLP 1048.000 D3).
import { posix } from 'node:path';
import { webRequestURL } from './origin.mjs';

// Exact's own files at the site's root: the JS target's entry, the wasm target's glue, its document glue, its
// navigation module, audio session entry and wasm. An entry may neither load nor preload one again.
const OWN_FILES = new Set(['app.js', 'glue.js', 'document-glue.js', 'navigation.js', 'audio-session.js', 'app.wasm']);
// Whether a URL names one: resolved as the browser resolves it against the shells' `<base href="/">` at the app's
// origin (`app.origin`, else a stand-in no other URL names), so a relative URL, a protocol-relative one and one that
// names the origin whole are all the site's own; then as host/web/serve.mjs `treeFile` answers the path the page
// sends: one its `webRequestURL` takes, decoded and resolved as a file's path (`%2f` a separator, a repeated or
// trailing one dropped, dot segments resolved). Its case is kept, since a host may tell `App.js` from `app.js`.
const STAND_IN = 'https://assets.invalid/', SERVED = new URL('http://exact.invalid').origin;
const ownFile = (url, origin) => {
  const base = URL.canParse('/', origin ?? '') ? new URL('/', origin) : new URL(STAND_IN);
  if (typeof url !== 'string' || !URL.canParse(url, base)) return false;
  const asked = new URL(url, base), served = asked.origin === base.origin && webRequestURL(asked.pathname);
  if (!served || served.origin !== SERVED) return false;
  let path;
  try { path = decodeURIComponent(served.pathname); } catch { return false; }
  return OWN_FILES.has(posix.resolve('/', '.' + path).slice(1));
};
// A `rel`'s tokens: HTML reads it as a set of space-separated, ASCII case-insensitive keywords.
const rels = (a) => typeof a.rel === 'string' ? a.rel.toLowerCase().split(/[\t\n\f\r ]+/).filter(Boolean) : [];
const folded = (value) => typeof value === 'string' ? value.trim().toLowerCase() : value;
/** What Exact writes into a page's head itself, or reads there, which `host.web.head` may not write again, since a
 * second one conflicts with it or is read in its place: each a test of the entry's attributes (names in lower case,
 * `true` as ''), with where Exact's comes from. Values are folded to lower case where HTML matches them so (`rel`, a
 * MIME type, a meta's `name` or `property`), and a URL's are not. */
const OWN_HEAD = [
  ['meta', (a) => a.charset !== undefined || folded(a.name) === 'viewport', 'the web build'],
  ['meta', (a) => /^(description|robots|twitter:.*)$/.test(folded(a.name)) || /^og:/.test(folded(a.property)), 'the Contract `head` element'],
  ['meta', (a) => folded(a.name) === 'theme-color', "app.json's theme_color"],
  // Which the renderers read as the page's fixed scheme (host/web-js/render.mjs `fixedScheme`).
  ['meta', (a) => folded(a.name) === 'color-scheme', "app.json's userInterfaceStyle"],
  // `exact-native` (the page's native modules) and `exact-dev-generation` (the dev loop's build), which the runtime reads.
  ['meta', (a) => folded(a.name)?.startsWith('exact-'), 'the web build'],
  ['link', (a) => rels(a).includes('canonical'), 'the Contract `head` element'],
  ['link', (a) => rels(a).some((rel) => rel === 'icon' || rel === 'manifest'), "app.json's icons and the web build's manifest.json"],
  ['link', (a, origin) => rels(a).some((rel) => rel === 'preload' || rel === 'modulepreload') && ownFile(a.href, origin), 'the web build'],
  ['script', (a, origin) => ownFile(a.src, origin), 'the web build, which loads it once'],
  // `application/vnd.exact.checkpoint` and the other types of Exact's own data, which the runtime reads from the first
  // such element in the document (the renderers' checkpoint; the wasm target's `alternate` envelope link).
  ['script', (a) => folded(a.type)?.startsWith('application/vnd.exact.'), 'the renderers'],
  ['link', (a) => folded(a.type)?.startsWith('application/vnd.exact.'), 'the web build'],
];
// In a script's text HTML's tokenizer ends the element at `</script` followed by whitespace, `/` or `>`, and leaves
// plain script data at `<!--`, the one way into the escaped states, where a later `<script` keeps the real end tag from
// ending it (HTML, 13.2.5 "Tokenization": the script data, script data less-than sign, script data escape start and
// script data double escape start states). Nothing else there is a tag. A JSON text is written with `\u003C` for that
// `<`, which JSON reads as `<`. A JavaScript text is never rewritten: `\u003C` outside a string is a syntax error, and
// telling the one from the other takes a JavaScript parser, so such a text is refused, and its author writes it so the
// page's parser cannot see it.
const SCRIPT_DATA_TAG = /<(?=!--|\/script[\t\n\f\r />])/gi;
function scriptTextProblems(at, text) {
  const found = new Set([...text.matchAll(/<(!--|\/script)[\t\n\f\r />]?/gi)].filter((m) => m[1] === '!--' || /[\t\n\f\r />]/.test(m[0].at(-1)))
    .map((m) => m[1].toLowerCase()));
  return [...found].map((seen) => `${at}: holds "<${seen}", which ${seen === '!--' ? "puts the page's HTML parser in a state where the script's end tag may not end it" : "ends the script element there"}; write it so the parser cannot see it: "\\x3C${seen}" or '<' + '${seen}' inside a string, or a space after "<" in code`);
}
/** Why each of `host.web.head`'s entries cannot be written, naming the entry (LLP 1048.000 D3): what `readManifest`
 * says in place of the schema's, whose alternatives say only that an entry fits none. An attribute's name is an XML
 * name in ASCII, as `setAttribute` takes and the HTML tokenizer reads whole; one name twice, in any case, is refused. */
export function webHeadProblems(manifest) {
  const head = manifest.host?.web?.head;
  if (head === undefined) return [];
  if (!Array.isArray(head)) return ['host.web.head: expected a list of tags'];
  return head.flatMap((entry, i) => {
    const at = `host.web.head[${i}]`, keys = entry && typeof entry === 'object' && !Array.isArray(entry) ? Object.keys(entry) : null;
    if (keys?.length !== 1) return [`${at}: ${JSON.stringify(entry)} is not one tag; an entry is {"script": {...}}, {"link": {...}} or {"meta": {...}}`];
    const [tag] = keys, attrs = entry[tag];
    if (!['script', 'link', 'meta'].includes(tag)) return [`${at}: ${tag} is not a tag the head takes (script, link, meta)`];
    if (!attrs || typeof attrs !== 'object' || Array.isArray(attrs)) return [`${at}.${tag}: expected an object of attributes`];
    const said = Object.create(null);
    const problems = Object.entries(attrs).flatMap(([name, value]) => {
      if (name === 'text') return tag !== 'script' ? [`${at}.${tag}.text: a ${tag} has no content`] : typeof value !== 'string' && (!value || typeof value !== 'object') ? [`${at}.${tag}.text: expected a string, or JSON as an object or list`]
        : typeof value === 'string' ? scriptTextProblems(`${at}.${tag}.text`, value) : [];
      if (!/^[A-Za-z_:][A-Za-z0-9_:.-]*$/.test(name)) return [`${at}.${tag}: ${JSON.stringify(name)} is not an attribute name`];
      const lower = name.toLowerCase();
      if (Object.hasOwn(said, lower)) return [`${at}.${tag}: ${JSON.stringify(name)} is the attribute ${JSON.stringify(lower)} again`];
      said[lower] = typeof value === 'string' ? value : value === true ? '' : undefined;
      return typeof value !== 'string' && value !== true ? [`${at}.${tag}.${name}: ${JSON.stringify(value)}; an attribute is a string, or true for a bare one such as async`] : [];
    });
    for (const [own, writes, from] of OWN_HEAD) if (own === tag && writes(said, manifest.app?.origin)) problems.push(`${at}: Exact writes this ${tag} itself, from ${from}`);
    return problems;
  });
}
/** `host.web.head`'s entries as HTML, one line each: what the web builds write into the shell after its sheet, so
 * every document composed over the shell carries them (LLP 1048.000 D3). A script's `text` that is an object or list
 * is written as JSON (`application/ld+json`, `importmap`, `speculationrules`), with `SCRIPT_DATA_TAG`'s `<` as
 * `\u003C`; one that is a string is written as it is, `scriptTextProblems` having refused what HTML would read there.
 * Throws on an entry `webHeadProblems` refuses. */
export function webHead(manifest) {
  const problems = webHeadProblems(manifest);
  if (problems.length) throw new Error(`app.json:\n  ${problems.join('\n  ')}`);
  const escape = (value) => value.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
  return (manifest.host?.web?.head ?? []).map((entry) => {
    const [[tag, { text, ...attrs }]] = Object.entries(entry);
    const open = `<${tag}${Object.entries(attrs).map(([name, value]) => value === true ? ` ${name}` : ` ${name}="${escape(value)}"`).join('')}>`;
    return tag === 'script' ? `${open}${typeof text === 'object' ? JSON.stringify(text).replace(SCRIPT_DATA_TAG, '\\u003C') : text ?? ''}</script>\n` : `${open}\n`;
  }).join('');
}

/** After the root, before the runtime: a classic script also precedes a capture script's deferred import. */
export function webAudioSession(manifest) {
  return manifest.audio_session ? '<script src="./audio-session.js"></script>\n' : '';
}

// A directive's name and value, as CSP3 §2.3 writes them: a name of letters, digits and `-` (lower case here, so a
// key names one directive), and a value of visible ASCII but `,` (which starts a second policy) and `;` (a second
// directive), with spaces and tabs between its sources.
const DIRECTIVE_NAME = /^[a-z0-9-]+$/, DIRECTIVE_VALUE = /^[\t \x21-\x2B\x2D-\x3A\x3C-\x7E]*$/, SOURCE = /^[\x21-\x2B\x2D-\x3A\x3C-\x7E]+$/;
const CSP_KEYS = ['csp', 'cspReportOnly'];
/** Why `host.web.csp` or `host.web.cspReportOnly` cannot be sent, naming the entry (LLP 1048.000 D11). The browser
 * reads each value; the build refuses only what would not be one directive of one policy. */
export function webCspProblems(manifest) {
  return CSP_KEYS.flatMap((key) => {
    const policy = manifest.host?.web?.[key], at = `host.web.${key}`;
    if (policy === undefined || policy === false) return [];
    if (!policy || typeof policy !== 'object' || Array.isArray(policy)) return [`${at}: expected false or an object of directives`];
    return Object.entries(policy).flatMap(([name, value]) => {
      if (!DIRECTIVE_NAME.test(name)) return [`${at}: ${JSON.stringify(name)} is not a directive name in lower case (letters, digits and "-")`];
      if (typeof value === 'string') return DIRECTIVE_VALUE.test(value) ? [] : [`${at}.${name}: ${JSON.stringify(value)} holds a comma, a semicolon, a line break or a character outside ASCII`];
      if (!Array.isArray(value)) return [`${at}.${name}: expected a list of sources to add, or a string that replaces Exact's`];
      return value.flatMap((source, i) => typeof source === 'string' && SOURCE.test(source) ? []
        : [`${at}.${name}[${i}]: ${JSON.stringify(source)} is not one source (no spaces, commas or semicolons)`]);
    });
  });
}
/** What the render server reads at start (`.gen/csp.json`): the app's part of each policy, or `null` when it says
 * none. Throws on a policy `webCspProblems` refuses. */
export function webCspFile(manifest) {
  const problems = webCspProblems(manifest);
  if (problems.length) throw new Error(`app.json:\n  ${problems.join('\n  ')}`);
  const said = Object.fromEntries(CSP_KEYS.filter((key) => manifest.host?.web?.[key] !== undefined).map((key) => [key, manifest.host.web[key]]));
  return Object.keys(said).length ? JSON.stringify(said) + '\n' : null;
}
