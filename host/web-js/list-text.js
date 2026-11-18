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
// The fixed lead or trail's paragraphs, counted across its children
// (LLP 1070 §14). Fixed content is always mounted.
function fixedText(c, trail) {
  const out = [];
  if (!c.fixed) return out;
  let side = 0;
  for (const el of c.el.childNodes) {
    if (el === c.fixed.a) side = -1;
    else if (el === c.fixed.b) side = 1;
    else if (el.nodeType === 1 && side === (trail ? 1 : 0)) out.push(...domParagraphs(el));
  }
  return out;
}
// Endpoints, from list-selection.js, name a keyed row or the fixed lead or
// trail (`fixed`), as the runner's `ListTextPosition` does: the lead is
// ordered before row 0 and the trail after the last row. Offsets slice the
// authored text, as the runner's do.
export function listText(c, a, b) {
  const len = c.index.len, ordinal = p => p.fixed == null ? c.index.pos.get(p.key) : p.fixed ? len : -1;
  let lo = [-1, 0, 0], hi = [len, Infinity, Infinity];
  if (a) {
    lo = [ordinal(a), a.paragraph, a.offset];
    hi = [ordinal(b), b.paragraph, b.offset];
    if (lo[0] == null || hi[0] == null) return '';
    if (compare(lo, hi) > 0) [lo, hi] = [hi, lo];
  }
  const out = [];
  for (let row = lo[0]; row <= hi[0]; row++) {
    const paragraphs = row < 0 || row === len ? fixedText(c, row === len) : rowText(c, row);
    for (const [paragraph, [text, selectable]] of paragraphs.entries()) {
      if (!selectable || compare([row, paragraph], lo.slice(0, 2)) < 0 || compare([row, paragraph], hi.slice(0, 2)) > 0) continue;
      const first = row === lo[0] && paragraph === lo[1], last = row === hi[0] && paragraph === hi[1];
      const part = text.slice(first ? lo[2] : 0, last ? hi[2] : text.length);
      if (part) out.push(part);
    }
  }
  return out.join('\n\n');
}
function compare(a, b) { for (let i = 0; i < a.length; i++) if (a[i] !== b[i]) return a[i] - b[i]; return 0; }
