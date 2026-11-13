// Builds a Contract source as a web app and hands its rendered pages to a check, for the
// document tests (`document-render.test.mjs`, `document-layout.test.mjs`).
import { expect } from 'bun:test';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { webCompiler } from './module.mjs';

export async function renderedPages(source, check, files = {}, manifest = {}, renderer = 'js') {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/head-render-test-'));
  try {
    if (renderer === 'js') writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Head Render Test', app: { id: 'test.heads', name: 'Head Render Test' }, host: { web: {} }, ...manifest }));
    writeFileSync(resolve(dir, 'app.contract'), source);
    for (const [name, text] of Object.entries(files)) {
      mkdirSync(dirname(resolve(dir, name)), { recursive: true });
      writeFileSync(resolve(dir, name), text);
    }
    const app = renderer === 'rust' ? ['caltrain', '--contract', resolve(dir, 'app.contract')] : ['head-render-test'];
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', ...app, '--out', resolve(dir, 'dist'), '--render', renderer], {
      cwd: root, env: { ...process.env, ...(renderer === 'js' ? { EXACT_APP_DIR: dir } : {}) }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    const html = path => readFileSync(resolve(dir, 'dist', path, 'index.html'), 'utf8');
    await check(path => {
      const page = html(path);
      return [/<title>([^<]*)<\/title>/.exec(page)?.[1], /<meta name="description" content="([^"]*)">/.exec(page)?.[1]];
    }, html, resolve(dir, 'dist'));
  } finally { rmSync(dir, { recursive: true, force: true }); }
}

// A `--render rust` build is made once, outside every test's own time: cold, build.mjs compiling Caltrain's
// data module and its renderer (caltrain-render) outlasted a test's 60 or 180 s, and more so behind another build's
// cargo lock. The warm-up is a build as the tests make one, so it builds exactly what they will reuse.
export const warmRustRenderer = () => renderedPages('routes nav\n  home "/" render=build\n\ncomponent Warm\n  view\n    text "warm"\n', () => {}, {}, {}, 'rust');
