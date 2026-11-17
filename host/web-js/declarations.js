// A node's custom properties (LLP 1123 D5), `--a: <length>; --b: <length>`, as one row's value: each written as itself,
// and one the node no longer declares removed. A `;` inside a comment, a string or a function is no boundary.
export function declare(e, t) {
  const now = new Map();
  for (const d of declarations(t ?? '')) {
    const [, written, value] = NAME.exec(d) ?? [];
    const name = written && decoded(written);
    if (name && !name.startsWith('--exact-')) now.set(name, value.trim());
  }
  for (const name of e.$declared ?? []) if (!now.has(name)) e.style.removeProperty(name);
  for (const [name, value] of now) e.style.setProperty(name, value);
  e.$declared = [...now.keys()];
}

// A declaration: a custom property's name, in CSS's grammar with its escapes, then its value; white space and comments
// may stand around the name.
const NAME = /^(?:\s|\/\*[\s\S]*?\*\/)*(--(?:[\w-]|[^\x00-\x7f]|\\(?:[0-9a-fA-F]{1,6}\s?|[^\n0-9a-fA-F]))+)(?:\s|\/\*[\s\S]*?\*\/)*:([\s\S]*)$/;

// A name with its escapes decoded, as the CSSOM names the property: a code point CSS has none of (zero, a surrogate,
// past U+10FFFF) is U+FFFD, as CSS Syntax 3 §4.3.7 decodes it.
const decoded = name => name.replace(/\\(?:([0-9a-fA-F]{1,6})\s?|([^\n]))/g, (_, hex, c) => {
  if (!hex) return c;
  const n = parseInt(hex, 16);
  return String.fromCodePoint(n === 0 || (n >= 0xd800 && n <= 0xdfff) || n > 0x10ffff ? 0xfffd : n);
});

// The declarations of `t`, split at each top-level `;`.
function declarations(t) {
  const out = [];
  let at = 0, depth = 0, quote = null;
  for (let i = 0; i < t.length; i++) {
    const c = t[i];
    if (c === '\\') i++;
    else if (quote) quote = c === quote ? null : quote;
    else if (c === '/' && t[i + 1] === '*') {
      const end = t.indexOf('*/', i + 2);
      i = end < 0 ? t.length : end + 1;
    } else if (c === '"' || c === "'") quote = c;
    else if (c === '(') depth++;
    else if (c === ')') depth = Math.max(0, depth - 1);
    else if (c === ';' && depth === 0) {
      out.push(t.slice(at, i));
      at = i + 1;
    }
  }
  out.push(t.slice(at));
  return out;
}
