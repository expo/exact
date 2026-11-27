// The element tree: elements built or adopted from a rendered page, their props and styles, and view ids.
import { markdownText } from './markdown.js';
import { oneActivation, suppressedInvocation, releaseAsset as rel } from './elements.js';
import { listRow, listOwner, fieldText, renderMarkup } from './navigation.js';
import { Docs } from './document.js';
import { Refusal } from './shape.js';
import { regions } from './regions.js';
import { declare } from './declarations.js';
import { backdropValue as backdropCss } from './backdrop.js';
import { onEnd, effect, untracked, say, inflight, After, Paint, Probes, Media, Links, Scrolls, Selects } from './rt.js';
import { Adopt, adopting, PrepareTasks } from './mount.js';
import { autofocusProp } from './focus.js';
const SVG = 'http://www.w3.org/2000/svg';
/** An element under `p`: its static class, attributes and text. */
export function h(p, tag, cls, attrs, text, ns) {
  if (Adopt) {
    const e = adopt(p, tag, cls, attrs);
    // A literal `draggable` is the app's too (drag.js `writeDraggable`)
    if (attrs?.draggable !== undefined) PropHooks.draggable?.(e, attrs.draggable);
    if (tag === 'video' || tag === 'audio') Media.media(e, attrs);
    if (tag === 'iframe') Hooks.frame(e, attrs?.['data-app-src']);
    if (tag === 'button') oneActivation(e);
    return e;
  }
  const e = ns ? document.createElementNS(ns, tag) : document.createElement(tag),
    owner = listOwner(p);
  e.$exactTree = true;
  e.authoredInert = attrs?.inert !== undefined;
  // `bits`, below: the paint facts the compiler wrote, kept as paint.js `facts` reads them rather than read back from
  // the attribute names; a bound fact's `P` clears them (LLP 1071.001 A6)
  if (cls !== 0) e.setAttribute('class', 'c' + cls);
  let bits = 0;
  if (attrs) {
    for (const k in attrs)
      if (NOT_DOM.test(k)) e['$' + k] = attrs[k];
      else if (k === 'autofocus') autofocusProp(e, attrs[k] !== 'false');
      else if (k === 'draggable' && PropHooks.draggable) PropHooks.draggable(e, attrs[k]);
      else {
        e.setAttribute(k, rel(k, attrs[k]));
        if (k === 'data-exact-f' || k.startsWith('data-exact-f-')) bits |= Number(attrs[k]);
      }
    if (tag === 'a') Links?.made(e, attrs);
    if ('data-scrolldocument' in attrs) Docs.add(e);
    if (tag === 'img') Hooks.image?.(e);
  }
  if (Paint) e.$paintFacts = bits;
  if (text !== 0) e.textContent = text;
  if (tag === 'iframe') Hooks.frame(e, attrs?.['data-app-src']);
  if (tag === 'button') oneActivation(e);
  p.append(owner ? Hooks.listItem(e, owner) : e);
  // an `audio` is the same media host (LLP 1042 §8)
  if (tag === 'video' || tag === 'audio') Media.media(e, attrs);
  Paint?.list(e);
  return e;
}
/** A block text takes its HTML tag from its initial role and level. Later changes update the ARIA attributes on that element. */
export function headingText(p, cls, attrs, text, role, level) {
  const heading = (role ? untracked(role) : attrs.role) === 'heading';
  const value = level ? untracked(level) : Number(attrs['aria-level']);
  const n = Number.isInteger(value) && value >= 1 ? value : 2;
  return h(p, heading && n <= 6 ? 'h' + n : 'div', cls, attrs, text);
}
/** An SVG element (the compiler knows the node's type; element.rs's tag). */
export const hs = (p, tag, cls, attrs, text) => h(p, tag, cls, attrs, text, SVG);
/** A canvas: the host's surface element under its children (`glue.js`). Adopted, its inline alignment goes, as a
 * spinner's: its class follows the live parent. */
export function cv(e) {
  if (Adopt) {
    e.style.removeProperty('justify-self');
    const s = at(e);
    if (s?.dataset?.surface !== undefined) {
      e.$n = s.nextSibling;
      return;
    }
  }
  const s = document.createElement('canvas');
  s.dataset.surface = '';
  s.style.cssText = 'position:absolute;inset:0;width:100%;height:100%;display:block;z-index:-1';
  if (Adopt) e.insertBefore(s, at(e));
  else e.append(s);
}
// Adopt with a cursor per parent: match tags, restore classes and region anchors, and drop renderer view ids.
// A mismatch abandons adoption and builds afresh (LLP 1048.000 D6).
export class Mismatch extends Refusal {}
// The next node to adopt under `p`; insertions go before it and never move it.
export const at = p => (p.$n === undefined ? (p.$n = p.firstChild) : p.$n);
function adopt(p, tag, cls, attrs) {
  let e = at(p);
  while (e && e.nodeType !== 1) e = e.nextSibling;
  const shell = e;
  if (e?.hasAttribute('data-exact-list-item')) e = listRow(e);
  if (!e || e.localName.toLowerCase() !== tag.toLowerCase())
    throw new Mismatch(`adoption: expected <${tag}>, found ${e ? '<' + e.localName + '>' : 'nothing'}`);
  p.$n = shell.nextSibling;
  e.$exactTree = true;
  e.authoredInert = attrs?.inert !== undefined;
  e.$paintWaiting = false;
  e.$paintFacts = null;
  e.removeAttribute('data-exact-paint');
  // Inline rows stay, except spinner alignment: its class follows the live parent.
  if (attrs?.['data-exact-progress'] !== undefined) e.style.removeProperty('justify-self');
  if (e.hasAttribute('data-view')) e.removeAttribute('data-view');
  if (
    attrs?.['data-exact-own-isolation'] === undefined &&
    !e.hasAttribute('data-exact-own-isolation') &&
    !e.hasAttribute('data-exact-policy') &&
    e.style.isolation
  )
    e.style.removeProperty('isolation');
  if (cls !== 0 && e.getAttribute('class') !== 'c' + cls) e.setAttribute('class', 'c' + cls);
  if (attrs) {
    const rendered = Links && tag === 'a' ? e.getAttribute('rel') : null;
    for (const k in attrs) {
      const v = rel(k, attrs[k]);
      if (NOT_DOM.test(k)) e['$' + k] = v;
      else if (k === 'autofocus') autofocusProp(e, v !== 'false');
      else if (e.getAttribute(k) !== v) e.setAttribute(k, v);
    }
    if (tag === 'a') Links?.made(e, attrs, rendered);
    if ('data-scrolldocument' in attrs) Docs.add(e);
    if (tag === 'img') Hooks.image?.(e);
  }
  Paint?.list(e);
  return e;
}
export function mark(p) {
  const c = document.createComment('');
  p.insertBefore(c, at(p));
  return c;
}
// Present means true: a bound `autofocus` false is no attribute (rows.rs writes the static one so).
const BOOL =
    /^(disabled|readonly|inert|checked|multiple|autoplay|controls|loop|muted|playsinline|disablepictureinpicture|disableremoteplayback|autofocus)$/,
  // An iframe's partition and Apple words, kept on the element, never DOM attributes (a browser has neither,
  // LLP 1020.002 Q4).
  NOT_DOM =
    /^(partition|appleAllowsBackForwardNavigationGestures|appleAllowsMagnification|appleInactiveSchedulingPolicy)$/i;
/** A loaded piece's own handling of a prop (symbols.js's `src`): true when handled. */
export const PropHooks = {};
// Which field composes, for a piece loaded later (LLP 1114.007).
if (typeof addEventListener === 'function')
  for (const [type, on] of [
    ['compositionstart', true],
    ['compositionend', false],
  ])
    addEventListener(
      type,
      ev => {
        ev.target.exactComposing = on;
      },
      true,
    );
const Composing = new WeakSet(),
  Held = new WeakMap(),
  HeldTask = new WeakMap();
function writeValue(e, v, replace) {
  // An option's getter may equal its label without carrying an explicit value.
  if (e.localName === 'option') {
    if (e.getAttribute('value') !== v) e.value = v;
    return;
  }
  // the app's `value` never ends a composition (LLP 1115.000 §5, ruling 6; the wasm host's `writeValue`, glue.js):
  // another value written while a field's text composes is held, the last one, and written when the composition has
  // ended and its last `input` was heard.
  if (!HeldTask.has(e) && (e.localName === 'input' || e.localName === 'textarea')) {
    HeldTask.set(e, 0);
    e.addEventListener('compositionstart', () => {
      clearTimeout(HeldTask.get(e));
      Composing.add(e);
    });
    e.addEventListener('compositionend', () => {
      HeldTask.set(
        e,
        setTimeout(() => {
          Composing.delete(e);
          if (Held.has(e)) writeValue(e, ...Held.get(e));
          e.exactDecorations?.sync();
        }, 0),
      );
    });
  }
  if (e.value !== v && Composing.has(e)) {
    Held.set(e, [v, replace]);
    e.$echo?.(() => {
      if (Held.get(e)?.[0] === e.value) { Held.delete(e); e.$echo(); }
    });
    return;
  }
  if (Held.delete(e)) e.$echo?.();
  if (e.value !== v) {
    if (replace) replace(e, v);
    else e.value = v;
    return true;
  }
}
// A field can share a record with another editor. A change to another member
// only reaches the widget when its own bound text changes.
export function fieldValue(e, f) {
  let accepted;
  const show = () => {
    if (writeValue(e, accepted, e.$field?.replace)) e.exactDecorations?.sync();
  };
  const binding = effect(() => {
    const value = fieldText(e, String(f() ?? ''));
    if (accepted === value) return;
    accepted = value;
    show();
  });
  return { binding, show };
}
// a URL that navigates (navigation.js `navigableURL`); an iframe's is iframes.js's
const navigable = v => {
  try {
    return ['http:', 'https:', 'mailto:', 'tel:'].includes(new URL(v, document.baseURI).protocol);
  } catch {
    return false;
  }
};
/** A dynamic prop, by the DOM name the live host uses (`applyProps`). */
export function P(e, name, f) {
  if (PrepareTasks && ['navigationKey', 'navigationBack', 'aria-selected', 'aria-controls', 'id'].includes(name))
    (e.$taskProps ??= {})[name] = f;
  if (name === 'data-scrolldocument') Docs.add(e);
  effect(() => {
    let v = f();
    v = rel(name, v == null ? null : typeof v === 'boolean' ? String(v) : String(v));
    if (name === 'tabindex') e.exactAuthoredTabIndex = v;
    // A URL that navigates is written only if the web host's policy takes it (navigation.js `navigableURL`): a refused
    // link loses its `href`. An iframe's `src` and `sandbox` are iframes.js's `PropHooks`.
    if (name === 'href' && v != null && (!navigable(v) || v === '')) v = null;
    // no material, cleared as the runner clears it (bridge.rs `prop_value`)
    if (name === 'backgroundMaterial' && v === 'none') v = null;
    if (PropHooks[name]?.(e, v)) return;
    if (NOT_DOM.test(name)) return void (e['$' + name] = v);
    // media.js: `paused`, `volume`, `currentTime` … are the glue's; an `app:/` source its own
    if (e.$media && Media.mediaProp(e, name, v)) return;
    // `$unset`: no text, as the runner's cleared row, not an empty one (difftest's drive.mjs)
    if (name === 'text') {
      e.$unset = v == null;
      if (!e.childElementCount && e.textContent !== (v ?? '')) e.textContent = v ?? '';
    } else if (name === 'value') {
      if (e.localName === 'select') {
        Selects.add(e);
        e.$value = v ?? '';
        e.$set = true;
      }
      // a cleared option's value is its label again
      if (v == null && e.localName === 'option') e.removeAttribute('value');
      else writeValue(e, v ?? '');
      e.exactDecorations?.sync();
    } else if (name === 'scrollTop' || name === 'scrollLeft') {
      // Kept as `$scrollTop`, `$scrollLeft`: where the scroller opens when its page arrives (fragment.js, LLP 1153 D5.1).
      e['$' + name] = v == null ? undefined : Number(v);
      if (v != null) (Scrolls.get(e) ?? Scrolls.set(e, {}).get(e))[name] = Number(v);
    } else if (BOOL.test(name)) {
      if (name === 'inert') e.authoredInert = v === 'true';
      if (name === 'autofocus') autofocusProp(e, v === 'true');
      else e.toggleAttribute(name, v === 'true');
      // HTML focus fixup, now, for a control only (glue.js)
      if (name === 'disabled' && v === 'true' && document.activeElement === e && e.matches(':disabled')) e.blur();
      if (name === 'checked') e.checked = e.$checked = v === 'true';
      if (name === 'muted') e.muted = v === 'true';
    } else if (v == null) {
      if (e.hasAttribute(name) || suppressedInvocation(e, name)) {
        e.removeAttribute(name);
        if (name.startsWith('data-exact-')) Paint?.facts(e);
      }
    } else if (e.getAttribute(name) !== v) {
      e.setAttribute(name, v);
      if (name.startsWith('data-exact-')) Paint?.facts(e);
    }
    if (name === 'role') Hooks.listRoles?.(e);
    else if (name === 'popover' && e.parentElement?.hasAttribute('data-exact-list-item')) Hooks.listItem(e);
    if (e.localName === 'a' && (name === 'rel' || name === 'target' || name === 'href')) Links?.prop(e, name, v);
  });
}
/** A reader uses built pieces; new text loads the parser on demand. */
export function md(e, f, prepared) {
  markdownText(e, f, prepared, { effect, inflight, adopting, renderMarkup });
}
/** Views by id, one id space for the agent and the GPU module (the web
 * host's `exact.views`): an element gets an id when either first asks. */
export const Views = new Map();
export const Ids = new WeakMap();
let NextView = 1;
const forgetView = e => {
  const i = Ids.get(e);
  if (!e.isConnected && Views.get(i) === e) Views.delete(i);
};
let ViewObserver = null;
export function viewId(e) {
  let i = Ids.get(e);
  if (!i) {
    i = NextView++;
    Ids.set(e, i);
  }
  if (!Views.has(i)) onEnd(() => queueMicrotask(() => forgetView(e)));
  Views.set(i, e);
  watchViews();
  return i;
}
function watchViews() {
  if (ViewObserver || typeof MutationObserver !== 'function') return;
  ViewObserver = new MutationObserver(records => {
    for (const r of records)
      for (const e of r.removedNodes) {
        forgetView(e);
        for (const child of e.querySelectorAll?.('*') ?? []) forgetView(child);
      }
  });
  ViewObserver.observe(document.documentElement, { childList: true, subtree: true });
}
export const mapped = (v, map, clear) =>
  typeof v === 'string' && !v.trim() ? (clear === undefined ? map(null) : clear) : map(v);
function splitCssValues(text) {
  const parts = [];
  let depth = 0, start = -1;
  for (let i = 0; i <= text.length; i++) {
    const c = text[i];
    if (c === '(') { if (start < 0) start = i; depth++; }
    else if (c === ')') {
      if (--depth < 0) return null;
      if (depth === 0 && start >= 0) { parts.push(text.slice(start, i + 1)); start = -1; }
    } else if ((c === undefined || /\s/.test(c)) && depth === 0) {
      if (start >= 0) { parts.push(text.slice(start, i)); start = -1; }
    } else if (start < 0) start = i;
  }
  return depth === 0 ? parts : null;
}
function cssSupports(prop, text) {
  return typeof CSS !== 'undefined' && typeof CSS.supports === 'function' ? CSS.supports(prop, text) : false;
}
export function translateParts(v) {
  const text = String(v ?? '').trim();
  if (!text || /^(?:none|unset|initial|revert|revert-layer)$/i.test(text) || !cssSupports('translate', text)) return null;
  const parts = splitCssValues(text.replace(/\/\*[\s\S]*?\*\//g, ' ').trim());
  return parts && parts.length >= 1 && parts.length <= 3 ? parts : null;
}
export function scaleAxis(v, axis) {
  const text = String(v ?? '').trim();
  if (!text || /^(?:none|unset|initial|revert|revert-layer)$/i.test(text) || !cssSupports('scale', text)) return '1';
  const parts = splitCssValues(text.replace(/\/\*[\s\S]*?\*\//g, ' ').trim());
  if (!parts || parts.length < 1 || parts.length > 3) return '1';
  const n = parts[axis] ?? parts[0];
  return n.endsWith('%') ? parseFloat(n) / 100 : n;
}
const LH_UNIT = /^(?:px|in|cm|mm|q|pt|pc|ch|lh|rem|[sld]?v[wh]|vmin|vmax|cq[whib]|cqmin|cqmax)$/i;
const LH_CONST = /^(?:e|pi|-?infinity|nan)$/i;
const LH_FN = /^(?:calc|min|max|clamp)$/i;
function lhTokens(raw) {
  const out = [];
  let i = 0, ws = false, n = raw.length;
  const esc = () => {
    if (i + 1 >= n || /[\n\f\r]/.test(raw[i + 1])) return null;
    i++;
    const hex = /^[0-9a-f]{1,6}/i.exec(raw.slice(i));
    if (hex) {
      i += hex[0].length;
      if (i < n && /[\t\n\f\r ]/.test(raw[i])) { if (raw[i] === '\r' && raw[i + 1] === '\n') i++; i++; }
      const cp = parseInt(hex[0], 16);
      return cp === 0 || cp > 0x10ffff || (cp >= 0xd800 && cp <= 0xdfff) ? '\uFFFD' : String.fromCodePoint(cp);
    }
    return raw[i++];
  };
  const ident = () => {
    let s = '';
    while (i < n) {
      if (raw[i] === '\\') { const c = esc(); if (c === null) return null; s += c; }
      else if (/[\w\u0080-\uffff-]/.test(raw[i])) s += raw[i++];
      else break;
    }
    return s;
  };
  while (i < n) {
    const c = raw[i];
    if (/[\t\n\f\r ]/.test(c)) { ws = true; i++; continue; }
    if (c === '/' && raw[i + 1] === '*') { const end = raw.indexOf('*/', i + 2); i = end < 0 ? n : end + 2; continue; }
    if ('(),*/'.includes(c)) { out.push({ k: c, ws }); ws = false; i++; continue; }
    if ((c === '+' || c === '-') && (i + 1 >= n || (!/[\d.]/.test(raw[i + 1]) && !(c === '-' && (raw[i + 1] === '\\' || /[a-z_\u0080-\uffff-]/i.test(raw[i + 1])))))) {
      out.push({ k: c, ws }); ws = false; i++; continue;
    }
    const num = /^[+-]?(?:\d+(?:\.\d+)?|\.\d+)(?:e[+-]?\d+)?/i.exec(raw.slice(i));
    if (num) {
      i += num[0].length;
      if (raw[i] === '%') return null;
      const unit = raw[i] === '\\' || /[a-z_\u0080-\uffff-]/i.test(raw[i] ?? '') ? ident() : '';
      if (unit === null) return null;
      out.push({ k: unit ? 'd' : 'n', v: num[0], u: unit.toLowerCase(), ws });
      ws = false;
      continue;
    }
    if (c === '\\' || /[a-z_\u0080-\uffff-]/i.test(c)) {
      const id = ident();
      if (!id) return null;
      if (raw[i] === '(') { i++; out.push({ k: 'f', v: id.toLowerCase(), ws }); }
      else out.push({ k: 'i', v: id.toLowerCase(), ws });
      ws = false;
      continue;
    }
    return null;
  }
  return out;
}
function parseLineHeightMath(raw) {
  const toks = lhTokens(raw);
  if (!toks || !toks.length || toks[0].k !== 'f' || !LH_FN.test(toks[0].v)) return null;
  let i = 0, values = 0;
  const sum = d => {
    let left = prod(d);
    if (!left) return null;
    while (i < toks.length && (toks[i].k === '+' || toks[i].k === '-')) {
      const op = toks[i];
      if (!op.ws || i + 1 >= toks.length || !toks[i + 1].ws) return null;
      i++;
      const right = prod(d);
      if (!right || right.t !== left.t) return null;
      left = { t: left.t, s: `${left.s} ${op.k} ${right.s}` };
    }
    return left;
  };
  const prod = d => {
    let left = atom(d);
    if (!left) return null;
    while (i < toks.length && (toks[i].k === '*' || toks[i].k === '/')) {
      const op = toks[i++].k;
      const right = atom(d);
      if (!right || (op === '/' ? right.t !== 'n' : left.t === 'l' && right.t === 'l')) return null;
      left = { t: left.t === 'l' || right.t === 'l' ? 'l' : 'n', s: `${left.s} ${op} ${right.s}` };
    }
    return left;
  };
  const atom = d => {
    const t = toks[i++];
    if (!t) return null;
    if (t.k === 'n') return ++values <= 32 ? { t: 'n', s: t.v } : null;
    if (t.k === 'i') return LH_CONST.test(t.v) && ++values <= 32 ? { t: 'n', s: t.v } : null;
    if (t.k === 'd') {
      if (!LH_UNIT.test(t.u) || ++values > 32) return null;
      if (t.u === 'ch' || t.u === 'lh') {
        const px = Number(t.v) * (t.u === 'ch' ? 8 : 16);
        return { t: 'l', s: Number.isFinite(px) ? `${px}px` : `(${px < 0 ? '-infinity' : 'infinity'} * 1px)` };
      }
      return { t: 'l', s: `${t.v}${t.u}` };
    }
    if (t.k === '(') {
      if (d >= 32) return null;
      const v = sum(d + 1);
      const close = toks[i++];
      if (!v || (close && close.k !== ')')) return null;
      return { t: v.t, s: `(${v.s})` };
    }
    if (t.k === 'f' && LH_FN.test(t.v)) {
      if (d >= 32) return null;
      if (t.v === 'calc') {
        const v = sum(d + 1);
        const close = toks[i++];
        if (!v || (close && close.k !== ')')) return null;
        return { t: v.t, s: `calc(${v.s})` };
      }
      const args = [];
      let ty = null;
      for (;;) {
        const none = t.v === 'clamp' && args.length !== 1 && toks[i]?.k === 'i' && toks[i].v === 'none'
          && (!toks[i + 1] || toks[i + 1].k === ',' || toks[i + 1].k === ')');
        if (none) { i++; args.push('none'); }
        else {
          const a = sum(d + 1);
          if (!a || (ty && ty !== a.t)) return null;
          ty = a.t;
          args.push(a.s);
        }
        const sep = toks[i++];
        if (sep?.k === ',') continue;
        if (!sep || sep.k === ')') break;
        return null;
      }
      if (!ty || (t.v === 'clamp' && args.length !== 3)) return null;
      return { t: ty, s: `${t.v}(${args.join(', ')})` };
    }
    return null;
  };
  const res = atom(0);
  return res && res.t === 'l' && i >= toks.length ? res.s : null;
}
// Both line-height and its inheritance marker read this binding. Journal a refused value once per change.
export function lineHeight(e, value) {
  const t = typeof value === 'string' && value.trim();
  // Theme and metric names are resolved by the next projection. A CSS value
  // the browser accepts still has to fit the kernel's line-height grammar.
  const length = t && /^([+-]?(?:\d+(?:\.\d+)?|\.\d+)(?:e[+-]?\d+)?)(px|rem|em|ch|lh|[sld]?v[wh]|vmin|vmax|cq[whib]|cqmin|cqmax)$/i.exec(t);
  if (length && Number(length[1]) >= 0 && Number.isFinite(Math.fround(Number(length[1]))) &&
    (length[2].toLowerCase() !== 'px' || length[2] === 'px')) {
    delete e.$badLeading;
    return value;
  }
  const math = typeof value === 'string' ? parseLineHeightMath(value) : null;
  if (math) { delete e.$badLeading; return math; }
  if (t && ((t.includes('(') && !t.startsWith('var(')) || !Number.isNaN(Number(t)) ||
    (cssSupports('line-height', t) && !/^(?:normal|unset|inherit)$/i.test(t) && !t.startsWith('var(')))) {
    if (value !== e.$badLeading) invalidStyle(e, 'line-height', value);
    e.$badLeading = value;
    return 'unset';
  }
  delete e.$badLeading;
  return value;
}
export function invalidStyle(e, prop, v) {
  say(`view ${viewId(e)}: invalid ${prop} value ${JSON.stringify(v)}; unset`);
}
export function S(e, prop, unit, f) {
  Probes?.bind(e, prop, unit, f);
  let rendered = Adopt;
  const fx = effect(() => {
    css(e, prop, unit, f(), rendered);
    rendered = false;
  });
  Probes?.track(e, prop, fx);
}
let Scratch = null;
const Normal = new Map(),
  same = v => v.replace(/\btransparent\b/g, 'rgba(0, 0, 0, 0)');
export function normal(prop, t) {
  const k = prop + '\0' + t;
  let v = Normal.get(k);
  if (v === undefined) {
    const s = (Scratch ??= document.createElement('i').style);
    s.removeProperty(prop);
    s.setProperty(prop, t);
    Normal.set(k, (v = same(s.getPropertyValue(prop))));
    if (Normal.size > 4096) Normal.clear();
  }
  return v;
}
// A row's CSS text: authored blanks explicitly clear (LLP 1115.000 D2); internal custom properties may carry an empty
// token (text-style presence); `auto` on a maximum is CSS's unbounded `none` (LLP 1102 §3.11); a bound
// `-exact-hairline` is the sheet's one device pixel, as css.rs writes a literal one (LLP 1115.000 D10.3); a layout
// region is its safe area (LLP 1123 D10).
export const cssText = (prop, unit, v) => {
  const t =
    v == null
      ? null
      : typeof v === 'number'
        ? v + unit
        : !String(v).trim() && !prop.startsWith('--')
          ? 'unset'
          : /^\s*-exact-hairline\s*$/i.test(v)
            ? 'var(--exact-hairline, 1px)'
            : regions(String(v));
  return t === 'auto' && (prop === 'max-width' || prop === 'max-height') ? 'none' : t;
};
export const GPU_PLACEMENT = true; // The build clears this for apps without a GPU module.
export function css(e, prop, unit, v, rendered) {
  // The value this binding last wrote: the same again writes nothing (a write restyles and repaints the node, for every
  // dynamic row a list update touched).
  const last = (e.$css ??= {}),
    t = cssText(prop, unit, v);
  if (last[prop] === t) return;
  last[prop] = t;
  if (prop === '--') return declare(e, t);
  if (prop === '--exact-animation-trigger') {
    if (v === 'none') e.setAttribute('data-exact-animation-trigger', 'none');
    else e.removeAttribute('data-exact-animation-trigger');
    return;
  }
  // The `animation` shorthand resets the longhands a timeline's consumer writes again after it (LLP 1057.003 D5).
  if (prop === 'animation')
    for (const p of ['animation-timeline', 'animation-range', 'animation-play-state']) delete last[p];
  // An adopted node's inline style is the renderer's: a value it already shows is not written again (a write
  // restyles and repaints the node). (`transparent` is the color the renderer writes as rgba(0, 0, 0, 0).)
  const valid = t == null || prop.startsWith('--') || t === 'unset' || normal(prop, t);
  if (!valid) invalidStyle(e, prop, v);
  if (rendered) {
    const now = e.style.getPropertyValue(prop);
    if (t == null ? !now : same(now) === (normal(prop, t) || 'unset')) return;
  }
  if (prop in MOVES) moved(e, MOVES[prop]);
  const placement = GPU_PLACEMENT && e.exactPlaced && /^(position|transform|transform-origin|z-index|visibility|clip-path)$/.test(prop)
    ? globalThis.exact?.gpu : null;
  placement?.beforeStyle(e);
  if (t == null) e.style.removeProperty(prop);
  else {
    // A value not yet seen is probed on the detached style `normal` keeps, so the live node gets only the final
    // write (LLP 1071.001 A8). Every property takes `unset`: one that reads back empty is a property this browser
    // does not have (Chrome's `-moz-osx-font-smoothing`), not a refusal.
    e.style.setProperty(prop, valid ? t : 'unset');
  }
  placement?.afterStyle(e);
}
// A box whose transform changes frame after frame is promoted to a compositor layer of its own while it changes, as
// `will-change` asks (CSS Will Change 1 §2): the browser then moves and scales the layer's pixels instead of
// rasterizing its content again at each step, which for a box of iframes costs frames and input latency
// (expo/exact#543). The hint goes once the transform has held for `SETTLED` ms, so the content is rasterized sharp
// at its new scale. Two changes within `STEADY` ms of each other are a change every frame (a 10 Hz floor).
const MOVES = { transform: 'transform', translate: 'translate', scale: 'scale', rotate: 'rotate', '--exact-scale': 'scale' },
  STEADY = 100,
  SETTLED = 200;
function moved(e, prop) {
  const now = performance.now(),
    m = (e.$moved ??= { at: -Infinity, props: new Set(), timer: 0 }),
    steady = now - m.at < STEADY;
  m.at = now;
  if (!steady || m.props.has(prop)) return;
  m.props.add(prop);
  e.style.setProperty('will-change', [...m.props].join(', '));
  if (!m.timer) {
    const settle = () => {
      const held = performance.now() - m.at;
      if (held < SETTLED) return void (m.timer = setTimeout(settle, SETTLED - held));
      m.timer = 0;
      m.props.clear();
      e.style.removeProperty('will-change');
    };
    m.timer = setTimeout(settle, SETTLED);
  }
}
/** `S` for a row that can reference an element (`url(#…)`): an authored id
 * names the node the kernel's `resolve_id` would, the nearest in the tree
 * (the deepest common ancestor's first, LLP 1055.000 D3), by its DOM id; a
 * name not yet in the tree is resolved again after the commit builds it. */
let Refs = null;
export function Sr(e, prop, unit, f) {
  Probes?.bind(e, prop, unit, f);
  effect(() => {
    const v = f(),
      write = () => {
        let missing = false;
        const r =
          v == null
            ? v
            : String(v).replace(/url\(\s*["']?#([^"')\s]+)["']?\s*\)/g, (m, name) => {
                const findId = (root, target) => {
                  if (typeof CSS !== 'undefined' && typeof CSS.escape === 'function')
                    return root.querySelector(`[data-exact-id="${CSS.escape(target)}"]`);
                  for (const el of root.getElementsByTagName('*'))
                    if (el.getAttribute('data-exact-id') === target) return el;
                  return null;
                };
                for (let a = e.parentElement; a; a = a.parentElement) {
                  const c = findId(a, name);
                  if (c) return `url(#${c.id})`;
                }
                missing = true;
                return m;
              });
        css(e, prop, unit, r);
        return missing;
      };
    if (write()) {
      if (!Refs) {
        Refs = [];
        After.push(() => {
          for (const w of Refs.splice(0)) w();
        });
      }
      Refs.push(write);
    }
  });
}
export const backdropValue = (v, report = true) =>
  backdropCss(v, report ? why => say(`unset backdrop-filter: ${why}`) : undefined);
/** Loaded pieces' hooks: `style(e, prop, value)` takes a dynamic row's write on a node the motion piece holds
 * (motion.js); `image(e)` an image built or adopted (symbols.js); `frame(e, v)` an
 * iframe's app file (iframes.js); `listItem(e, owner)` and `listRoles(e)` a List's row and its role (list-items.js). */
export const Hooks = {};
/** `S` on a node the motion engine follows (`mo`): while a hold owns it, a
 * write goes to the authored style the hold restores. */
export function Sm(e, prop, unit, f) {
  Probes?.bind(e, prop, unit, f);
  effect(() => {
    const v = f();
    // Held: the hold's authored style takes it, and what css() last wrote no longer says what shows.
    if (Hooks.style?.(e, prop, cssText(prop, unit, v))) {
      if (e.$css) delete e.$css[prop];
    } else css(e, prop, unit, v);
  });
}
