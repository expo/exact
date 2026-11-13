import { test, expect } from 'bun:test';
import { existsSync } from 'node:fs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';

function runtimeServer() {
  return Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/favicon.ico') { return new Response(null, { status: 204 }); }
    if (path.endsWith('/names.js')) { return new Response('export const sourceTypes = {};', { headers: { 'content-type': 'text/javascript' } }); }
    if (path === '/') { return new Response('<div id="exact-root"></div>', { headers: { 'content-type': 'text/html' } }); }
    const local = new URL('../..' + path, import.meta.url), file = existsSync(local) ? local : new URL('../web/' + path.split('/').pop(), import.meta.url);
    return new Response(hostFixtureFile(file), { headers: { 'content-type': 'text/javascript' } });
  } });
}

// A tool whose action's commit is refused ends `failed` `refused-commit` (the runner's `modules/calls.rs`), never
// `ok` over the state the refusal put back, through the agent's door and through WebMCP's.
test('a tool whose commit is refused does not report it applied', async () => {
  const server = runtimeServer();
  let browser, page;
  try {
    browser = await suiteBrowser();
    page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      globalThis.__exactRender = true;
      const registered = {};
      document.modelContext = { registerTool: tool => { registered[tool.name] = tool; return Promise.resolve(); } };
      const rt = await import('/host/web-js/rt.js'), { tools } = await import('/host/web-js/calls.js');
      const n = rt.sig(0, 'n');
      rt.clock.agent = true;
      rt.mount(() => {});
      const face = { digest: 'd', doors: ['web'] }, row = action => [() => action, {}, 'writes', [false, false, true], 'A tool.', null,
        () => n(), null, face, 'n'];
      tools({
        add: row(rt.act(() => rt.W(n, n() + 1))),
        mistyped: row(rt.act(() => rt.W(n, 'one'))),
        throws: row(rt.act(() => { rt.W(n, 5); throw new rt.Refusal('a refusal in the body'); })),
      });
      for (let i = 0; i < 8; i++) await Promise.resolve();
      const agent = async tool => {
        const { call } = globalThis.exact.calls.call({ tool });
        for (let i = 0; i < 64 && globalThis.exact.calls.called(call).outcome == null; i++) await Promise.resolve();
        const { outcome, certainty, reason, value } = globalThis.exact.calls.called(call);
        return [outcome, certainty, reason, value, n()];
      };
      const web = async tool => {
        const { outcome, certainty, reason, value } = await registered[tool].execute({});
        return [outcome, certainty, reason, value, n()];
      };
      return [await agent('add'), await agent('mistyped'), await agent('throws'), await web('mistyped'), await web('throws'),
        await web('add')];
    });
    expect(result).toEqual([
      ['ok', 'applied', '', 1, 1],
      ['failed', 'uncertain', 'refused-commit', null, 1],
      ['failed', 'uncertain', 'refused-commit', null, 1],
      ['failed', 'uncertain', 'refused-commit', null, 1],
      ['failed', 'uncertain', 'refused-commit', null, 1],
      ['ok', 'applied', '', 2, 2],
    ]);
  } finally {
    await page?.close();
    await browser?.close();
    server.stop(true);
  }
}, 60000);
