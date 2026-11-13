// A button its author filled says so (`data-exact-said~="background-color"`,
// LLP 1129 D1: neutral.css's hover is then a wash of its own ink), as
// element.rs writes it for a fill known when the page is compiled. A fill the
// author bound (`background-color=theme.card`) is known only on the page: when
// the pointer comes over, presses or clicks a `button` that is not a platform
// control (`data-button-style`), and when the scheme changes under one hovered,
// its own inline fill is read as the page resolves it and the word follows it.
// Imported by an app's module only when its plan has such a button (exact-web-js `commands.rs`, LLP 1047.002 D1).
import { alpha, halves } from "./said.js";

// element.rs's rule for `light-dark()`: filled when either half is. A half written as a token is taken as filled.
const clearHalf = h => /^\s*transparent\s*$/i.test(h) || /^\s*#(?:[0-9a-f]{3}0|[0-9a-f]{6}00)\s*$/i.test(h) || alpha(h.trim()) === 0;
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
