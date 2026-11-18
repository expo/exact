import { test, expect } from 'bun:test';
import { existsSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { open } from '../../scripts/agent.mjs';
import { webCompiler } from './module.mjs';

test('the clock delivers world records before timers and follows camera deadlines', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/gpu-clock-test-'));
  let session, server;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'GPU Clock Test', app: { id: 'test.gpu-clock', name: 'GPU Clock Test' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), `component App
  state count = 0
  state shown = 0
  state waiting = false
  action publish
    count = count + 1
  task notice key=count
    change(begin)
  action begin
    if count > 0
      waiting = true
  task swap when waiting
    after(120, finish)
  action finish
    shown = count
    waiting = false
  view
    column
      button press=publish testId="publish"
        text "Publish"
      text \`\${shown}\` testId="shown"
`);
    const dist = resolve(dir, 'dist');
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'gpu-clock-test', '--out', dist, '--render', 'none'], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
      const path = new URL(req.url).pathname;
      const file = path.startsWith('/shared/') ? resolve(root, 'host/web', path.slice(8))
        : resolve(dist, path === '/' ? 'index.html' : path.slice(1));
      return existsSync(file) ? new Response(Bun.file(file)) : new Response('Not found', { status: 404 });
    } });
    session = await open({ host: 'web', url: server.url.href });
    await session.carrier.evaluate(`(() => {
      const exact = globalThis.exact;
      exact.gpu = {
        settled: async () => [],
        clock: () => ({ pending: exact.clock.now < 250, settleAt: 250,
          reply: { world: [{ canvas: 1, quiescent: exact.clock.now >= 250 }] } }),
      };
    })()`);
    const settled = await session.clock('settle');
    expect(settled).toMatchObject({ clock: 250, settled: true, world: [{ canvas: 1, quiescent: true }] });
    await session.carrier.evaluate(`(() => {
      globalThis.exact.gpu.clock = () => ({ pending: true,
        reply: { world: [{ canvas: 1, quiescent: false, changing: ['camera'] }] } });
    })()`);
    const pending = await session.clock('settle');
    expect(pending.settled).toBe(false);
    expect(pending.world).toEqual([{ canvas: 1, quiescent: false, changing: ['camera'] }]);
    for (const [count, operation] of [[1, '+800'], [2, 'settle']]) {
      await session.carrier.evaluate(`(() => {
        const exact = globalThis.exact, at = exact.clock.now + 100;
        let published = false;
        exact.gpu.clock = () => {
          if (!published && exact.clock.now >= at) {
            published = true;
            document.querySelector('[data-testid="publish"]').click();
          }
          return { pending: !published, settleAt: at + 700, nextTickAt: published ? undefined : at,
            reply: {world: [{canvas: 1, quiescent: published}]} };
        };
      })()`);
      await session.clock(operation);
      expect(await session.carrier.evaluate(`document.querySelector('[data-testid="shown"]').textContent`)).toBe(String(count));
    }
  } finally {
    await session?.close(); server?.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 120_000);
