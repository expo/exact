import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { copyFileSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createDocument } from './dom.js';
import { webCompiler } from './module.mjs';
import { renderer } from './render.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { standIns } from '../web/tests/stand-ins.mjs';

// The render's DOM orders nodes as DOM §4.4 does (midi.js sorts its listeners by it).
test('the render DOM compares document positions, across trees one way each', () => {
  const doc = createDocument(), root = doc.getElementById('exact-root');
  const a = doc.createElement('div'), b = doc.createElement('div');
  root.append(a, b);
  a.textContent = 'text';
  expect([a.compareDocumentPosition(b), b.compareDocumentPosition(a)]).toEqual([4, 2]);
  expect([a.compareDocumentPosition(a.firstChild), a.firstChild.compareDocumentPosition(a)]).toEqual([16 | 4, 8 | 2]);
  const loose = doc.createElement('p'), other = createDocument().getElementById('exact-root');
  for (const [x, y] of [[a, loose], [loose, other], [a, other]]) {
    const there = x.compareDocumentPosition(y), back = y.compareDocumentPosition(x);
    expect([there & 1, there & 32, back & 1, back & 32]).toEqual([1, 32, 1, 32]);
    expect([there & 6, back & 6].sort()).toEqual([2, 4]);
    expect(x.compareDocumentPosition(y)).toBe(there);
  }
});

test('the render DOM reflects link relations and image alternatives in both directions', () => {
  const doc = createDocument();
  for (const [tag, prop, value] of [['a', 'rel', 'external noopener'], ['img', 'alt', 'A person']]) {
    const el = doc.createElement(tag);
    expect(el[prop]).toBe('');
    el[prop] = value;
    expect(el.getAttribute(prop)).toBe(value);
    el.setAttribute(prop, 'authored');
    expect(el[prop]).toBe('authored');
    el.removeAttribute(prop);
    expect(el[prop]).toBe('');
    el[prop] = '';
    expect(el.getAttribute(prop)).toBe('');
  }
});

test('the render DOM selects options by their value or label', () => {
  const doc = createDocument(), select = doc.createElement('select');
  const a = doc.createElement('option'), b = doc.createElement('option'), explicit = doc.createElement('option');
  a.textContent = 'a'; explicit.textContent = 'b c';
  const run = doc.createElement('span'); run.textContent = ' c ';
  b.append(' \tb\n', run);
  explicit.setAttribute('value', 'other');
  select.append(a, doc.createComment(), b, explicit);
  select.value = 'b c';
  expect(select.options.map(o => o.hasAttribute('selected'))).toEqual([false, true, false]);
  select.value = 'other';
  expect(select.options.map(o => o.hasAttribute('selected'))).toEqual([false, false, true]);
  explicit.setAttribute('value', '');
  select.value = '';
  expect(select.options.map(o => o.hasAttribute('selected'))).toEqual([false, false, true]);
  select.value = 'a';
  expect(select.options.map(o => o.hasAttribute('selected'))).toEqual([true, false, false]);
});

test.each([
  ['duplicate', 'b', [['First', 'b'], ['b']], [true, false]],
  ['nonbreaking space', 'b\u00a0c', [['a'], ['b\u00a0c']], [false, true]],
  ['HTML whitespace', 'b c', [['a'], ['\t b\n\r\f c ']], [false, true]],
  ['other whitespace', '\u00a0b\u2003c\v', [['a'], ['\u00a0b\u2003c\v']], [false, true]],
])('the render DOM selects the first matching option: %s', (_, value, options, selected) => {
  const doc = createDocument(), select = doc.createElement('select');
  for (const [label, own] of options) {
    const option = doc.createElement('option'); option.textContent = label;
    if (own !== undefined) option.setAttribute('value', own);
    select.append(option);
  }
  select.value = value;
  expect(select.options.map(o => o.hasAttribute('selected'))).toEqual(selected);
  expect(select.value).toBe(value);
});

async function withRenderRuntime(run) {
  const dir = mkdtempSync(resolve(import.meta.dir, '../../target/select-runtime-test-'));
  const previous = globalThis.document, doc = createDocument();
  globalThis.document = doc;
  try {
    const real = ['rt.js', 'clock.js', 'resources.js', 'tree.js', 'events.js', 'pieces.js', 'mount.js', 'strings.js', 'elements.js', 'shape.js', 'overlay.js', 'held.js', 'roster.js', 'budget.js'];
    for (const name of real) copyFileSync(resolve(import.meta.dir, name), resolve(dir, name));
    for (const [name, source] of standIns(real)) writeFileSync(resolve(dir, name), source);
    await run(doc, await import(resolve(dir, 'rt.js')));
  } finally {
    globalThis.document = previous;
    rmSync(dir, { recursive: true, force: true });
  }
}

test.each([['', false], ['b', false], ['', true], ['b', true]])(
  'the runtime selects %j and rechecks changed option labels (explicit value: %j)',
  async (value, explicit) => withRenderRuntime((doc, { P, settled }) => {
    const select = doc.createElement('select'); doc.root.append(select);
    P(select, 'value', () => value);
    const a = doc.createElement('option'), b = doc.createElement('option');
    a.textContent = 'a'; b.textContent = value;
    if (explicit) P(b, 'value', () => value);
    select.append(a, b); settled(true);
    expect(select.options.map(o => o.hasAttribute('selected'))).toEqual([false, true]);
    P(b, 'text', () => 'c'); settled(true);
    expect(select.options.map(o => o.hasAttribute('selected'))).toEqual([false, explicit]);
    expect(select.value).toBe(explicit ? value : 'a');
    expect(b.getAttribute('value')).toBe(explicit ? value : null);
    P(a, 'text', () => value); settled(true);
    expect(select.value).toBe(value);
    P(b, 'value', () => ''); P(select, 'value', () => '');
    a.textContent = 'a'; settled(true);
    expect(select.options.map(o => o.hasAttribute('selected'))).toEqual([false, true]);
    expect(b.value).toBe('');
    P(b, 'value', () => null); settled(true);
    expect([b.getAttribute('value'), b.value]).toEqual([null, 'c']);
  }),
);

test.each([false, true])('the runtime marks the first option selected in a multiple select (explicit value: %j)',
  async explicit => withRenderRuntime((doc, { P, settled }) => {
    const select = doc.createElement('select'); doc.root.append(select);
    select.setAttribute('multiple', ''); P(select, 'value', () => 'a');
    for (const label of ['a', 'b']) {
      const option = doc.createElement('option'); option.textContent = label;
      if (explicit) P(option, 'value', () => label);
      select.append(option);
    }
    settled(true);
    expect(select.options.map(o => o.hasAttribute('selected'))).toEqual([true, false]);
    select.value = 'b'; settled(true);
    expect(select.value).toBe('b');
    P(select, 'value', () => 'a'); settled(true);
    expect(select.options.map(o => o.hasAttribute('selected'))).toEqual([true, false]);
  }),
);

test.each(['js', 'rust'])('%s-rendered selects show label-only options before and after adoption', async how => {
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/select-document-test-'));
  let server, browser;
  try {
    const source = resolve(dir, 'app.contract'), dist = resolve(dir, 'dist');
    writeFileSync(source, `routes nav
  home "/" render=build

component App
  state pick = "b"
  state label = "b"
  action choose(v: string)
    pick = v
  action relabel
    label = "c"
  view
    column
      select value=pick change=choose testId="plain"
        option "a"
        option "b"
      select value=pick change=choose testId="valued"
        option "b" value="a"
        option "B" value="b"
      select value="b c" testId="spaces"
        option "a"
        option
          text "  b\\t"
          text " c\\n "
      select value=pick testId="empty"
        option "a"
        option "b" value=""
        option "b"
      select value="b" testId="duplicate"
        option "First" value="b"
        option "b"
      select value="" testId="empty-label"
        option "a"
        option ""
      select value="" testId="empty-value"
        option "a"
        option "B" value=""
      select value="b\u00a0c" testId="nbsp"
        option "a"
        option "b\u00a0c"
      select multiple=true value="a" testId="multiple-label"
        option "a"
        option "b"
      select multiple=true value="a" testId="multiple-value"
        option "A" value="a"
        option "B" value="b"
      select value=pick testId="bound"
        option "a"
        option label value=pick testId="bound-option"
      select value="b" text-transform="uppercase" testId="uppercase"
        option "a"
        option "b"
      select value="bc" testId="mixed"
        option "a"
        option "b"
          text "c"
      button "Keep selection" press=choose("b") testId="wake"
      button "Change label" press=relabel testId="relabel"
      text pick testId="chosen"
`);
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'caltrain', '--contract', source, '--out', dist, '--render', how], {
      cwd: root, env: process.env, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
      const path = new URL(req.url).pathname;
      return new Response(Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1))));
    } });
    browser = await suiteBrowser();
    for (const javaScriptEnabled of [false, true]) {
      const page = await browser.newPage({ javaScriptEnabled });
      await page.goto(server.url.href);
      const selections = () => page.evaluate(() => [...document.querySelectorAll('select')].map(e => [e.value, e.selectedIndex]));
      const expected = [['b', 1], ['b', 1], ['b c', 1], ['b', 2], ['b', 0], ['', 1], ['', 1], ['b\u00a0c', 1],
        ['a', 0], ['a', 0], ['b', 1], ['b', 1], ['bc', 1]];
      expect(await selections()).toEqual(expected);
      if (javaScriptEnabled) {
        expect(await page.evaluate(() => !!globalThis.exact?.ready)).toBe(false);
        await page.evaluate(() => { globalThis.renderedSelect = document.querySelector('[data-testid="plain"]'); });
        await page.getByTestId('wake').click();
        await page.waitForFunction(() => globalThis.exact?.journal?.some(line => line.endsWith('adopted the document')));
        expect(await page.evaluate(() => renderedSelect === document.querySelector('[data-testid="plain"]'))).toBe(true);
        expect(await selections()).toEqual(expected);
        await page.getByTestId('relabel').click();
        await page.waitForFunction(() => document.querySelector('[data-testid="bound-option"]').textContent === 'c');
        expect(await page.getByTestId('bound-option').getAttribute('value')).toBe('b');
        expect(await selections()).toEqual(expected);
        await page.getByTestId('plain').selectOption('a');
        await page.waitForFunction(() => document.querySelector('[data-testid="chosen"]').textContent === 'a');
        expect(await page.getByTestId('valued').inputValue()).toBe('a');
      }
      await page.close();
    }
  } finally {
    await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true });
  }
}, 180000);

test('Bun-rendered bound links and symbols keep their attributes through adoption', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/dom-render-test-'));
  let server, browser;
  try {
    const dist = resolve(dir, 'dist');
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'DOM Render Test', app: { id: 'test.dom-render', name: 'DOM Render Test' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), `component App
  state url = "https://example.com/a"
  state role = "person"
  action local
    url = "/about"
  action external
    url = "https://example.com/a"
  view
    column
      link href=url testId="lnk"
        text "site"
      link href=url target="_self" testId="authored"
        text "same tab"
      link href=url rel="noreferrer" testId="kept"
        text "kept"
      image \`symbol:\${role}\` testId="sym"
      image \`symbol:\${role}\` alt="A person" testId="named"
      button press=local testId="local"
        text "make it local"
      button press=external testId="external"
        text "make it external"
`);
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', dir, '--out', dist, '--render', 'js'], { cwd: root, env: process.env, stdout: 'pipe', stderr: 'pipe' });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    const render = renderer(dist);
    render.ahead = false;
    writeFileSync(resolve(dist, 'index.html'), (await render('/')).html);
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
      const path = new URL(req.url).pathname;
      return new Response(Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1))));
    } });
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    expect(await page.evaluate(() => !!globalThis.exact?.ready)).toBe(false);
    await page.evaluate(() => { globalThis.renderedLink = document.querySelector('[data-testid="lnk"]'); });
    const read = async () => ({
      attrs: await page.evaluate(() => {
        const el = id => document.querySelector(`[data-testid="${id}"]`);
        return {
          link: ['href', 'target', 'rel'].map(k => el('lnk').getAttribute(k)),
          authored: ['target', 'rel'].map(k => el('authored').getAttribute(k)),
          kept: el('kept').getAttribute('rel'),
          symbols: ['sym', 'named'].map(id => el(id).getAttribute('alt')),
        };
      }),
      images: await page.getByRole('img').count(),
    });
    const before = await read();
    await page.getByTestId('external').click();
    await page.waitForFunction(() => globalThis.exact?.journal?.some(line => line.endsWith('adopted the document')));
    expect(await page.evaluate(() => renderedLink === document.querySelector('[data-testid="lnk"]'))).toBe(true);
    const adopted = await read();
    await page.getByTestId('local').click();
    await page.waitForFunction(() => document.querySelector('[data-testid="lnk"]').getAttribute('href') === '/about');
    const local = await read();
    await page.getByTestId('external').click();
    await page.waitForFunction(() => document.querySelector('[data-testid="lnk"]').getAttribute('href') === 'https://example.com/a');
    const external = await read();
    // An authored `rel` keeps its tokens, and the ones a new browsing context needs join it (LLP 1141.002 D1).
    const expected = (link, kept) => ({ attrs: { link, authored: ['_self', null], kept, symbols: ['', 'A person'] }, images: 1 });
    const out = expected(['https://example.com/a', '_blank', 'external noopener'], 'noreferrer external noopener');
    expect({ before, adopted, local, external }).toEqual({ before: out, adopted: out, local: expected(['/about', null, null], 'noreferrer'), external: out });
  } finally {
    await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true });
  }
}, 60000);

// An attribute's `<` and `>` are written as references (HTML, "Serializing HTML fragments", attribute mode), as
// document.rs writes them, so the hero image scan, which ends a tag at its first `>` (render.mjs `heroImage`), reads
// every attribute of an image and of a covered route: the page preloads the image it shows first.
test.each(['js', 'rust'])('%s-rendered pages escape a > in an attribute and preload the image shown first', async how => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/dom-escape-test-'));
  try {
    const source = resolve(dir, 'app.contract'), dist = resolve(dir, 'dist');
    writeFileSync(source, `routes nav
  home "/" render=build
    detail "/detail" render=build

component Pages
  action follow(url: string)
    nav = go(nav, url)
  view
    main navigationKey=\`\${top(nav).id}\` navigationBack="back" navigate=follow
      for e in stack(nav) key=e.id
        column navigationKey=\`\${e.id}\` position="absolute" inset=0 aria-label="1 > 0"
          if e.name == "home"
            image "https://example.com/home.png" width=96 alt="home"
          if e.name == "detail"
            image "https://example.com/one.png" width=10 alt="1 > 0"
            image "https://example.com/two.png" width=10 alt="second"
`);
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'caltrain', '--contract', source, '--out', dist, '--render', how], {
      cwd: root, env: process.env, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    const html = path => readFileSync(resolve(dist, path, 'index.html'), 'utf8');
    const preloads = path => html(path).match(/<link rel="preload" as="image"[^>]*>/g);
    expect([preloads(''), preloads('detail')]).toEqual([['<link rel="preload" as="image" href="https://example.com/home.png">'],
      ['<link rel="preload" as="image" href="https://example.com/one.png">']]);
    expect([html('detail').includes('alt="1 &gt; 0"'), html('detail').includes('="1 > 0"')]).toEqual([true, false]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 180000);

test('a generated concatenated row key reuses unchanged rows on an item update', async () => {
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/each-key-test-'));
  const previous = globalThis.document, render = globalThis.__exactRender;
  globalThis.__exactRender = true;
  globalThis.document = createDocument();
  try {
    const source = resolve(dir, 'app.contract');
    writeFileSync(source, `shape Item
  id: string
  value: number
component App
  state items = range(1000).map(n => Item(id=\`\${n}\`, value=n))
  view
    column
      for item in items key="row-" + item.id
        text \`\${item.value}\`
`);
    const compiler = webCompiler();
    expect(compiler.cmd).not.toBe('cargo');
    const built = spawnSync(compiler.cmd, [...compiler.pre, 'js', source, '-o', dir], { cwd: root, encoding: 'utf8' });
    expect({ status: built.status, error: built.status ? built.stdout + built.stderr : '' }).toEqual({ status: 0, error: '' });
    const real = ['rt.js', 'clock.js', 'resources.js', 'tree.js', 'events.js', 'pieces.js', 'mount.js', 'strings.js', 'elements.js', 'shape.js', 'overlay.js', 'held.js', 'roster.js', 'budget.js', 'rows.js', 'router.js', 'route-pattern.js', 'schedule.js'];
    for (const name of real) copyFileSync(resolve(import.meta.dir, name), resolve(dir, name));
    for (const [name, source] of standIns(real)) writeFileSync(resolve(dir, name), source);
    const rows = resolve(dir, 'rows.js');
    writeFileSync(rows, readFileSync(rows, 'utf8').replace('export function inOrder(list) {',
      'export let orderingPasses = 0; export function inOrder(list) { orderingPasses++;'));
    writeFileSync(resolve(dir, 'counted-rt.js'), `export * from './rt.js';
import { each as realEach } from './rt.js';
export let keyEvaluations = 0;
export function each(p, list, key, row, pure) {
  return realEach(p, list, (...args) => { keyEvaluations++; return key(...args); }, row, pure);
}`);
    const program = resolve(dir, 'app.js');
    writeFileSync(program, readFileSync(program, 'utf8').replace('from"./rt.js"', 'from"./counted-rt.js"'));
    const rt = await import(resolve(dir, 'rt.js'));
    const counts = await import(resolve(dir, 'counted-rt.js'));
    const ordering = await import(rows);
    const [slots] = (await import(program)).default();
    expect(counts.keyEvaluations).toBe(1000);
    const passes = ordering.orderingPasses, keys = counts.keyEvaluations;
    const items = slots[0], next = items().slice();
    next[500] = ['500', -1];
    expect(rt.commit(() => rt.W(items, next))).not.toBe(false);
    expect(counts.keyEvaluations - keys).toBe(1);
    expect(ordering.orderingPasses - passes).toBe(0);
    expect(globalThis.document.getElementById('exact-root').textContent.split('-1')).toHaveLength(2);
  } finally {
    globalThis.document = previous;
    if (render === undefined) delete globalThis.__exactRender; else globalThis.__exactRender = render;
    rmSync(dir, { recursive: true, force: true });
  }
});
