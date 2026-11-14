import { expect, test } from 'bun:test';
import { existsSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';

// The runtime's modules in Chrome, over an empty `#exact-root`.
const root = resolve(import.meta.dir, '../..');
async function fixture(run) {
  let browser, server;
  try {
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const path = new URL(request.url).pathname;
      if (path === '/favicon.ico') return new Response(null, { status: 204 });
      if (path === '/app.css') return new Response('');
      if (path === '/') return new Response('<link rel="stylesheet" href="/app.css"><div id="exact-root"></div>', { headers: { 'content-type': 'text/html' } });
      if (path.endsWith('/names.js')) return new Response('export const sourceTypes={};', { headers: { 'content-type': 'text/javascript' } });
      const local = path.startsWith('/host/') ? resolve(root, '.' + path) : resolve(root, 'host', path.slice(1));
      const file = existsSync(local) ? local : resolve(root, 'host/web-js', path.split('/').pop());
      const actual = existsSync(file) ? file : resolve(root, 'host/web', path.split('/').pop());
      return new Response(hostFixtureFile(actual), { headers: { 'content-type': path.endsWith('.css') ? 'text/css' : 'text/javascript' } });
    } });
    browser = await suiteBrowser('chromium');
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await run(page);
  } finally { await browser?.close(); server?.stop(true); }
}

test.each([false, true])('keyed moves preserve row identity and handlers (several roots: %s)', multi => fixture(async page => {
  const result = await page.evaluate(async multi => {
    const rt = await import('/host/web-js/rt.js');
    rt.clock.agent = true;
    const items = rt.sig([1, 2, 3, 4]), picked = [];
    rt.mount(() => rt.each(document.getElementById('exact-root'), items, item => item(), (p, item) => {
      const button = rt.h(p, 'button', 0, { 'data-row': String(item()) }, String(item()));
      button.addEventListener('click', () => picked.push(item()));
      if (multi) rt.h(p, 'span', 0, { 'data-tail': String(item()) }, 'tail');
    }, 1));
    const original = [...document.querySelectorAll('button')];
    const moves = [];
    for (const next of [[4, 1, 2, 3], [3, 2, 1, 4], [1, 3, 4, 2]]) {
      rt.commit(() => rt.W(items, next));
      const buttons = [...document.querySelectorAll('button')];
      moves.push(buttons.map(e => e === original[Number(e.dataset.row) - 1]));
      for (const e of buttons) e.click();
    }
    return { moves, picked, order: [...document.querySelectorAll('[data-row], [data-tail]')].map(e => e.dataset.row ?? 'tail' + e.dataset.tail) };
  }, multi);
  expect(result.moves.flat().every(Boolean)).toBe(true);
  expect(result.picked).toEqual([4, 1, 2, 3, 3, 2, 1, 4, 1, 3, 4, 2]);
  expect(result.order).toEqual([1, 3, 4, 2].flatMap(n => multi ? [String(n), 'tail' + n] : [String(n)]));
}));

test('a style binding writes the live style once, including its first value and refusals', () => fixture(async page => {
  const result = await page.evaluate(async () => {
    const { css } = await import('/host/web-js/rt.js');
    const el = document.createElement('span'), writes = [];
    const set = el.style.setProperty, remove = el.style.removeProperty;
    el.style.setProperty = function(...args) { writes.push(args); return set.apply(this, args); };
    el.style.removeProperty = function(...args) { writes.push(args); return remove.apply(this, args); };
    const colors = [], counts = [];
    for (const value of ['rgb(12, 34, 56)', 'not-a-color', 'rgb(23, 45, 67)', 'rgb(23, 45, 67)', '', null]) {
      writes.length = 0;
      css(el, 'color', '', value);
      colors.push(el.style.color); counts.push(writes.length);
    }
    el.style.color = 'rgb(34, 56, 78)'; writes.length = 0;
    css(el, 'color', '', 'rgb(34, 56, 78)', true);
    const adopted = writes.length;
    css(el, '--empty', '', '');
    return { colors, counts, adopted, empty: el.style.getPropertyValue('--empty') };
  });
  expect(result.colors).toEqual(['rgb(12, 34, 56)', 'unset', 'rgb(23, 45, 67)', 'rgb(23, 45, 67)', 'unset', '']);
  expect(result.counts).toEqual([1, 1, 1, 0, 1, 1]);
  expect(result.adopted).toBe(0);
  expect(result.empty).toBe('');
}));

// The mount looks at the document once; after that a commit looks only while a field built or bound may be waiting.
test('a commit looks for autofocus fields only while one may be waiting', () => fixture(async page => {
  const result = await page.evaluate(async () => {
    const rt = await import('/host/web-js/rt.js');
    const root = document.getElementById('exact-root'), query = root.querySelectorAll;
    const scans = [];
    let n = 0;
    root.querySelectorAll = function(selector) {
      if (selector.includes('[autofocus]')) n++;
      return query.call(this, selector);
    };
    const step = (name, f) => { const was = n; f(); scans.push([name, n - was]); };
    const label = rt.sig('first'), enabled = rt.sig(false);
    let first, second;
    step('mount', () => rt.mount(() => rt.P(rt.h(root, 'span', 0, 0, 0), 'text', label)));
    step('text', () => rt.commit(() => rt.W(label, 'second')));
    step('literal', () => { first = rt.h(root, 'input', 0, { autofocus: 'true' }, 0); rt.commit(() => {}); });
    const literal = document.activeElement === first;
    first.blur();
    step('after', () => rt.commit(() => rt.W(label, 'third')));
    step('quiet', () => rt.commit(() => rt.W(label, 'fourth')));
    step('bound', () => { second = rt.h(root, 'input', 0, 0, 0); rt.P(second, 'autofocus', enabled); rt.commit(() => rt.W(enabled, true)); });
    return { scans, literal, bound: document.activeElement === second };
  });
  expect(result.literal).toBe(true);
  expect(result.bound).toBe(true);
  // `after` looks once more and finds every field offered.
  expect(result.scans).toEqual([['mount', 1], ['text', 0], ['literal', 1], ['after', 1], ['quiet', 0], ['bound', 1]]);
}));

test('autofocus candidates follow DOM order after a move and are offered only once', () => fixture(async page => {
  const result = await page.evaluate(async () => {
    const rt = await import('/host/web-js/rt.js'), root = document.getElementById('exact-root');
    const back = rt.h(root, 'input', 0, { autofocus: 'true' }, 0);
    const front = rt.h(root, 'input', 0, { autofocus: 'true' }, 0);
    root.insertBefore(front, back);
    rt.commit(() => {});
    const first = document.activeElement === front;
    front.blur();
    rt.commit(() => {});
    const second = document.activeElement === back;
    back.blur();
    rt.commit(() => {});
    return { first, second, done: document.activeElement === document.body };
  });
  expect(result).toEqual({ first: true, second: true, done: true });
}));

test.each([false, true])('deferred adoption preserves autofocus suppression (carried reload: %s)', carried => fixture(async page => {
  const result = await page.evaluate(async carried => {
    const rt = await import('/host/web-js/rt.js');
    const { prepareDev } = await import('/host/web-js/checkpoint.js');
    const root = document.getElementById('exact-root');
    root.innerHTML = '<div></div>'.repeat(9999) + '<div><input autofocus style="display:none"></div>';
    const field = root.lastElementChild.firstChild;
    if (carried) {
      const cp = document.createElement('script');
      cp.type = 'application/vnd.exact.dev-checkpoint';
      cp.textContent = '{}';
      document.head.append(cp);
    }
    rt.checkpoint().kept = new Map();
    const restore = prepareDev(), now = performance.now.bind(performance);
    let elapsed = now(), adopted = 0;
    // Exhaust the adoption budget after the first row without depending on machine speed.
    performance.now = () => elapsed += 100;
    try {
      rt.mount(() => rt.each(root, () => Array.from({ length: 10000 }, (_, i) => i), item => item(), (p, item) => {
        adopted++;
        const row = rt.h(p, 'div', 0, 0, 0);
        if (item() === 9999) rt.h(row, 'input', 0, { autofocus: 'true' }, 0);
      }, 1));
    } finally { performance.now = now; }
    restore();
    const initial = adopted;
    field.dispatchEvent(new Event('input', { bubbles: true }));
    const deferred = adopted;
    field.style.display = '';
    rt.commit(() => {});
    return { initial, deferred, identity: root.lastElementChild.firstChild === field, focused: document.activeElement === field };
  }, carried);
  expect(result).toEqual({ initial: 1, deferred: 2, identity: true, focused: !carried });
}));

// A row waiting for its adoption slice has run no `h()`: its rendered field is a candidate from the mount's offer on.
test.each(['earlier', 'parsed'])('a rendered field awaiting adoption is a candidate at mount (%s)', mode => fixture(async page => {
  const result = await page.evaluate(async mode => {
    const rt = await import('/host/web-js/rt.js');
    const root = document.getElementById('exact-root');
    root.innerHTML = '<div>' + '<div></div>'.repeat(5) + '<div><input autofocus></div>' + '<div></div>'.repeat(4) + '</div><input autofocus>';
    const waiting = root.firstChild.children[5].firstChild, after = root.lastChild;
    // As the browser's own autofocus does while it parses the page.
    if (mode === 'parsed') waiting.focus();
    rt.checkpoint().kept = new Map();
    const now = performance.now.bind(performance);
    let elapsed = now();
    performance.now = () => elapsed += 100;
    try {
      rt.mount(() => {
        const list = rt.h(root, 'div', 0, 0, 0);
        rt.each(list, () => Array.from({ length: 10 }, (_, i) => i), item => item(), (p, item) => {
          const row = rt.h(p, 'div', 0, 0, 0);
          if (item() === 5) rt.h(row, 'input', 0, { autofocus: 'true' }, 0);
        }, 1);
        rt.h(root, 'input', 0, { autofocus: 'true' }, 0);
      });
    } finally { performance.now = now; }
    const atMount = document.activeElement === waiting ? 'waiting' : document.activeElement === after ? 'after' : 'other';
    if (mode === 'parsed') waiting.blur();
    // The slice adopts the row, then a commit runs.
    waiting.dispatchEvent(new Event('input', { bubbles: true }));
    rt.commit(() => {});
    const afterCommit = document.activeElement === waiting ? 'waiting' : document.activeElement === after ? 'after' : 'body';
    return { atMount, afterCommit, identity: root.firstChild.children[5].firstChild === waiting };
  }, mode);
  // Offered at mount, the waiting field takes the focus (or keeps the parser's); the field after it waits its turn.
  expect(result).toEqual({ atMount: 'waiting', afterCommit: mode === 'parsed' ? 'after' : 'waiting', identity: true });
}));
