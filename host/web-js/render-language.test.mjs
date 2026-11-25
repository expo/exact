import { test, expect } from 'bun:test';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { webCompiler } from './module.mjs';
import { pageServer } from './render.mjs';
import { createHash } from 'node:crypto';
import { chromium, suiteBrowser } from '../../scripts/agent-launch.mjs';
import * as playwright from 'playwright-core';
import { engineTests } from '../../scripts/coverage.mjs';

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

async function renderedPages(source, files, manifest, check, flags = []) {
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
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'render-language-test', '--out', dist, '--render', 'js', ...flags], {
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

const engineTest = await engineTests();
for (const engine of ['chromium', 'firefox', 'webkit']) engineTest[engine]('split strings paint the chosen language before interaction and survive an offline launch', async () => {
  const source = `routes nav
  home "/" render=build

component App
  state first = t("hello")
  action choose(tag: string)
    setLocale(tag)
  view
    column
      input autofocus=true value="" testId="field"
      text first testId="first"
      text t("hello") testId="hello"
      button press=choose("de") testId="de"
        text "Deutsch"
      button press=choose("en") testId="en"
        text "English"
      button press=choose("fr") testId="fr"
        text "Français"
${Array.from({length: 256}, (_, i) => `      text t("key${i}")`).join('\n')}
`;
  const strings = lang => Object.fromEntries([['hello', lang], ...Array.from({length: 256}, (_, i) =>
    [`key${i}`, createHash('sha256').update(lang + i).digest('hex')])]);
  const files = Object.fromEntries(['en', 'de', 'fr'].map(lang => [`strings/${lang}.json`, JSON.stringify(strings(lang))]));
  await renderedPages(source, files, {}, async dist => {
    const assets = JSON.parse(readFileSync(resolve(dist, 'routes.json'))).files;
    const german = assets['strings-de.json'], french = assets['strings-fr.json'];
    expect([german, french]).not.toContain(undefined);
    const { promise: requested, resolve: arrived } = Promise.withResolvers();
    const { promise: held, resolve: release } = Promise.withResolvers();
    const requests = [];
    let refuseFrench = true;
    const server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(request) {
      const path = new URL(request.url).pathname.slice(1) || 'index.html';
      requests.push(path);
      if (path === german) { arrived(); await held; }
      if (path === french && refuseFrench) return Response.json({hello: 'wrong'}, { status: 503, headers: {'Cache-Control': 'public, max-age=31536000, immutable'} });
      return new Response(Bun.file(resolve(dist, path)), { headers: { 'Cache-Control': 'public, max-age=31536000, immutable' } });
    } });
    // WebKit discards an ephemeral site's Cache Storage when its last page leaves. Exercise an ordinary profile.
    const context = await playwright[engine].launchPersistentContext('', {headless: true, locale: 'en-US',
      ...(engine === 'chromium' ? {executablePath: chromium().executable} : {})});
    try {
      const page = await context.newPage(), url = `http://127.0.0.1:${server.port}`;
      page.setDefaultTimeout(6000);
      const shows = lang => page.waitForFunction(lang => document.querySelector('[data-testid=hello]')?.textContent === lang, lang);
      await page.goto(url + '/shell.html');
      await shows('en');
      expect(requests).not.toContain(german);
      await page.getByTestId('de').click();
      await requested;
      expect(await page.getByTestId('hello').textContent()).toBe('en');
      await page.getByTestId('en').click();
      const cold = await context.newPage();
      const entry = Promise.withResolvers();
      await cold.route('**/app-*.js', async route => { await entry.promise; await route.continue(); });
      await cold.addInitScript(() => localStorage.setItem('exact.state.#locale', '"de-DE"'));
      await cold.goto(url, {waitUntil: 'domcontentloaded'});
      expect(await cold.evaluate(() => getComputedStyle(document.documentElement).opacity)).toBe('0');
      entry.resolve();
      const arrivedAtPage = page.waitForResponse(response => response.url().endsWith('/' + german));
      release();
      await (await arrivedAtPage).finished();
      await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
      expect(await page.getByTestId('hello').textContent()).toBe('en');
      await page.getByTestId('de').click();
      await shows('de');
      await cold.waitForFunction(() => document.querySelector('[data-testid=first]')?.textContent === 'de' && globalThis.exact?.ready === true);
      expect(await cold.evaluate(() => getComputedStyle(document.documentElement).opacity)).toBe('1');
      expect(await cold.evaluate(() => document.activeElement?.dataset.testid)).toBe('field');
      await cold.keyboard.type('x');
      expect(await cold.getByTestId('field').inputValue()).toBe('x');
      await cold.close();
      const missing = await context.newPage();
      await missing.addInitScript(() => localStorage.setItem('exact.state.#locale', '"fr"'));
      await missing.goto(url + '/shell.html');
      await missing.waitForFunction(() => globalThis.exact?.journal?.some(line => line.includes('language fr failed:')));
      expect(await missing.getByTestId('first').textContent()).toBe('en');

      await page.getByTestId('de').click();
      await context.setOffline(true);
      await page.getByTestId('fr').click();
      await page.waitForFunction(() => exact.journal.some(line => line.includes('setLocale("fr") refused')));
      expect(await page.getByTestId('hello').textContent()).toBe('de');
      await page.goto('about:blank');
      await page.goto(url + '/shell.html');
      await shows('de');
      expect(await page.getByTestId('first').textContent()).toBe('de');
      await context.setOffline(false);
      expect(await missing.evaluate(async file => !!await (await caches.open('exact.tables')).match(file), french)).toBe(false);
      await missing.evaluate(async file => {
        await (await caches.open('exact.tables')).put(file, Response.json({hello: 'wrong'}, {status: 503}));
        await fetch(file); // A cacheable HTTP error left by an older build.
      }, french);
      refuseFrench = false;
      await missing.reload();
      await missing.waitForFunction(() => document.querySelector('[data-testid=first]')?.textContent === 'fr');
      await missing.close();
    } finally { release(); await context.close(); server.stop(true); }
  }, ['--production']);
}, 60000);

for (const delayed of ['cache', 'stylesheet']) engineTest.chromium(`the chosen language is usable with a stalled ${delayed}`, async () => {
  const padding = createHash('shake256', { outputLength: 12000 }).update('padding').digest('hex');
  const source = SOURCE.replace('  view', '  action choose(tag: string)\n    setLocale(tag)\n  view') +
    '      button press=choose("en") testId="en"\n        text "English"\n      text t("padding")\n';
  await renderedPages(source, Object.fromEntries(['en', 'de'].map(lang =>
    [`strings/${lang}.json`, JSON.stringify({hello: lang, padding})])), {}, async dist => {
    const server = Bun.serve({hostname: '127.0.0.1', port: 0, fetch(request) {
      return new Response(Bun.file(resolve(dist, new URL(request.url).pathname.slice(1) || 'index.html')));
    }});
    const browser = await suiteBrowser();
    try {
      const page = await browser.newPage({locale: 'en-US'});
      await page.addInitScript(delayed => {
        localStorage.setItem('exact.state.#locale', '"de"');
        if (delayed === 'cache') CacheStorage.prototype.open = () => new Promise(() => {});
      }, delayed);
      if (delayed === 'stylesheet') await page.route('**/style-*.css', () => {});
      await page.goto(`http://127.0.0.1:${server.port}`, {waitUntil: 'domcontentloaded'});
      await page.waitForFunction(() => !document.documentElement.inert && globalThis.exact?.ready === true,
        null, {timeout: 6000});
      expect(await page.getByTestId('hello').textContent()).toBe('de');
      if (delayed === 'stylesheet') expect(await page.evaluate(() => exact.journal.join('\n'))).toMatch(/timed out.*de/);
      if (delayed === 'cache') {
        await page.goto('about:blank');
        await page.route('**/strings-de-*.json', route => route.abort());
        await page.goto(`http://127.0.0.1:${server.port}`, {waitUntil: 'domcontentloaded'});
        await page.waitForFunction(() => globalThis.exact?.ready === true);
        expect(await page.getByTestId('hello').textContent()).toBe('de');
      }
      await page.getByTestId('en').click();
      expect(await page.getByTestId('hello').textContent()).toBe('en');
    } finally { await browser.close(); server.stop(true); }
  }, ['--production']);
}, 60000);

test('small multilingual releases keep the tables at the original inline call site', async () => {
  await renderedPages(SOURCE, {...AR, ...EN}, {}, async dist => {
    const { files } = JSON.parse(readFileSync(resolve(dist, 'routes.json')));
    expect(Object.keys(files).filter(name => name.startsWith('strings-'))).toEqual([]);
    const shell = readFileSync(resolve(dist, 'index.html'), 'utf8');
    expect(shell).not.toContain('languageLaunch');
    const compiled = readFileSync(resolve(dist, '.gen/app.js'), 'utf8');
    expect(compiled).not.toContain('export const languageTables');
  }, ['--production']);
}, 60000);

for (const scenario of ['held answers', 'failed answers', 'malformed answers', 'missing rendered language', 'stalled rendered tables', 'changed system language'])
  engineTest.chromium(`rendered adoption with ${scenario}`, async () => {
    const padding = createHash('shake256', {outputLength: 6000}).update('padding').digest('hex');
    const files = Object.fromEntries(['en', 'de', 'ar'].map(lang =>
      [`strings/${lang}.json`, JSON.stringify({hello: lang, padding})]));
    files['app.ts'] = "export const grants = ''; export const answer = () => 'server answer';";
    const source = SOURCE.replace('  view', '  resource said = reply(1) as shape string\n  view') +
      '      for i in range(4) key=i\n        text t("hello") testId="row"\n' +
      '      text said testId="said"\n      text t("padding")\n      button press=selectLanguage("de") testId="de"\n        text "Deutsch"\n  action selectLanguage(tag: string)\n    setLocale(tag)\n';
    await renderedPages(source, files, {}, async dist => {
      const held = Promise.withResolvers(), asked = Promise.withResolvers();
      let html = readFileSync(resolve(dist, 'index.html'), 'utf8');
      let repaired = false;
      const checkpoint = /(<script type="application\/vnd.exact.checkpoint"[^>]*?)>([\s\S]*?)<\/script>/.exec(html);
      if (scenario.endsWith('answers')) html = html.replace(checkpoint[0], checkpoint[1] + ' data-src="/answer.json"></script>');
      const arabicPage = scenario === 'missing rendered language' || scenario === 'stalled rendered tables';
      if (arabicPage)
        html = html.replace('<html lang="en" dir="ltr">', '<html lang="ar" dir="rtl">').replace(/(data-testid="(?:hello|row)"[^>]*>)en</g, '$1ar<');
      const server = Bun.serve({hostname: '127.0.0.1', port: 0, async fetch(req) {
        const path = new URL(req.url).pathname;
        if (path === '/') return new Response(html, {headers: {'content-type': 'text/html'}});
        if (path === '/answer.json') {
          asked.resolve();
          if (!repaired && scenario === 'failed answers') return new Response(null, {status: 404, headers: {'Cache-Control': 'public, max-age=3600'}});
          if (!repaired && scenario === 'malformed answers') return Response.json({location: '/', answers: [null]});
          await held.promise;
          return new Response(checkpoint[2]);
        }
        if (scenario === 'changed system language' && path.endsWith('.css')) { asked.resolve(); await held.promise; }
        if (scenario === 'stalled rendered tables' && !repaired && path.startsWith('/strings-')) await held.promise;
        if (scenario === 'missing rendered language' && path.startsWith('/strings-ar-')) return new Response(null, {status: 404});
        return new Response(Bun.file(resolve(dist, path.slice(1))));
      }});
      const browser = await suiteBrowser();
      try {
        const page = await browser.newPage({locale: scenario === 'missing rendered language' ? 'ar' : 'de'});
        page.setDefaultTimeout(8000);
        if (scenario === 'changed system language') await page.addInitScript(() => {
          // Force a slice after the first row and hold it past the first client paint.
          const now = performance.now.bind(performance);
          let elapsed = 0;
          performance.now = () => now() + (elapsed += 20);
          scheduler.postTask = () => new Promise(() => {});
        });
        if (scenario.endsWith('answers')) await page.route('**/ts-data-*.js', () => {});
        await page.goto(server.url.href, {waitUntil: 'domcontentloaded'});
        if (scenario === 'missing rendered language') await page.getByTestId('de').dispatchEvent('input');
        if (scenario === 'changed system language') {
          await asked.promise;
          await page.waitForFunction(async () => (await (await caches.open('exact.tables')).keys()).length > 0);
          await page.evaluate(() => Object.defineProperty(navigator, 'language', {value: 'ar'}));
          held.resolve();
          await page.waitForFunction(() => globalThis.exact?.ready === true);
          expect(await page.evaluate(() => [document.documentElement.lang, document.documentElement.dir,
            document.querySelector('[data-testid=hello]').textContent])).toEqual(['ar', 'rtl', 'ar']);
          expect(await page.getByTestId('row').allTextContents()).toEqual(Array(4).fill('ar'));
        } else {
          await page.waitForFunction(() => globalThis.exact?.journal?.some(line => line.includes('language')) && !document.documentElement.inert);
          const lang = arabicPage ? 'ar' : 'en';
          const result = await page.evaluate(() => [document.documentElement.lang, document.documentElement.dir,
            document.querySelector('[data-testid=hello]').textContent, document.querySelector('[data-testid=said]').textContent,
            exact.ready === true, exact.journal.filter(line => line.includes('language'))]);
          expect(result.slice(0, 5)).toEqual([lang, lang === 'ar' ? 'rtl' : 'ltr', lang, 'server answer', false]);
          expect(result[5]).toHaveLength(1);
          expect(result[5][0]).toMatch(new RegExp(`language ${scenario === 'missing rendered language' ? 'ar' : 'de'}.*(failed|timed out).*${lang}`));
          if (scenario.endsWith('answers') || scenario === 'stalled rendered tables') {
            repaired = true;
            if (scenario !== 'stalled rendered tables') held.resolve();
            if (scenario !== 'held answers') await page.evaluate(() => dispatchEvent(new Event('online')));
            await page.waitForFunction(() => globalThis.exact?.ready === true);
            expect(await page.getByTestId('said').textContent()).toBe('server answer');
            expect(await page.getByTestId('hello').textContent()).toBe('de');
            expect(await page.evaluate(() => exact.journal.filter(line => line.includes('language')))).toHaveLength(1);
          }
        }
      } finally { held.resolve(); await browser.close(); server.stop(true); }
    }, ['--production']);
  }, 60000);

for (const scenario of ['two stalled tables', 'three stalled tables', 'a table requested at 4999 ms'])
  engineTest.chromium(`the client shell reveals at the launch deadline with ${scenario}`, async () => {
    const padding = createHash('shake256', {outputLength: 6000}).update('padding').digest('hex');
    const files = Object.fromEntries(['en', 'de', 'ar', 'fr'].map(tag =>
      [`strings/${tag}.json`, JSON.stringify({hello: tag, padding})]));
    const source = SOURCE.replace('  view', '  state count = 0\n  action increment\n    count = count + 1\n  view') +
      '      text t("padding")\n      button press=increment testId="counter"\n        text \`${count}\`\n';
    await renderedPages(source, files, {}, async dist => {
      const entry = Promise.withResolvers(), release = Promise.withResolvers();
      const seen = new Map(['de', 'ar', 'fr'].map(tag => [tag, Promise.withResolvers()]));
      const server = Bun.serve({hostname: '127.0.0.1', port: 0, async fetch(request) {
        const file = new URL(request.url).pathname.slice(1);
        if (/^app-.*\.js$/.test(file)) await entry.promise;
        const tag = /^strings-([a-z]+)-/.exec(file)?.[1];
        if (tag) { seen.get(tag).resolve(); await release.promise; }
        return new Response(Bun.file(resolve(dist, file)));
      }});
      const browser = await suiteBrowser();
      try {
        const page = await browser.newPage({locale: 'de'});
        const systemLanguage = tag => page.evaluate(tag => {
          Object.defineProperty(navigator, 'language', {configurable: true, value: tag});
          dispatchEvent(new Event('languagechange'));
        }, tag);
        await page.clock.install({time: 0});
        await page.clock.pauseAt(1000);
        await page.goto(server.url.href + 'shell.html', {waitUntil: 'commit'});
        await seen.get('de').promise;
        if (scenario === 'three stalled tables') await systemLanguage('ar');
        if (scenario === 'a table requested at 4999 ms') {
          await page.clock.fastForward(4999);
          await systemLanguage('fr');
        }
        entry.resolve();
        await page.waitForLoadState('domcontentloaded');
        if (scenario === 'three stalled tables') await seen.get('ar').promise;
        if (scenario === 'a table requested at 4999 ms') await seen.get('fr').promise;
        else {
          await systemLanguage('fr');
          await page.clock.fastForward(4999);
        }
        const content = () => page.evaluate(() => [
          document.querySelector('[data-testid=hello]')?.textContent ?? null, globalThis.exact?.ready === true,
        ]);
        expect(await content()).toEqual([null, false]);
        await page.clock.runFor(1);
        expect(await content()).toEqual(['en', true]);
        await seen.get('fr').promise;
        await page.getByTestId('counter').dispatchEvent('click');
        expect(await page.getByTestId('counter').textContent()).toBe('1');
        await page.clock.resume();
        release.resolve();
        await page.waitForFunction(() => document.querySelector('[data-testid=hello]')?.textContent === 'fr');
        expect(await page.getByTestId('counter').textContent()).toBe('1');
      } finally { entry.resolve(); release.resolve(); await browser.close(); server.stop(true); }
    }, ['--production']);
  }, 60000);
