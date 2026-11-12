// A Rust module that does not load, in Chrome, on built apps: the journal says so once, naming the response's
// status or the failed fetch; a mixed app (Update Lab: app.ts and a Rust module) still starts; each resource whose
// source the module owns fails, naming the load, and keeps what it showed; and nothing is thrown on the page.
import { afterAll, beforeAll, expect, test as bunTest } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, realpathSync, rmSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { cdpBrowser, chromium } from '../../scripts/agent-launch.mjs';
import { need } from '../../scripts/coverage.mjs';

const { unavailable } = chromium();
const test = need('chrome', unavailable, { test: bunTest });
const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const apps = ['update-lab', 'caltrain'];
let dir;
const servers = {};
// Per app, how the module is answered: HTTP 503, or its connection reset.
const mode = {};
beforeAll(async () => {
  if (unavailable) return;
  dir = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-rust-module-load-')));
  const { serveBuildTree } = await import('../web/serve.mjs');
  for (const app of apps) {
    const out = resolve(dir, app);
    const built = spawnSync(process.execPath, ['host/web-js/build.mjs', app, '--out', out, '--render', 'none'], { cwd: root, encoding: 'utf8' });
    if (built.status !== 0) throw new Error(built.stdout + built.stderr);
    servers[app] = createServer((req, res) => {
      if (!req.url.startsWith('/rust/wasm/app.module.wasm')) return void serveBuildTree(out, req, res);
      if (mode[app] === 'reset') return void req.socket.destroy();
      res.writeHead(503, { 'content-type': 'text/plain' });
      res.end('unavailable');
    });
    await new Promise(ok => servers[app].listen(0, '127.0.0.1', ok));
  }
}, 600_000);
afterAll(() => {
  for (const server of Object.values(servers)) server.close();
  if (dir) rmSync(dir, { recursive: true, force: true });
});

const status = 'rust/wasm/app.module.wasm answered HTTP 503', reset = 'rust/wasm/app.module.wasm could not be fetched: Failed to fetch';
for (const [app, how, why, owned] of [
  ['update-lab', '503', status, ['rustConstants']],
  ['update-lab', 'reset', reset, ['rustConstants']],
  ['caltrain', '503', status, ['station', 'northBoard', 'allStations']],
  ['caltrain', 'reset', reset, ['station', 'northBoard', 'allStations']],
]) {
  test(`${app}: a module answered with ${how === '503' ? 'HTTP 503' : 'a reset connection'} is said, and the app runs`, async () => {
    mode[app] = how;
    // Launched in the test: under Bun 1.4.2 a browser launched in a hook or an earlier test has closed its pipe.
    const suite = await cdpBrowser(), { cdp } = suite, { sessionId } = await suite.page(), thrown = [];
    try {
      cdp.listeners.push(m => { if (m.sessionId === sessionId && m.method === 'Runtime.exceptionThrown') thrown.push(m.params.exceptionDetails.exception?.description ?? m.params.exceptionDetails.text); });
      const call = (method, params) => cdp.send(method, params, sessionId);
      const evaluate = async expression => {
        const r = await call('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true });
        if (r.exceptionDetails) throw new Error(r.exceptionDetails.exception?.description ?? r.exceptionDetails.text);
        return r.result.value;
      };
      await call('Runtime.enable', {});
      await call('Page.navigate', { url: `http://127.0.0.1:${servers[app].address().port}/` });
      const read = () => evaluate(`globalThis.exact?.ready === true && { nodes: document.querySelectorAll('#exact-root *').length,
        said: [...exact.journal].filter(l => l.includes('Rust module did not load') && !/ (resource|send) /.test(l)),
        failed: Object.fromEntries(exact.resources.filter(r => r.failed).map(r => [r.name, r.error])) }`);
      let page;
      for (const end = Date.now() + 20_000; Date.now() < end; await new Promise(ok => setTimeout(ok, 50))) {
        page = await read();
        if (page && owned.every(name => page.failed[name])) break;
      }
      expect(page.said).toEqual([expect.stringMatching(new RegExp(`^t=[\\d.]+ the Rust module did not load: ${why}$`))]);
      expect(page.nodes).toBeGreaterThan(0);
      for (const name of owned) expect(page.failed[name]).toEndWith(`the Rust module did not load: ${why}`);
      expect(thrown).toEqual([]);
    } finally { await suite.close(); }
  }, 60_000);
}
