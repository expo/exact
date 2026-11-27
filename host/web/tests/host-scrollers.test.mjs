// @ref LLP 1123 D9, D14, D17 — the boxes the host's own sheets make scroll are scrollers to the safe-area executor, which
// finds them by `HOST_SCROLLERS` (observe.js) rather than reading every ancestor's computed overflow. Each rule in
// `index.html` and `neutral.css` that gives an `overflow` of `auto` or `scroll` is listed here with the selector that
// finds its boxes; a rule this list lacks fails the test until both lists name it.
import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { HOST_SCROLLERS } from '../navigation/observe.js';

const COVERS = {
  '[data-scroll="true"]': '[data-scroll="true"]',
  '#exact-root [role="tablist"][data-exact-sidebar-tabs]': '[role="tablist"][data-exact-sidebar-tabs]',
  ':where(#exact-root [role="tablist"][data-exact-tab-bar])': '[role="tablist"][data-exact-tab-bar]',
  '#exact-root [data-exact-route-popover]:popover-open': '[data-exact-route-popover]',
  ':where(#exact-root dialog[role="alertdialog"]:not([data-exact-authored-dialog]) > [data-alertpart="message"])': 'dialog[role="alertdialog"] > [data-alertpart="message"]',
};

test("every rule of the host's sheets that makes a box scroll is found by the executor", async () => {
  const web = resolve(import.meta.dir, '..'), html = readFileSync(`${web}/index.html`, 'utf8');
  const sheets = [...[...html.matchAll(/<style[^>]*>([\s\S]*?)<\/style>/g)].map((m) => m[1]), readFileSync(`${web}/neutral.css`, 'utf8')];
  const browser = await suiteBrowser('chromium');
  try {
    const page = await browser.newPage();
    await page.setContent('<!doctype html><title>sheets</title>');
    const found = await page.evaluate((sheets) => {
      for (const text of sheets) document.head.append(Object.assign(document.createElement('style'), { textContent: text }));
      const out = [];
      // Each style rule, a nested one's selector taken with its parent's
      const walk = (rules, parent) => {
        for (const r of rules) {
          if (r instanceof CSSStyleRule) {
            const selector = !parent ? r.selectorText : r.selectorText.includes('&') ? r.selectorText.replaceAll('&', `:is(${parent})`) : `${parent} ${r.selectorText}`;
            if (['overflow', 'overflowX', 'overflowY', 'overflowBlock', 'overflowInline'].some((p) => /\b(auto|scroll)\b/.test(r.style[p] ?? ''))) out.push(selector);
            if (r.cssRules?.length) walk(r.cssRules, selector);
          } else if (r.cssRules) walk(r.cssRules, parent);
        }
      };
      for (const sheet of document.styleSheets) walk(sheet.cssRules, null);
      return out;
    }, sheets);
    expect(found.sort()).toEqual(Object.keys(COVERS).sort());
    for (const cover of Object.values(COVERS)) expect(HOST_SCROLLERS).toContain(cover);
  } finally {
    await browser.close();
  }
}, 30_000);
