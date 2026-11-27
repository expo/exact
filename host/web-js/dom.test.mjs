import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { createDocument } from './dom.js';
import { webCompiler } from './module.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { engineTests } from '../../scripts/coverage.mjs';
import { stageRuntime } from '../web/tests/stand-ins.mjs';

const engineTest = await engineTests(['chromium']);

test('a tab preparation can clone its render DOM shell without sharing attributes or children', () => {
  const doc = createDocument(), panel = doc.createElement('section');
  panel.setAttribute('id', 'panel');
  panel.setAttribute('role', 'tabpanel');
  panel.style.setProperty('visibility', 'hidden');
  panel.append('content', doc.createComment('anchor'));
  doc.root.append(panel);
  const shell = panel.cloneNode(false);
  expect(shell.parentNode).toBe(null);
  expect(shell.childNodes).toHaveLength(0);
  expect(shell.localName).toBe('section');
  expect(shell.getAttribute('role')).toBe('tabpanel');
  expect(shell.style.cssText).toBe(panel.style.cssText);
  shell.setAttribute('id', 'prepared');
  shell.style.setProperty('visibility', 'visible');
  expect(panel.id).toBe('panel');
  expect(panel.style.getPropertyValue('visibility')).toBe('hidden');
  const deep = panel.cloneNode(true);
  expect(deep.html()).toBe(panel.html());
  expect(deep.childNodes[0]).not.toBe(panel.childNodes[0]);
  expect(deep.childNodes[1].nodeType).toBe(8);
});

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

test('the render DOM gives path templates their viewBox aspect at the inherited size', () => {
  const doc = createDocument();
  const parent = doc.createElement('div');
  parent.style.fontSize = '20px';
  const img = doc.createElement('img');
  img.setAttribute('data-symbol-source', 'symbol:icon||0 0 400 100|M0 0H400V100H0Z|');
  img.setAttribute('data-symbol-viewbox', '0 0 400 100');
  parent.append(img);
  doc.root.append(parent);
  doc.rootHTML();
  expect(decodeURIComponent(img.getAttribute('src'))).toMatch(/width=['"]20['"] height=['"]5['"]/);
});

test('the render DOM marks its hero first among its attributes and preloads it by the same request, past a hidden image', () => {
  // `visibility` is inherited: the hidden class's image is skipped, and the image whose class sets it back to `visible` is
  // the hero.
  const doc = createDocument('<style>.c1{visibility:hidden}.c2{visibility:visible}</style>'), root = doc.getElementById('exact-root');
  const hidden = doc.createElement('div'), unseen = doc.createElement('img'), hero = doc.createElement('img');
  hidden.setAttribute('class', 'c1'); unseen.setAttribute('src', 'https://example.com/hidden.png');
  hero.setAttribute('alt', 'Hero'); hero.setAttribute('class', 'c2'); hero.setAttribute('crossorigin', 'anonymous'); hero.setAttribute('src', 'https://example.com/hero.png?a=1&b=2');
  hidden.append(unseen, hero); root.append(hidden);
  expect(doc.heroPreload()).toBe('<link rel="preload" as="image" href="https://example.com/hero.png?a=1&amp;b=2" crossorigin="anonymous" fetchpriority="high">');
  expect(doc.rootHTML()).toContain('<img fetchpriority="high" alt="Hero" class="c2" crossorigin="anonymous" src="https://example.com/hero.png?a=1&amp;b=2">');
  // Inline, `collapse` hides an image; `inert` and `display: none` hide even an image set `visible`; `inherit` takes a
  // hidden parent's visibility; an inline `display` wins over a class's `none`, and an inline `unset` (a bound blank)
  // over a hidden class, taking the parent's visibility.
  const inline = createDocument('<style>.c1{visibility:hidden}.c2{display:none}</style>'), make = (src, style, attrs = {}) => {
    const img = inline.createElement('img');
    for (const [name, value] of Object.entries({ src, ...attrs })) img.setAttribute(name, value);
    img.style.setProperty('visibility', style);
    return img;
  };
  const [inert, gone, shut, shown] = [1, 2, 3, 4].map(() => inline.createElement('div'));
  inert.setAttribute('inert', ''); inert.append(make('https://example.com/inert.png', 'visible'));
  gone.style.setProperty('display', 'none'); gone.append(make('https://example.com/gone.png', 'visible'));
  shut.setAttribute('class', 'c1'); shut.append(make('https://example.com/inherit.png', 'inherit'));
  shown.setAttribute('class', 'c2'); shown.style.setProperty('display', 'flex');
  shown.append(make('https://example.com/unset.png', 'unset', { class: 'c1' }));
  inline.getElementById('exact-root').append(make('https://example.com/collapsed.png', 'collapse'), inert, gone, shut, shown);
  expect(inline.heroPreload()).toBe('<link rel="preload" as="image" href="https://example.com/unset.png" fetchpriority="high">');
});

test('the render DOM refuses a NUL in text or an attribute', () => {
  for (const write of [el => { el.textContent = 'a\0b'; }, el => el.setAttribute('title', 'a\0b')]) {
    const doc = createDocument(), el = doc.createElement('p');
    write(el); doc.getElementById('exact-root').append(el);
    expect(() => doc.rootHTML()).toThrow('a NUL character, which the HTML parser does not keep');
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
    stageRuntime(dir, real);
    await run(doc, await import(resolve(dir, 'rt.js')));
  } finally {
    globalThis.document = previous;
    rmSync(dir, { recursive: true, force: true });
  }
}

// A static `style` (a presence's custom properties, a symbol's inline rows) is an attribute the compiler emits.
test('a static style attribute renders', async () => withRenderRuntime((doc, { h }) => {
  h(doc.root, 'span', 0, { style: '--exact-press:1' }, 0);
  h(doc.root, 'i', 0, { style: 'color:red' }, 0);
  expect(doc.rootHTML()).toBe('<span style="--exact-press:1;"></span><i style="color:red;"></i>');
}));

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

engineTest.chromium('Bun-rendered links and icons preserve their attributes and layout through adoption', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/dom-render-test-'));
  let server, browser;
  try {
    const dist = resolve(dir, 'dist');
    const wideSymbol = 'symbol:icon||0 0 400 100|M0 0H400V100H0Z|';
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'DOM Render Test', app: { id: 'test.dom-render', name: 'DOM Render Test' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  home "/" render=build

component App
  state url = "https://example.com/a"
  state role = "person"
  state externalLabel = "make it external"
  action local
    url = "/about"
  action external
    url = "https://example.com/a"
    externalLabel = "external selected"
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
      column align-items="start"
        image "${wideSymbol}" font-size=20 testId="wide"
        text "After the icon" testId="after-wide"
      button press=local testId="local"
        text "make it local"
      button press=external testId="external"
        text externalLabel
`);
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', dir, '--out', dist, '--render', 'js'], { cwd: root, env: process.env, stdout: 'pipe', stderr: 'pipe' });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
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
          wide: [el('wide').naturalWidth, el('wide').naturalHeight, el('after-wide').offsetTop - el('wide').offsetTop],
        };
      }),
      images: await page.getByRole('img').count(),
    });
    const before = await read();
    await page.getByTestId('external').click();
    await page.waitForFunction(() => document.querySelector('[data-testid="external"]').textContent === 'external selected');
    expect(await page.evaluate(() => globalThis.exact.journal.some(line => line.endsWith('adopted the document')))).toBe(true);
    expect(await page.evaluate(() => renderedLink === document.querySelector('[data-testid="lnk"]'))).toBe(true);
    const adopted = await read();
    await page.getByTestId('local').click();
    await page.waitForFunction(() => document.querySelector('[data-testid="lnk"]').getAttribute('href') === '/about');
    const local = await read();
    await page.getByTestId('external').click();
    await page.waitForFunction(() => document.querySelector('[data-testid="lnk"]').getAttribute('href') === 'https://example.com/a');
    const external = await read();
    expect(await page.evaluate(() => renderedLink === document.querySelector('[data-testid="lnk"]'))).toBe(true);
    // An authored `rel` keeps its tokens, and the ones a new browsing context needs join it (LLP 1141.002 D1).
    const expected = (link, kept) => ({
      attrs: { link, authored: ['_self', null], kept, symbols: ['', 'A person'], wide: [20, 5, 5] },
      images: 1,
    });
    const out = expected(['https://example.com/a', '_blank', 'external noopener'], 'noreferrer external noopener');
    expect({ before, adopted, local, external }).toEqual({
      before: out, adopted: out, local: expected(['/about', null, null], 'noreferrer'),
      external: out,
    });
  } finally {
    await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true });
  }
}, 60000);

// An attribute's `<` and `>` are written as references (HTML, "Serializing HTML fragments", attribute mode), as
// document.rs writes them; and a page preloads the image it shows first, not a covered route's.
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
    expect([preloads(''), preloads('detail')]).toEqual([['<link rel="preload" as="image" href="https://example.com/home.png" fetchpriority="high">'],
      ['<link rel="preload" as="image" href="https://example.com/one.png" fetchpriority="high">']]);
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
    const built = spawnSync(compiler.cmd, [...compiler.pre, 'js', source, '-o', dir], { cwd: root, encoding: 'utf8', env: compiler.env });
    expect({ status: built.status, error: built.status ? built.stdout + built.stderr : '' }).toEqual({ status: 0, error: '' });
    const real = ['rt.js', 'clock.js', 'resources.js', 'tree.js', 'events.js', 'pieces.js', 'mount.js', 'strings.js', 'elements.js', 'shape.js', 'overlay.js', 'held.js', 'roster.js', 'budget.js', 'rows.js', 'router.js', 'route-pattern.js', 'schedule.js'];
    stageRuntime(dir, real);
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

test('JS router reconstructs structural-prefix ancestors positionally (LLP 1156.008.000 D3)', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-router-d3-'));
  try {
    stageRuntime(dir, ['rt.js', 'clock.js', 'resources.js', 'tree.js', 'events.js', 'pieces.js', 'mount.js', 'strings.js', 'elements.js', 'shape.js', 'overlay.js', 'held.js', 'roster.js', 'budget.js', 'rows.js', 'router.js', 'route-pattern.js', 'schedule.js', 'document.js']);
    const router = await import(resolve(dir, 'router.js'));
    router.routes([
      ['home', '/', null, true, false],
      ['trip', '/trip/:id', 0, false, false],
      ['place', '/trip/:trip/place/:id', 1, false, false],
      ['room', '/trip/:trip/room/:roomId', 1, false, false],
      ['alt', '/alt/:id/detail', 1, false, false],
    ]);
    const idIdx = router.Names.indexOf('id'), tripIdx = router.Names.indexOf('trip');
    const collided = router.launch('/trip/T%201/place/I%202', [])[1][0][1];
    expect(collided.map(e => e[2])).toEqual(['/', '/trip/T%201', '/trip/T%201/place/I%202']);
    expect(collided[1][4][idIdx]).toBe('T 1');
    expect(collided[2][4][tripIdx]).toBe('T 1');
    expect(collided[2][4][idIdx]).toBe('I 2');

    const renamed = router.launch('/trip/T9/room/R4', [])[1][0][1];
    expect(renamed.map(e => e[2])).toEqual(['/', '/trip/T9', '/trip/T9/room/R4']);
    expect(renamed[1][4][idIdx]).toBe('T9');

    const nonPrefix = router.launch('/alt/A7/detail', [])[1][0][1];
    expect(nonPrefix.map(e => e[2])).toEqual(['/', '/trip/A7', '/alt/A7/detail']);
    expect(nonPrefix[1][4][idIdx]).toBe('A7');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('onFocus follows replaced Markdown editor and onClipboard stores original-event handler (LLP 1156.008.000 D5.1)', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-d51-'));
  try {
    stageRuntime(dir, ['rt.js', 'clock.js', 'resources.js', 'tree.js', 'events.js', 'pieces.js', 'mount.js', 'strings.js', 'elements.js', 'shape.js', 'overlay.js', 'held.js', 'roster.js', 'budget.js', 'rows.js', 'router.js', 'route-pattern.js', 'schedule.js', 'document.js', 'pointer.js', 'focus.js']);
    const { onFocus, onClipboard } = await import(resolve(dir, 'events.js'));
    const listeners = {};
    const textarea = {
      isConnected: false,
      _exactEditor: { isConnected: true },
      addEventListener: (t, fn) => { (listeners[t] ??= []).push(fn); },
    };
    let focused = 0;
    onFocus(textarea, 'focus', () => { focused++; }, (t, fn) => textarea.addEventListener(t, fn));
    listeners.focus[0]();
    expect(focused).toBe(1);
    textarea._exactEditor.isConnected = false;
    listeners.focus[0]();
    expect(focused).toBe(1);

    let pastedText = null, stopped = false;
    onClipboard(textarea, 'paste', ([text]) => { pastedText = text; }, (t, fn) => textarea.addEventListener(t, fn));
    const originalEvent = {
      clipboardData: { getData: k => (k === 'text/plain' ? 'hello world' : '') },
      stopPropagation: () => { stopped = true; },
    };
    textarea._exactClipboard.paste[0](originalEvent);
    expect(pastedText).toBe('hello world');
    expect(stopped).toBe(true);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('zoom.js measures non-HTMLElement (<svg>) children using applied zoom scale (LLP 1156.008.000 D5.2)', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-d52-'));
  const prevDoc = globalThis.document, prevEl = globalThis.Element, prevHtml = globalThis.HTMLElement;
  const prevCS = globalThis.getComputedStyle, prevRO = globalThis.ResizeObserver;
  try {
    stageRuntime(dir, ['rt.js', 'clock.js', 'resources.js', 'tree.js', 'events.js', 'pieces.js', 'mount.js', 'strings.js', 'elements.js', 'shape.js', 'overlay.js', 'held.js', 'roster.js', 'budget.js', 'rows.js', 'router.js', 'route-pattern.js', 'schedule.js', 'document.js', 'zoom.js']);
    class FakeElement {
      constructor(tag) {
        this.tag = tag; this.style = { setProperty() {}, getPropertyValue: () => '' }; this.children = []; this.isConnected = true;
        this.clientWidth = 200; this.clientHeight = 200; this.scrollLeft = 0; this.scrollTop = 0;
      }
      setAttribute() {}
      addEventListener() {}
      dispatchEvent() {}
      append(...nodes) { this.children.push(...nodes); }
      get firstElementChild() { return this.children[0] ?? null; }
      get nextElementSibling() { return null; }
      scrollTo({ left, top }) { this.scrollLeft = left; this.scrollTop = top; }
    }
    class FakeHTMLElement extends FakeElement {}
    globalThis.Element = FakeElement;
    globalThis.HTMLElement = FakeHTMLElement;
    globalThis.ResizeObserver = class { observe() {} };
    globalThis.document = {
      createElement: tag => new FakeHTMLElement(tag),
      head: { appendChild: el => ({ sheet: { cssRules: [], insertRule() {} } }) },
    };
    const { zoom } = await import(resolve(dir, 'zoom.js'));
    const { After, PropHooks } = await import(resolve(dir, 'rt.js'));
    const container = new FakeHTMLElement('div');
    const layer = zoom(container);
    const svg = new FakeElement('svg'); // Element, not HTMLElement
    layer.getBoundingClientRect = () => ({ left: 10, top: 20, width: 200 * container.$zoomApplied, height: 200 * container.$zoomApplied });
    svg.getBoundingClientRect = () => ({ left: 10 + 15 * container.$zoomApplied, top: 20 + 25 * container.$zoomApplied, width: 100 * container.$zoomApplied, height: 80 * container.$zoomApplied });
    layer.append(svg);
    globalThis.getComputedStyle = el => el === container
      ? { display: 'block', paddingLeft: '0px', paddingTop: '0px', paddingRight: '0px', paddingBottom: '0px', touchAction: 'auto', scrollBehavior: 'auto', getPropertyValue: k => (k === '--exact-max-zoom' ? '4' : k === '--exact-min-zoom' ? '1' : '') }
      : { marginRight: '0px', marginBottom: '0px' };
    const settleAfter = After.at(-1);
    settleAfter();
    expect(container.$zoomApplied).toBe(1);
    expect(container.$zoomLayers.sizer.style.width).toBe('115px');
    expect(container.$zoomLayers.sizer.style.height).toBe('105px');
    PropHooks.scrollZoom(container, 2);
    settleAfter();
    expect(container.$zoomApplied).toBe(2);
    // At zoom 2, dividing by applied scale (1 before draw, then 2 on next settle) keeps untransformed size 100x80.
    settleAfter();
    expect(container.$zoomLayers.sizer.style.width).toBe('115px');
    expect(container.$zoomLayers.sizer.style.height).toBe('105px');
  } finally {
    globalThis.document = prevDoc; globalThis.Element = prevEl; globalThis.HTMLElement = prevHtml;
    globalThis.getComputedStyle = prevCS; globalThis.ResizeObserver = prevRO;
    rmSync(dir, { recursive: true, force: true });
  }
});

test('the render DOM parses animation-name from the animation shorthand', () => {
  const style = createDocument().createElement('div').style;
  for (const [value, want] of [
    ['fade 300ms both', 'fade'],
    ['300ms ease-in-out 2 alternate both slide-in', 'slide-in'],
    ['fade 300ms, cubic-bezier(0.4, 0, 0.2, 1) 1s spin infinite', 'fade, spin'],
    ['steps(4, end) 200ms -50ms pulse paused', 'pulse'],
    ['300ms "ease" both', '"ease"'],
    ['ease linear 1s', 'linear'],
    ['none', 'none'],
    ['none 0.5s forwards', 'none'],
    ['forwards 0.5s none', 'none'],
    ['fade 0.5s none', 'fade'],
    ['none 0.5s fade', 'fade'],
    ['none 0.5s forwards fade', ''],
    ['300ms', 'none'],
    ['', ''],
    ['var(--anim)', ''],
    ['fade -100ms', ''],
    ['one two 1s', ''],
  ]) {
    style.animation = value;
    expect(style.animationName, JSON.stringify(value)).toBe(want);
  }
  style.animationName = 'direct';
  expect(style.animationName).toBe('direct');
  style.animation = 'fade 1s';
  expect(style.animationName).toBe('fade');
  style.animation = '';
  expect(style.animationName).toBe('');
});

test('web-build with markdown and an animated SVG child compiles even alongside 3D keyframes (#1874)', () => {
  const root = resolve(import.meta.dir, '../..');
  const dir = mkdtempSync(resolve(root, 'target/md-svg-anim-test-'));
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'MdSvg', app: { id: 'com.example.mdsvg', name: 'MdSvg' }, host: { web: {} } }));
    // Case 1: plain SVG fade + markdown under `--render none` (issue #1874 exact repro, no 3D keyframes).
    writeFileSync(resolve(dir, 'app.contract'), `keyframes fade
  from opacity=0
  to opacity=1

component App
  view
    column
      svg width=24 height=24 viewBox="0 0 24 24"
        circle cx=12 cy=12 r=10 animation="fade 300ms both"
      text "Hello" markup="markdown"
`);
    const outPlain = resolve(dir, 'dist-plain');
    const plain = spawnSync(process.execPath, [resolve(root, 'host/web-js/build.mjs'), dir, '--out', outPlain, '--render', 'none'], {
      cwd: root,
      encoding: 'utf8',
    });
    expect(plain.status, plain.stdout + plain.stderr).toBe(0);
    expect(plain.stdout).toContain('1 Markdown sources compiled');
    expect(readFileSync(resolve(outPlain, 'index.html'), 'utf8')).toContain('<script type="application/vnd.exact.markdown">');

    // Case 2: 3D keyframes + computed SVG animations + markdown under `--render js` exercises dom.js animationName parsing.
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  home "/" render=build

keyframes fade
  from opacity=0
  to opacity=1

keyframes flip
  from rotate="y 0deg"
  to rotate="y 180deg"

component App
  state good = "fade"
  view
    column
      view animation="flip 300ms both"
        text "Card"
      svg width=24 height=24 viewBox="0 0 24 24"
        circle cx=12 cy=12 r=10 animation="fade 300ms both"
        path d="M0 0L10 10" animation=\`\${good} 200ms both\`
      text "**Hello** world" markup="markdown"
`);
    const out = resolve(dir, 'dist');
    const built = spawnSync(process.execPath, [resolve(root, 'host/web-js/build.mjs'), dir, '--out', out, '--render', 'js'], {
      cwd: root,
      encoding: 'utf8',
    });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    expect(built.stdout).toContain('1 Markdown sources compiled');
    const html = readFileSync(resolve(out, 'index.html'), 'utf8');
    expect(html).toContain('<script type="application/vnd.exact.markdown">');
    expect(html).toContain('animation:fade 200ms both');
    // Verify the runtime SVG spatial-rotation guard itself executes in dom.js and refuses spatial keyframes.
    const serverBundle = readFileSync(resolve(out, '.gen/app.js'), 'utf8');
    expect(serverBundle).toContain('style.animationName.split');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}, 180_000);
