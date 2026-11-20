import { test, expect } from 'bun:test';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createServer } from 'node:http';
import { withoutBase, withBase, mountedHref } from './route-pattern.js';
import { readManifest, webAppManifest } from '../../scripts/app.mjs';
import { webManifestAtBase } from '../../scripts/web-head.mjs';
import { installData } from '../../scripts/install-page.mjs';
import { serveBuildTree } from '../web/serve.mjs';
import { renderer, sitePages } from './render.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { engineTests } from '../../scripts/coverage.mjs';

const engineTest = await engineTests();

const root = resolve(import.meta.dir, '../..');
const manifest = base => ({ name: 'Mounted', app: { id: 'test.mounted', name: 'Mounted', origin: 'https://site.invalid' }, host: { web: base === undefined ? {} : { base } } });
const workspace = () => mkdtempSync(resolve(root, 'target/base-path-'));
const save = (dir, name, text) => writeFileSync(resolve(dir, name), text);
async function build(dir, args = []) {
  const child = Bun.spawn([process.execPath, 'host/web-js/build.mjs', dir, '--out', resolve(dir, 'dist'), ...(args.includes('--render') ? [] : ['--render', 'js']), ...args], { cwd: root, stdout: 'pipe', stderr: 'pipe' });
  const [status, out, err] = await Promise.all([child.exited, new Response(child.stdout).text(), new Response(child.stderr).text()]);
  return { status, output: out + err };
}
async function serving(dist, run) {
  const pages = sitePages(dist), server = createServer((req, res) => serveBuildTree(dist, req, res, { pages }));
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  try { await run(`http://127.0.0.1:${server.address().port}`); }
  finally { server.closeAllConnections(); await new Promise(ok => server.close(ok)); }
}

test('a mount strips only whole path segments and prefixes even a route named like its mount', () => {
  for (const [location, want] of [['/app', '/'], ['/app/', '/'], ['/app?q=1#x', '/?q=1#x'], ['/app/book/a%2Fb?q=1#x', '/book/a%2Fb?q=1#x'], ['/application', null], ['/book/a', null], ['/app%2Fbook/a', null]]) expect(withoutBase(location, '/app/')).toBe(want);
  expect(withBase('/app/book/a', '/app/')).toBe('/app/app/book/a');
  expect(withBase('/book/a?q=1#x', '/')).toBe('/book/a?q=1#x');
});

test('the manifest accepts canonical mounts and refuses paths that a browser would reinterpret', () => {
  const dir = workspace();
  try {
    for (const base of ['/', '/app/', '/one/two/', '/v1.2/']) {
      save(dir, 'app.json', JSON.stringify(manifest(base)));
      expect(readManifest(dir, 'mounted').host.web.base).toBe(base);
    }
    for (const base of ['', 'app/', '/app', '//host/', '/a//b/', '/./', '/a/../', '/%2e/', '/a?b/', '/a#b/', '/a\\b/', '/a"b/', '/a b/']) {
      save(dir, 'app.json', JSON.stringify(manifest(base)));
      expect(() => readManifest(dir, 'mounted'), base).toThrow('host.web.base');
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('manifest and install destinations mount once without changing the author manifest', () => {
  const app = { ...manifest('/app/'), id: 'test.mounted', start_url: '/app', shortcuts: [{ name: 'Item', url: '/book/1', icons: [{ src: 'assets/icon.png' }] }], icons: [{ src: 'assets/icon.png' }] };
  const before = JSON.stringify(app), web = webAppManifest(app, 'Mounted');
  expect(web).toMatchObject({ id: '/app/test.mounted', start_url: '/app/app', scope: '/app/', shortcuts: [{ url: '/app/book/1', icons: [{ src: '/app/assets/icon.png' }] }] });
  expect(installData(app).platforms.web.methods[0].url).toBe('/app/app');
  expect(JSON.stringify(app)).toBe(before);
  expect(webAppManifest(app, 'Mounted')).toEqual(web);
});

test('mounted manifest destinations accept encoded characters and reject dot segments', () => {
  for (const path of ['/café', '/a b', '/caf%C3%A9', '/a%20b', '/open?u=%s']) {
    const web = webManifestAtBase({ start_url: path, id: path, icons: [{ src: path }],
      share_target: { action: path }, file_handlers: [{ action: path }], protocol_handlers: [{ url: path }],
      shortcuts: [{ url: path, icons: [{ src: path }] }] }, '/app/');
    expect(web).toEqual({ start_url: '/app' + path, scope: '/app/', id: '/app' + path,
      icons: [{ src: '/app' + path }], share_target: { action: '/app' + path },
      file_handlers: [{ action: '/app' + path }], protocol_handlers: [{ url: '/app' + path }],
      shortcuts: [{ url: '/app' + path, icons: [{ src: '/app' + path }] }] });
  }
  for (const path of ['/./x', '/../x', '/a/../b', '/%2e/x', '/%2E%2e/x', '/.%2e/x', '/%2e./x', '/a/..', '/a\\..\\b', '/a/..\tb']) {
    expect(() => webManifestAtBase({ start_url: path }, '/app/'), path).toThrow('host.web.base');
  }
});

const source = `routes nav
  home "/" render=build
    item "/item" render=request
    kept "/kept" render=cached
  notfound render=request

component App
  action follow(url: string)
    nav = go(nav, url)
  action replace
    nav = replace(nav, path("kept") + "?q=1#part")
  action back
    nav = back(nav)
  view
    main navigationKey=\`\${top(nav).id}\` navigate=follow
      for e in stack(nav) key=e.id
        column navigationKey=\`\${e.id}\`
          head title=e.name
          text e.name testId=\`name-\${e.name}\`
          link href=path("item") testId=\`item-\${e.name}\`
            text "Item"
          button "Replace" press=replace testId=\`replace-\${e.name}\`
          button "Back" press=back testId=\`back-\${e.name}\`
          text "Part" id="part"
`;

engineTest.chromium('build, request and cached pages, generation assets and install pages stay under the mount', async () => {
  const dir = workspace(); let browser;
  try {
    save(dir, 'app.json', JSON.stringify(manifest('/app/'))); save(dir, 'app.contract', source.replace('item "/item" render=request', 'item "/item" render=request activate=interaction').replace('kept "/kept" render=cached', 'kept "/kept" render=cached activate=interaction').replace('component App\n', 'component App\n  resource message = message() as shape string else empty()\n').replace('          head title=e.name', '          text message\n          head title=e.name'));
    save(dir, 'app.ts', "export const grants = '';\nexport function answer(): string { return 'x'.repeat(2048); }\n");
    const made = await build(dir); expect(made.status, made.output).toBe(0);
    const dist = resolve(dir, 'dist');
    expect(readFileSync(resolve(dist, 'index.html'), 'utf8')).toContain('href="/app/item"');
    expect(readFileSync(resolve(dist, 'sitemap.xml'), 'utf8')).toContain('https://site.invalid/app/');
    save(dist, '.exact-build.json', JSON.stringify({ exactBuild: 1, target: 'js', files: [{ name: 'app.js', sha256: 'base-test' }] }));
    await serving(dist, async origin => {
      for (const path of ['/', '/item', '/application/', '/.exact/install/', '/app.js']) expect((await fetch(origin + path)).status, path).toBe(404);
      for (const name of ['item', 'kept']) {
        const response = await fetch(`${origin}/app/${name}`), html = await response.text();
        expect(response.status).toBe(200); expect(html).toContain(`<title>${name}</title>`); expect(html).toContain('href="/app/item"');
        const checkpoint = /data-src="([^"]+)"/.exec(html)?.[1];
        expect(checkpoint).toStartWith('/app/.exact/checkpoint/');
        const answers = await fetch(origin + checkpoint); expect(answers.status).toBe(200);
        expect((await answers.json()).location).toBe(`/app/${name}`);
        expect(html).toContain('<base href="/app/.exact/gen/');
      }
      const installed = await fetch(origin + '/app/.exact/install/');
      expect(installed.status).toBe(200); expect(await installed.text()).toContain('href="/app/"');
      browser = await suiteBrowser(); const page = await browser.newPage(), errors = [], requests = [];
      page.on('pageerror', error => errors.push(error.message)); page.on('request', request => requests.push(new URL(request.url()).pathname));
      await page.goto(origin + '/app/item'); await page.getByTestId('name-item').waitFor();
      await page.getByTestId('replace-item').click(); await page.waitForURL('**/app/kept?q=1#part');
      // The page at its own location under the mount is adopted (LLP 1048.000 D6)
      expect(await page.evaluate(() => exact.journal.filter(l => / document: | adopted the document$/.test(l)).map(l => l.slice(l.indexOf(' ') + 1)))).toEqual(['adopted the document']);
      await page.getByTestId('back-kept').click(); await page.waitForURL('**/app/');
      await page.getByTestId('item-home').click(); await page.waitForURL('**/app/item');
      await page.goBack(); await page.waitForURL('**/app/');
      await page.goForward(); await page.waitForURL('**/app/item');
      expect(errors).toEqual([]); expect(requests.every(path => path.startsWith('/app/'))).toBe(true);
    });
  } finally { await browser?.close(); rmSync(dir, { recursive: true, force: true }); }
}, 120000);

engineTest.chromium.each([false, true])('bundled fetches without a network grant stay in the mounted generation (production: %s)', async production => {
  const dir = workspace(); let browser;
  try {
    save(dir, 'app.json', JSON.stringify(manifest('/app/')));
    save(dir, 'app.contract', 'component App\n  resource message = message() as shape string\n  view\n    text message testId="message"\n');
    save(dir, 'app.ts', "export const grants = '';\nexport async function answer() { const r = await fetch('/assets/a.txt'); return r.status + ': ' + await r.text(); }\n");
    mkdirSync(resolve(dir, 'assets')); save(dir, 'assets/a.txt', 'bundled asset');
    const made = await build(dir, ['--render', 'none', ...(production ? ['--production'] : [])]); expect(made.status, made.output).toBe(0);
    const dist = resolve(dir, 'dist');
    save(dist, '.exact-build.json', JSON.stringify({ exactBuild: 1, target: 'js', files: [{ name: 'app.js', sha256: 'asset-test' }] }));
    await serving(dist, async origin => {
      browser = await suiteBrowser(); const page = await browser.newPage(), assets = [];
      page.on('request', request => { if (request.url().endsWith('/assets/a.txt')) assets.push(new URL(request.url()).pathname); });
      await page.goto(origin + '/app/');
      await page.waitForFunction(() => document.querySelector('[data-testid="message"]')?.textContent.includes(':'));
      expect(await page.getByTestId('message').textContent()).toBe('200: bundled asset');
      expect(assets).toHaveLength(1); expect(assets[0]).toMatch(/^\/app\/\.exact\/gen\/[a-f0-9]{32}\/assets\/a\.txt$/);
    });
  } finally { await browser?.close(); rmSync(dir, { recursive: true, force: true }); }
}, 120000);

engineTest.chromium.each(['item', '/../item'])('literal navigation canonicalizes %s before mounting and keeps its fragment', async location => {
  const dir = workspace(); let browser;
  try {
    save(dir, 'app.json', JSON.stringify(manifest('/app/')));
    save(dir, 'app.contract', source.replace('  action replace', `  action literal\n    nav = push(nav, "${location}?q=1#part")\n  action replace`)
      .replace('          head title=e.name', '          button "Literal" press=literal testId=`literal-${e.name}`\n          head title=e.name'));
    const made = await build(dir, ['--render', 'none']); expect(made.status, made.output).toBe(0);
    await serving(resolve(dir, 'dist'), async origin => {
      browser = await suiteBrowser(); const page = await browser.newPage();
      await page.goto(origin + '/app/'); await page.getByTestId('literal-home').click();
      await page.waitForFunction(() => location.search === '?q=1');
      expect(new URL(page.url()).pathname + new URL(page.url()).search + new URL(page.url()).hash).toBe('/app/item?q=1#part');
      await page.getByTestId('name-item').waitFor();
    });
  } finally { await browser?.close(); rmSync(dir, { recursive: true, force: true }); }
}, 120000);

test('the server redirects the slashless mount root and keeps its query', async () => {
  const dir = workspace();
  try {
    save(dir, 'routes.json', JSON.stringify({ base: '/app/', routes: [] }));
    save(dir, 'index.html', '<p>Home</p>');
    await serving(dir, async origin => {
      for (const query of ['', '?q=1']) {
        const response = await fetch(origin + '/app' + query, { redirect: 'manual' });
        expect(response.status).toBe(308); expect(response.headers.get('location')).toBe('/app/' + query);
      }
    });
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

engineTest.chromium.each([false, true])('slashless mount roots open home in the browser (static fallback: %s)', async fallback => {
  const dir = workspace(); let browser;
  try {
    save(dir, 'app.json', JSON.stringify(manifest('/app/'))); save(dir, 'app.contract', source);
    const made = await build(dir, ['--render', 'none']); expect(made.status, made.output).toBe(0);
    await serving(resolve(dir, 'dist'), async origin => {
      browser = await suiteBrowser(); const page = await browser.newPage();
      if (fallback) await page.route(url => url.pathname === '/app', route => route.fulfill({ contentType: 'text/html', body: readFileSync(resolve(dir, 'dist/shell.html'), 'utf8') }));
      for (const query of ['', '?q=1']) {
        await page.goto(origin + '/app' + query);
        await page.waitForFunction(() => document.querySelector('[data-testid="name-home"], [data-testid="name-notfound"]'));
        expect(await page.getByTestId('name-notfound').count()).toBe(0);
        expect(new URL(page.url()).pathname + new URL(page.url()).search).toBe('/app/' + query);
      }
    });
  } finally { await browser?.close(); rmSync(dir, { recursive: true, force: true }); }
}, 120000);

test('an explicit root base produces exactly the default build bytes', async () => {
  const dir = workspace();
  const files = dir => readdirSync(dir, { withFileTypes: true }).flatMap(entry => entry.name === '.gen' ? [] : entry.isDirectory() ? files(resolve(dir, entry.name)).map(([name, bytes]) => [entry.name + '/' + name, bytes]) : [[entry.name, readFileSync(resolve(dir, entry.name)).toString('base64')]]).sort(([a], [b]) => a.localeCompare(b));
  try {
    save(dir, 'app.contract', source); save(dir, 'app.json', JSON.stringify(manifest()));
    let made = await build(dir, ['--production']); expect(made.status, made.output).toBe(0);
    const before = files(resolve(dir, 'dist'));
    save(dir, 'app.json', JSON.stringify(manifest('/')));
    made = await build(dir, ['--production']); expect(made.status, made.output).toBe(0);
    expect(files(resolve(dir, 'dist'))).toEqual(before);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 120000);

engineTest.chromium('Shelf deep links, path links and pushes work under /app/, and an outside location draws notfound', async () => {
  const dir = workspace(); let browser;
  try {
    for (const name of ['app.json', 'app.contract', 'app.ts', 'assets']) cpSync(resolve(root, 'apps/shelf', name), resolve(dir, name), { recursive: true });
    const app = JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8')); app.host.web.base = '/app/'; delete app.preload;
    save(dir, 'app.json', JSON.stringify(app));
    const made = await build(dir, ['--render', 'none']); expect(made.status, made.output).toBe(0);
    await serving(resolve(dir, 'dist'), async origin => {
      browser = await suiteBrowser(); const page = await browser.newPage(), errors = [];
      page.on('pageerror', error => errors.push(error.message));
      await page.route('https://openlibrary.org/**', route => route.fulfill({ json: { works: [], entries: [] } }));
      await page.goto(origin + '/app/book/seed-2'); await page.getByTestId('book-detail').waitFor();
      await page.getByRole('heading', { name: 'Piranesi', exact: true }).waitFor();
      await page.goto(origin + '/app/'); await page.getByTestId('book-seed-2').waitFor();
      expect(await page.getByTestId('book-seed-2').getAttribute('href')).toBe('/app/book/seed-2');
      await page.getByTestId('book-seed-2').click(); await page.waitForURL('**/app/book/seed-2');
      await page.goBack(); await page.waitForURL('**/app/');
      await page.getByTestId('add-book').click(); await page.waitForURL('**/app/add');
      await page.getByTestId('add-cancel').click(); await page.waitForURL('**/app/');
      // A static host with one fallback page can deliver the shell even outside the mount.
      await page.route(origin + '/book/seed-2', route => route.fulfill({ contentType: 'text/html', body: readFileSync(resolve(dir, 'dist/shell.html'), 'utf8') }));
      await page.goto(origin + '/book/seed-2'); await page.getByText('Page not found', { exact: true }).waitFor();
      expect(new URL(page.url()).pathname).toBe('/book/seed-2'); expect(errors).toEqual([]);
    });
  } finally { await browser?.close(); rmSync(dir, { recursive: true, force: true }); }
}, 120000);

test('unsupported renderers, update bakes and authentication refuse a non-root base', async () => {
  const dir = workspace();
  try {
    save(dir, 'app.json', JSON.stringify(manifest('/app/'))); save(dir, 'app.contract', source);
    for (const [args, message] of [[['--plan', resolve(dir, 'absent.plan')], 'native update bakes require /'], [['--render', 'rust'], 'Rust renderer requires /']]) {
      const made = await build(dir, args); expect(made.status).not.toBe(0); expect(made.output).toContain(message);
    }
    for (const [script, args, message] of [
      ['host/web/build.mjs', [dir, '--wasm', '--out', resolve(dir, 'wasm')], 'wasm target and native update store require /'],
      ['scripts/deploy.mjs', [dir], 'signed update store require /'],
      ['host/web/dev.mjs', ['--app', dir], 'web/native development loop requires /'],
    ]) {
      const child = Bun.spawn([process.execPath, script, ...args], { cwd: root, stdout: 'pipe', stderr: 'pipe' });
      const [status, out, err] = await Promise.all([child.exited, new Response(child.stdout).text(), new Response(child.stderr).text()]);
      expect(status, out + err).not.toBe(0); expect(out + err).toContain(message);
    }
    save(dir, 'app.ts', "export const grants = 'auth.session https://login.invalid\\nauth.callback test.mounted:/oauth';\nexport function answer(): string { return ''; }\n");
    const made = await build(dir); expect(made.status).not.toBe(0); expect(made.output).toContain('auth callbacks currently require /');
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 120000);

test('a mounted build publishes preferred native link claims', async () => {
  const dir = workspace();
  try {
    save(dir, 'app.json', JSON.stringify({ ...manifest('/app/'), handle_links: 'preferred' }));
    save(dir, 'app.contract', source);
    const made = await build(dir, ['--render', 'none']);
    expect(made.status, made.output).toBe(0);
    expect(JSON.parse(readFileSync(resolve(dir, 'dist/manifest.json'), 'utf8'))).toMatchObject({ handle_links: 'preferred', scope: '/app/' });
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 120000);

engineTest.chromium.each([' https://example.com/item', '\tht\ntps://example.com/item '])('a bound external link with URL whitespace leaves the mounted app: %j', async destination => {
  const dir = workspace(); let browser;
  try {
    save(dir, 'app.json', JSON.stringify(manifest('/app/')));
    save(dir, 'app.contract', source.replace('component App\n', `component App\n  state destination = ${JSON.stringify(destination)}\n`)
      .replace('          head title=e.name', '          link href=destination target="_self" testId="external"\n            text "External"\n          head title=e.name'));
    const made = await build(dir, ['--render', 'none']); expect(made.status, made.output).toBe(0);
    await serving(resolve(dir, 'dist'), async origin => {
      browser = await suiteBrowser(); const page = await browser.newPage();
      await page.route('https://example.com/**', route => route.fulfill({ contentType: 'text/html', body: '<h1>External page</h1>' }));
      await page.goto(origin + '/app/');
      expect(await page.getByTestId('external').getAttribute('href')).toBe('https://example.com/item');
      await page.getByTestId('external').click();
      await page.waitForURL('https://example.com/item', { timeout: 5000 });
      expect(await page.getByRole('heading').textContent()).toBe('External page');
    });
  } finally { await browser?.close(); rmSync(dir, { recursive: true, force: true }); }
}, 120000);

test('an explicit manifest scope and the start URL default stay narrowed under the mount', () => {
  for (const start_url of ['/notes/', '/notes/index.html']) {
    const app = { ...manifest('/app/'), start_url };
    expect(webAppManifest({ ...app, scope: '/notes/' }, 'Mounted').scope).toBe('/app/notes/');
    expect(webAppManifest(app, 'Mounted').scope).toBe('/app/notes/');
  }
});

test('scope-extension instructions use the published web identity without native claims', async () => {
  const dir = workspace();
  try {
    const app = { ...manifest('/app/'), id: 'notes', start_url: '/notes/', scope: '/notes/', handle_links: 'auto', scope_extensions: [{ type: 'origin', origin: 'https://other.invalid' }] };
    app.host.ios = { team: 'TESTTEAM' };
    app.host.android = { certificates: ['AB:'.repeat(31) + 'AB'] };
    save(dir, 'app.json', JSON.stringify(app)); save(dir, 'app.contract', 'component App\n  view\n    text "Notes"\n');
    const made = await build(dir, ['--render', 'none']); expect(made.status, made.output).toBe(0);
    const published = JSON.parse(readFileSync(resolve(dir, 'dist/manifest.json'), 'utf8'));
    const identity = new URL(published.id, new URL(published.start_url, app.app.origin).origin).href;
    expect(identity).toBe('https://site.invalid/app/notes');
    expect(made.output).toContain(`web-app-origin-association ${JSON.stringify({ [identity]: {} })}`);
    expect(made.output).not.toContain('open the app');
    expect(made.output).not.toContain('apple-app-site-association');
    expect(made.output).not.toContain('assetlinks.json');
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 120000);

test.each(['request', 'cached'])('a dotted mount renders its %s home for Accept */*', async render => {
  const dir = workspace();
  try {
    save(dir, 'app.json', JSON.stringify(manifest('/v1.2/')));
    save(dir, 'app.contract', source.replace('home "/" render=build', `home "/" render=${render}`));
    const made = await build(dir); expect(made.status, made.output).toBe(0);
    await serving(resolve(dir, 'dist'), async origin => {
      for (const method of ['GET', 'HEAD']) {
        const response = await fetch(origin + '/v1.2/', { method, headers: { accept: '*/*' } });
        expect(response.status).toBe(200);
        if (method === 'GET') expect(/<title>([^<]*)<\/title>/.exec(await response.text())?.[1]).toBe('home');
      }
    });
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 120000);

engineTest.chromium.each(['links', 'parameter push'])('the routes corpus keeps %s and browser history under /app/', async mode => {
  const dir = workspace(); let browser;
  try {
    save(dir, 'app.json', JSON.stringify(manifest('/app/')));
    save(dir, 'app.ts', "export const grants = ''; export function answer() { return []; }\n");
    save(dir, 'app.contract', readFileSync(resolve(root, 'contract/corpus/routes.contract'), 'utf8')
      .replace('  tab prompts', '    login "/login"\n    article "/article/:slug"\n    overlap "/app/item"\n  tab prompts')
      .replace('  state initialUrl', '  state slug = "one"\n  action changeSlug\n    slug = "two"\n  state initialUrl')
      .replace('          button appearance="none" id="back"', '          text e.name testId=`name-${e.name}`\n          text e.url testId=`url-${e.name}`\n          link href="/login" testId=`login-${e.name}`\n            text "Login"\n          link href=`/article/${slug}` testId=`article-${e.name}`\n            text "Article"\n          link href=path("overlap") testId=`overlap-${e.name}`\n            text "Overlap"\n          button "Change slug" press=changeSlug testId=`slug-${e.name}`\n          button appearance="none" id="back"'));
    const made = await build(dir, ['--render', 'none']); expect(made.status, made.output).toBe(0);
    await serving(resolve(dir, 'dist'), async origin => {
      browser = await suiteBrowser(); const page = await browser.newPage(), errors = [];
      page.on('pageerror', error => errors.push(error.message));
      await page.goto(origin + '/app/'); await page.getByTestId('name-home').waitFor();
      if (mode === 'links') {
        expect(await page.getByTestId('login-home').getAttribute('href')).toBe('/app/login');
        expect(await page.getByTestId('article-home').getAttribute('href')).toBe('/app/article/one');
        await page.getByTestId('slug-home').click();
        expect(await page.getByTestId('article-home').getAttribute('href')).toBe('/app/article/two');
        await page.getByTestId('login-home').click(); await page.waitForURL('**/app/login', { timeout: 5000 });
        await page.getByTestId('article-login').click(); await page.waitForURL('**/app/article/two', { timeout: 5000 });
        expect(await page.getByTestId('url-article').textContent()).toBe('/article/two');
        await page.goBack(); await page.waitForURL('**/app/login');
        await page.goForward(); await page.waitForURL('**/app/article/two');
        await page.getByTestId('overlap-article').click(); await page.waitForURL('**/app/app/item');
        expect(await page.getByTestId('url-overlap').textContent()).toBe('/app/item');
      } else {
        await page.getByTestId('push-post-0').click(); await page.waitForURL('**/app/post/42', { timeout: 5000 });
        expect(await page.getByTestId('url-post').textContent()).toBe('/post/42');
        await page.goBack(); await page.waitForURL('**/app/');
        await page.goForward(); await page.waitForURL('**/app/post/42');
      }
      expect(await page.evaluate(() => history.state.url)).toBe(new URL(page.url()).pathname);
      await page.reload(); await page.getByTestId(mode === 'links' ? 'name-overlap' : 'name-post').waitFor();
      await page.goBack(); await page.waitForURL(mode === 'links' ? '**/app/article/two' : '**/app/');
      expect(errors).toEqual([]);
    });
  } finally { await browser?.close(); rmSync(dir, { recursive: true, force: true }); }
}, 120000);


engineTest.chromium.each(['media', 'markdown', 'head', 'head-whitespace', 'one-off'])('mounted URL boundary: %s before activation', async kind => {
  const dir = workspace(); let browser;
  try {
    save(dir, 'app.json', JSON.stringify(manifest('/app/')));
    const added = kind === 'media' ? '          image "/assets/pixel.svg" testId="picture"\n          image "./assets/pixel.svg" testId="relative-picture"\n          image "/assets/../assets/pixel.svg" testId="dot-picture"\n          image "/assets/../../outside.svg" testId="escape-picture"\n          video poster="/assets/pixel.svg" testId="movie"\n'
      : kind === 'markdown' ? '          text md markup="markdown"\n          button "Change Markdown" press=changeMarkdown testId=`markdown-${e.name}`\n' : '';
    save(dir, 'app.contract', source.replace('component App\n', kind === 'markdown' ? 'component App\n  state md = "[Markdown item](/item)"\n  action changeMarkdown\n    md = "[Changed Markdown](/kept)"\n' : 'component App\n').replace('          head title=e.name', added + (kind.startsWith('head')
      ? '          head title=e.name canonical=(e.name == "home" ? "/item" : e.name == "kept" ? "#part" : "https://else.invalid/item") image=(e.name == "home" ? "/assets/pixel.svg" : e.name == "kept" ? "?version=1" : "https://else.invalid/pixel.svg")' : '          head title=e.name')));
    if (kind === 'head-whitespace') save(dir, 'app.contract', readFileSync(resolve(dir, 'app.contract'), 'utf8').replace('\"#part\"', '\" \t#part \"').replace('\"?version=1\"', '\" \t?version=1 \"'));
    mkdirSync(resolve(dir, 'assets')); save(dir, 'assets/pixel.svg', '<svg xmlns="http://www.w3.org/2000/svg" width="2" height="2"><rect width="2" height="2" fill="red"/></svg>');
    const made = await build(dir); expect(made.status, made.output).toBe(0);
    const dist = resolve(dir, 'dist'), html = readFileSync(resolve(dist, 'index.html'), 'utf8');
    if (kind === 'media') {
      expect(html).toContain('src="./assets/pixel.svg"'); expect(html).toContain('poster="./assets/pixel.svg"');
      expect(html).not.toContain('src="/app/assets/pixel.svg"');
      expect(html).not.toContain('/../'); expect(html).toContain('src="/app/outside.svg"');
      // A static host has no generation rewriting or router fallback.
      const server = createServer((req, res) => {
        const path = req.url.split('?')[0];
        if (!path.startsWith('/app/')) { res.writeHead(404); res.end(); return; }
        try { res.setHeader('content-type', path.endsWith('.svg') ? 'image/svg+xml' : path.endsWith('.js') ? 'text/javascript' : 'text/html'); res.end(readFileSync(resolve(dist, path.slice(5) || 'index.html'))); }
        catch { res.writeHead(404); res.end(); }
      });
      await new Promise(ok => server.listen(0, '127.0.0.1', ok));
      try {
        browser = await suiteBrowser(); const page = await browser.newPage({ javaScriptEnabled: false });
        await page.goto(`http://127.0.0.1:${server.address().port}/app/`);
        expect(await page.getByTestId('picture').evaluate(e => e.complete && e.naturalWidth > 0)).toBe(true);
      } finally { server.closeAllConnections(); await new Promise(ok => server.close(ok)); }
      save(dist, '.exact-build.json', JSON.stringify({ exactBuild: 1, target: 'js', files: [{ name: 'app.js', sha256: 'media-test' }] }));
      await serving(dist, async origin => {
        browser ??= await suiteBrowser(); const page = await browser.newPage();
        await page.goto(origin + '/app/item');
        await page.waitForFunction(() => document.querySelector('[data-testid="picture"]')?.naturalWidth > 0);
        await page.getByTestId('replace-item').click(); await page.waitForURL('**/app/kept?q=1#part');
        for (const id of ['picture', 'relative-picture', 'dot-picture']) expect(await page.getByTestId(id).last().evaluate(e => new URL(e.src).pathname)).toMatch(/^\/app\/\.exact\/gen\/[a-f0-9]{32}\/assets\/pixel.svg$/);
        expect(await page.getByTestId('movie').last().evaluate(e => new URL(e.poster).pathname)).toContain('/assets/pixel.svg');
      });
    } else if (kind === 'markdown') {
      expect(html).toContain('href="/app/item"');
      expect(html).not.toContain('href="/item"');
      await serving(dist, async origin => {
        browser = await suiteBrowser(); const page = await browser.newPage(); await page.goto(origin + '/app/');
        await page.getByRole('link', { name: 'Markdown item' }).click(); await page.waitForURL('**/app/item', { timeout: 5000 });
        await page.getByTestId('name-item').waitFor();
        await page.getByTestId('markdown-item').click();
        await page.getByRole('link', { name: 'Changed Markdown' }).click(); await page.waitForURL('**/app/kept', { timeout: 5000 });
        await page.getByTestId('name-kept').waitFor();
      });
    } else if (kind.startsWith('head')) {
      for (const tag of ['<link rel="canonical" href="https://site.invalid/app/item">', '<meta property="og:url" content="https://site.invalid/app/item">', '<meta property="og:image" content="https://site.invalid/app/assets/pixel.svg">', '<meta name="twitter:image" content="https://site.invalid/app/assets/pixel.svg">']) expect(html).toContain(tag);
      const other = (await renderer(dist)('/app/item')).html;
      expect(other).toContain('href="https://else.invalid/item"');
      const references = (await renderer(dist)('/app/kept')).html;
      expect(references).toContain('href="https://site.invalid/app/#part"');
      for (const name of ['og:image', 'twitter:image']) expect(references).toContain(`${name}" content="https://site.invalid/app/?version=1"`);
      for (const name of ['og:image', 'twitter:image']) expect(other).toContain(`${name}" content="https://else.invalid/pixel.svg"`);
    } else {
      const child = Bun.spawn([process.execPath, 'host/web-js/render.mjs', dist, '/item'], { cwd: root, stdout: 'pipe', stderr: 'pipe' });
      const [status, out, err] = await Promise.all([child.exited, new Response(child.stdout).text(), new Response(child.stderr).text()]);
      expect(status, err).toBe(0); expect(out).toContain('<title>item</title>');
    }
  } finally { await browser?.close(); rmSync(dir, { recursive: true, force: true }); }
}, 120000);


for (const [browser, url] of [['chrome', undefined], ['chrome', '/item'], ['firefox', undefined], ['webkit', undefined]]) engineTest[browser](`the ${browser} driver opens the mounted launch location${url ? ` ${url}` : ''}`, async () => {
  const dir = workspace(); let session;
  try {
    save(dir, 'app.json', JSON.stringify(manifest('/app/'))); save(dir, 'app.contract', source);
    const child = Bun.spawn([process.execPath, 'host/web/build.mjs', dir, '--render', 'none'], { cwd: root, env: { ...process.env, EXACT_WEB_DIST: resolve(dir, 'dist') }, stdout: 'pipe', stderr: 'pipe' });
    const [status, out, err] = await Promise.all([child.exited, new Response(child.stdout).text(), new Response(child.stderr).text()]);
    expect(status, out + err).toBe(0);
    const { open } = await import('../../scripts/agent-session.mjs');
    session = await open({ host: 'web', browser, app: dir, webDist: resolve(dir, 'dist'), url, env: { XDG_CACHE_HOME: resolve(dir, 'cache') } });
    const tree = JSON.stringify(await session.tree());
    expect(tree).toContain(url ? 'name-item' : 'name-home');
  } finally { await session?.close(); rmSync(dir, { recursive: true, force: true }); }
}, 120000);


test('mounted destinations retain external URLs and canonicalize paths before adding the mount', () => {
  for (const value of ['https://else.invalid/item', '//else.invalid/item', 'mailto:a@b.invalid', 'data:image/png;base64,AA==', 'blob:https://site.invalid/id', 'app:/picture', 'symbol:star', '#part', '?q=1']) expect(mountedHref(value, '/app/')).toBe(value);
  for (const value of ['/../item', '/a/%2e%2e/item', 'a/../item', '/a\\..\\item']) expect(mountedHref(value + '?q=1#part', '/app/')).toBe('/app/item?q=1#part');
});

test('mounted destinations normalize URL whitespace before classifying the destination', () => {
  for (const [value, expected] of [
    ['\u0000 https://example.com/item \u001f', 'https://example.com/item'],
    ['h\tt\rt\nps://example.com/item', 'https://example.com/item'],
    [' \t//example.com/item\r', '//example.com/item'],
    [' \n#part ', '#part'], [' \t?q=1 ', '?q=1'],
    [' /it\tem?q=1#part ', '/app/item?q=1#part'],
    [' java\nscript:alert(1) ', 'javascript:alert(1)'],
    [' \u0000\r\n\t', ''],
  ]) expect(mountedHref(value, '/app/')).toBe(expected);
  expect(mountedHref(null, '/app/')).toBeNull();
  expect(mountedHref(undefined, '/app/')).toBeUndefined();
});

test.each(['image "/assets/pixel.svg" srcset="/assets/pixel.svg 1x"', 'form action="/item"'])('unadmitted URL attributes fail compilation: %s', async node => {
  const dir = workspace();
  try {
    save(dir, 'app.json', JSON.stringify(manifest('/app/'))); save(dir, 'app.contract', `component App\n  view\n    ${node}\n`);
    const made = await build(dir, ['--render', 'none']);
    expect(made.status).not.toBe(0); expect(made.output).toMatch(/has no attribute|requires a button inside a canvas/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 120000);

test('association discovery stays at the origin root beside the site mount', async () => {
  const dir = workspace();
  try {
    save(dir, 'routes.json', JSON.stringify({ base: '/app/', routes: [] }));
    mkdirSync(resolve(dir, '.well-known'));
    for (const name of ['apple-app-site-association', 'assetlinks.json']) save(dir, '.well-known/' + name, '{}');
    await serving(dir, async origin => {
      for (const name of ['apple-app-site-association', 'assetlinks.json']) {
        const response = await fetch(origin + '/.well-known/' + name);
        expect(response.status, name).toBe(200); expect(await response.json()).toEqual({});
      }
      expect((await fetch(origin + '/.well-known/private.json')).status).toBe(404);
    });
  } finally { rmSync(dir, { recursive: true, force: true }); }
});


test('the generated robots file is discoverable at the origin root', async () => {
  const dir = workspace();
  try {
    save(dir, 'routes.json', JSON.stringify({ base: '/app/', routes: [] }));
    save(dir, 'robots.txt', 'User-agent: *\nAllow: /app/\nSitemap: https://site.invalid/app/sitemap.xml\n');
    await serving(dir, async origin => {
      const response = await fetch(origin + '/robots.txt');
      expect(response.status).toBe(200); expect(await response.text()).toContain('https://site.invalid/app/sitemap.xml');
    });
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
