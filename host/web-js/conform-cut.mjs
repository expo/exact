// What a JS page's build cut, which conformance checks no element inside its root has after each step (conform.mjs).
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { tokens as pageTokens } from './page-style.mjs';

/** From the build's `.gen`: the tokens its sheet was cut for (LLP 1047.003 D1: tags and attributes nothing inside the
 * root writes, whose rules left the sheet), and the behaviours of its navigation and input piece it left out (LLP
 * 1047.002 D2.1, D4.2). None for a page served from a URL. */
export function pageCut(dist) {
  const gen = name => JSON.parse(dist && existsSync(resolve(dist, '.gen', name)) ? readFileSync(resolve(dist, '.gen', name), 'utf8') : '[]');
  return { tokens: new Set(gen('sheet-cut.json')), behaviours: gen('left-out.json') };
}
// What each behaviour acts on, as a selector inside a root. Only what the plan makes matches one, and the compiler
// leaves a behaviour out only where the plan can make none of it, so a match is a case its rule missed.
export const LEFT_OUT = {
  TABS: ['a tablist', '[navigationBack] [role="tablist"]'],
  SPLITS: ['a list or a split', '[navigationBack] :is([data-exact-list], [navigationSplit])'],
  SHEETS: ['a presented route', '[navigationPresentation="modal"], [navigationPresentation="fullscreen"]'],
  BARS: ['a route\'s header', '[navigationKey] > header, [navigationKey] > [data-exact-list-item] > header'],
  CLEAR_BARS: ['a clear bar', '[data-navigationbar="transparent"] > header'],
  PRESS_PAINT: ['a pressed paint', '[style*="--exact-press-opacity"], [style*="--exact-press-background-color"], [style*="--exact-press-color"]'],
  STATE_LOOKS: ['a state look', '[data-exact-looks]'],
  PANS: ['a pan', '[data-exact-on~="pan"]'],
  ROVING: ['a tablist, a menu or a chooser', '[role="tablist"], [role="menu"], [role="alertdialog"]'],
  RADIO_GROUPS: ['a radio group', '[role="radiogroup"]'],
  SHORTCUTS: ['a keyboard shortcut', '[aria-keyshortcuts]'],
};
/** What of `cut` an element inside a root has, read in one evaluation of the page: a phrase for each, for a failure. */
export async function uncut(evaluate, cut) {
  if (!cut.tokens.size && !cut.behaviours.length) return [];
  const [html, found] = await evaluate(`(() => { const roots = [...document.querySelectorAll('#exact-root')];
    return [${cut.tokens.size ? "roots.map(root => root.innerHTML).join('')" : "''"},
      ${JSON.stringify(cut.behaviours.map(name => LEFT_OUT[name][1]))}.flatMap((at, i) => roots.some(root => root.querySelector(at)) ? [i] : [])]; })()`);
  return [...[...pageTokens(html)].filter(t => cut.tokens.has(t)).map(t => `${t}, which its sheet was cut for`),
    ...found.map(i => `${LEFT_OUT[cut.behaviours[i]][0]}, which its build left out (${cut.behaviours[i]})`)];
}
