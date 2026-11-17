// Logical copy uses live paragraphs for mounted rows and compiled, read-only
// expressions for unmounted rows. Projecting a row never mounts its views.
import { scope, end, untracked } from './rt.js';

const controls = new Set(['button', 'link', 'tab', 'menuitem', 'menuitemcheckbox', 'menuitemradio', 'option', 'checkbox', 'radio', 'switch', 'treeitem']);
function state(parent, props) {
  const next = { ...parent };
  if (props.control || controls.has(props.role)) next.select = false;
  if (props.select && props.select !== 'auto') next.select = props.select !== 'none';
  if (props.visibility) next.hidden = props.visibility === 'hidden';
  next.inert ||= !!props.inert;
  return next;
}
function domState(el) {
  if (!el) return { select: true, hidden: false, inert: false };
  const css = getComputedStyle(el);
  return state(domState(el.parentElement), { control: el.matches('button,input,textarea,select'), role: el.getAttribute('role'),
    select: css.userSelect, visibility: css.visibility, inert: el.hasAttribute('inert') && !el.$collectionFixed });
}
// Match logical projection's authored text, retaining mounted component state.
function authoredText(node) {
  if (node.nodeType === 3) return node.data;
  if (node.nodeType !== 1 || getComputedStyle(node).display === 'none') return '';
  if (node.$source !== undefined) return node.$source;
  if (node.localName === 'img') return node.getAttribute('alt') ?? node.getAttribute('aria-label') ?? '';
  return [...node.childNodes].map(authoredText).join('');
}
function domParagraphs(el) {
  const out = [];
  for (let p = el.parentElement; p; p = p.parentElement) if (getComputedStyle(p).display === 'none') return out;
  function visit(p) {
    if (p.$list || getComputedStyle(p).display === 'none') return;
    if (p.matches('[data-exact-text]')) {
      const s = domState(p);
      out.push([authoredText(p), s.select && !s.hidden && !s.inert]);
      return;
    }
    for (const child of p.children) visit(child);
  }
  visit(el);
  return out;
}
function projected(nodes, inherited, out, inline = false) {
  for (const [kind, props, children] of nodes) {
    if (props.display === 'none') continue;
    const s = state(inherited, props);
    if (kind === 1 && !inline) out.push(['', s.select && !s.hidden && !s.inert]);
    if ((kind === 1 || kind === 2 && inline) && out.length) out.at(-1)[0] += props.text ?? '';
    projected(children, s, out, inline || kind === 1);
  }
}
function rowText(c, position) {
  const mounted = c.mounted.find(m => m.position === position);
  if (mounted) return domParagraphs(mounted.wrapper);
  const out = [];
  const temporary = scope(() => untracked(() => projected(c.o.text(() => c.items[position], () => position), domState(c.el), out)), null);
  end(temporary);
  return out;
}
export function listText(c, a, b) {
  let lo = [0, 0, 0], hi = [c.index.len - 1, Infinity, Infinity], loAt = null, hiAt = null;
  if (a) {
    lo = [c.index.pos.get(a.key), a.paragraph, a.offset];
    hi = [c.index.pos.get(b.key), b.paragraph, b.offset];
    if (lo[0] == null || hi[0] == null) return '';
    [loAt, hiAt] = compare(lo, hi) > 0 ? [b, a] : [a, b];
    if (compare(lo, hi) > 0) [lo, hi] = [hi, lo];
  }
  const out = [], append = paragraphs => {
    for (const [text, selectable] of paragraphs) if (selectable && text) out.push(text);
  };
  function fixed(trail) {
    if (!c.fixed) return;
    let after = false;
    for (const el of c.el.childNodes) {
      if (el === c.fixed.a) { if (!trail) break; }
      if (el === c.fixed.b) { after = true; continue; }
      if (el.nodeType === 1 && (trail ? after : true)) append(domParagraphs(el));
    }
  }
  if (!a) fixed(false);
  for (let row = lo[0]; row <= hi[0]; row++) {
    for (const [paragraph, [text, selectable]] of rowText(c, row).entries()) {
      if (!selectable || compare([row, paragraph], lo.slice(0, 2)) < 0 || compare([row, paragraph], hi.slice(0, 2)) > 0) continue;
      // An endpoint's offset counts the rendered text captured with it: a
      // paragraph the selection cuts copies what the reader saw, and a whole
      // one copies its authored source.
      const first = row === lo[0] && paragraph === lo[1], last = row === hi[0] && paragraph === hi[1];
      const rendered = (first ? loAt : last ? hiAt : null)?.rendered;
      let part = text;
      if (rendered) {
        const start = first ? lo[2] : 0, end = last ? Math.min(hi[2], rendered.text.length) : rendered.text.length;
        if (start > 0 || end < rendered.text.length) part = clip(rendered, start, end);
      } else if (first || last) {
        part = text.slice(first ? lo[2] : 0, last ? hi[2] : text.length);
      }
      if (part) out.push(part);
    }
  }
  if (!a) fixed(true);
  return out.join('\n\n');
}
function clip({ text, hidden }, start, end) {
  let out = '', at = start;
  for (const [a, b] of hidden) {
    if (b <= at || a >= end) continue;
    out += text.slice(at, Math.max(at, a)); at = Math.min(end, b);
  }
  return out + text.slice(at, end);
}
function compare(a, b) { for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return a[i] - b[i]; return 0; }
