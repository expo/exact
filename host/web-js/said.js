import { normal } from "./rt.js";
// A button its author filled says so (`data-exact-said~="background-color"`,
// LLP 1129 D1: neutral.css's hover is then a wash of its own ink), as
// element.rs writes it for a fill known when the page is compiled. A fill the
// author bound (`background-color=theme.card`) is known only on the page: when
// the pointer comes over, presses or clicks a `button` that is not a platform
// control (`data-button-style`), and when the scheme changes under one hovered,
// its own inline fill is read as the page resolves it and the word follows it.
export const alpha = c => {
  const hex = /^#(?:[0-9a-f]{3}([0-9a-f])|[0-9a-f]{6}([0-9a-f]{2}))$/i.exec(c);
  if (hex) return hex[1] ? parseInt(hex[1], 16) / 15 : parseInt(hex[2], 16) / 255;
  const slash = /\/\s*(none|[\d.]+%?)\s*\)$/.exec(c);
  if (slash) return slash[1] === 'none' ? 0 : parseFloat(slash[1]) / (slash[1].endsWith('%') ? 100 : 1);
  const rgba = /^rgba\(.*,\s*([\d.]+)\s*\)$/.exec(c);
  return rgba ? parseFloat(rgba[1]) : c === 'transparent' ? 0 : 1;
};
// element.rs's rule for `light-dark()`: filled when either half is. A half written as a token is taken as filled.
const clearHalf = h => /^\s*transparent\s*$/i.test(h) || /^\s*#(?:[0-9a-f]{3}0|[0-9a-f]{6}00)\s*$/i.test(h) || alpha(h.trim()) === 0;
export function halves(v) {
  const m = /^\s*light-dark\((.*)\)\s*$/i.exec(v); if (!m) return null;
  let depth = 0; for (let i = 0; i < m[1].length; i++) { const c = m[1][i]; if (c === '(') depth++; else if (c === ')') depth--; else if (c === ',' && !depth) return [m[1].slice(0, i), m[1].slice(i + 1)]; }
  return null;
}

/** The root's bound background `v` when the browser reads it as a colour opaque in both schemes, which makes it the
 * page's canvas (canvas.rs); else null, and the shell's canvas stays. */
export function canvasColor(v) {
  const t = v == null ? "" : normal("background-color", String(v));
  return t && (halves(t) ?? [t]).every(h => alpha(h.trim()) >= 1) ? v : null;
}
function sayFill(e) {
  const v = e.style.getPropertyValue('background-color');
  if (!v && !e.$saidBound) return; // never bound: element.rs said what was written
  const words = (e.getAttribute('data-exact-said') ?? '').split(' ').filter(w => w && w !== 'background-color');
  const pair = v && halves(v);
  if (v && (pair ? !pair.every(clearHalf) : alpha(getComputedStyle(e).backgroundColor) > 0)) words.unshift('background-color');
  e.$saidBound = !!v;
  const said = words.join(' ');
  if (said !== (e.getAttribute('data-exact-said') ?? '')) { if (said) e.setAttribute('data-exact-said', said); else e.removeAttribute('data-exact-said'); }
}
const near = t => t?.closest?.('button:not([data-button-style])');
const hovered = () => { for (const b of document.querySelectorAll('button:not([data-button-style]):hover')) sayFill(b); };
if (typeof document !== 'undefined' && document.addEventListener) {
  // A press can change the fill it hovers: read again once the press's own commit has landed.
  for (const kind of ['pointerover', 'pointerdown', 'click']) document.addEventListener(kind, ev => { const b = near(ev.target); if (b) { sayFill(b); requestAnimationFrame(() => sayFill(b)); } }, true);
  globalThis.matchMedia?.('(prefers-color-scheme: dark)').addEventListener?.('change', hovered);
  // A page adopted under a resting pointer.
  queueMicrotask(hovered);
}
