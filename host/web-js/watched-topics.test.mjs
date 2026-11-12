import { test, expect } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { webCompiler } from './module.mjs';

// LLP 1016.002 on the JS target: a changed topic asks again exactly the resources whose current answers watch it,
// and a fresh answer replaces its resource's topics (D2), as the runner's `changed` does. `read(x)` watches the
// document `x` and numbers its answers; `open` and `pinned` share the source.
test('a changed topic asks again only the resources whose current answers watch it', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/watched-topics-test-'));
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
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'watched', '--out', resolve(dir, 'dist'), '--render', 'none'], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
      const path = new URL(req.url).pathname;
      const file = Bun.file(resolve(dir, 'dist', path === '/' ? 'index.html' : path.slice(1)));
      return await file.exists() ? new Response(file) : new Response('not found', { status: 404 });
    } });
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
