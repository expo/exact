import { test, expect } from 'bun:test';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { open } from '../../scripts/agent.mjs';
import { webCompiler } from './module.mjs';

// A child's requests on the JS target (LLP 1035.005.004), as the runner's
// runner/tests/it/owned.rs drives them: each instance of a component that
// declares a resource owns its own request, made with the instance and let
// go when it ends.

const root = resolve(import.meta.dir, '../..');

// `subjectWorks(s)` answers when the test lands it (`hold`) or at once, with `s`
// as its one work; `now(x)` answers at once; every ask is recorded in
// `globalThis.asks`.
const appModule = (hold = true) => `export const appId = 'test.owned';
export const grants = '';
const g = globalThis as any;
g.asks ??= []; g.held ??= [];
export function answer(name: string, args: any): any {
  const a = String(args[0] ?? '');
  g.asks.push(name + '(' + a + ')');
  if (name === 'subjectWorks') return ${hold} ? new Promise(done => g.held.push(() => done({ ok: true, works: [a] }))) : Promise.resolve({ ok: true, works: [a] });
  if (name === 'now') return 'now ' + a;
  if (name === 'emptyWorks') return { ok: true, works: ['ph'] };
  throw new Error('no source ' + name);
}
`;

const DISCOVER = `routes nav
  tab home "/"
  tab discover "/discover"

shape Subject
  ok: bool
  works: string[]

component App
  action pick(name: string)
    nav = select(nav, name)
  view
    main navigationKey=\`\${top(nav).id}\` navigationBack="back"
      column
        for t in nav.tabs key=t.name
          column role="tabpanel" id=\`panel-\${t.name}\` testId=\`panel-\${t.name}\`
            for e in t.stack key=e.id
              column navigationKey=\`\${e.id}\`
                if e.name == "discover"
                  Discover(shown=(nav.tab == t.name))
                else
                  text "Home" testId="home"
      row role="tablist"
        button press=pick("home") testId="tab-home" role="tab" aria-controls="panel-home" aria-selected=(nav.tab == "home")
          text "Home"
        button press=pick("discover") testId="tab-discover" role="tab" aria-controls="panel-discover" aria-selected=(nav.tab == "discover")
          text "Discover"

component Discover
  props
    shown: bool
  state subject = "sf"
  resource works = subjectWorks(subject) when shown as shape Subject else empty(ok=true)
  action choose(s: string)
    subject = s
  action reload
    refresh works
  view
    column
      text (pending(works) ? \`loading \${works.works.join(",")}\` : works.works.join(",")) testId="works"
      button press=choose("fantasy") testId="fantasy"
        text "Fantasy"
      button press=reload testId="reload"
        text "Reload"
`;

const BOOKS = `routes nav
  list "/"
  book "/book/:id"

shape Subject
  ok: bool
  works: string[]

component App
  action open(id: string)
    nav = push(nav, path("book", id))
  action swap(id: string)
    nav = replace(nav, path("book", id))
  action back
    nav = back(nav)
  view
    main
      button press=open("1") testId="open-1"
        text "1"
      button press=open("2") testId="open-2"
        text "2"
      button press=swap("2") testId="swap-2"
        text "Swap"
      button press=back testId="back"
        text "Back"
      for e in stack(nav) key=e.id
        if e.name == "book"
          Book(id=e.params.id)
        else
          text "List" testId="list"

component Book
  props
    id: string
  resource works = subjectWorks(id) as shape Subject else empty()
  action reload
    refresh works
  view
    column
      text \`\${id}: \${works.works.join(",")}\` testId=\`book-\${id}\`
      button press=reload testId=\`reload-\${id}\`
        text "Reload"
`;

const CHAIN = `routes nav
  home "/"
  scan "/scan"

component App
  action scan
    nav = push(nav, "/scan")
  view
    main
      button press=scan testId="scan-open"
        text "Scan"
      for e in stack(nav) key=e.id
        if e.name == "scan"
          Scan()
        else
          text "Home"

component Scan
  state code = "978"
  resource scanned = now(code) as shape string
  derive isbn = scanned == "" ? "" : \`\${scanned}!\`
  resource lookup = now(isbn) as shape string
  view
    text \`\${scanned} / \${lookup}\` testId="scan"
`;

const FEED = `component App
  view
    main
      Feed(topic="t")

component Feed
  props
    topic: string
  state n = 1
  resource got = now(\`\${topic}\${n}\`) as shape string
  action more
    n = n + 1
  view
    column
      text got testId="got"
      button press=more testId="more"
        text "More"
`;

/** Builds `source` for the web, opens it at `path` and hands `run` the page
 * and its helpers; `render` renders its `render=build` pages. */
async function fixture(source, run, { path = '/', render = false, hold = true } = {}) {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/owned-test-'));
  // open() resolves a path through EXACT_APP_DIR, which is shared across test files.
  const appDir = process.env.EXACT_APP_DIR;
  let browser, server;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Owned', app: { id: 'test.owned', name: 'Owned' } }));
    writeFileSync(resolve(dir, 'app.contract'), source);
    writeFileSync(resolve(dir, 'app.ts'), appModule(hold));
    const dist = resolve(dir, 'dist');
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'owned', '--out', dist, '--render', render ? 'js' : 'none'], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
      const p = new URL(req.url).pathname;
      if (p.startsWith('/shared/')) return new Response(Bun.file(resolve(root, 'host/web', p.slice('/shared/'.length))));
      const file = resolve(dist, p.slice(1)), page = resolve(dist, p.slice(1), 'index.html');
      if (existsSync(file) && p.includes('.')) return new Response(Bun.file(file));
      return new Response(Bun.file(existsSync(page) ? page : resolve(dist, 'shell.html')));
    } });
    browser = await open({ host: 'web', url: new URL(path, server.url).href, webDist: dist });
    const text = async id => (await browser.tree()).nodes.find(n => n.props.testId === id)?.props.text ?? null;
    const asks = () => browser.carrier.evaluate('globalThis.asks ?? []');
    // Answers every request out; how many there were.
    const land = async () => {
      const n = await browser.carrier.evaluate('(() => { const h = (globalThis.held ?? []).splice(0); h.forEach(f => f()); return h.length; })()');
      await browser.clock('settle');
      return n;
    };
    await run({ browser, text, asks, land });
  } finally {
    if (appDir === undefined) delete process.env.EXACT_APP_DIR;
    else process.env.EXACT_APP_DIR = appDir;
    await browser?.close();
    server?.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}

test('a tab screen asks while shown and keeps its answer while hidden', () => fixture(DISCOVER, async ({ browser, text, asks, land }) => {
  expect(await asks()).toEqual([]);
  expect(await text('works')).toBe(null);

  await browser.tap('tab-discover');
  expect(await asks()).toEqual(['subjectWorks(sf)']);
  expect(await text('works')).toBe('loading ');
  expect(await land()).toBe(1);
  expect(await text('works')).toBe('sf');

  // The agent lists it by instance path, apart from the root's resources.
  const state = await browser.state();
  expect(state.owned).toEqual(['works']);
  expect(Object.keys(state.resources)).not.toContain('works#1');
  expect(Object.values(state.instances)).toEqual([{ works: { ok: true, works: ['sf'] } }]);

  await browser.tap('tab-home');
  expect((await asks()).length).toBe(1);
  expect(await text('works')).toBe('sf');
  expect(await land()).toBe(0);

  await browser.tap('tab-discover');
  expect(await asks()).toEqual(['subjectWorks(sf)', 'subjectWorks(sf)']);
  expect(await text('works')).toBe('loading sf');
  await land();

  await browser.tap('fantasy');
  expect((await asks()).at(-1)).toBe('subjectWorks(fantasy)');
  await land();
  expect(await text('works')).toBe('fantasy');
  await browser.tap('reload');
  expect((await asks()).length).toBe(4);
  expect(await text('works')).toBe('loading fantasy');
  await land();
  expect(await text('works')).toBe('fantasy');

  // `perf` counts every instance's asks under the declared name.
  const perf = await browser.perf();
  expect(perf.resources.find(r => r.name === 'works')?.asked).toBe(4);
}), 180_000);

test('a popped screen lets its request go and its late answer changes nothing', () => fixture(BOOKS, async ({ browser, text, asks, land }) => {
  await browser.tap('open-1');
  expect(await asks()).toEqual(['subjectWorks(1)']);
  // Keep the first answer back past the screen's end.
  await browser.carrier.evaluate('globalThis.first = globalThis.held.splice(0)');
  await browser.tap('back');
  expect(await text('book-1')).toBe(null);
  await browser.tap('open-1');
  expect((await asks()).length).toBe(2);
  expect(await text('book-1')).toBe('1: ');
  await browser.carrier.evaluate('globalThis.first.forEach(f => f())');
  await browser.clock('settle');
  expect(await text('book-1')).toBe('1: ');
  expect(await land()).toBe(1);
  expect(await text('book-1')).toBe('1: 1');
}), 180_000);

test('replace keeps the screen and asks with the new parameters', () => fixture(BOOKS, async ({ browser, text, asks, land }) => {
  await browser.tap('open-1');
  await land();
  await browser.tap('swap-2');
  expect(await asks()).toEqual(['subjectWorks(1)', 'subjectWorks(2)']);
  expect(await text('book-2')).toBe('2: 1');
  await land();
  expect(await text('book-2')).toBe('2: 2');
}), 180_000);

test('two instances ask and refresh apart', () => fixture(BOOKS, async ({ browser, text, asks, land }) => {
  for (const id of ['1', '2', '1']) await browser.tap(`open-${id}`);
  expect(await asks()).toEqual(['subjectWorks(1)', 'subjectWorks(2)', 'subjectWorks(1)']);
  const paths = Object.keys((await browser.state()).instances);
  expect(paths.length).toBe(3);
  expect(await land()).toBe(3);
  // The covered screen's reload reaches only it.
  const reloads = (await browser.tree()).nodes.filter(n => n.props.testId === 'reload-1');
  expect(reloads.length).toBe(2);
  await browser.tap(reloads[0].id);
  expect((await asks()).length).toBe(4);
  expect((await browser.state()).pending.map(p => p.name)).toEqual(['works']);
}), 180_000);

test('an unopened tab asks nothing and a shown one asks at that render', () => fixture(DISCOVER.replace('when shown ', ''), async ({ browser, asks }) => {
  expect(await asks()).toEqual([]);
  await browser.tap('tab-discover');
  expect(await asks()).toEqual(['subjectWorks(sf)']);
}), 180_000);

test('an immediate answer lands and what reads it asks then', () => fixture(CHAIN, async ({ browser, text, asks }) => {
  await browser.tap('scan-open');
  await browser.clock('settle');
  const seen = await asks();
  expect(seen[0]).toBe('now(978)');
  expect(seen.at(-1)).toBe('now(now 978!)');
  expect(await text('scan')).toBe('now 978 / now now 978!');
}), 180_000);

test('a use outside every region asks with the root', () => fixture(FEED, async ({ browser, text, asks }) => {
  await browser.clock('settle');
  expect(await asks()).toEqual(['now(t1)']);
  expect(await text('got')).toBe('now t1');
  await browser.tap('more');
  await browser.clock('settle');
  expect(await asks()).toEqual(['now(t1)', 'now(t2)']);
  expect(await text('got')).toBe('now t2');
}), 180_000);

test("an owned request's source placeholder shows in the making render", () => fixture(DISCOVER.replace('else empty(ok=true)', 'else emptyWorks()'), async ({ text, land }) => {
  expect(await text('works')).toBe('loading ph');
  expect(await land()).toBe(1);
  expect(await text('works')).toBe('sf');
}, { path: '/discover' }), 180_000);

test("a rendered page's instances answer the boot's first frame", () => fixture(DISCOVER.replace('tab discover "/discover"', 'tab discover "/discover" render=build'), async ({ browser, text, asks, land }) => {
  // The page holds the instance's answer: the boot shows it and asks nothing.
  expect(await text('works')).toBe('sf');
  expect(await asks()).toEqual([]);
  // An instance's later ask is its own.
  await browser.tap('fantasy');
  expect(await asks()).toEqual(['subjectWorks(fantasy)']);
  await land();
  expect(await text('works')).toBe('fantasy');
}, { path: '/discover', render: true, hold: false }), 180_000);
