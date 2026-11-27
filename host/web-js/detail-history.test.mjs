// A detail that gives way to the next as a new visit (`push(back(nav), url)`, as a split's detail does beside its list,
// LLP 1127) takes the browser entry of the visit it replaced (routes.js `commit`): Back from the new detail goes to the
// list, as the app's own Back does, rather than to a page the app no longer holds. With an entry ahead, which only a
// push drops, or two new visits in its place, it is pushed.
import { test, expect, beforeAll, afterAll } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { webCompiler } from './module.mjs';

const SOURCE = `routes nav
  home "/"
    item "/item/:id"

component App
  derive current = top(nav)
  action stack
    nav = push(nav, path("item", "b"))
  action twice
    nav = push(push(back(nav), path("item", "b")), path("item", "c"))
  action back
    nav = back(nav)
  action follow(url: string)
    let moved = go(nav, url)
    let another = current.name == "item" and top(moved).name == "item" and top(moved).params.id != current.params.id
    nav = another and depth(moved) == depth(nav) + 1 ? push(back(nav), url) : moved

  view
    main navigationKey=\`\${current.id}\` navigationBack="back" navigate=follow width="100%" height="100%"
      for e in stack(nav) key=e.id
        column navigationKey=\`\${e.id}\` position="absolute" top=0 bottom=0 left=0 right=0
          if e.id == current.id
            text (e.name == "home" ? "Home" : \`Item \${e.params.id}\`) testId="title"
            link href=path("item", "a") testId="to-a"
              text "A"
            link href=path("item", "b") testId="to-b"
              text "B"
            if e.name == "item"
              button id="back" press=back testId="back"
                text "Back"
              button press=stack testId="stack"
                text "Stack"
              button press=twice testId="twice"
                text "Twice"
`;

let dir, server, mounted;
beforeAll(async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..');
  dir = mkdtempSync(resolve(root, 'target/detail-history-test-'));
  writeFileSync(resolve(dir, 'app.contract'), SOURCE);
  // The app built at the site's root and mounted at /app/ (`host.web.base`), each served where it is mounted
  for (const base of [undefined, '/app/']) {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Detail History', app: { id: 'test.detail-history', name: 'Detail History' }, host: { web: base ? { base } : {} } }));
    const dist = resolve(dir, base ? 'mounted' : 'dist'), prefix = base ?? '/';
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'detail-history', '--out', dist, '--render', 'none'], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    const served = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
      const path = new URL(req.url).pathname.slice(prefix.length - 1), file = Bun.file(resolve(dist, path === '/' || !path.includes('.') ? 'index.html' : path.slice(1)));
      return await file.exists() ? new Response(file) : new Response('not found', { status: 404 });
    } });
    if (base) mounted = served; else server = served;
  }
}, 120_000);
afterAll(() => { server?.stop(true); mounted?.stop(true); if (dir) rmSync(dir, { recursive: true, force: true }); });

async function page(at = server, home = '/') {
  const browser = await suiteBrowser(), context = await browser.newContext({ viewport: { width: 390, height: 700 } });
  const p = await context.newPage(), errors = [];
  p.on('pageerror', e => errors.push(e.message));
  const until = (want) => p.waitForFunction(w => document.querySelector('[data-testid=title]')?.textContent === w.title && location.pathname === w.path, want);
  await p.goto(new URL(home, at.url).href);
  await until({ title: 'Home', path: home });
  return { p, errors, until, close: () => context.close() };
}

test('Back from a detail that took another\'s place goes to the list, not to the detail it replaced', async () => {
  const { p, errors, until, close } = await page();
  try {
    await p.click('[data-testid=to-a]');
    await until({ title: 'Item a', path: '/item/a' });
    await p.click('[data-testid=to-b]');
    await until({ title: 'Item b', path: '/item/b' });
    await p.evaluate(() => history.back());
    await until({ title: 'Home', path: '/' });
    // Nothing of a is ahead either: Forward returns to b
    await p.evaluate(() => history.forward());
    await until({ title: 'Item b', path: '/item/b' });
    expect(errors).toEqual([]);
  } finally { await close(); }
}, 60_000);

test('a detail that takes another\'s place with an entry ahead is pushed, dropping what is ahead', async () => {
  const { p, errors, until, close } = await page();
  try {
    await p.click('[data-testid=to-a]');
    await until({ title: 'Item a', path: '/item/a' });
    await p.click('[data-testid=stack]');
    await until({ title: 'Item b', path: '/item/b' });
    await p.click('[data-testid=back]');
    await until({ title: 'Item a', path: '/item/a' });
    // b's entry is ahead of a's; b opened in a's place is pushed, and Back reaches a's entry
    await p.click('[data-testid=to-b]');
    await until({ title: 'Item b', path: '/item/b' });
    expect(await p.evaluate(() => navigation.currentEntry.index === navigation.entries().length - 1)).toBe(true);
    await p.evaluate(() => history.back());
    await until({ title: 'Item a', path: '/item/a' });
    expect(errors).toEqual([]);
  } finally { await close(); }
}, 60_000);

test('two new visits in the place of the detail shown are pushed, so Back reaches the shown one\'s entry', async () => {
  const { p, errors, until, close } = await page();
  try {
    await p.click('[data-testid=to-a]');
    await until({ title: 'Item a', path: '/item/a' });
    // b, then c above it, in a's place: c's entry is pushed, since b has none for Back to reach
    await p.click('[data-testid=twice]');
    await until({ title: 'Item c', path: '/item/c' });
    expect(await p.evaluate(() => navigation.entries().length)).toBe(3);
    await p.evaluate(() => history.back());
    await p.waitForFunction(() => location.pathname === '/item/a');
    expect(errors).toEqual([]);
  } finally { await close(); }
}, 60_000);

test('a detail in the place of one above another detail is pushed, so Back and Forward keep the one beneath', async () => {
  const { p, errors, until, close } = await page();
  try {
    await p.goto(new URL('/item/c', server.url).href);
    await until({ title: 'Item c', path: '/item/c' });
    // b above c, then a in b's place: c is no list's root, so a's entry is pushed
    await p.click('[data-testid=stack]');
    await until({ title: 'Item b', path: '/item/b' });
    await p.click('[data-testid=to-a]');
    await until({ title: 'Item a', path: '/item/a' });
    await p.evaluate(() => history.back());
    await until({ title: 'Item b', path: '/item/b' });
    await p.evaluate(() => history.forward());
    await until({ title: 'Item a', path: '/item/a' });
    await p.click('[data-testid=back]');
    await until({ title: 'Item c', path: '/item/c' });
    expect(errors).toEqual([]);
  } finally { await close(); }
}, 60_000);

test('on a site mounted at /app/, Back from a detail that took another\'s place goes to the list', async () => {
  const { p, errors, until, close } = await page(mounted, '/app/');
  try {
    await p.click('[data-testid=to-a]');
    await until({ title: 'Item a', path: '/app/item/a' });
    await p.click('[data-testid=to-b]');
    await until({ title: 'Item b', path: '/app/item/b' });
    await p.evaluate(() => history.back());
    await until({ title: 'Home', path: '/app/' });
    expect(errors).toEqual([]);
  } finally { await close(); }
}, 60_000);
