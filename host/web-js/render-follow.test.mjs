import { expect, test } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { webCompiler } from './module.mjs';

// A page rendered ahead of time runs the app's module where no browser is (render.mjs's VM has no
// ResizeObserver): an ordinary scroller's `scrollFollowEnd` (follow.js) renders, and follows once the page boots.
test('a route with an ordinary scrollFollowEnd scroller renders', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/render-follow-test-'));
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Render Follow', app: { id: 'test.render-follow', name: 'Render Follow' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), [
      'routes nav', '  home "/" render=build', '', 'component App', '  state following = true', '  view',
      '    scroll height=200 scrollFollowEnd=following', '      text "the transcript"', '',
    ].join('\n'));
    const dist = resolve(dir, 'dist');
    const built = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'render-follow-test', '--out', dist, '--render', 'js'], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([built.exited, new Response(built.stdout).text(), new Response(built.stderr).text()]);
    expect(status, stdout + stderr).toBe(0);
    expect(stdout + stderr).not.toMatch(/failed|ResizeObserver/);
    expect(readFileSync(resolve(dist, 'index.html'), 'utf8')).toContain('the transcript');
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 300_000);
