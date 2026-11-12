import { expect, test } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';

const root = resolve(import.meta.dir, '../..');
const target = resolve(process.env.CARGO_TARGET_DIR ?? resolve(root, 'target'));

async function start(command, cwd) {
  const child = Bun.spawn(command, { cwd, stdout: 'pipe', stderr: 'pipe' });
  const errors = new Response(child.stderr).text();
  const lines = createInterface({ input: (await import('node:stream')).Readable.fromWeb(child.stdout) });
  const ready = new Promise((ready, failed) => {
    lines.on('line', line => { const url = /http:\/\/[^/]+\//.exec(line)?.[0]; if (url) ready(url); });
    child.exited.then(async () => failed(new Error(`renderer ended before listening: ${await errors}`)));
  });
  return { url: await ready, async close() { child.kill('SIGTERM'); await child.exited; lines.close(); } };
}

// LLP 1048.007 D10, host/render and host/web-js/render.mjs: named answers load with the runtime.
test.each(['rust', 'js'])('%s document pages defer large answers and recover after eviction', async how => {
  const dir = mkdtempSync(resolve(root, 'target/document-checkpoints-'));
  let server, browser;
  try {
    const source = resolve(dir, 'app.contract'), dist = resolve(dir, 'dist');
    writeFileSync(source, `shape Departure
  id: string
  train: number
  service: string
  headsign: string
  at: number

routes nav
  home "/" render=request
  small "/small" render=request
  cached "/cached" render=cached
  built "/built" render=build
  idle "/idle" render=request activate=idle
  fill "/fill/:id" render=request

component App
  state n = 0
  resource items = board("mv", "north", top(nav).name == "small" ? 1787961600000 : 1787875200000) as shape Departure[]
  action more
    n = n + 1
  view
    main
      button press=more testId="more"
        text \`pressed \${n}\`
      for it in items key=it.id
        text it.service testId=\`item-\${it.id}\`
`);
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'caltrain', '--contract', source, '--out', dist, '--render', how], { cwd: root, stdout: 'pipe', stderr: 'pipe' });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect(status, stdout + stderr).toBe(0);
    expect(readFileSync(resolve(dist, 'built/index.html'), 'utf8')).not.toContain('data-src=');
    // The internal builder leaves the receipt to its caller; serve uses it to identify the JS target.
    if (how === 'js') writeFileSync(resolve(dist, '.exact-build.json'), JSON.stringify({ target: 'js' }));
    const command = how === 'rust'
      ? [resolve(target, 'host-dev', process.platform === 'win32' ? 'caltrain-render.exe' : 'caltrain-render'), '--serve', dist, '--port', '0']
      : [process.execPath, resolve(root, 'host/web/serve.mjs'), dist, '--loopback', '--port', '0'];
    server = await start(command, root);
    const get = async path => {
      const response = await fetch(new URL(path, server.url));
      expect(response.status).toBe(200);
      return response.text();
    };
    const checkpoint = html => /<script type="application\/vnd\.exact\.checkpoint"([^>]*)>([^<]*)<\/script>/.exec(html);
    const src = html => /data-src="([^"]+)"/.exec(checkpoint(html)[1])?.[1];
    for (const path of ['/small', '/idle']) {
      const html = await get(path);
      expect(src(html)).toBeUndefined();
      expect(JSON.parse(checkpoint(html)[2]).answers.length).toBeGreaterThan(0);
    }
    for (const path of ['/', '/cached']) {
      const html = await get(path), named = src(html);
      expect(named).toMatch(/^\/\.exact\/checkpoint\/[0-9a-f]{32}\.json$/);
      expect(checkpoint(html)[2]).toBe('');
      const answers = await fetch(new URL(named, server.url));
      expect([answers.status, answers.headers.get('content-type'), answers.headers.get('cache-control')])
        .toEqual([200, 'application/json', 'private, max-age=3600, immutable']);
      expect((await answers.json()).location).toBe(path);
    }
    browser = await suiteBrowser();
    for (const kept of [true, false]) {
      const tab = await browser.newPage({ viewport: { width: 390, height: 800 } });
      await tab.goto(server.url);
      const named = await tab.evaluate(() => {
        const c = document.querySelector('script[type="application/vnd.exact.checkpoint"]');
        document.querySelector('[data-testid^="item-"]').$mark = true;
        return [c.dataset.src, c.textContent, !!globalThis.exact?.ready];
      });
      expect(named[0]).toMatch(/^\/\.exact\/checkpoint\/[0-9a-f]{32}\.json$/);
      expect(named.slice(1)).toEqual(['', false]);
      if (!kept) {
        for (let i = 0; i < 513; i++) await get(`/fill/${i}`);
        const gone = await fetch(new URL(named[0], server.url));
        expect([gone.status, gone.headers.get('cache-control'), await gone.text()])
          .toEqual([404, 'no-store', 'These answers are no longer kept; the page builds itself afresh.\n']);
      }
      await tab.click('[data-testid="more"]');
      await tab.waitForFunction(() => document.querySelector('[data-testid="more"]')?.textContent === 'pressed 1', null, { timeout: 10000 });
      // This source answers synchronously, so recomputing after eviction can still adopt the same document.
      expect(await tab.evaluate(() => {
        const item = document.querySelector('[data-testid^="item-"]');
        return [!!item, !!item?.$mark];
      })).toEqual([true, true]);
      expect(await tab.evaluate(() => JSON.parse(document.querySelector('script[type="application/vnd.exact.checkpoint"]').textContent).answers.length > 0)).toBe(kept);
      await tab.close();
    }
  } finally {
    await browser?.close(); await server?.close(); rmSync(dir, { recursive: true, force: true });
  }
}, 600000);

test('the wasm document entry fetches named answers on intent and leaves missing answers empty', async () => {
  const html = `<!doctype html><div id="exact-root"><button data-view="1" data-exact-on="press">Act</button></div>
    <script type="application/vnd.exact.checkpoint" data-activate="interaction" data-digest="digest" data-src="/answers"></script>
    <script type="module" src="/document-glue.js"></script>`;
  const glue = readFileSync(resolve(root, 'host/web/document-glue.js'), 'utf8');
  const runtime = `const page = exact.documentPage; await page.answers; globalThis.checkpoint = page.checkpoint;`;
  let fetched = 0, kept = true;
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
    switch (new URL(req.url).pathname) {
      case '/': return new Response(html, { headers: { 'content-type': 'text/html' } });
      case '/document-glue.js': return new Response(glue, { headers: { 'content-type': 'text/javascript' } });
      case '/glue.js': return new Response(runtime, { headers: { 'content-type': 'text/javascript' } });
      case '/answers': fetched++; return new Response(kept ? '{}' : 'gone', { status: kept ? 200 : 404 });
      default: return new Response('', { status: 404 });
    }
  } });
  const browser = await suiteBrowser();
  try {
    for (kept of [true, false]) {
      const tab = await browser.newPage();
      await tab.goto(server.url.href);
      await tab.waitForFunction(() => !!globalThis.exact?.documentPage);
      const before = fetched;
      expect(before).toBe(kept ? 0 : 1);
      expect(await tab.evaluate(() => globalThis.checkpoint)).toBeUndefined();
      await tab.click('button');
      await tab.waitForFunction(() => globalThis.checkpoint !== undefined);
      expect(fetched).toBe(before + 1);
      expect(await tab.evaluate(() => globalThis.checkpoint)).toBe(kept ? 'digest\n{}' : '');
      await tab.close();
    }
  } finally { await browser.close(); server.stop(true); }
});

test('the wasm document entry preloads a deferred checkpoint’s data module before the runtime loads', async () => {
  const html = `<!doctype html><div id="exact-root"><button data-view="1" data-exact-on="press">Act</button></div>
    <script type="application/vnd.exact.checkpoint" data-activate="interaction" data-src="/answers"></script>
    <script type="module" src="/document-glue.js"></script>`;
  const glue = readFileSync(resolve(root, 'host/web/document-glue.js'), 'utf8');
  const checkpoint = JSON.stringify({ logic: 'data', answers: ['x'.repeat(1025)] });
  let releaseAnswers, releaseRuntime;
  const answers = new Promise(resolve => { releaseAnswers = resolve; });
  const runtime = new Promise(resolve => { releaseRuntime = resolve; });
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
    switch (new URL(req.url).pathname) {
      case '/': return new Response(html, { headers: { 'content-type': 'text/html' } });
      case '/document-glue.js': return new Response(glue, { headers: { 'content-type': 'text/javascript' } });
      case '/answers': await answers; return new Response(checkpoint, { headers: { 'content-type': 'application/json' } });
      case '/glue.js': await runtime; return new Response('globalThis.runtimeLoaded = true;', { headers: { 'content-type': 'text/javascript' } });
      case '/app.wasm': await runtime; return new Response(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]), { headers: { 'content-type': 'application/wasm' } });
      default: return new Response('', { headers: { 'content-type': 'text/javascript' } });
    }
  } });
  let browser;
  try {
    browser = await suiteBrowser();
    const tab = await browser.newPage();
    await tab.goto(server.url.href);
    await tab.waitForFunction(() => !!globalThis.exact?.documentPage);
    expect(await tab.locator('link[rel="preload"],link[rel="modulepreload"]').count()).toBe(0);
    const started = Promise.all(['/answers', '/app.wasm', '/navigation.js', '/glue.js'].map(path =>
      tab.waitForRequest(request => new URL(request.url()).pathname === path)));
    await tab.click('button');
    await started;
    expect(await tab.evaluate(() => document.querySelector('script[type="application/vnd.exact.checkpoint"]').textContent)).toBe('');
    releaseAnswers();
    const preloads = await tab.evaluate(async () => {
      await exact.documentPage.answers;
      return [...document.querySelectorAll('link[rel="preload"],link[rel="modulepreload"]')].map(link => new URL(link.href).pathname);
    });
    expect(preloads).toEqual(expect.arrayContaining(['/app.wasm', '/navigation.js', '/input-glue.js', '/timer-glue.js', '/module-glue.js', '/http-body.js']));
    await tab.waitForFunction(() => document.querySelector('link[href$="module-prelude.js"]') && document.querySelector('link[href$="module-binary.js"]'));
    expect(await tab.evaluate(() => globalThis.runtimeLoaded)).toBeUndefined();
    await tab.close();
  } finally {
    releaseAnswers(); releaseRuntime();
    await browser?.close(); server.stop(true);
  }
}, 60000);
