import { test, expect } from 'bun:test';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { webCompiler } from './module.mjs';
import { pageServer } from './render.mjs';

// A render has no launch locale: it reads the base table, whose name and direction are the page's `lang` and `dir`
// (LLP 1060 D4), as the native render host writes them (host/render/src/page.rs). The manifest's `lang` is the page's
// only for a plan with no table (LLP 1115.000 D4.6).
const SOURCE = 'routes nav\n  home "/" render=build\n\ncomponent App\n  view\n    column\n      text t("hello") testId="hello"\n';
const AR = { 'strings/ar.json': '{"hello":"مرحبا"}' }, EN = { 'strings/en.json': '{"hello":"Hello"}' };
const CASES = [
  ['an English base alone', {}, EN, 'en', 'ltr', 'Hello'],
  ['an Arabic base alone', { strings: { base: 'ar' } }, AR, 'ar', 'rtl', 'مرحبا'],
  ['an Arabic base with English', { strings: { base: 'ar' } }, { ...AR, ...EN }, 'ar', 'rtl', 'مرحبا'],
  ['an English base under an Arabic manifest', { lang: 'ar' }, { ...AR, ...EN }, 'en', 'ltr', 'Hello'],
  ['no table under an Arabic manifest', { lang: 'ar' }, {}, 'ar', 'ltr', 'plain'],
];
const opening = html => /<html[^>]*>/.exec(html)?.[0];
const said = html => /data-testid="hello"[^>]*>([^<]*)</.exec(html)?.[1];

async function renderedPages(source, files, manifest, check) {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), app = mkdtempSync(resolve(root, 'target/render-language-test-'));
  try {
    writeFileSync(resolve(app, 'app.json'), JSON.stringify({ name: 'Render Language Test', app: { id: 'test.language', name: 'Render Language Test' }, host: { web: {} }, ...manifest }));
    writeFileSync(resolve(app, 'app.contract'), source);
    for (const [path, body] of Object.entries(files)) {
      mkdirSync(dirname(resolve(app, path)), { recursive: true });
      writeFileSync(resolve(app, path), body);
    }
    const dist = resolve(app, 'dist');
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'render-language-test', '--out', dist, '--render', 'js'], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: app }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    await check(dist);
  } finally { rmSync(app, { recursive: true, force: true }); }
}

for (const [name, manifest, files, lang, dir, text] of CASES) test(`a JavaScript render of ${name} is lang="${lang}" dir="${dir}" and waits for interaction`, async () => {
  await renderedPages(Object.keys(files).length ? SOURCE : SOURCE.replace('t("hello")', '"plain"'), files, manifest, async dist => {
    const built = readFileSync(resolve(dist, 'index.html'), 'utf8');
    expect([opening(built), said(built)]).toEqual([`<html lang="${lang}" dir="${dir}">`, text]);
    // The page renderer composes its heads alike: a navigation's head, flushed before the render, and a whole page.
    const page = pageServer(dist, undefined, () => {});
    const streamed = await page('/', h => ({ 'sec-fetch-dest': 'document' })[h]);
    expect(streamed.body).toBeInstanceOf(ReadableStream);
    const flushed = await new Response(streamed.body).text();
    expect([opening(flushed), said(flushed)]).toEqual([`<html lang="${lang}" dir="${dir}">`, text]);
    const whole = await page('/?whole', () => undefined);
    expect([opening(whole.body), said(whole.body)]).toEqual([`<html lang="${lang}" dir="${dir}">`, text]);
    // Reading strings alone leaves nothing for the runtime before interaction (LLP 1048.007 D1).
    for (const html of [built, flushed, whole.body]) {
      expect(html).toContain('data-activate="interaction"');
      expect(html).not.toMatch(/<link\b[^>]*rel="modulepreload"[^>]*href="\.?\/app\.js"/);
    }
  });
}, 60000);

test('a request page with strings is kept unless its source draws a random number', async () => {
  const api = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch: () => new Response('shared answer', {
    headers: { 'cache-control': 'public, max-age=60' },
  }) });
  try {
    const origin = `http://127.0.0.1:${api.port}`;
    await renderedPages(`shape Note
  text: string

routes nav
  fresh "/" render=request
  random "/random" render=request

component App
  resource note = note(top(nav).name) as shape Note
  view
    main
      text t("hello") testId="hello"
      text note.text
`, { ...EN, 'app.ts': `import type { Answer, Result } from './app.contract.d.ts';
export const appId = 'test.language';
export const grants = 'net.fetch ${origin}';
export const answer: Answer = async (_source, [name]): Promise<Result<'note'>> => {
  if (name === 'random') crypto.getRandomValues(new Uint8Array(1));
  const response = await fetch('${origin}/note');
  return { text: await response.text() };
};
` }, {}, async dist => {
      const log = [], page = pageServer(dist, undefined, line => log.push(line));
      for (const location of ['/', '/', '/random', '/random']) {
        const response = await page(location, () => undefined);
        expect(response.status).toBe(200);
        expect(response.headers['cache-control']).toBe('no-store');
        expect(said(response.body)).toBe('Hello');
        expect(response.body).toContain('shared answer');
      }
      const renders = location => log.filter(line => line.startsWith(`render ${location} 200 `)).length;
      expect([renders('/'), renders('/random')]).toEqual([1, 2]);
    });
  } finally { api.stop(true); }
}, 60000);
