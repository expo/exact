// app.json's `host.web.head`: the tags the web builds write into every document's `<head>` (LLP 1048.000 D3).

/** What Exact writes into a page's head itself, which `host.web.head` may not write again, since a second one
 * conflicts with it: `<meta>` by its `charset`, `name` or `property`, and `<link>` by its `rel`, each with where it
 * comes from. */
const OWN_HEAD = [
  ['meta', 'charset', /^/, 'the web build'], ['meta', 'name', /^viewport$/, 'the web build'],
  ['meta', 'name', /^(description|robots|twitter:.*)$/, 'the Contract `head` element'], ['meta', 'property', /^og:/, 'the Contract `head` element'],
  ['link', 'rel', /^canonical$/, 'the Contract `head` element'], ['meta', 'name', /^theme-color$/, "app.json's theme_color"],
  ['link', 'rel', /^(icon|manifest)$/, "app.json's icons and the web build's manifest.json"],
];
/** Why each of `host.web.head`'s entries cannot be written, naming the entry (LLP 1048.000 D3): what `readManifest`
 * says in place of the schema's, whose alternatives say only that an entry fits none. */
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
    const problems = Object.entries(attrs).flatMap(([name, value]) =>
      name === 'text' ? (tag !== 'script' ? [`${at}.${tag}.text: a ${tag} has no content`] : typeof value !== 'string' && (!value || typeof value !== 'object') ? [`${at}.${tag}.text: expected a string, or JSON as an object or list`] : [])
      : !/^[^\s"'>/=\x00-\x1f\x7f]+$/.test(name) ? [`${at}.${tag}: ${JSON.stringify(name)} is not an attribute name`]
      : typeof value !== 'string' && value !== true ? [`${at}.${tag}.${name}: ${JSON.stringify(value)}; an attribute is a string, or true for a bare one such as async`] : []);
    for (const [own, key, value, from] of OWN_HEAD) {
      const said = typeof attrs[key] === 'string' ? attrs[key].toLowerCase() : attrs[key] === true ? '' : null;
      if (own === tag && said !== null && value.test(said)) problems.push(`${at}: Exact writes this ${tag} itself, from ${from}`);
    }
    return problems;
  });
}
/** `host.web.head`'s entries as HTML, one line each: what the web builds write into the shell after its sheet, so
 * every document composed over the shell carries them (LLP 1048.000 D3). A script's `text` that is an object or list
 * is written as JSON (`application/ld+json`, `importmap`, `speculationrules`). Its `<!--`, `<script` and `</script`
 * are written as `\u003C…`, which a JavaScript or JSON literal reads alike, so it cannot end its element early (HTML,
 * "Restrictions for contents of script elements"). Throws on an entry `webHeadProblems` refuses. */
export function webHead(manifest) {
  const problems = webHeadProblems(manifest);
  if (problems.length) throw new Error(`app.json:\n  ${problems.join('\n  ')}`);
  const escape = (value) => value.replace(/&/g, '&amp;').replace(/"/g, '&quot;').replace(/</g, '&lt;');
  return (manifest.host?.web?.head ?? []).map((entry) => {
    const [[tag, { text, ...attrs }]] = Object.entries(entry);
    const open = `<${tag}${Object.entries(attrs).map(([name, value]) => value === true ? ` ${name}` : ` ${name}="${escape(value)}"`).join('')}>`;
    return tag === 'script' ? `${open}${(typeof text === 'object' ? JSON.stringify(text) : text ?? '').replace(/<(?=!--|\/?script)/gi, '\\u003C')}</script>\n` : `${open}\n`;
  }).join('');
}
