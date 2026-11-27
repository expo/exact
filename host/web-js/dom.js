// The DOM the JS runtime needs, and no more, for rendering a page under
// Bun (`render.mjs`): elements, text, comments and fragments in a tree,
// attributes in insertion order, an inline style map, and HTML serialization
// by the rules `host/web/src/document.rs` writes (escaping, void elements,
// boolean attributes). Region anchors (comments) are not written: adoption
// inserts its own.
import { symbolPlaceholder, symbolSize } from '../web/symbol-geometry.js';

const VOID = new Set(['img', 'input', 'br', 'hr', 'meta', 'link', 'source', 'area', 'col', 'embed', 'wbr']);
// Text, or a quoted attribute value, as document.rs `escape` writes it: an attribute value's `<` and `>` are references
// too, as in HTML's serializer. A NUL has no spelling the parser keeps, so a render with one is refused.
export const esc = (s, quoted) => String(s).replace(quoted ? /[&"<>\r\0]/g : /[&<>\r\0]/g, c => {
  if (c === '\0') throw new Error('a NUL character, which the HTML parser does not keep');
  return { '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', '\r': '&#13;' }[c];
});
// An attribute as document.rs `attribute` writes it: nothing for `null`, and an empty value bare (the parser reads `name`
// as `name=""`).
const attr = (name, value) => (value === null ? '' : value === '' ? ` ${name}` : ` ${name}="${esc(value, true)}"`);

let Trees = 0;
class Node {
  constructor(type) { this.nodeType = type; this.parentNode = null; this.childNodes = []; }
  cloneNode(deep = false) {
    let copy;
    if (this.nodeType === 1) {
      copy = new Element(this.localName, this.fonts);
      copy.attrs = new Map(this.attrs);
      copy.style.cssText = this.style.cssText;
    } else if (this.nodeType === 3) copy = new Text(this.data);
    else if (this.nodeType === 8) copy = new Comment();
    else if (this.nodeType === 11) copy = new Fragment();
    else copy = new Node(this.nodeType);
    if (deep) copy.append(...this.childNodes.map(c => c.cloneNode(true)));
    return copy;
  }
  get parentElement() { return this.parentNode?.nodeType === 1 ? this.parentNode : null; }
  get firstChild() { return this.childNodes[0] ?? null; }
  get nextSibling() { const s = this.parentNode?.childNodes; return s ? s[s.indexOf(this) + 1] ?? null : null; }
  // rt.js `each` empties a region that is all its parent holds by these: without them every sibling went too.
  get previousSibling() { const s = this.parentNode?.childNodes; return s ? s[s.indexOf(this) - 1] ?? null : null; }
  get lastChild() { return this.childNodes.at(-1) ?? null; }
  get isConnected() { let n = this; while (n.parentNode) n = n.parentNode; return n.nodeType === 9; }
  // DOM §4.4 (midi.js orders listeners by it): PRECEDING 2, FOLLOWING 4, CONTAINS 8, CONTAINED_BY 16; a node in
  // another tree is DISCONNECTED 1 | IMPLEMENTATION_SPECIFIC 32 with a direction that stays put: the trees' roots by when first compared.
  compareDocumentPosition(other) {
    if (other === this) return 0;
    const path = n => { const p = []; for (; n; n = n.parentNode) p.unshift(n); return p; };
    const a = path(this), b = path(other);
    if (a[0] !== b[0]) return 1 | 32 | ((a[0].$tree ??= ++Trees) < (b[0].$tree ??= ++Trees) ? 4 : 2);
    let i = 0; while (i < a.length && i < b.length && a[i] === b[i]) i++;
    if (i === a.length) return 16 | 4;
    if (i === b.length) return 8 | 2;
    const kids = a[i - 1].childNodes;
    return kids.indexOf(b[i]) > kids.indexOf(a[i]) ? 4 : 2;
  }
  append(...nodes) { for (const n of nodes) this.insertBefore(typeof n === 'string' ? new Text(n) : n, null); }
  // The nodes as one fragment, then before the first child (rt.js `each`'s row fragments).
  prepend(...nodes) { const f = new Fragment(); f.append(...nodes); this.insertBefore(f, this.firstChild); }
  insertBefore(n, ref) {
    const list = n.nodeType === 11 ? n.childNodes.splice(0) : [n];
    for (const x of list) { x.parentNode?.removeChild(x); x.parentNode = this; }
    const at = ref ? this.childNodes.indexOf(ref) : -1;
    this.childNodes.splice(at < 0 ? this.childNodes.length : at, 0, ...list);
    return n;
  }
  removeChild(n) { const i = this.childNodes.indexOf(n); if (i >= 0) this.childNodes.splice(i, 1); n.parentNode = null; return n; }
  remove() { this.parentNode?.removeChild(this); }
  appendChild(n) { this.insertBefore(n, null); return n; }
  replaceChildren(...nodes) { for (const c of this.childNodes) c.parentNode = null; this.childNodes = []; this.append(...nodes); }
  before(...nodes) { for (const n of nodes) this.parentNode.insertBefore(n, this); }
  after(...nodes) { const next = this.nextSibling; for (const n of nodes) this.parentNode.insertBefore(n, next); }
  get textContent() { return this.childNodes.map(c => c.textContent).join(''); }
  set textContent(v) { this.replaceChildren(...(v === '' ? [] : [new Text(v)])); }
  addEventListener() {} removeEventListener() {} dispatchEvent() {}
}
class Text extends Node { constructor(t) { super(3); this.data = String(t); } get textContent() { return this.data; } html() { return esc(this.data); } }
class Comment extends Node { constructor() { super(8); } get textContent() { return ''; } html() { return ''; } }
class Fragment extends Node { constructor() { super(11); } }
// CSS Animations 1 §4.2: extract the `animation-name` list from an `animation` shorthand.
const ANIM_TIMING = new Set(['ease', 'linear', 'ease-in', 'ease-out', 'ease-in-out', 'step-start', 'step-end']);
const ANIM_DIR = new Set(['normal', 'reverse', 'alternate', 'alternate-reverse']);
const ANIM_FILL = new Set(['forwards', 'backwards', 'both']);
const ANIM_PLAY = new Set(['running', 'paused']);
const ANIM_WIDE = new Set(['initial', 'inherit', 'unset', 'revert', 'revert-layer']);
const ANIM_TIME = /^[+-]?(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?m?s$/i;
const ANIM_NUM = /^(?:\d+\.?\d*|\.\d+)(?:e[+-]?\d+)?$/i;
function animationNames(raw) {
  const text = raw.replace(/\/\*[\s\S]*?\*\//g, '').trim();
  if (!text) return '';
  if (ANIM_WIDE.has(text.toLowerCase())) return text.toLowerCase();
  const layers = [];
  let depth = 0, quote = '', start = 0;
  for (let i = 0; i < text.length; i++) {
    const c = text[i];
    if (quote) { if (c === '\\') i++; else if (c === quote) quote = ''; continue; }
    if (c === '"' || c === "'") { quote = c; continue; }
    if (c === '(') depth++;
    else if (c === ')') { if (--depth < 0) return ''; }
    else if (c === ',' && depth === 0) { layers.push(text.slice(start, i)); start = i + 1; }
  }
  if (depth !== 0 || quote) return '';
  layers.push(text.slice(start));
  const names = [];
  for (const layer of layers) {
    const tokens = [];
    let t0 = -1;
    for (let i = 0; i <= layer.length; i++) {
      const c = layer[i];
      if (quote) { if (c === '\\') i++; else if (c === quote) quote = ''; continue; }
      if (c === '"' || c === "'") { if (t0 < 0) t0 = i; quote = c; continue; }
      if (c === '(') { if (t0 < 0) t0 = i; depth++; continue; }
      if (c === ')') { depth--; continue; }
      if ((c === undefined || /\s/.test(c)) && depth === 0) {
        if (t0 >= 0) { tokens.push(layer.slice(t0, i)); t0 = -1; }
      } else if (t0 < 0) t0 = i;
    }
    if (!tokens.length) return '';
    let timing = false, iter = false, dir = false, fill = '', play = false, times = 0, name = null;
    for (const token of tokens) {
      const lower = token.toLowerCase();
      if (lower.startsWith('var(')) return '';
      if (token[0] === '"' || token[0] === "'") { if (name !== null) return ''; name = token; continue; }
      if (token.includes('(')) {
        if (lower.startsWith('cubic-bezier(') || lower.startsWith('steps(') || lower.startsWith('linear(')) {
          if (timing) return '';
          timing = true;
          continue;
        }
        return '';
      }
      if (ANIM_TIME.test(token)) {
        if ((times === 0 && token[0] === '-') || ++times > 2) return '';
        continue;
      }
      if (!iter && (lower === 'infinite' || ANIM_NUM.test(token))) { iter = true; continue; }
      if (!timing && ANIM_TIMING.has(lower)) { timing = true; continue; }
      if (!dir && ANIM_DIR.has(lower)) { dir = true; continue; }
      if (lower === 'none') {
        if (!fill) { fill = 'none'; continue; }
        if (name === null) { name = 'none'; continue; }
        return '';
      }
      if (ANIM_FILL.has(lower)) {
        if (!fill) { fill = 'explicit'; continue; }
        if (fill === 'none' && name === null) { fill = 'explicit'; name = 'none'; continue; }
      }
      if (!play && ANIM_PLAY.has(lower)) { play = true; continue; }
      if (name !== null) return '';
      name = token;
    }
    names.push(name ?? 'none');
  }
  return names.join(', ');
}
class Style {
  constructor() { this.map = new Map(); }
  setProperty(k, v) { if (v !== '' && v != null) this.map.set(k, String(v)); }
  removeProperty(k) { this.map.delete(k); }
  getPropertyValue(k) { return this.map.get(k) ?? ''; }
  get animation() { return this.getPropertyValue('animation'); }
  set animation(v) {
    this.removeProperty('animation-name');
    if (v === '' || v == null) this.removeProperty('animation');
    else this.setProperty('animation', v);
  }
  get animationName() { return this.getPropertyValue('animation-name') || animationNames(this.getPropertyValue('animation')); }
  set animationName(v) {
    if (v === '' || v == null) this.removeProperty('animation-name');
    else this.setProperty('animation-name', v);
  }
  set fontSize(v) { this.setProperty('font-size', v); } set fontWeight(v) { this.setProperty('font-weight', v); }
  set fontStyle(v) { this.setProperty('font-style', v); } set fontFamily(v) { this.setProperty('font-family', v); }
  set textDecoration(v) { this.setProperty('text-decoration', v); } set opacity(v) { this.setProperty('opacity', v); }
  get isolation() { return this.getPropertyValue("isolation"); }
  set isolation(v) { this.setProperty("isolation", v); }
  get cssText() { return [...this.map].map(([k, v]) => `${k}:${v};`).join(''); }
  set cssText(t) { this.map.clear(); for (const d of t.split(';')) { const i = d.indexOf(':'); if (i > 0) this.map.set(d.slice(0, i).trim(), d.slice(i + 1).trim()); } }
}
class Element extends Node {
  constructor(tag, fonts) { super(1); this.fonts = fonts; this.localName = tag; this.attrs = new Map(); this.style = new Style(); const data = k => 'data-' + k.replace(/[A-Z]/g, c => '-' + c.toLowerCase());
    // Read as written: rt.js reads `dataset.exactOn` and the like back.
    this.dataset = new Proxy({}, { set: (_, k, v) => (this.setAttribute(data(k), v), true), get: (_, k) => typeof k === 'string' ? this.getAttribute(data(k)) ?? undefined : undefined,
      has: (_, k) => typeof k === 'string' && this.hasAttribute(data(k)), deleteProperty: (_, k) => (this.removeAttribute(data(k)), true) }); }
  get id() { return this.getAttribute("id") ?? ""; }
  get attributes() { return [...this.attrs].map(([name, value]) => ({ name, value })); }
  getAttributeNames() { return [...this.attrs.keys()]; }
  get children() { return this.childNodes.filter(c => c.nodeType === 1); }
  get nextElementSibling() { let n = this.nextSibling; while (n && n.nodeType !== 1) n = n.nextSibling; return n; }
  get tagName() { return this.localName.toUpperCase(); }
  // The `style` attribute is the inline style map, which `html` writes.
  setAttribute(k, v) { if (k === 'style') this.style.cssText = String(v); else this.attrs.set(k, String(v)); }
  getAttribute(k) { return k === 'style' ? this.style.cssText || null : this.attrs.get(k) ?? null; }
  hasAttribute(k) { return k === 'style' ? !!this.style.cssText : this.attrs.has(k); }
  removeAttribute(k) { if (k === 'style') this.style.cssText = ''; else this.attrs.delete(k); }
  toggleAttribute(k, on) { if (on) this.attrs.set(k, ''); else this.attrs.delete(k); }
  get childElementCount() { return this.childNodes.filter(c => c.nodeType === 1).length; }
  get firstElementChild() { return this.childNodes.find(c => c.nodeType === 1) ?? null; }
  getElementsByTagName(tag) { return this.childNodes.flatMap(c => c.nodeType === 1 ? [...(tag === '*' || c.localName === tag ? [c] : []), ...c.getElementsByTagName(tag)] : []); }
  querySelectorAll() { return []; }
  querySelector(sel) {
    const m = /^\[data-exact-id="((?:\\.|[^"\\])*)"\]$/.exec(sel);
    if (!m) return null;
    const target = unescapeCssAttr(m[1]);
    return this.getElementsByTagName('*').find(el => el.getAttribute('data-exact-id') === target) ?? null;
  }
  contains() { return false; }
  // A tag and attributes only (`[a]`, `[a=v]`, `[a~=v]`, the value quoted or not), comma-separated: rt.js `on` asks
  // whether a press is native, and navigation.js `markBars` reads a header as `barItems` does.
  matches(sel) {
    return sel.split(',').some(p => {
      const m = /^([\w-]*)((?:\[[\w-]+(?:~?=(?:"[^"]*"|[\w-]+))?\])*)$/.exec(p.trim());
      return !!m && (!m[1] || m[1] === this.localName) && [...m[2].matchAll(/\[([\w-]+)(?:(~?)=(?:"([^"]*)"|([\w-]+)))?\]/g)].every(([, k, word, quoted, bare]) => {
        const v = this.getAttribute(k), want = quoted ?? bare;
        return v !== null && (want === undefined || (word ? v.split(/\s+/).includes(want) : v === want));
      });
    });
  }
  get value() {
    if (this.localName === 'select') {
      const options = this.options;
      return (options.find(o => o.hasAttribute('selected')) ?? options.find(o => !o.hasAttribute('disabled')))?.value ?? '';
    }
    if (this.localName === 'option') return this.getAttribute('value') ?? this.textContent.replace(/[\t\n\f\r ]+/g, ' ').replace(/^ | $/g, '');
    return this.localName === 'textarea' ? this.textContent : this.getAttribute('value') ?? '';
  }
  get options() { return this.childNodes.filter(c => c.localName === 'option'); }
  // A select's value is the option that carries it, `selected` (document.rs).
  set value(v) {
    if (this.localName === 'select') {
      const options = this.options, selected = options.find(o => o.value === String(v));
      for (const o of options) o.toggleAttribute('selected', o === selected);
    } else if (this.localName === 'textarea') this.textContent = v;
    else if (v === '' && this.localName !== 'option') this.removeAttribute('value');
    else this.setAttribute('value', v);
  }
  set checked(v) { this.toggleAttribute('checked', !!v); }
  get checked() { return this.hasAttribute('checked'); }
  set muted(v) {} pause() {} play() { return Promise.resolve(); }
  set className(v) { this.setAttribute('class', v); }
  // Read only (symbols.js `tinted`): the class attribute's names.
  get classList() { const names = (this.getAttribute('class') ?? '').split(/\s+/).filter(Boolean); return Object.assign(names, { contains: c => names.includes(c) }); }
  set href(v) { this.setAttribute('href', v); }
  // Reflected as a browser reflects it, so an icon symbols.js draws from its file is written with the file (element.rs).
  set src(v) { this.setAttribute('src', v); }
  // Reflected as a browser reflects it, so a link renderMarkup sends out of the app is written with it.
  get target() { return this.getAttribute('target') ?? ''; }
  set target(v) { this.setAttribute('target', v); }
  get rel() { return this.getAttribute('rel') ?? ''; }
  set rel(v) { this.setAttribute('rel', v); }
  get alt() { return this.getAttribute('alt') ?? ''; }
  set alt(v) { this.setAttribute('alt', v); }
  html(inheritedFont = 16) {
    // Symbol images need a real natural size before adoption: the element's
    // font size, from its static class or live inline row, else inherited.
    let font = inheritedFont;
    for (const cls of (this.getAttribute('class') ?? '').split(/\s+/)) if (this.fonts?.has(cls)) font = fontPx(this.fonts.get(cls), inheritedFont);
    const ownFont = this.style.getPropertyValue('font-size');
    if (ownFont !== '') font = fontPx(ownFont, inheritedFont);
    const source = this.getAttribute('data-symbol-source');
    // An icon's file keeps its own source (LLP 1121 D10).
    if (this.localName === 'img' && source !== null && !this.hasAttribute('data-symbol-file')) {
      const box = this.getAttribute('data-symbol-viewbox') || '0 0 24 24';
      this.setAttribute('src', symbolPlaceholder(...symbolSize(box, font, source)));
    }
    // A head is the page's <head>, never an element in the root (as document.rs).
    if (this.localName === 'template') return '';
    let out = `<${this.localName}`;
    const style = this.style.cssText;
    for (const [k, v] of this.attrs) out += attr(k, v);
    if (style) out += attr('style', style);
    out += '>';
    if (VOID.has(this.localName)) return out;
    return out + this.childNodes.map(c => c.html(font)).join('') + `</${this.localName}>`;
  }
}

// A `font-size` in px, `em` (of the inherited size) or `rem` (of CSS's
// initial 16: a render has no reader's setting), as the browser resolves it.
const fontPx = (value, inherited) => {
  const m = /^\s*([\d.]+)(px|em|rem)?\s*$/i.exec(value);
  if (!m) return inherited;
  const unit = (m[2] ?? 'px').toLowerCase();
  return Number(m[1]) * (unit === 'em' ? inherited : unit === 'rem' ? 16 : 1);
};

// The hero `<img>`'s priority, which its preload carries too (document_before.rs `HeroImage::PRIORITY`).
const HERO_PRIORITY = 'high';
// The `visibility` values an element can set; any other (`unset`, `inherit`) takes its parent's.
const VISIBILITIES = new Set(['visible', 'hidden', 'collapse']);

// CSS Values 4 §10 + CSSOM §2.1: minimal `CSS` global for SSR (`render.mjs`), matching browser
// `CSS.supports` for the properties the JS runtime validates (`line-height`, `translate`, `scale`)
// and `CSS.escape` for scoped `url(#...)` lookups.
const CSS_NUM = '[+-]?(?:\\d+(?:\\.\\d+)?|\\.\\d+)(?:e[+-]?\\d+)?';
const CSS_LEN_UNIT =
  'px|rem|em|ex|rex|cap|rcap|ch|rch|ic|ric|lh|rlh|[sld]?v[whib]|[sld]?vmin|[sld]?vmax|cm|mm|q|in|pc|pt|cq[whib]|cqmin|cqmax';
const CSS_NUM_RE = new RegExp('^' + CSS_NUM + '$', 'i');
const CSS_PCT_RE = new RegExp('^' + CSS_NUM + '%$', 'i');
const CSS_LEN_RE = new RegExp('^' + CSS_NUM + '(?:' + CSS_LEN_UNIT + ')$', 'i');
const CSS_ZERO_RE = /^[+-]?(?:0+(?:\.0+)?|\.0+)(?:e[+-]?\d+)?$/i;
const CSS_TOKEN_RE = new RegExp(`^(?:(${CSS_NUM})(?:(${CSS_LEN_UNIT})|(%))?|([a-z0-9-]+)\\(|([(),*/+-]))`, 'i');
const CSS_MATH_SAME = new Set(['calc', 'abs', 'min', 'max', 'clamp', 'hypot', 'mod', 'rem', 'round']);
const CSS_MATH_NUM = new Set(['sqrt', 'exp', 'sin', 'cos', 'tan', 'asin', 'acos', 'atan', 'atan2', 'pow', 'log', 'sign']);
const CSS_ROUND_MODES = new Set(['nearest', 'up', 'down', 'to-zero']);
function stripCssComments(text) {
  let out = '', i = 0;
  while (i < text.length) {
    if (text[i] === '/' && text[i + 1] === '*') {
      const end = text.indexOf('*/', i + 2);
      if (end < 0) return null;
      out += ' ';
      i = end + 2;
    } else out += text[i++];
  }
  return out.trim();
}
function splitCssValues(text) {
  const parts = [];
  let depth = 0, start = -1;
  for (let i = 0; i <= text.length; i++) {
    const c = text[i];
    if (c === '(') {
      if (start < 0) start = i;
      depth++;
    } else if (c === ')') {
      if (--depth < 0) return null;
      if (depth === 0 && start >= 0) {
        parts.push(text.slice(start, i + 1));
        start = -1;
      }
    } else if ((c === undefined || /\s/.test(c)) && depth === 0) {
      if (start >= 0) {
        parts.push(text.slice(start, i));
        start = -1;
      }
    } else if (start < 0) start = i;
  }
  return depth === 0 ? parts : null;
}
function combineCssType(a, b, mixLenPct) {
  if (!a || !b) return null;
  if (a === b) return a;
  if (mixLenPct && a !== 'num' && b !== 'num') return 'lenpct';
  return null;
}
const CSS_CONST_RE = /^(?:e|pi|-?infinity|nan)(?![a-z0-9_(-])/i;
function cssMathType(token, mixLenPct) {
  let i = 0;
  const skip = () => { while (i < token.length && /\s/.test(token[i])) i++; };
  const parseSum = depth => {
    let left = parseProd(depth);
    if (!left) return null;
    for (;;) {
      const wsBefore = i;
      skip();
      const c = token[i];
      if ((c === '+' || c === '-') && i > wsBefore && i + 1 < token.length && /\s/.test(token[i + 1])) {
        i++;
        left = combineCssType(left, parseProd(depth), mixLenPct);
        if (!left) return null;
      } else {
        i = wsBefore;
        return left;
      }
    }
  };
  const parseProd = depth => {
    let left = parseAtom(depth);
    if (!left) return null;
    for (;;) {
      const wsBefore = i;
      skip();
      const c = token[i];
      if (c === '*' || c === '/') {
        i++;
        const right = parseAtom(depth);
        if (!right) return null;
        if (c === '/') {
          if (right !== 'num') return null;
        } else if (left === 'num') left = right;
        else if (right !== 'num') return null;
      } else {
        i = wsBefore;
        return left;
      }
    }
  };
  const parseAtom = depth => {
    skip();
    const rest = token.slice(i);
    const k = CSS_CONST_RE.exec(rest);
    if (k) { i += k[0].length; return 'num'; }
    const m = CSS_TOKEN_RE.exec(rest);
    if (!m) return null;
    if (m[1]) {
      i += m[0].length;
      return m[2] ? 'len' : m[3] ? 'pct' : 'num';
    }
    if (m[5] === '(') {
      if (depth >= 32) return null;
      i++;
      const t = parseSum(depth + 1);
      skip();
      if (!t || token[i] !== ')') return null;
      i++;
      return t;
    }
    if (m[4]) {
      if (depth >= 32) return null;
      const fn = m[4].toLowerCase();
      const isSame = CSS_MATH_SAME.has(fn), isNum = CSS_MATH_NUM.has(fn);
      if (!isSame && !isNum) return null;
      i += m[0].length;
      let acc = null, count = 0;
      if (fn === 'round') {
        skip();
        const mode = /^([a-z-]+)\s*,/i.exec(token.slice(i));
        if (mode && CSS_ROUND_MODES.has(mode[1].toLowerCase())) i += mode[0].length;
      }
      for (;;) {
        skip();
        const none = fn === 'clamp' && count !== 1 && /^none(?=\s*[,)])/i.exec(token.slice(i));
        if (none) { i += none[0].length; }
        else {
          const arg = parseSum(depth + 1);
          if (!arg) return null;
          if (isNum) {
            if (fn === 'sign' || fn === 'atan2') {
              acc = acc === null ? arg : combineCssType(acc, arg, mixLenPct);
              if (!acc) return null;
            } else if (arg !== 'num') return null;
          } else {
            acc = acc === null ? arg : combineCssType(acc, arg, mixLenPct);
            if (!acc) return null;
          }
        }
        count++;
        skip();
        if (token[i] === ',') { i++; continue; }
        if (token[i] === ')') { i++; break; }
        return null;
      }
      if ((fn === 'calc' || fn === 'abs' || fn === 'sqrt' || fn === 'exp' || fn === 'sin' || fn === 'cos'
        || fn === 'tan' || fn === 'asin' || fn === 'acos' || fn === 'atan' || fn === 'sign') && count !== 1) return null;
      if ((fn === 'pow' || fn === 'atan2' || fn === 'mod' || fn === 'rem') && count !== 2) return null;
      if ((fn === 'log' || fn === 'round') && (count < 1 || count > 2)) return null;
      if (fn === 'clamp' && (count !== 3 || !acc)) return null;
      return isNum ? 'num' : acc;
    }
    return null;
  };
  const head = /^([a-z0-9-]+)\(/i.exec(token);
  if (!head || (!CSS_MATH_SAME.has(head[1].toLowerCase()) && !CSS_MATH_NUM.has(head[1].toLowerCase()))) return null;
  const t = parseAtom(0);
  skip();
  return t && i === token.length ? t : null;
}
function hasBalancedSubstitution(clean) {
  if (!/(?:^|[^a-z0-9_-])(?:var\(\s*--[a-z0-9_-]|env\(\s*[a-z_-])/i.test(clean) || /[;{}]/.test(clean)) return false;
  let depth = 0;
  for (let i = 0; i < clean.length; i++) {
    if (clean[i] === '(') depth++;
    else if (clean[i] === ')' && --depth < 0) return false;
  }
  return depth === 0;
}
function cssSupports(prop, value) {
  if (typeof prop !== 'string' || typeof value !== 'string') return false;
  const p = prop.trim().toLowerCase(), clean = stripCssComments(value);
  if (!clean) return false;
  if (p === 'line-height') {
    if (/^(?:normal|unset|inherit|initial|revert|revert-layer)$/i.test(clean)) return true;
    if (CSS_NUM_RE.test(clean) || CSS_PCT_RE.test(clean) || CSS_LEN_RE.test(clean)) return true;
    if (hasBalancedSubstitution(clean)) return true;
    return cssMathType(clean, true) !== null;
  }
  if (p === 'translate') {
    if (/^(?:none|unset|inherit|initial|revert|revert-layer)$/i.test(clean)) return true;
    const parts = splitCssValues(clean);
    if (!parts || parts.length < 1 || parts.length > 3) return false;
    return parts.every((tok, idx) => {
      if (CSS_LEN_RE.test(tok) || CSS_ZERO_RE.test(tok) || (idx < 2 && CSS_PCT_RE.test(tok))) return true;
      const t = cssMathType(tok, idx < 2);
      return idx < 2 ? t === 'len' || t === 'pct' || t === 'lenpct' : t === 'len';
    });
  }
  if (p === 'scale') {
    if (/^(?:none|unset|inherit|initial|revert|revert-layer)$/i.test(clean)) return true;
    const parts = splitCssValues(clean);
    if (!parts || parts.length < 1 || parts.length > 3) return false;
    return parts.every(tok => {
      if (CSS_NUM_RE.test(tok) || CSS_PCT_RE.test(tok)) return true;
      const t = cssMathType(tok, false);
      return t === 'num' || t === 'pct';
    });
  }
  return false;
}
function cssEscape(value) {
  const s = String(value);
  let out = '';
  for (let i = 0; i < s.length; i++) {
    const code = s.charCodeAt(i);
    if (code === 0) out += '\uFFFD';
    else if ((code >= 1 && code <= 0x1f) || code === 0x7f || (i === 0 && code >= 0x30 && code <= 0x39)
      || (i === 1 && code >= 0x30 && code <= 0x39 && s.charCodeAt(0) === 0x2d))
      out += '\\' + code.toString(16) + ' ';
    else if (i === 0 && s.length === 1 && code === 0x2d) out += '\\-';
    else if (code >= 0x80 || code === 0x2d || code === 0x5f
      || (code >= 0x30 && code <= 0x39) || (code >= 0x41 && code <= 0x5a) || (code >= 0x61 && code <= 0x7a))
      out += s[i];
    else out += '\\' + s[i];
  }
  return out;
}
function unescapeCssAttr(s) {
  return s.replace(/\\([0-9a-f]{1,6}\s?|.)/gi, (_, g) =>
    /^[0-9a-f]+$/i.test(g.trim()) ? String.fromCodePoint(parseInt(g.trim(), 16)) : g);
}
export function createCSS() {
  return { supports: cssSupports, escape: cssEscape };
}

/** A document for one render: `#exact-root` in a body, a head for metas. */
export function createDocument(shell = '') {
  // The shell's classes that set a font size, `display: none`, and a visibility (a static style is a class: emit.rs).
  const fonts = new Map(), displays = new Map(), visibilities = new Map();
  for (const rule of shell.matchAll(/\.([\w-]+)\{([^}]+)\}/g)) {
    const size = /(?:^|;)font-size:([^;]+)(?:;|$)/.exec(rule[2]);
    if (size) fonts.set(rule[1], size[1]);
    if (/(?:^|;)display:none(?:;|$)/.test(rule[2])) displays.set(rule[1], 'none');
    const visibility = /(?:^|;)visibility:(visible|hidden|collapse)(?:;|$)/.exec(rule[2]);
    if (visibility) visibilities.set(rule[1], visibility[1]);
  }
  // `el`'s own value for `property`: its inline value, else its class's in `byClass` (a rendered element's one class
  // is its static style's: tree.js).
  const own = (el, property, byClass) => el.style.getPropertyValue(property) || byClass.get(el.getAttribute('class')) || '';
  /** The page's hero in `el`'s subtree: its first shown image with a source, as document_before.rs `Before::scan` finds
   * it. `null` when that image is lazy, which a row's is (LLP 1048.007 D5): the page opens with a list and has no
   * hero; `undefined` when the subtree has no such image. A head (`template`), a popover or dialog (closed until a
   * script opens it), a covered subtree (a route the stack covers is `inert`) and one with `display: none` hold none.
   * `inherited` is the parent's `visibility`, which CSS inherits: a descendant may set it back to `visible`, and
   * `unset` or `inherit` (a bound blank writes `unset`: tree.js) takes the parent's, as the kernel clears the row for
   * them. A symbol (`data-symbol-source`) is an icon, no hero. */
  const heroIn = (el, inherited = 'visible') => {
    if (el.localName === 'template' || el.localName === 'dialog' || el.hasAttribute('popover') || el.hasAttribute('inert')
      || own(el, 'display', displays) === 'none') return undefined;
    const declared = own(el, 'visibility', visibilities);
    const visibility = VISIBILITIES.has(declared) ? declared : inherited;
    if (el.localName === 'img') {
      const src = el.getAttribute('src') ?? '';
      if (visibility !== 'visible' || !src || src.startsWith('data:') || el.hasAttribute('data-symbol-source')) return undefined;
      return el.getAttribute('loading') === 'lazy' ? null : el;
    }
    for (const child of el.childNodes) if (child.nodeType === 1) { const hero = heroIn(child, visibility); if (hero !== undefined) return hero; }
    return undefined;
  };
  const doc = new Node(9);
  const root = new Element('div'); root.setAttribute('id', 'exact-root');
  const head = new Element('head'); const body = new Element('body');
  doc.append(head, body); body.append(root);
  Object.assign(doc, {
    title: '', head, body, documentElement: body, styleSheets: [], // symbols.js reads a tint from the stylesheets: a render has none
    createElement: t => new Element(t, fonts), createElementNS: (_, t) => new Element(t, fonts),
    createTextNode: t => new Text(t), createComment: () => new Comment(), createDocumentFragment: () => new Fragment(),
    getElementById: id => (id === 'exact-root' ? root : id ? root.getElementsByTagName('*').find(e => e.getAttribute('id') === id) ?? null : null), // navigation.js `opens` names a popover or dialog
    querySelector: sel => (/meta\[name="description"\]/.test(sel) ? head.childNodes.find(m => m.getAttribute?.('name') === 'description') ?? null : null),
    querySelectorAll: () => [],
    root, rootHTML: () => root.childNodes.map(c => c.html()).join(''),
    /** Marks the hero `<img>` (`heroIn`) at high priority, first among its attributes as document.rs writes it, and
     * returns its preload, which makes the `<img>`'s request: its URL, `crossorigin` and priority, as
     * `HeroImage::preload` writes them. A page's likely largest paint is fetched early, as web.dev's LCP guidance has
     * a hero image. '' when the page has none. Call it before `rootHTML`, which writes the mark. */
    heroPreload: () => {
      const hero = heroIn(root);
      if (!hero) return '';
      hero.attrs = new Map([['fetchpriority', HERO_PRIORITY], ...hero.attrs]);
      return `<link rel="preload" as="image"${attr('href', hero.getAttribute('src'))}${attr('crossorigin', hero.getAttribute('crossorigin'))}`
        + ` fetchpriority="${HERO_PRIORITY}">`;
    },
  });
  return doc;
}
