// B5's precedence guard (LLP 1071.001 part B): non-important shell rules stay below two IDs.
// App classes have two IDs and a class (emit.rs), lowered arms three IDs and a class (lower.js).
// This keeps shell defaults below both, as they are below inline rows.
import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { withLook } from '../look.mjs';
import { symbolSheet, wasmSource } from './glue-fixture.mjs';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';

const WEB = resolve(import.meta.dir, '..'), WEB_JS = resolve(WEB, '../web-js');
// The shell's sheet with its look, and the sheets the glue writes itself (their text, as the modules hold it).
const shell = withLook(readFileSync(resolve(WEB, 'index.html'), 'utf8')).match(/<style>([\s\S]*?)<\/style>/g).map(s => s.replace(/<\/?style>/g, '')).join('\n');
const injected = [
  [resolve(WEB, 'input-glue.js'), /textContent: "(\[data-exact-snap\][^"]*)"/],
  [resolve(WEB_JS, 'modes.js'), /sheet\.textContent = "([^"]*)"/],
].map(([file, at]) => { const m = at.exec(readFileSync(file, 'utf8')); if (!m) throw new Error(`${file}: its sheet's text moved`); return m[1]; });
injected.push(symbolSheet(wasmSource));

// Only the ID column matters here. :is(), :not(), :has() and nesting take the maximum
// of their alternatives; :where() contributes zero (Selectors 4 §17, CSS Nesting §4).
function sheetIds() {
  const ids = (selector, parent = 0) => {
    let at = 0;
    const quoted = quote => {
      while (at < selector.length) {
        const c = selector[at++];
        if (c === '\\') at++;
        else if (c === quote) break;
      }
    };
    const scan = (implicit = false) => {
      let count = 0, max = 0, nesting = false, anyNesting = false;
      const end = () => {
        max = Math.max(max, count + (implicit && !nesting ? parent : 0));
        anyNesting ||= nesting;
        count = 0; nesting = false;
      };
      while (at < selector.length) {
        const c = selector[at++];
        if (c === '\\') at++;
        else if (c === '"' || c === "'") quoted(c);
        else if (c === '[') {
          while (at < selector.length) {
            const c = selector[at++];
            if (c === '\\') at++;
            else if (c === '"' || c === "'") quoted(c);
            else if (c === ']') break;
          }
        } else if (c === '#') count++;
        else if (c === '&') { count += parent; nesting = true; }
        else if (c === '(') {
          const zero = selector.slice(at - 7, at).toLowerCase() === ':where(';
          const arg = scan();
          if (!zero) count += arg.ids;
          nesting ||= arg.nesting;
        } else if (c === ')') break;
        else if (c === ',') end();
      }
      end();
      return { ids: max, nesting: anyNesting };
    };
    return scan(true).ids;
  };
  const out = [];
  const walk = (rules, parent = 0, path = []) => {
    for (const r of rules) {
      const weight = r.selectorText ? ids(r.selectorText, parent) : parent;
      const selectors = r.selectorText ? [...path, r.selectorText] : path;
      if (r.style && weight >= 2) for (const p of r.style) {
        if (r.style.getPropertyPriority(p) !== 'important') out.push({ selector: selectors.join(' { '), property: p, ids: weight });
      }
      if (r.cssRules) walk(r.cssRules, weight, selectors);
    }
  };
  let n = 0;
  for (const s of document.styleSheets) { n += s.cssRules.length; walk(s.cssRules); }
  return { rules: n, twoIds: out };
}

test('the shell guard counts ID specificity through alternatives and nesting', async () => {
  const suite = await suiteBrowser();
  try {
    const page = await (await suite.newContext()).newPage();
    await page.setContent(`<style>
      :is(#a, #b) { --is: 1; }
      :not(#a, #b) { --not: 1; }
      :has(#a, #b) { --has: 1; }
      #a :where(#b, #c:not(#d)) { --where: 1; }
      [data-label="#a, #b"] { --attribute: 1; }
      .escaped\\#a\\#b { --escaped: 1; }
      #a, #b { :is(& > header, & > article) { --nested: 1; } }
      #a #b { --important: 1 !important; }
      #a #b { --compound: 1; }
      :is(#a, #b #c) { --max: 1; }
      #a:not(:has(#b, #c)) { --sum: 1; }
      :nth-child(2n of #a #b, #c) { --nth: 1; }
      #a, #b #c { & > header { --parent-max: 1; } }
      #a { & & { --repeated: 1; } #b { --implicit: 1; } }
      #a #b { :where(&) > header { --nested-where: 1; } }
      #a { @media (min-width: 0px) { & #b { --grouped: 1; } } }
    </style>`);
    const found = await page.evaluate(sheetIds);
    expect(found.twoIds.map(({ property, ids }) => [property, ids])).toEqual([
      ['--compound', 2], ['--max', 2], ['--sum', 2], ['--nth', 2],
      ['--parent-max', 2], ['--repeated', 2], ['--implicit', 2], ['--grouped', 2],
    ]);
  } finally { await suite.close(); }
});

test('no non-important shell rule has ID specificity above one', async () => {
  const suite = await suiteBrowser();
  try {
    const page = await (await suite.newContext()).newPage();
    await page.setContent(`<style>${shell}</style>${injected.map(t => `<style>${t}</style>`).join('')}<div id="exact-root"></div>`);
    const found = await page.evaluate(sheetIds);
    expect(found.rules).toBeGreaterThan(300);
    expect(found.twoIds).toEqual([]);
  } finally { await suite.close(); }
});

test('authored inline rows, classes and lowered arms override modal defaults', async () => {
  const suite = await suiteBrowser();
  try {
    const page = await (await suite.newContext({ colorScheme: 'light' })).newPage();
    const rows = {
      bar: 'background-color: rgb(1, 2, 3); border: 3px solid rgb(4, 5, 6); box-shadow: rgb(7, 8, 9) 0px 0px 4px;',
      title: 'padding: 7px; grid-area: 2 / 2 / 3 / 3;',
      body: 'min-height: 31px; grid-area: 1 / 2 / 2 / 3;',
      button: 'margin: 5px; align-self: end; border-radius: 3px; border-color: rgb(10, 11, 12); grid-row: 2;',
    };
    const authored = (root, mode) => `${root} { ${Object.entries(rows).map(([part, row]) => `& .${mode}-${part} { ${row} }`).join('\n')} }`;
    const fixtures = [];
    for (const wrapped of [false, true]) for (const mode of ['unsaid', 'inline', 'class', 'arm']) {
      const id = `${mode}-${wrapped}`;
      const attrs = part => `id="${id}-${part}" ${mode === 'inline' ? `style="${rows[part]}"` : `class="${mode}-${part}"`}`;
      const header = `<header ${attrs('bar')}><h2 ${attrs('title')}>Title</h2><button data-button-style="bordered" ${attrs('button')}>Save</button></header>`;
      const body = `<div ${attrs('body')}>Body</div>`;
      fixtures.push(`<div navigationPresentation="modal">${wrapped ? `<div data-exact-list-item>${header}</div><div data-exact-list-item>${body}</div>` : header + body}</div>`);
    }
    // Put authored classes first so source order cannot conceal excess shell specificity.
    await page.setContent(`<style>${authored('#exact-root#exact-root', 'class')}${authored('#exact-root#exact-root#exact-root', 'arm')}</style>
      <style>${shell}</style><div id="exact-root" style="--exact-curve-reach:1">${fixtures.join('')}</div>`);
    const got = await page.evaluate(() => {
      const css = (id, part) => getComputedStyle(document.getElementById(`${id}-${part}`));
      return Object.fromEntries(['unsaid', 'inline', 'class', 'arm'].flatMap(mode => [false, true].map(wrapped => {
        const id = `${mode}-${wrapped}`, bar = css(id, 'bar'), title = css(id, 'title'), body = css(id, 'body'), button = css(id, 'button');
        return [id, {
          bar: [bar.display, bar.backgroundColor, bar.borderTopWidth, bar.boxShadow],
          title: [title.paddingTop, title.gridArea],
          body: [body.minHeight, body.gridArea],
          button: [button.marginTop, button.alignSelf, button.borderTopLeftRadius, button.borderTopColor, button.gridRow],
        }];
      })));
    });
    for (const wrapped of [false, true]) {
      expect(got[`unsaid-${wrapped}`]).toEqual({
        bar: ['grid', 'rgba(0, 0, 0, 0)', '0px', 'none'],
        title: ['24px', '1 / 1 / 2 / -1'], body: ['0px', '2 / 1 / 3 / -1'],
        button: ['12px', 'center', '8px', 'color(srgb 0 0 0 / 0.45)', '3'],
      });
      for (const mode of ['inline', 'class', 'arm']) expect([mode, wrapped, got[`${mode}-${wrapped}`]]).toEqual([mode, wrapped, {
        bar: ['grid', 'rgb(1, 2, 3)', '3px', 'rgb(7, 8, 9) 0px 0px 4px 0px'],
        title: ['7px', '2 / 2 / 3 / 3'], body: ['31px', '1 / 2 / 2 / 3'],
        button: ['5px', 'end', '3px', 'rgb(10, 11, 12)', '2'],
      }]);
    }
  } finally { await suite.close(); }
});
