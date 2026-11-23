// An element's `data-*` words on the JS target (LLP 1075.003 §3.3; rt.js
// re-exports it). A read also sees literal attributes emitted with the node.
import { inertAncestor, modalLayers } from './navigation.js';

/** The runner's object of strings as one attribute per word; a word the
 * element's last value had and this one lacks goes. A hatched node's hatch
 * hears the change (hatches.js, LLP 1075.003.000). A drag's carrier words
 * are left out: the page writes `data-datatransferurl` and
 * `data-datatransferfiles` from the values drag.js admits, and
 * `data-datatransfertext` from the text prop (LLP 1094.000 D2), and Contract
 * refuses them as an app's words, which a plan baked before it did may still
 * hold. */
const CARRIERS = new Set(["datatransferurl", "datatransferfiles", "datatransfertext"]);
export function ds(e, json) {
  const words = Object.fromEntries(Object.entries(JSON.parse(json ?? "{}")).filter(([w]) => !CARRIERS.has(w))), last = e.$ds ?? [];
  let changed = false;
  for (const w of last) if (!Object.hasOwn(words, w)) { e.removeAttribute("data-" + w); changed = true; }
  for (const [w, v] of Object.entries(words)) if (e.getAttribute("data-" + w) !== v) { e.setAttribute("data-" + w, v); changed = true; }
  e.$ds = Object.keys(words);
  if (changed) e.$ht?.();
}

export const x_querySelectorAll = selector => {
  const match = /^\[data-([a-z][a-z0-9]*(?:-[a-z][a-z0-9]*)*)~="([^"\\\n\r]+)"\]$/.exec(selector);
  if (!match) return [];
  const root = document.getElementById("exact-root");
  const live = modalLayers();
  return [...root.querySelectorAll(selector)].filter(el => {
    if (!el.id || inertAncestor(el) || el.closest("[data-exiting]")) return false;
    if (live && !live.some(layer => layer.contains(el))) return false;
    for (let at = el; at && at !== root; at = at.parentElement) if (getComputedStyle(at).display === "none") return false;
    return true;
  }).map(el => el.id);
};
