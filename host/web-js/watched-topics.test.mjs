import { expect } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { webCompiler } from './module.mjs';
import { engineTests } from '../../scripts/coverage.mjs';

const engineTest = await engineTests(['chromium']);

const root = resolve(import.meta.dir, '../..');
const scratch = () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  return mkdtempSync(resolve(root, 'target/watched-topics-test-'));
};
/** Builds the app in `dir` and serves it. */
async function serve(dir, name) {
  const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', name, '--out', resolve(dir, 'dist'), '--render', 'none'], {
    cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
  });
  const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
  expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
  return Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
    const path = new URL(req.url).pathname;
    const file = Bun.file(resolve(dir, 'dist', path === '/' ? 'index.html' : path.slice(1)));
    return await file.exists() ? new Response(file) : new Response('not found', { status: 404 });
  } });
}

// LLP 1016.002 on the JS target: a changed topic asks again exactly the resources whose current answers watch it,
// and a fresh answer replaces its resource's topics (D2), as the runner's `changed` does. `read(x)` watches the
// document `x` and numbers its answers; `open` and `pinned` share the source.
engineTest.chromium('a changed topic asks again only the resources whose current answers watch it', async () => {
  const dir = scratch();
  let server, browser;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Watched', app: { id: 'test.watched-topics', name: 'Watched' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), `component App
  state path = "a"
  resource open = read(path) as shape string
  resource pinned = read("p") as shape string
  action go(p: string)
    path = p
  view
    column
      text open testId="open"
      text pinned testId="pinned"
      button press=go("b") testId="b"
        text "b"
`);
    writeFileSync(resolve(dir, 'app.ts'), `export const appId = 'test.watched-topics';
export const grants = '';
let answers = 0;
export function answer(name: string, args: unknown[], _store: unknown, storage: any): any {
  if (name === 'read') {
    storage.fs.watch('doc:/' + args[0]);
    return \`read \${args[0]} #\${++answers}\`;
  }
  throw new Error('no source ' + name);
}
`);
    server = await serve(dir, 'watched');
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const shown = () => page.evaluate(() => ['open', 'pinned'].map(id => document.querySelector(`[data-testid="${id}"]`)?.textContent).join(' | '));
    const announce = topic => page.evaluate(t => { globalThis.exact.announceModule(`doc-watch:doc:/${t}`); }, topic);
    await page.waitForFunction(() => document.querySelector('[data-testid="pinned"]')?.textContent?.startsWith('read'));
    expect(await shown()).toBe('read a #1 | read p #2');
    await page.getByTestId('b').click();
    await page.waitForFunction(() => document.querySelector('[data-testid="open"]').textContent.startsWith('read b'));
    expect(await shown()).toBe('read b #3 | read p #2');
    // `open` no longer watches `a`, and `pinned` never did.
    await announce('a');
    expect(await shown()).toBe('read b #3 | read p #2');
    await announce('p');
    expect(await shown()).toBe('read b #3 | read p #4');
    expect(await page.evaluate(() => globalThis.exact.watchState())).toEqual([{ path: 'doc:/b', recursive: false }, { path: 'doc:/p', recursive: false }]);
  } finally {
    await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true });
  }
}, 180_000);

// LLP 1030.004 D2: another tab's write to an app folder asks again this tab's resource whose answer listed it.
engineTest.chromium("another tab's write asks again the resource that listed its folder", async () => {
  const dir = scratch();
  let server, browser;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Tabs', app: { id: 'test.watched-tabs', name: 'Tabs' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), `component App
  state n = 0
  resource listed = files() as shape string
  resource made = make(n) as shape string
  action add()
    n = n + 1
  view
    column
      text listed testId="listed"
      text made testId="made"
      button press=add() testId="add"
        text "add"
`);
    writeFileSync(resolve(dir, 'app.ts'), `export const appId = 'test.watched-tabs';
export const grants = 'fs.read app:/tmp\\nfs.write app:/tmp';
export async function answer(name: string, args: number[], _store: unknown, storage: any): Promise<string> {
  if (name === 'files') return 'files ' + (await storage.fs.readdir('app:/tmp')).map((e: any) => e.name ?? e).sort().join(',');
  if (name === 'make') {
    if (args[0] > 0) await storage.fs.writeFile('app:/tmp/f' + args[0], new Uint8Array([1]));
    return 'made ' + args[0];
  }
  throw new Error('no source ' + name);
}
`);
    server = await serve(dir, 'tabs');
    browser = await suiteBrowser();
    const context = await browser.newContext(), [a, b] = [await context.newPage(), await context.newPage()];
    // Waits for `page` to show `want` at `id`, else says what it shows and its journal.
    const shows = (page, id, want) => page.waitForFunction(([id, want]) => document.querySelector(`[data-testid="${id}"]`)?.textContent === want, [id, want], { timeout: 10_000 })
      .catch(async () => {
        const [now, journal] = await page.evaluate(id => [document.querySelector(`[data-testid="${id}"]`)?.textContent, globalThis.exact?.journal ?? []], id);
        throw new Error(`${id} shows ${now}, not ${want}:\n${journal.join('\n')}`);
      });
    for (const page of [a, b]) { await page.goto(server.url.href); await shows(page, 'listed', 'files '); }
    await a.getByTestId('add').click();
    await shows(a, 'made', 'made 1');
    await shows(b, 'listed', 'files f1');
  } finally {
    await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true });
  }
}, 180_000);
