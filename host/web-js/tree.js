// The element tree: elements built or adopted from a rendered page, their props and styles, and view ids.
import { markdownText } from './markdown.js';
import { oneActivation, suppressedInvocation, releaseAsset as rel } from './elements.js';
import {
  listRow,
  listItem,
  syncListItemRoles,
  listOwner,
  commitGuestOrigin,
  pageGivenFiles,
  renderMarkup,
} from './navigation.js';
import { Docs } from './document.js';
import { Refusal } from './shape.js';
import { regions } from './regions.js';
import './said.js';
import { backdropValue as backdropCss } from './backdrop.js';
import { onEnd, effect, say, inflight, data, After, Paint, Probes, Media, Links, Scrolls, Selects } from './rt.js';
import { Adopt, adopting } from './mount.js';
import { input } from './pieces.js';
const SVG = 'http://www.w3.org/2000/svg';
/** An element under `p`: its static class, attributes and text. */
export function h(p, tag, cls, attrs, text, ns) {
  // (a look snaps by its edges too, LLP 1121 D8)
  if (attrs?.['aria-keyshortcuts'] != null || attrs?.['data-exact-looks'] != null) input();
  if (Adopt) {
    const e = adopt(p, tag, cls, attrs);
    if (tag === 'video' || tag === 'audio') Media.media(e, attrs);
    if (tag === 'iframe') frame(e, attrs?.['data-app-src']);
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
      else {
        e.setAttribute(k, rel(k, attrs[k]));
        if (k === 'data-exact-f' || k.startsWith('data-exact-f-')) bits |= Number(attrs[k]);
      }
    if (tag === 'a') Links?.made(e, attrs);
    if ('data-scrolldocument' in attrs) Docs.add(e);
    if (tag === 'img' && ('data-symbol-source' in attrs || 'data-app-src' in attrs)) Hooks.image?.(e);
  }
  if (Paint) e.$paintFacts = bits;
  if (text !== 0) e.textContent = text;
  if (tag === 'iframe') frame(e, attrs?.['data-app-src']);
  if (tag === 'button') oneActivation(e);
  p.append(owner ? listItem(e, owner) : e);
  // an `audio` is the same media host (LLP 1042 §8)
  if (tag === 'video' || tag === 'audio') Media.media(e, attrs);
  Paint?.list(e);
  return e;
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
      else if (e.getAttribute(k) !== v) e.setAttribute(k, v);
    }
    if (tag === 'a') Links?.made(e, attrs, rendered);
    if ('data-scrolldocument' in attrs) Docs.add(e);
    if (tag === 'img' && ('data-symbol-source' in attrs || 'data-app-src' in attrs)) Hooks.image?.(e);
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
function writeValue(e, v) {
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
          if (Held.has(e)) writeValue(e, Held.get(e));
          e.exactDecorations?.sync();
        }, 0),
      );
    });
  }
  if (e.value !== v && Composing.has(e)) return Held.set(e, v);
  Held.delete(e);
  if (e.value !== v) e.value = v;
}
const navigable = (v, e) => {
    try {
      const p = new URL(v, document.baseURI).protocol;
      return (
        ['http:', 'https:', 'mailto:', 'tel:'].includes(p) || (p === 'data:' && e?.localName === 'iframe' && opaque(e))
      );
    } catch {
      return false;
    }
  },
  // a frame also shows a `data:` document in an opaque sandbox (navigation.js `frameURL`)
  opaque = e =>
    e.hasAttribute('sandbox') &&
    !e
      .getAttribute('sandbox')
      .toLowerCase()
      .split(/[\t\n\f\r ]+/)
      .includes('allow-same-origin');
function frame(e, v) {
  commitGuestOrigin(e);
  // an app file, or a page of the app's origin whose query may name one (LLP 1020.001 D7; picker-glue.js decides)
  const app = !!v?.startsWith('app:/') || pageGivenFiles(v ?? '');
  if (app) e.setAttribute('data-app-src', v);
  else e.removeAttribute('data-app-src');
  if (globalThis.__exactRender) return app;
  if (!e.$frame) {
    e.$frame = true;
    e.addEventListener(
      'load',
      ev => {
        // Else the src that loaded, for the agent's tree, which loads after boot and cannot read another origin's
        // readiness.
        if (e.exactAppFrame && !e.exactAppFrame.url) ev.stopImmediatePropagation();
        else e.exactLoaded = e.getAttribute('src');
      },
      true,
    );
    onEnd(() => e.exactAppFrame?.dispose());
  }
  if (e.exactAppFrame?.path === v) return true;
  e.exactAppFrame?.dispose();
  e.exactAppFrame = null;
  if (!app) return false;
  const s = (e.exactAppFrame = {
    path: v,
    live: true,
    url: null,
    dispose() {
      this.live = false;
      if (this.url) URL.revokeObjectURL(this.url);
    },
  });
  e.removeAttribute('src');
  inflight.n++;
  globalThis.exact ??= {};
  import(new URL('./picker-glue.js', import.meta.url).href)
    .then(m =>
      m.appFrame(
        e,
        s,
        data.appId,
        () => commitGuestOrigin(e),
        u => e.setAttribute('src', navigable(u, e) ? u : 'about:blank'),
      ),
    )
    .finally(() => inflight.n--);
  return true;
}
/** A dynamic prop, by the DOM name the live host uses (`applyProps`). */
export function P(e, name, f) {
  if (name === 'aria-keyshortcuts') input();
  if (name === 'data-scrolldocument') Docs.add(e);
  effect(() => {
    let v = f();
    v = rel(name, v == null ? null : typeof v === 'boolean' ? String(v) : String(v));
    const sandboxChanged = name === 'sandbox' && e.localName === 'iframe' && e.getAttribute(name) !== v;
    if (name === 'tabindex') e.exactAuthoredTabIndex = v;
    // A URL that navigates is written only if the web host's policy takes it (navigation.js `navigableURL`,
    // `frameURL`): a refused link loses its `href`, an iframe shows about:blank and keeps its source for its sandbox to
    // decide.
    if (name === 'src' && e.localName === 'iframe') {
      e.$src = v;
      if (frame(e, v)) return;
      if (v != null && !navigable(v, e)) v = 'about:blank';
    } else if (name === 'href' && v != null && (!navigable(v) || v === '')) v = null;
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
      if (v != null) (Scrolls.get(e) ?? Scrolls.set(e, {}).get(e))[name] = Number(v);
    } else if (BOOL.test(name)) {
      if (name === 'inert') e.authoredInert = v === 'true';
      e.toggleAttribute(name, v === 'true');
      // HTML focus fixup, now, for a control only (glue.js)
      if (name === 'disabled' && v === 'true' && document.activeElement === e && e.matches(':disabled')) e.blur();
      if (name === 'checked') e.checked = e.$checked = v === 'true';
      if (name === 'muted') e.muted = v === 'true';
      if (name === 'autofocus') {
        if (v === 'true') e.removeAttribute('data-autofocus');
        else e.setAttribute('data-autofocus', 'false');
      }
    } else if (v == null) {
      if (e.hasAttribute(name) || suppressedInvocation(e, name)) {
        e.removeAttribute(name);
        if (name.startsWith('data-exact-')) Paint?.facts(e);
      }
    } else if (e.getAttribute(name) !== v) {
      e.setAttribute(name, v);
      if (name.startsWith('data-exact-')) Paint?.facts(e);
    }
    if (name === 'role') syncListItemRoles(e);
    else if (name === 'popover' && e.parentElement?.hasAttribute('data-exact-list-item')) listItem(e);
    if (e.localName === 'a' && (name === 'rel' || name === 'target' || name === 'href')) Links?.prop(e, name, v);
    // a new sandbox takes effect on navigation, even to the same source; a literal source stays in src, a refused
    // `data:` source in $src
    if (name === 'sandbox' && e.localName === 'iframe' && !e.exactAppFrame) {
      const source = e.$src ?? e.getAttribute('src');
      if (source != null) {
        const s = navigable(source, e) ? source : 'about:blank';
        if (sandboxChanged || e.getAttribute('src') !== s) e.setAttribute('src', s);
      }
    }
    if ((name === 'src' || name === 'sandbox') && e.localName === 'iframe') commitGuestOrigin(e);
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
export function S(e, prop, unit, f) {
  Probes?.bind(e, prop, unit, f);
  let rendered = Adopt;
  effect(() => {
    css(e, prop, unit, f(), rendered);
    rendered = false;
  });
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
export function css(e, prop, unit, v, rendered) {
  // The value this binding last wrote: the same again writes nothing (a write restyles and repaints the node, for every
  // dynamic row a list update touched).
  const last = (e.$css ??= {}),
    t = cssText(prop, unit, v);
  if (last[prop] === t) return;
  last[prop] = t;
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
  if (rendered) {
    const now = e.style.getPropertyValue(prop);
    if (t == null ? !now : same(now) === (normal(prop, t) || 'unset')) return;
  }
  if (t == null) return e.style.removeProperty(prop);
  // A value the browser took before (`Normal`, read back after its first write) is written once; one it refused falls
  // to `unset` at once. Only a value not yet seen pays the remove before the write that lets the read-back tell a
  // refusal from an earlier value (LLP 1071.001 A8). Every property takes `unset`: one that reads back empty is a
  // property this browser does not have (Chrome's `-moz-osx-font-smoothing`), not a refusal.
  const k = prop + '\0' + t,
    known = prop.startsWith('--') || t === 'unset' || Normal.get(k);
  if (known) return e.style.setProperty(prop, t);
  if (known === undefined) {
    e.style.removeProperty(prop);
    e.style.setProperty(prop, t);
    const got = same(e.style.getPropertyValue(prop));
    Normal.set(k, got);
    if (Normal.size > 4096) Normal.clear();
    if (got) return;
  }
  e.style.setProperty(prop, 'unset');
  say(`unset ${prop}: ${JSON.stringify(v)} is not a value it takes`);
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
                for (let a = e.parentElement; a; a = a.parentElement) {
                  const c = a.querySelector(`[data-exact-id="${CSS.escape(name)}"]`);
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
 * (motion.js); `image(e)` an image built or adopted with a symbol or `app:/` source (symbols.js). */
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
