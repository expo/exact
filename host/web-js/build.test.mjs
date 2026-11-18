import { test, expect, mock, spyOn } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, renameSync, rmSync, symlinkSync, writeFileSync, statSync, utimesSync } from 'node:fs';
import { createServer, get as httpGet } from 'node:http';
import { tmpdir } from 'node:os';
import { brotliDecompressSync } from 'node:zlib';
import { dirname, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { buildModule, moduleGrants, undeclaredRustData, webCompiler } from './module.mjs';
import { builtFile } from './content-names.mjs';
import { rustDataOf } from '../../scripts/app-shells.mjs';
import { spawnOwned } from '../../scripts/test-children.mjs';
import { open } from '../../scripts/agent.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { patchLines } from '../../scripts/app.mjs';
import { pinned } from '../../scripts/test-workspace.mjs';

test('a media-only page applies its audio policy while activation waits and preserves a claim when the agent loads', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/audio-session-'));
  const dist = resolve(dir, 'dist');
  let browser, server;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Media', app: { id: 'test.media', name: 'Media' }, audio_session: 'ambient', host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), 'routes nav\n  home "/" render=build activate=interaction\n\ncomponent App\n  view\n    column\n      audio controls=true\n');
    const built = spawnSync(process.execPath, ['host/web-js/build.mjs', 'media', '--out', dist, '--render', 'js'], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, encoding: 'utf8',
    });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const path = new URL(request.url).pathname;
      return new Response(Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1))));
    } });
    browser = await suiteBrowser();
    const p = await browser.newPage(), errors = [], scripts = [];
    p.on('pageerror', e => errors.push(e.message));
    p.on('request', r => { if (r.url().endsWith('.js')) scripts.push(new URL(r.url()).pathname); });
    await p.addInitScript(() => {
      let type = 'auto';
      window.sessionWrites = [];
      Object.defineProperty(navigator, 'audioSession', { value: { get type() { return type; }, set type(value) { sessionWrites.push(value); type = value; } } });
      window.contexts = 0;
      window.AudioContext = class { constructor() { contexts++; throw new Error('unexpected sound output'); } };
    });
    await p.goto(server.url.href);
    await p.locator('audio').click();
    expect(await p.evaluate(() => ({ type: navigator.audioSession.type, ready: !!globalThis.exact?.ready, contexts }))).toEqual({ type: 'ambient', ready: false, contexts: 0 });
    expect(scripts).toEqual([`/${builtFile(dist, 'audio-session.js')}`]);
    await p.goto(new URL('shell.html', server.url).href);
    await p.waitForFunction(() => globalThis.exact?.ready);
    const agent = readdirSync(dist).find(name => /^agent-.*\.js$/.test(name));
    expect(agent).toBeDefined();
    expect(await p.evaluate(async agent => {
      navigator.audioSession.type = 'play-and-record';
      await import('./' + agent);
      return { type: navigator.audioSession.type, writes: sessionWrites, contexts };
    }, agent)).toEqual({ type: 'play-and-record', writes: ['ambient', 'play-and-record'], contexts: 0 });
    expect(errors).toEqual([]);
  } finally { await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true }); }
}, 60000);

test.each([[false, false], [true, false], [true, true]])('a list first opened offline uses prepared chunks without evaluating them at boot (production: %s, interaction: %s)', async (production, interaction) => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/offline-list-')), dist = resolve(dir, 'dist');
  let browser, server;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Offline List', app: { id: 'test.offline-list', name: 'Offline List' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), (interaction ? 'routes nav\n  home "/" render=build activate=interaction\n\n' : '') + `component OfflineList
  state wakes = 0
  state shown = false
  state items = [${Array.from({ length: 100 }, (_, i) => i).join(', ')}]
  action boot
    wakes = wakes + 1
  action reveal
    shown = true
  view
    column
      button \`Wake \${wakes}\` press=boot testId="boot"
      button "Show list" press=reveal testId="reveal"
      if shown
        list virtualized=true estimated-item-height=40 height=120 width=320 testId="rows"
          for item in items key=item
            text \`Row \${item}\` height=40 testId=\`row-\${item}\`\n`);
    const built = spawnSync(process.execPath, ['host/web-js/build.mjs', 'offline-list', '--out', dist, '--render', interaction ? 'js' : 'none',
      ...(production ? ['--production'] : [])], { cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, encoding: 'utf8' });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    expect(built.stdout).toMatch(/own plan code by component \(shared bodies counted per use\)[\s\S]*OfflineList: \d+ B \(1 instance\)/);
    expect(built.stdout).toContain(`${dist}: ${builtFile(dist, 'app.js')} ${statSync(resolve(dist, builtFile(dist, 'app.js'))).size} B, built`);
    const collection = readdirSync(dist).find(name => /^collection-glue-.*\.js$/.test(name));
    expect(collection).toBeDefined();
    // A probe in the deferred chunk distinguishes preparation from evaluation.
    writeFileSync(resolve(dist, collection), 'globalThis.collectionEvaluated = true;\n' + readFileSync(resolve(dist, collection), 'utf8'));
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const path = new URL(request.url).pathname;
      return new Response(Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1))), { headers: { 'cache-control': 'no-store' } });
    } });
    browser = await suiteBrowser();
    const page = await browser.newPage(), errors = [], failed = [];
    page.on('pageerror', error => errors.push(error.message));
    page.on('console', message => { if (message.type() === 'error') errors.push(message.text()); });
    page.on('requestfailed', request => failed.push(request.url()));
    await page.addInitScript(() => {
      globalThis.preparedModules = [];
      document.addEventListener('load', event => {
        if (event.target instanceof HTMLLinkElement && event.target.rel === 'modulepreload') preparedModules.push(event.target.href);
      }, true);
    });
    await page.goto(server.url.href);
    await page.getByTestId('reveal').waitFor();
    if (interaction) {
      await page.getByTestId('boot').dispatchEvent('click');
      await page.waitForFunction(() => document.querySelector('[data-testid=boot]')?.textContent === 'Wake 1');
    }
    // The entry prepares the list's controller as it starts: the head names only what runs then.
    await page.waitForFunction(() => preparedModules.some(href => /\/collection-glue-/.test(href))
      && [...document.querySelectorAll('link[rel=modulepreload]')].every(link => preparedModules.includes(link.href)));
    expect(await page.evaluate(() => globalThis.collectionEvaluated)).toBeUndefined();
    expect(await page.getByTestId('rows').count()).toBe(0);
    await page.context().setOffline(true);
    await page.getByTestId('reveal').dispatchEvent('click');
    await page.evaluate(() => exact.lists.pending());
    expect(errors).toEqual([]);
    expect(failed).toEqual([]);
    expect(await page.evaluate(() => globalThis.collectionEvaluated)).toBe(true);
    await page.getByTestId('rows').evaluate(el => { el.scrollTop = 3600; });
    await page.getByTestId('row-90').waitFor();
    expect(await page.getByTestId('row-0').count()).toBe(0);
  } finally {
    await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true });
  }
}, 60000);

test.each([false, true])('named focus, blur, scheme, clipboard and haptic commands work (production: %s)', async production => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/linked-commands-'));
  const dist = resolve(dir, 'dist');
  let browser, server;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Commands', app: { id: 'test.linked-commands', name: 'Commands' } }));
    writeFileSync(resolve(dir, 'app.contract'), `component Commands
  action apply
    focus("field")
    blur("other")
    setScheme("dark")
    copyText("copied")
    haptic("selection")
  action clear
    blur("field")
    setScheme("system")
  action windowFocus
    focus()
    blur()
    haptic("success")
  view
    column
      input id="field" testId="field"
      input id="other"
      button "Apply" press=apply testId="apply"
      button "Clear" press=clear testId="clear"
      button "Window" press=windowFocus testId="window"
`);
    const built = spawnSync(process.execPath, ['host/web-js/build.mjs', 'commands', '--out', dist, '--render', 'none',
      ...(production ? ['--production'] : [])], { cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, encoding: 'utf8' });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const path = new URL(request.url).pathname;
      return new Response(Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1))));
    } });
    browser = await suiteBrowser();
    const page = await browser.newPage(), errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.addInitScript(() => {
      globalThis.calls = [];
      window.focus = () => calls.push('window');
      Object.defineProperty(navigator, 'clipboard', { value: { writeText: text => calls.push(text) } });
      Object.defineProperty(navigator, 'vibrate', { value: ms => calls.push(ms) });
    });
    // Production refuses agent mode even when the URL asks for it.
    await page.goto(server.url.href + '?agent');
    await page.getByTestId('apply').waitFor();
    await page.getByTestId('apply').dispatchEvent('click');
    expect(await page.evaluate(() => [document.activeElement.id, document.documentElement.style.colorScheme, calls]))
      .toEqual(['field', 'dark', production ? ['copied', 5] : [5]]);
    await page.getByTestId('clear').dispatchEvent('click');
    expect(await page.evaluate(() => [document.activeElement === document.body, document.documentElement.style.colorScheme]))
      .toEqual([true, '']);
    await page.getByTestId('field').focus();
    await page.getByTestId('window').dispatchEvent('click');
    expect(await page.evaluate(() => [document.activeElement === document.body, calls]))
      .toEqual([true, production ? ['copied', 5, 'window', 12] : [5, 12]]);
    if (!production) expect(await page.evaluate(() => exact.journal.some(line => line.endsWith(' focus window')))).toBe(true);
    await page.evaluate(() => { exact.held = { scheme: true }; });
    await page.getByTestId('apply').dispatchEvent('click');
    expect(await page.evaluate(() => [document.documentElement.style.colorScheme, exact.asked.scheme.value])).toEqual(['', 'dark']);
    await page.evaluate(() => exact.asked.scheme.apply());
    expect(await page.evaluate(() => document.documentElement.style.colorScheme)).toBe('dark');
    // The choice is kept, held or not, and the next load is in it when the page first renders.
    expect(await page.evaluate(() => Object.entries(localStorage).filter(([k]) => k.endsWith('#scheme')).map(([, v]) => v))).toEqual(['"dark"']);
    await page.addInitScript(() => {
      new MutationObserver((_, o) => {
        if (!document.getElementById('exact-root')?.firstChild) return;
        globalThis.firstScheme = document.documentElement.style.colorScheme;
        o.disconnect();
      }).observe(document, { childList: true, subtree: true });
    });
    await page.reload();
    await page.getByTestId('apply').waitFor();
    expect(await page.evaluate(() => globalThis.firstScheme)).toBe('dark');
    await page.getByTestId('field').focus();
    expect(await page.evaluate(() => document.activeElement.id)).toBe('field');
    await page.getByTestId('clear').dispatchEvent('click');
    expect(await page.evaluate(() => Object.keys(localStorage).filter(k => k.endsWith('#scheme')))).toEqual([]);
    expect(await page.evaluate(() => document.activeElement === document.body)).toBe(true);
    expect(errors).toEqual([]);
  } finally {
    await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true });
  }
}, 60000);

test.each([
  ['empty list first', [], false],
  ['two elements first', [1, 2], false],
  ['title before one element', [1], true],
  ['title before two elements', [1, 2], true],
  ['two open elements', [1, 2], false, true],
  ['kept answers with no arguments', [1, 2], false, false, true],
  ['title alone', null, true],
])('rendered checkpoints keep the declared resource types: %s', async (_label, ids, titleFirst, streaming = false, keeps = false) => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/resource-render-'));
  const previousApp = process.env.EXACT_APP_DIR, itemText = id => keeps ? 'item' : `item ${id}`;
  let server, browser;
  try {
    const title = '  resource title = title() as shape string\n';
    const each = ids === null ? '' : `  resource each = item(${keeps ? '' : 'id'}) for id in ids as shape R\n`;
    const dist = resolve(dir, 'dist');
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
      const url = new URL(req.url), path = url.pathname;
      if (path === '/events') return new Response(new ReadableStream({ start(controller) {
        controller.enqueue(new TextEncoder().encode(`data: item ${url.searchParams.get('id')}\n\n`));
      } }), { headers: { 'content-type': 'text/event-stream' } });
      const file = Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1)));
      return await file.exists() ? new Response(file) : new Response(null, { status: 404 });
    } });
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Resource Render', app: { id: 'test.resource-render', name: 'Resource Render' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  home "/" render=build
shape R
  text: string
component Probe
  state ids: number[] = ${JSON.stringify(ids ?? [])}
${titleFirst ? title + each : each + title}  view
    column
      text title testId="title"
${ids === null ? '' : '      for r in each key=r.text\n        text r.text\n'}`);
    writeFileSync(resolve(dir, 'app.ts'), streaming ? `export const grants = 'net.fetch ${server.url.origin}';
export const answer = (source: string, [id]: number[]): any => source === 'item' ? fetch('${server.url.origin}/events?id=' + id, {
  exactStream: (event: any) => ({ text: event.data }),
} as RequestInit) : 'Hello';
` : keeps ? `export const grants = '';
const saved = new Map<string, any>();
export const kept = (source: string, args: any[], value: any) => saved.set(source + JSON.stringify(args), value);
export const answer = (source: string, args: any[]): any => saved.get(source + JSON.stringify(args)) ?? (source === 'item' ? { text: 'item' } : 'Hello');
` : `export const grants = '';
export const answer = ${ids === null ? "() => 'Hello'" : "(source: string, [id]: number[]) => source === 'item' ? { text: 'item ' + id } : 'Hello'"};
`);
    const result = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'resource-render', '--out', dist], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([result.exited, new Response(result.stdout).text(), new Response(result.stderr).text()]);
    expect(status, stdout + stderr).toBe(0);
    const html = readFileSync(resolve(dist, 'index.html'), 'utf8');
    const checkpoint = JSON.parse(html.match(/<script type="application\/vnd\.exact\.checkpoint"[^>]*>(.*?)<\/script>/s)[1]);
    const titleAnswer = ['title', 'title', [], 'Hello'];
    const listAnswers = (ids ?? []).map(id => ['each', 'item', keeps ? [] : [id], { r: [itemText(id)] }]);
    expect(checkpoint.answers).toEqual(titleFirst ? [titleAnswer, ...listAnswers] : [...listAnswers, titleAnswer]);
    expect(checkpoint.open ?? []).toEqual(streaming ? listAnswers.map((_, i) => i) : []);
    for (const id of ids ?? []) expect(html).toContain(itemText(id));
    browser = await open({ host: 'web', app: dir, webDist: dist, url: server.url.href });
    await browser.clock('settle');
    const state = await browser.state();
    expect(state.resources).toEqual({ title: 'Hello', ...(ids === null ? {} : { each: ids.map(id => ({ text: itemText(id) })) }) });
    expect(state.resourceStatus.title.resource).toBe(titleFirst ? 0 : 1);
    const members = Object.entries(state.resourceStatus).filter(([name]) => name.startsWith('each['));
    expect(members).toHaveLength(ids?.length ?? 0);
    for (const [, status] of members) expect(status.resource).toBe(titleFirst ? 1 : 0);
    expect(await browser.carrier.evaluate('document.querySelector("[data-testid=title]").textContent')).toBe('Hello');
    for (const id of ids ?? []) expect(await browser.carrier.evaluate('document.getElementById("exact-root").textContent')).toContain(itemText(id));
  } finally {
    try {
      await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true });
    } finally {
      if (previousApp === undefined) delete process.env.EXACT_APP_DIR;
      else process.env.EXACT_APP_DIR = previousApp;
    }
  }
  expect(process.env.EXACT_APP_DIR).toBe(previousApp);
}, 60000);

test.each([
  ['empty', ''],
  ['network', 'net.fetch https://api.example'],
  ['storage', 'fs.read app:/data\nsqlite.open app:/data/notes.db'],
])('module metadata preserves grants: %s', async (_label, grants) => {
  const root = resolve(import.meta.dir, '../..');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/module-grants-')), path = resolve(dir, 'app.module.wasm');
  writeFileSync(path, new Uint8Array());
  const memory = new WebAssembly.Memory({ initial: 1 }), out = 1024, bytes = [];
  const u32 = n => bytes.push(n & 255, n >>> 8 & 255, n >>> 16 & 255, n >>> 24 & 255);
  const string = s => { const b = new TextEncoder().encode(s); u32(b.length); bytes.push(...b); };
  u32(12); bytes.push(0); string('test.grants'); string(grants);
  new Uint8Array(memory.buffer, out, bytes.length).set(bytes);
  const output = mock(() => out);
  let abi = 12;
  const instantiate = spyOn(WebAssembly, 'instantiate').mockResolvedValue({ instance: { exports: {
    memory, exact_logic_abi: () => abi, exact_logic_create: () => 1, exact_logic_alloc: () => 0,
    exact_logic_call(_session, pointer, length) {
      const request = new DataView(memory.buffer, pointer, length);
      return length === 5 && request.getUint32(0, true) === 12 && request.getUint8(4) === 0 ? 0 : 1;
    },
    exact_logic_output: output, exact_logic_output_len: () => bytes.length,
  } } });
  try {
    const actual = await moduleGrants(path);
    expect(output).toHaveBeenCalledTimes(1);
    expect(actual).toBe(grants);
    abi = 9;
    await expect(moduleGrants(path)).rejects.toThrow('Rust module ABI 9 is not 12');
  } finally {
    instantiate.mockRestore();
    rmSync(dir, { recursive: true, force: true });
  }
});

test.each([false, true])('optional hook warnings are confined to authored imports (authored: %s)', authored => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/web-js-warnings-'));
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Warnings', app: { id: 'test.warnings', name: 'Warnings' } }));
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    text "Warnings"\n');
    writeFileSync(resolve(dir, 'app.ts'), `${authored ? "import { value } from './ts-data.js';" : "const value = 'ok';"}
export const grants = '';
export const answer = () => value as never;
`);
    // The author's file has the glue's basename and probes the same names.
    writeFileSync(resolve(dir, 'ts-data.js'), "import * as source from './empty.js';\nexport const value = [source.kept, source.overlay, source.other].join(',');\n");
    writeFileSync(resolve(dir, 'empty.js'), 'export {};\n');
    const out = resolve(dir, 'dist');
    const result = spawnSync(process.execPath, ['host/web-js/build.mjs', 'warnings', '--out', out], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, encoding: 'utf8',
    });
    expect(result.status, result.stdout + result.stderr).toBe(0);
    expect(readFileSync(resolve(out, '.gen/server.js'), 'utf8')).toContain('__start');
    // app.ts names no appId: the glue's is app.json's.
    expect(readFileSync(resolve(out, '.gen/ts-data.js'), 'utf8')).toContain('const appId = "test.warnings";');
    const stderr = result.stderr.replace(/\x1b\[[0-9;]*m/g, '');
    if (authored) {
      for (const name of ['kept', 'overlay', 'other']) {
        expect(stderr).toContain(`[IMPORT_IS_UNDEFINED] Import \`${name}\` will always be undefined`);
      }
      expect(stderr).toContain('.gen/typescript/ts-data.js:');
      expect(stderr).not.toContain('.gen/ts-data.js:');
      expect(stderr.match(/\[IMPORT_IS_UNDEFINED\] Import /g)).toHaveLength(6);
    } else {
      expect(stderr).not.toContain('IMPORT_IS_UNDEFINED');
      expect(stderr).not.toContain('will always be undefined');
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}, 30000);

test('web validation drops unused package exports before refusing their environment reads', () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/web-unused-exports-'));
  const pkg = resolve(dir, 'node_modules/debug-helper'), out = resolve(dir, 'dist');
  try {
    mkdirSync(pkg, { recursive: true });
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Unused', app: { id: 'test.unused', name: 'Unused' } }));
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  resource message = message() as shape string\n  view\n    column\n      text message\n      canvas surface=spark() width=10 height=10\n');
    writeFileSync(resolve(pkg, 'package.json'), JSON.stringify({ name: 'debug-helper', version: '1.0.0', type: 'module', exports: './index.js', types: './index.d.ts' }));
    writeFileSync(resolve(pkg, 'index.js'), 'export const debug = () => import.meta.env.DEBUG;\n');
    writeFileSync(resolve(pkg, 'index.d.ts'), 'export declare const debug: () => string;\n');
    const build = () => spawnSync(process.execPath, ['host/web-js/build.mjs', dir, '--out', out, '--render', 'none'],
      { cwd: root, encoding: 'utf8', env: { ...process.env, EXACT_APP_DIR: dir } });
    const base = "export const grants = '';\nexport const surfaces = { spark: 0 };\nexport const draw = () => false;\n";
    writeFileSync(resolve(dir, 'app.ts'), base + "export const answer = () => 'ready';\nexport { debug } from 'debug-helper';\n");
    const built = build();
    expect(built.status, built.stdout + built.stderr).toBe(0);
    expect(readFileSync(resolve(out, builtFile(out, 'app.js')), 'utf8')).not.toContain('import.meta.env');
    for (const source of [
      base + 'export const answer = () => debug();\n',
      base.replace('() => false', '() => !!debug()') + "export const answer = () => 'ready';\n",
    ]) {
      writeFileSync(resolve(dir, 'app.ts'), "import { debug } from 'debug-helper';\n" + source);
      const refused = build();
      expect(refused.status).toBe(1);
      expect(refused.stderr).toContain('debug-helper@1.0.0 (index.js:1:28) reads import.meta.env');
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 30000);

test('the web build resolves drawing exports through the module graph', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..');
  const dir = mkdtempSync(resolve(root, 'target/web-js-exports-')), out = resolve(dir, 'dist');
  const base = "export const grants = '';\nexport const answer = () => 'answer';\n";
  const contract = 'component App\n  resource message = message() as shape string\n  view\n    column\n      text message\n';
  const optional = "import type { Draw } from './app.contract.d.ts';\n"
    + "export const surfaces = { spark: 0 };\nexport const draw: Draw = (_surface, _args, ctx) => { ctx.fillRect(0, 0, 1, 1); return false; };\n";
  const build = () => spawnSync(process.execPath, ['host/web-js/build.mjs', dir, '--out', out, '--render', 'none'],
    { cwd: root, encoding: 'utf8', env: { ...process.env, EXACT_APP_DIR: dir } });
  let browser, server;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Optional', app: { id: 'test.optional', name: 'Optional' }, rust: false }));
    writeFileSync(resolve(dir, 'app.contract'), contract + '      canvas surface=spark() width=10 height=10\n');
    writeFileSync(resolve(dir, 'optional.ts'), optional);
    writeFileSync(resolve(dir, 'drawing.cjs'), "exports.surfaces = { spark: 0 }; exports.draw = (_surface, _args, ctx) => { ctx.fillRect(0, 0, 1, 1); return false; };");
    for (const declaration of [
      "export * from './optional';",
      "export { draw, surfaces } from './optional';",
      "import * as made from './optional'; export var draw = made.draw, surfaces = made.surfaces;",
      "import * as made from './optional'; export const { draw, surfaces } = made;",
      "export\n{ draw, surfaces } from './optional';",
      "export { draw } from './drawing.cjs'; export * from './drawing.cjs';",
    ]) {
      writeFileSync(resolve(dir, 'app.ts'), base + declaration);
      const built = build();
      expect(built.status, declaration + '\n' + built.stdout + built.stderr).toBe(0);
      expect(readFileSync(resolve(out, '.gen/draw.js'), 'utf8')).toContain("export { drawer }");
    }
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const path = new URL(request.url).pathname;
      return new Response(Bun.file(resolve(out, path === '/' ? 'index.html' : path.slice(1))));
    } });
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.waitForFunction(() => {
      const canvas = document.querySelector('canvas');
      return canvas?.getContext('2d').getImageData(0, 0, 1, 1).data[3] === 255;
    });
    expect(await page.locator('body').innerText()).toContain('answer');
    writeFileSync(resolve(dir, 'app.contract'), contract);
    writeFileSync(resolve(dir, 'helper.cjs'), 'exports.helper = 1;');
    for (const declaration of [
      "/* export function draw() {} */\n// export const surfaces = {};\n",
      "export * from './helper.cjs';",
      "export type { draw, surfaces } from './optional';",
    ]) {
      writeFileSync(resolve(dir, 'app.ts'), base + declaration);
      const absent = build();
      expect(absent.status, absent.stdout + absent.stderr).toBe(0);
      expect(readFileSync(resolve(out, '.gen/draw.js'), 'utf8')).toContain('drawer = null');
    }
    for (const declaration of ["export { draw } from './optional';", "export { surfaces } from './optional';"]) {
      writeFileSync(resolve(dir, 'app.ts'), base + declaration);
      const refused = build();
      expect(refused.status).toBe(1);
      expect(refused.stderr).toContain('exports `draw` and `surfaces` together');
    }
    // A `draw` that is not a function fails the type check, as the native bake's seam does.
    writeFileSync(resolve(dir, 'app.contract'), contract + '      canvas surface=spark() width=10 height=10\n');
    writeFileSync(resolve(dir, 'number.ts'), 'export const surfaces = { spark: 0 };\nexport const draw = 42;\n');
    for (const declaration of ['export var draw = 42; export var surfaces = { spark: 0 };', "export * from './number';"]) {
      writeFileSync(resolve(dir, 'app.ts'), base + declaration);
      const refused = build();
      expect(refused.status, declaration + '\n' + refused.stdout + refused.stderr).toBe(1);
      expect(refused.stderr).toContain('not assignable');
    }
  } finally { await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true }); }
}, 60000);

test('a mount whose own tsconfig.json extends a file outside it builds for the web', () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/web-js-mount-tsconfig-')), app = resolve(dir, 'app');
  try {
    // A monorepo's package: its tsconfig.json extends the repository's.
    mkdirSync(resolve(dir, 'lib')); mkdirSync(app);
    writeFileSync(resolve(dir, 'tsconfig.json'), JSON.stringify({ compilerOptions: { strict: true } }));
    writeFileSync(resolve(dir, 'lib/tsconfig.json'), JSON.stringify({ extends: '../tsconfig.json' }));
    writeFileSync(resolve(dir, 'lib/greet.ts'), 'export const greet = (name: string): string => `Hello from ${name}.`;\n');
    writeFileSync(resolve(app, 'app.json'), JSON.stringify({ name: 'Mounted', app: { id: 'test.mounted', name: 'Mounted' }, typescript: { sources: { lib: '../lib' } } }));
    writeFileSync(resolve(app, 'app.contract'), 'component App\n  resource greeting = greeting("Mounted") as shape string\n  view\n    text greeting\n');
    writeFileSync(resolve(app, 'app.ts'), "import { greet } from './lib/greet';\nexport const grants = '';\nexport const answer = (_source: string, [name]: [string]) => greet(name) as never;\n");
    const out = resolve(app, 'dist');
    const result = spawnSync(process.execPath, ['host/web-js/build.mjs', 'mounted', '--out', out], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: app }, encoding: 'utf8',
    });
    expect(result.status, result.stdout + result.stderr).toBe(0);
    expect(readFileSync(resolve(out, '.gen/server.js'), 'utf8')).toContain('Hello from ');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}, 30000);

test('production pages and lazy host modules ignore agent facts, storage and entropy parameters', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/web-js-agent-'));
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Agent gate', id: 'test.agent-gate', app: { id: 'test.agent-gate', name: 'Agent gate' } }));
    writeFileSync(resolve(dir, 'app.contract'), `shape Time
  epochAtZero: number
  locale: string
component App
  resource time = exactTime() as shape Time
  view
    column
      input type="file"
      text time.epochAtZero.toString() testId="epoch"
      text time.locale testId="locale"
`);
    const build = (out, args = []) => {
      const result = spawnSync(process.execPath, ['host/web-js/build.mjs', 'agent-gate', '--out', out, '--render', 'none', ...args], {
        cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, encoding: 'utf8',
      });
      expect(result.status, result.stdout + result.stderr).toBe(0);
    };
    // A release over a bake's plan, and one with no bake (an app that names no native platform, LLP 1153 D3.6).
    const dev = resolve(dir, 'dev'), prod = resolve(dir, 'prod'), webOnly = resolve(dir, 'web-only');
    build(dev);
    build(prod, ['--production', '--plan', resolve(dev, 'app.plan')]);
    build(webOnly, ['--production']);
    const browser = await suiteBrowser();
    try {
      for (const dist of [dev, prod, webOnly]) {
        const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
          const path = new URL(request.url).pathname;
          return new Response(Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1))));
        } });
        const page = await browser.newPage();
        try {
          await page.goto(server.url.href + '?agent&epoch=1&seed=3&locale=zz');
          await page.waitForFunction(() => globalThis.exact?.ready);
          const epoch = Number(await page.getByTestId('epoch').textContent());
          const locale = await page.getByTestId('locale').textContent();
          if (dist === dev) {
            expect([epoch, locale]).toEqual([1, 'zz']);
          } else {
            expect(epoch).toBeGreaterThan(1e12);
            expect(locale).toBe(await page.evaluate(() => navigator.language));
          }
        } finally {
          await page.close();
          server.stop(true);
        }
      }
    } finally {
      await browser.close();
    }
    const development = await import(pathToFileURL(resolve(dev, 'storage-environment.js')).href);
    const appId = 'test.agent-gate', own = development.storageKey(appId, 'https://example.test/');
    expect(own).toMatch(/^test\.agent-gate@[a-f0-9]{12}$/);
    expect(development.storageKey(appId, 'https://example.test/?agent&storage=x')).toBe(`${own}/agent/x`);
    expect(development.storageKey(appId, 'https://example.test/?agent')).toBeNull();
    expect(development.agentSeed('http://localhost/?agent&seed=7')).toBe(7);
    for (const release of [prod, webOnly]) {
      const production = await import(pathToFileURL(resolve(release, 'storage-environment.js')).href);
      for (const origin of ['https://example.test', 'http://localhost']) {
        for (const query of ['', '?agent', '?agent&storage=x', '?agent&storage=../invalid']) {
          expect(production.storageKey(appId, origin + '/' + query)).toBe(appId);
        }
        expect(production.agentSeed(origin + '/?agent&seed=7')).toBeNull();
      }
      const shipped = readdirSync(release, { recursive: true }).filter(name => /\.m?js$/.test(name));
      expect(shipped).toContain(builtFile(release, 'picker-glue.js'));
      expect(shipped).toContain('storage-environment.js');
      expect(shipped).toContain('.gen/navigation.js');
      for (const name of shipped) {
        expect(readFileSync(resolve(release, name), 'utf8'), name).not.toContain('const AGENT_ADMITTED = true;');
      }
      // Nor the input piece's hold and press readout, the agent's, or the wasm host's pointer and link paths (LLP
      // 1047.002 D4.1).
      const piece = readFileSync(resolve(release, shipped.find(name => /^input-glue-.*\.js$/.test(name))), 'utf8');
      for (const marker of ['holdQuiet', 'pressShown', 'exactPointerDown', 'exact_route_match']) expect(piece, marker).not.toContain(marker);
    }
    // A development page keeps the agent's and still not the wasm host's.
    const piece = readFileSync(resolve(dev, readdirSync(dev).find(name => /^input-glue-.*\.js$/.test(name))), 'utf8');
    expect(piece).toContain('holdQuiet');
    expect(piece).not.toContain('exactPointerDown');
    // With no bake, the page links its web manifest and no envelope, and the install pages name a release.
    const page = readFileSync(resolve(webOnly, 'index.html'), 'utf8');
    expect(page).toContain('<link rel="manifest" href="./manifest.json">');
    expect(page).not.toContain('application/vnd.exact.envelope+json');
    expect(readdirSync(webOnly)).not.toContain('exact.json');
    expect(readFileSync(resolve(webOnly, '.exact/install.json'), 'utf8')).toContain('"mode": "Release build"');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}, 30000);


// `web-build --out` replaces the directory it names whole, so it refuses, before building, one that is or holds the
// app or this checkout (through a link too), one inside the app where a source scan would read it, and one already
// holding what is not a web build; each leaves the app as it was.
test('web-build --out refuses a directory it would destroy', () => {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
  const dir = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-site-out-'))), app = resolve(dir, 'site');
  try {
    mkdirSync(app);
    writeFileSync(resolve(app, 'app.json'), JSON.stringify({ name: 'Site', app: { id: 'test.site', name: 'Site' } }));
    writeFileSync(resolve(app, 'app.contract'), 'component App\n  view\n    text "Site"\n');
    symlinkSync(app, resolve(dir, 'link'));
    mkdirSync(resolve(dir, 'notes'));
    writeFileSync(resolve(dir, 'notes/todo.txt'), 'keep');
    writeFileSync(resolve(dir, 'file'), 'keep');
    // What a build keeps beside its directory (host/web/beside.mjs), each there and someone else's.
    const foreign = { 'export.previous': 'notes/todo.txt', 'export.gen': 'notes/todo.txt', 'export.leases': 'notes/todo.txt', 'export.gen.reaped': null, 'export.lock': null };
    for (const [name, inside] of Object.entries(foreign)) {
      if (inside) { mkdirSync(resolve(dir, name, 'notes'), { recursive: true }); writeFileSync(resolve(dir, name, inside), 'keep'); } else writeFileSync(resolve(dir, name), 'keep');
    }
    for (const [out, why] of [[app, /it is or holds the app/], [dir, /it is or holds the app/], [resolve(dir, 'link'), /it is or holds the app/],
      [root, /it is or holds this exact2 checkout/], [resolve(root, '..'), /it is or holds this exact2 checkout/], [resolve(app, 'site'), /inside the app, the build goes in dist/], [resolve(app, '.site'), /inside the app/],
      [resolve(dir, 'notes'), /holds files that are not a web build/], [resolve(dir, 'file'), /is not a directory/], [resolve(dir, 'export'), /export\.gen is there and is not builds kept by generation/]]) {
      const r = spawnSync(process.execPath, ['host/web/build.mjs', app, '--out', out], { cwd: root, env: { ...process.env, EXACT_APP_DIR: app }, encoding: 'utf8' });
      expect([out, r.status, why.test(r.stderr)], r.stderr).toEqual([out, 2, true]);
    }
    // Each such name refused alone, and left as it was.
    for (const name of Object.keys(foreign)) {
      const out = resolve(dir, `alone-${name.split('.')[0]}`), moved = resolve(dir, name.replace(/^export/, `alone-${name.split('.')[0]}`));
      renameSync(resolve(dir, name), moved);
      const r = spawnSync(process.execPath, ['host/web/build.mjs', app, '--out', out], { cwd: root, env: { ...process.env, EXACT_APP_DIR: app }, encoding: 'utf8' });
      expect([name, r.status, r.stderr.includes(`${moved} is there and is not`)], r.stderr).toEqual([name, 2, true]);
      expect(existsSync(out)).toBe(false);
      renameSync(moved, resolve(dir, name));
    }
    expect(readdirSync(app).sort()).toEqual(['app.contract', 'app.json']);
    for (const path of ['notes/todo.txt', 'export.previous/notes/todo.txt', 'export.gen/notes/todo.txt', 'export.leases/notes/todo.txt', 'export.lock', 'export.gen.reaped']) {
      expect(readFileSync(resolve(dir, path), 'utf8'), path).toBe('keep');
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}, 30000);

// A canvas whose surface no GPU module registers is a Canvas 2D surface, which a data source draws; with none that can,
// the build refuses, naming the surface, rather than ship a canvas that stays blank.
test('the web build refuses undrawn surfaces and routes primary and declared GPU surfaces', () => {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
  const dir = pinned(realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-undrawn-'))));
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Spark', app: { id: 'test.exact.spark', name: 'Spark' }, rust: false }));
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    canvas surface=spark(1) width=10 height=10\n');
    const build = () => spawnSync(process.execPath, ['host/web/build.mjs', 'spark', '--render', 'none'], { cwd: root, env: { ...process.env, EXACT_APP_DIR: dir, EXACT_WEB_DIST: resolve(dir, 'dist') }, encoding: 'utf8' });
    let r = build();
    expect([r.status, r.stderr.includes('spark: nothing draws the canvas surface spark: the app has no GPU module, and it has no data source to draw on Canvas 2D')], r.stderr).toEqual([1, true]);
    writeFileSync(resolve(dir, 'app.ts'), "export const appId = 'test.exact.spark';\nexport const grants = '';\n");
    r = build();
    expect([r.status, r.stderr.includes('spark: nothing draws the canvas surface spark: the app has no GPU module, and app.ts exports no `draw`')], r.stderr).toEqual([1, true]);
    // The cached stand-ins register the primary and the declared module without a GPU toolchain build.
    const cache = resolve(dir, 'target/web-js-gpu/spark'), deps = resolve(dir, 'target/wasm32-unknown-unknown/web');
    for (const d of [resolve(dir, 'gpu/src'), resolve(cache, 'gpu'), deps]) mkdirSync(d, { recursive: true });
    writeFileSync(resolve(dir, 'gpu/src/lib.rs'), '');
    writeFileSync(resolve(dir, 'Cargo.toml'), `[workspace]\nmembers=["gpu","gpu-world"]\nresolver="2"\n[patch.crates-io]\n${patchLines(dir).join('\n')}\n`);
    for (const name of ['gpu', 'gpu-world']) {
      mkdirSync(resolve(dir, name, 'src'), { recursive: true });
      writeFileSync(resolve(dir, name, 'src/lib.rs'), '');
      writeFileSync(resolve(dir, name, 'Cargo.toml'), `[package]\nname="spark-${name}"\nversion="0.1.0"\nedition="2021"\n`);
    }
    const locked = spawnSync('cargo', ['generate-lockfile', '--offline'], { cwd: dir, encoding: 'utf8' });
    expect(locked.status, locked.stderr).toBe(0);
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Spark', app: { id: 'test.exact.spark', name: 'Spark' }, rust: false, gpu: { modules: { world: ['spark'] } } }));
    const glue = "export function initSync() {}\nexport function gpu_surfaces() { return '[\"repel\"]'; }\n";
    for (const stem of ['gpu', 'gpu/world']) {
      writeFileSync(resolve(cache, `${stem}.js`), stem === 'gpu' ? glue : glue.replace('repel', 'spark'));
      writeFileSync(resolve(cache, `${stem}_bg.wasm`), '');
    }
    writeFileSync(resolve(cache, '.modules.json'), JSON.stringify([['gpu', 'spark-gpu'], ['gpu/world', 'spark-gpu-world']]));
    for (const crate of ['spark_gpu', 'spark_gpu_world']) writeFileSync(resolve(deps, `${crate}.d`), `${crate}: ${resolve(dir, 'app.json')}\n`);
    utimesSync(resolve(dir, 'app.json'), 0, 0);
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    canvas surface=repel(1) width=10 height=10\n');
    rmSync(resolve(dir, 'app.ts'));
    r = build();
    expect(r.status, r.stderr).toBe(0);
    expect([r.status, /gs\(e\d+,"repel"/.test(readFileSync(resolve(dir, 'dist/.gen/app.js'), 'utf8'))], r.stderr).toEqual([0, true]);
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    column\n      canvas surface=repel(1) width=10 height=10\n      canvas surface=spark(1) width=10 height=10\n');
    r = build();
    expect(r.status, r.stderr).toBe(0);
    expect(readFileSync(resolve(dir, 'dist/.gen/app.js'), 'utf8')).toMatch(/gs\(e\d+,"spark"/);
    expect(readFileSync(resolve(dir, 'dist/.gen/main.js'), 'utf8')).toContain('gpuModules: {"world":["spark"]}');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}, 60000);

// Issue #104: `web-build --json` names the build it made. The JS target is a release under `--out` alone, so the trust
// it reports is that, never an inherited EXACT_UPDATE_TRUST the page did not build under.
test('web-build --json reports the trust the JS target built under, not the environment\'s', () => {
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
  const dir = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-json-trust-')));
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Trust', app: { id: 'test.exact.trust', name: 'Trust' }, rust: false }));
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    text "hello"\n');
    const build = (...args) => {
      const r = spawnSync(process.execPath, ['host/web/build.mjs', 'trust', '--render', 'none', '--json', ...args], { cwd: root, env: { ...process.env, EXACT_APP_DIR: dir, EXACT_WEB_DIST: resolve(dir, 'target/web-dist'), EXACT_UPDATE_TRUST: 'production' }, encoding: 'utf8' });
      expect(r.status, r.stderr).toBe(0);
      // Run alone, the object is stdout's last line, after the build's own.
      return JSON.parse(r.stdout.trim().split('\n').at(-1));
    };
    const development = build();
    expect(development).toEqual({ path: resolve(dir, 'target/web-dist'), target: 'web', trust: 'development' });
    expect(readdirSync(development.path).some((name) => name.startsWith('agent-')), 'a development page carries agent support').toBe(true);
    expect(build('--out', resolve(dir, 'dist'))).toEqual({ path: resolve(dir, 'dist'), target: 'web', trust: 'production' });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);

// LLP 1153 D3: `web-build --out` writes a site any static host serves (its pages, the shell, the route table and
// 404.html, none of the build's intermediates but the server bundle), ends saying how a host serves it, and serve.mjs
// serves it so; `serve` (serve.mjs as the verb runs it) renders a `render=request` route's page for each request and
// keeps a `cached` one's.
test('web-build --out writes a site that serve.mjs serves by the hosting rule, rendering request and cached routes', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-site-')), app = resolve(dir, 'site'), out = resolve(app, 'dist');
  let server, serve;
  try {
    mkdirSync(app);
    writeFileSync(resolve(app, 'app.json'), JSON.stringify({ name: 'Site', app: { id: 'test.site', name: 'Site' } }));
    const source = 'shape Note\n  text: string\n\nroutes nav\n  home "/" render=build\n  item "/item/:id"\n  live "/live" render=request\n  feed "/feed" render=cached\n  doc "/doc/:id" render=request\n  notfound render=build\n\n'
      + 'component App\n  resource note = note("rendered") as shape Note\n  view\n    column\n      text top(nav).url\n      text note.text\n';
    writeFileSync(resolve(app, 'app.contract'), source);
    writeFileSync(resolve(app, 'app.ts'), "import type { Answer, Result, Sources } from './app.contract.d.ts';\nexport const appId = 'test.site';\nexport const grants = '';\n"
      + "const sources: Sources = { note: ([what]): Result<'note'> => ({ text: `The note, ${what}.` }) };\n"
      + 'export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);\n');
    const build = () => spawnSync(process.execPath, ['host/web/build.mjs', app, '--out', out], { cwd: root, env: { ...process.env, EXACT_APP_DIR: app }, encoding: 'utf8' });
    const built = build();
    expect(built.status, built.stdout + built.stderr).toBe(0);
    expect(built.stdout).toContain('site: no site-render entry, so its pages render under Bun (--render js)');
    expect(built.stdout).toContain('/item/:id: render=client, the default, so the browser draws the page only once app.js and its data have loaded; a page every reader sees alike arrives whole sooner as render=request');
    expect(built.stdout).toContain('/live: render=request, so serve renders its page for each request; a static host serves its shell');
    expect(built.stdout).toContain('/feed: render=cached, so serve renders its page and keeps it 60 s; a static host serves its shell');
    expect(built.stdout.trimEnd().split('\n').slice(-4).join('\n')).toMatch(/dist: the site, for any static host\. bun host\/web\/serve\.mjs .*dist serves it[^]*shell\.html with status 200; else 404\.html with status 404\.\n {2}A host with a single fallback page serves shell\.html[^]*\n {2}routes\.json `files` names the files named by their content: a host may cache them for good/);
    expect(JSON.parse(readFileSync(resolve(out, 'routes.json'), 'utf8')).routes).toEqual([
      { name: 'home', pattern: '/', render: 'build', notfound: false },
      { name: 'item', pattern: '/item/:id', render: 'client', notfound: false },
      { name: 'live', pattern: '/live', render: 'request', notfound: false },
      { name: 'feed', pattern: '/feed', render: 'cached', notfound: false },
      { name: 'doc', pattern: '/doc/:id', render: 'request', notfound: false },
      { name: 'notfound', pattern: '', render: 'build', notfound: true },
    ]);
    const files = readdirSync(out);
    for (const name of ['index.html', 'shell.html', '404.html', 'routes.json', '.exact-build.json']) expect(files).toContain(name);
    expect(readdirSync(resolve(out, '.gen')).sort()).toEqual(['page-style.json', 'server.js']);
    expect(files).not.toContain('app.plan.map.json');
    expect(JSON.parse(readFileSync(resolve(out, '.exact-build.json'), 'utf8')).inputs).toBeUndefined();
    // A site to host is a release, as delivery builds one: no file of it admits agent mode, and its install pages say so.
    for (const name of readdirSync(out, { recursive: true }).filter((name) => /\.m?js$/.test(name))) {
      expect(readFileSync(resolve(out, name), 'utf8'), name).not.toContain('const AGENT_ADMITTED = true;');
    }
    expect(readFileSync(resolve(out, '.exact/install.json'), 'utf8')).toContain('"mode": "Release build"');
    expect(readFileSync(resolve(out, 'index.html'), 'utf8')).toContain('<link rel="manifest" href="./manifest.json">');
    // Expect only the site and the persistent lock its swap takes beside it (build-lock.mjs)
    expect(readdirSync(app).filter((name) => name.startsWith('dist'))).toEqual(['dist', 'dist.lock']);
    // As a static host serves it: a request or cached route gets the shell, which has none of the data.
    const { serveBuildTree } = await import('../web/serve.mjs');
    server = createServer((req, res) => serveBuildTree(out, req, res));
    await new Promise(ok => server.listen(0, '127.0.0.1', ok));
    const page = (name) => readFileSync(resolve(out, name), 'utf8');
    const url = (path) => `http://127.0.0.1:${server.address().port}${path}`, html = { accept: 'text/html' };
    expect(page('shell.html')).not.toContain('The note, rendered.');
    for (const [path, accept, status, file] of [['/', 'text/html', 200, 'index.html'], ['/item/7', 'text/html', 200, 'shell.html'], ['/live', 'text/html', 200, 'shell.html'],
      ['/item/7', 'application/json', 404, '404.html'], ['/no/such/route', 'text/html', 404, '404.html'], ['/nope.png', '*/*', 404, '404.html']]) {
      const r = await fetch(url(path), { headers: { accept } });
      // (A served document names its build's generation in its base.)
      expect([path, accept, r.status, (await r.text()).replace(/<base href="[^"]*">/, '<base href="/">')]).toEqual([path, accept, status, page(file)]);
    }
    writeFileSync(resolve(out, 'index.md'), '# Home\n');
    writeFileSync(resolve(out, 'live.md'), '# Live\n');
    for (const [path, accept, type, body] of [['/', 'text/markdown', 'text/markdown', '# Home\n'], ['/live', 'text/markdown, text/html;q=0.9', 'text/markdown', '# Live\n'],
      ['/live', 'text/html, text/markdown;q=0.5', 'text/html', null], ['/item/7', 'text/markdown, text/html;q=0.9', 'text/html', null], ['/live', 'text/markdown;q=0.5, text/html', 'text/html', null], ['/', 'text/markdown, */*;q=0.1', 'text/markdown', '# Home\n']]) {
      const r = await fetch(url(path), { headers: { accept } });
      expect([path, accept, r.status, r.headers.get('content-type').split(';')[0], r.headers.get('vary')]).toEqual([path, accept, 200, type, 'Accept']);
      if (body) expect(await r.text()).toBe(body);
    }
    writeFileSync(resolve(out, '404.md'), '# Not found\n');
    for (const [accept, type, body] of [['text/markdown', 'text/markdown', '# Not found\n'], ['text/markdown, text/html;q=0.9', 'text/markdown', '# Not found\n'], ['text/html, text/markdown;q=0.5', 'text/html', page('404.html')]]) {
      const r = await fetch(url('/no/such/route'), { headers: { accept } });
      expect([accept, r.status, r.headers.get('content-type').split(';')[0], r.headers.get('vary'), (await r.text()).replace(/<base href="[^"]*">/, '<base href="/">')]).toEqual([accept, 404, type, 'Accept', body]);
    }
    // A page sent before the build is replaced still loads its build's files: the build replaced is kept beside the
    // output by its generation (`dist.gen`), as target/web-dist's is.
    const generation = async () => /<base href="\/\.exact\/gen\/([0-9a-f]{32})\/">/.exec(await (await fetch(url('/'), { headers: html })).text())?.[1];
    const before = await generation(), entry = builtFile(out, 'app.js'), script = readFileSync(resolve(out, entry));
    writeFileSync(resolve(app, 'app.contract'), source.replace('text note.text\n', 'text note.text\n      text "Changed."\n'));
    const rebuilt = build();
    expect(rebuilt.status, rebuilt.stdout + rebuilt.stderr).toBe(0);
    const after = await generation(), kept = await fetch(url(`/.exact/gen/${before}/${entry}`));
    expect([before === after, kept.status, Buffer.from(await kept.arrayBuffer()).equals(script)]).toEqual([false, 200, true]);
    expect(builtFile(out, 'app.js')).not.toBe(entry);
    expect(readdirSync(app).filter((name) => name.startsWith('dist')).sort()).toEqual(['dist', 'dist.gen', 'dist.lock']);
    // As `serve` serves it: each route's page rendered with its data, the live one per request, the cached one once.
    serve = await serving(root, out);
    const { get, renders, heard } = serve;
    const feed = [await get('/feed'), await get('/feed')], live = [await get('/live'), await get('/live')];
    for (const r of [...feed, ...live]) expect([r.status, r.body.includes('The note, rendered.'), r.body.includes('<div id="exact-root"></div>')]).toEqual([200, true, false]);
    expect(feed.map((r) => r.cache)).toEqual(['public, max-age=0, s-maxage=60', 'public, max-age=0, s-maxage=60']);
    expect(feed[1].body).toBe(feed[0].body);
    expect(live.map((r) => r.cache)).toEqual(['no-store', 'no-store']);
    // Each render is said before its page is sent: once the second live one is said, the cached page was rendered once.
    await heard(() => renders('/live') === 2);
    expect(renders('/feed')).toBe(1);
    // A site's server compresses what it sends (LLP 1048.007 D6): a page made for the request as it goes, a file by its
    // variant, which a reader revalidates by its ETag.
    const raw = (path) => new Promise((ok, no) => httpGet({ host: '127.0.0.1', port: serve.port, path, headers: { accept: 'text/html', 'accept-encoding': 'br, gzip' } },
      (r) => { const chunks = []; r.on('data', (c) => chunks.push(c)); r.on('end', () => ok({ headers: r.headers, body: Buffer.concat(chunks) })); }).on('error', no));
    const sent = await raw('/live');
    expect([sent.headers['content-encoding'], sent.headers.vary, brotliDecompressSync(sent.body).toString().includes('The note, rendered.')]).toEqual(['br', 'Accept-Encoding, Accept', true]);
    // A page rendered for the request varies by Accept too, and a page with no Markdown beside it is rendered for its own
    // path when Markdown is preferred, not for the `.md` path asked for first.
    writeFileSync(resolve(out, 'live.md'), '# Live\n');
    const md = await fetch(`http://127.0.0.1:${serve.port}/live`, { headers: { accept: 'text/markdown' } });
    expect([md.headers.get('content-type'), md.headers.get('vary'), await md.text()]).toEqual(['text/markdown; charset=utf-8', 'Accept', '# Live\n']);
    const docAsked = await fetch(`http://127.0.0.1:${serve.port}/doc/7`, { headers: { accept: 'text/markdown, text/html;q=0.9' } });
    const docBody = await docAsked.text();
    expect([docAsked.status, docBody.includes('>/doc/7<'), docBody.includes('>/doc/7.md<')]).toEqual([200, true, false]);
    // A file the build named by its content is kept for good; one with a fixed name is asked about again (LLP 1153.004).
    const file = await raw(`/${builtFile(out, 'app.js')}`), fixed = await raw('/app.plan');
    const hosted = await serving(root, out, { fromEnv: true });
    try { expect((await hosted.get('/live')).body).toContain('The note, rendered.'); } finally { await hosted.stop(); }
    expect([file.headers['cache-control'], /^"[\w-]+"$/.test(file.headers.etag ?? ''), fixed.headers['cache-control']])
      .toEqual(['public, max-age=31536000, immutable', true, 'no-cache']);
    expect(await get('/item/7')).toMatchObject({ status: 200, body: expect.not.stringContaining('The note, rendered.') });
    expect((await get('/no/such/route')).status).toBe(404);
    // A bad percent escape is no file, and a link in the build to a file outside it is not served (render.mjs
    // --serve serves through the same `serve`).
    writeFileSync(resolve(dir, 'outside.txt'), 'outside the build\n');
    symlinkSync(resolve(dir, 'outside.txt'), resolve(out, 'leak.js'));
    const statuses = await Promise.all(['/%', '/%ZZ', '/leak.js'].map(async (path) => (await fetch(`http://127.0.0.1:${serve.port}${path}`)).status));
    expect(statuses).toEqual([404, 404, 404]);
    rmSync(resolve(out, 'leak.js'));
    // Asked as curl asks (any type, or no Accept), a page rendered per request is sent as to a browser; the client's
    // route is no such page, and a request for another type gets the 404 page.
    const asked = async (path, headers) => { const r = await fetch(`http://127.0.0.1:${serve.port}${path}`, { headers }); return [r.status, (await r.text()).includes(`>${path}<`)]; };
    expect([await asked('/live', { accept: '*/*' }), await asked('/feed', {}), await asked('/item/7', { accept: '*/*' }), await asked('/live', { accept: 'application/json' })])
      .toEqual([[200, true], [200, true], [404, false], [404, false]]);
    // A rendered page is its build's, as a document from a file is: its base names its generation, whose runtime its
    // reader loads from there, even once another build replaces it.
    const rendered = await get('/live'), id = generationIn(rendered.body), loaded = builtFile(out, 'app.js'), runtime = readFileSync(resolve(out, loaded));
    expect(id).toBe(after);
    // Asked for under its generation (as a page's own navigation asks), the same page is rendered, not the notfound route.
    const under = await fetch(`http://127.0.0.1:${serve.port}/.exact/gen/${id}/live`, { headers: html });
    expect([under.status, (await under.text()).includes('The note, rendered.')]).toEqual([200, true]);
    writeFileSync(resolve(app, 'app.contract'), source.replace('text note.text\n', 'text note.text\n      text "Changed again."\n'));
    const again = build();
    expect(again.status, again.stdout + again.stderr).toBe(0);
    const next = await get('/live'), old = await fetch(`http://127.0.0.1:${serve.port}/.exact/gen/${id}/${loaded}`);
    expect([generationIn(next.body) === id, next.body.includes('Changed again.'), old.status, Buffer.from(await old.arrayBuffer()).equals(runtime)]).toEqual([false, true, 200, true]);
    // A source changed since the build: serve builds the site again before it answers a navigation, and answers a
    // build that fails with its output (LLP 1153.001 D3). A source is one however deep in the app it is.
    const deep = resolve(app, 'a/b/c/d/e/part.ts'), rebuilds = (text) => text.split('a source changed since the build: building it again').length - 1;
    mkdirSync(dirname(deep), { recursive: true });
    writeFileSync(deep, "export const part = 'one';\n");
    writeFileSync(resolve(app, 'app.contract'), source.replace('text note.text\n', 'text note.text\n      text "Edited."\n'));
    await new Promise(ok => setTimeout(ok, 350));
    expect((await get('/live')).body).toContain('Edited.');
    await heard((text) => rebuilds(text) === 1);
    writeFileSync(deep, "export const part = 'two';\n");
    await new Promise(ok => setTimeout(ok, 350));
    expect((await get('/live')).status).toBe(200);
    await heard((text) => rebuilds(text) === 2);
    // A source gone is a change too, though no file is newer than the build: an asset deleted is no longer served once
    // the next page has built the site again.
    mkdirSync(resolve(app, 'assets')); writeFileSync(resolve(app, 'assets/demo.txt'), 'Old asset\n');
    await new Promise(ok => setTimeout(ok, 350));
    expect((await get('/live')).status).toBe(200);
    await heard((text) => rebuilds(text) === 3);
    const asset = await fetch(`http://127.0.0.1:${serve.port}/assets/demo.txt`);
    expect([asset.status, await asset.text()]).toEqual([200, 'Old asset\n']);
    rmSync(resolve(app, 'assets/demo.txt')); await new Promise(ok => setTimeout(ok, 350));
    expect((await get('/live')).status).toBe(200);
    await heard((text) => rebuilds(text) === 4);
    expect((await fetch(`http://127.0.0.1:${serve.port}/assets/demo.txt`)).status).toBe(404);
    writeFileSync(resolve(app, 'app.contract'), source.replace('text note.text\n', 'text note.text\n      nosuchtag "x"\n'));
    await new Promise(ok => setTimeout(ok, 350));
    writeFileSync(resolve(out, 'live.md'), '# Stale live\n');
    const broken = await fetch(`http://127.0.0.1:${serve.port}/live`, { headers: { accept: 'text/markdown' } });
    const brokenBody = await broken.text();
    expect([broken.status, brokenBody.startsWith("The app's build failed"), brokenBody.includes('nosuchtag')]).toEqual([500, true, true]);
  } finally {
    await serve?.stop();
    if (server) { server.closeAllConnections(); await new Promise(ok => server.close(ok)); }
    rmSync(dir, { recursive: true, force: true });
  }
}, 90000);

// LLP 1153 D3.4: a home route rendered on a server is rendered by `serve`, though every build writes an index.html
// (the shell, which a static host serves), and so is a notfound route declared so, with 404.
test.each(['request', 'cached'])('serve renders home "/" render=%s and a notfound route rendered so', async (policy) => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-site-home-')), app = resolve(dir, 'site'), out = resolve(app, 'dist');
  let serve;
  try {
    site(app, `shape Note\n  text: string\n\nroutes nav\n  home "/" render=${policy}\n  item "/item/:id"\n  notfound render=${policy}\n\n`
      + 'component App\n  resource note = note("rendered") as shape Note\n  view\n    column\n      text top(nav).url\n      text note.text\n');
    const built = spawnSync(process.execPath, ['host/web/build.mjs', app, '--out', out], { cwd: root, env: { ...process.env, EXACT_APP_DIR: app }, encoding: 'utf8' });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    expect(built.stdout).toContain(`notfound: render=${policy}, so serve renders its page`);
    expect(readFileSync(resolve(out, 'index.html'), 'utf8')).not.toContain('The note, rendered.');
    serve = await serving(root, out);
    const home = await serve.get('/'), missing = await serve.get('/no/such/route'), item = await serve.get('/item/7');
    expect([home.status, home.body.includes('The note, rendered.'), generationIn(home.body) !== undefined]).toEqual([200, true, true]);
    expect(home.cache).toBe(policy === 'cached' ? 'public, max-age=0, s-maxage=60' : 'no-store');
    expect([home.retryAfter, missing.retryAfter]).toEqual([null, null]);
    expect([missing.status, missing.body.includes('The note, rendered.'), missing.body.includes('/no/such/route')]).toEqual([404, true, true]);
    expect([item.status, item.body.includes('The note, rendered.')]).toEqual([200, false]);
  } finally {
    await serve?.stop();
    rmSync(dir, { recursive: true, force: true });
  }
}, 60000);

test('serve preserves the retry delay for unfinished pages, request and cached, GET and HEAD', async () => {
  const root = resolve(import.meta.dir, '../..');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-site-retry-')), app = resolve(dir, 'site'), out = resolve(app, 'dist');
  // A fetch its server never answers: work still in flight at the deadline. (A promise nothing settles holds no page,
  // as the render host settles: render-storage.test.mjs.)
  const silent = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch: () => new Promise(() => {}) });
  let serve;
  try {
    site(app, 'shape Note\n  text: string\n\nroutes nav\n  home "/" render=request\n  feed "/feed" render=cached\n\ncomponent App\n  resource note = note("waiting") as shape Note\n  view\n    text note.text\n');
    writeFileSync(resolve(app, 'app.ts'), `export const grants = 'net.fetch ${silent.url.origin}';\nexport const answer = (): any => fetch('${silent.url.origin}/').then(r => r.json());\n`);
    const built = spawnSync(process.execPath, ['host/web/build.mjs', app, '--out', out], { cwd: root, env: { ...process.env, EXACT_APP_DIR: app }, encoding: 'utf8' });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    serve = await serving(root, out);
    // Each unfinished page waits out the render deadline, so each policy and each method is asked once
    const page = await serve.get('/');
    expect([page.status, page.cache, page.retryAfter]).toEqual([503, 'no-store', '1']);
    const head = await fetch(`http://127.0.0.1:${serve.port}/feed`, { method: 'HEAD', headers: { accept: 'text/html', 'accept-encoding': 'gzip' } });
    expect([head.status, head.headers.get('retry-after'), head.headers.get('content-encoding'), await head.text()]).toEqual([503, '1', 'gzip', '']);
  } finally {
    await serve?.stop(); silent.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 60000);

/** The test app `site` in `app` with `source` as its app.contract and a data source `note`. */
test('host.web.head is in the head of the shell, a page built ahead, one rendered per request and the 404 page, once', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-site-head-')), app = resolve(dir, 'site'), out = resolve(app, 'dist');
  let serve;
  try {
    site(app, 'shape Note\n  text: string\n\nroutes nav\n  home "/" render=build\n  live "/live" render=request\n  notfound render=build\n\n'
      + 'component App\n  resource note = note("rendered") as shape Note\n  view\n    column\n      text top(nav).url\n      text note.text\n');
    writeFileSync(resolve(app, 'app.json'), JSON.stringify({ name: 'Site', app: { id: 'test.site', name: 'Site' }, host: { web: { head: [
      { link: { rel: 'preconnect', href: 'https://analytics.example' } },
      { script: { async: true, src: 'https://analytics.example/tag.js?id=1&x=2' } },
      { script: { text: "for (let i = 0; i<scripts.length; i++); window.seen = '\\x3C/SCRIPT>\\x3C!--';" } },
      { script: { type: 'application/ld+json', text: { '@type': 'Person', name: '</script>' } } },
      // Text that spells the shell's root and a preload line: the renderers find the shell's own places, not these.
      { script: { text: 'const root = \'<div id="exact-root"></div>\', preload = `<link rel="modulepreload" href="./x.js">\n`;' } },
    ] } } }));
    const head = '<link rel="preconnect" href="https://analytics.example">\n<script async src="https://analytics.example/tag.js?id=1&amp;x=2"></script>\n'
      + "<script>for (let i = 0; i<scripts.length; i++); window.seen = '\\x3C/SCRIPT>\\x3C!--';</script>\n<script type=\"application/ld+json\">{\"@type\":\"Person\",\"name\":\"\\u003C/script>\"}</script>\n"
      + '<script>const root = \'<div id="exact-root"></div>\', preload = `<link rel="modulepreload" href="./x.js">\n`;</script>\n';
    const built = spawnSync(process.execPath, ['host/web/build.mjs', app, '--out', out], { cwd: root, env: { ...process.env, EXACT_APP_DIR: app }, encoding: 'utf8' });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    // Once, and before the root: only head elements come before it.
    const placed = (html) => [html.split(head).length - 1, html.indexOf(head) + head.length <= html.lastIndexOf('<div id="exact-root"')];
    for (const name of ['shell.html', 'index.html', '404.html']) expect([name, ...placed(readFileSync(resolve(out, name), 'utf8'))]).toEqual([name, 1, true]);
    serve = await serving(root, out);
    const live = await serve.get('/live');
    expect([live.status, live.body.includes('The note, rendered.'), ...placed(live.body)]).toEqual([200, true, 1, true]);
  } finally {
    await serve?.stop();
    rmSync(dir, { recursive: true, force: true });
  }
}, 300_000);

// An `--inline` shell holds the bundle in its entry's place, after the root, and the bundle carries the app's text: a
// text and a head script that spell the root are both outside the shell's own places (render.mjs `blanked`).
test('a page built from an --inline shell is composed in the shell\'s own root and entry, whatever its scripts spell', () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-site-inline-')), app = resolve(dir, 'site'), out = resolve(app, 'dist');
  try {
    site(app, 'shape Note\n  text: string\n\nroutes nav\n  home "/" render=build\n\n'
      + 'component App\n  resource note = note("rendered") as shape Note\n  view\n    column\n      text note.text\n      text "<div id=\\"exact-root\\"></div>"\n');
    const entry = '<script>const r = \'<div id="exact-root"></div>\';</script>\n';
    writeFileSync(resolve(app, 'app.json'), JSON.stringify({ name: 'Site', app: { id: 'test.site', name: 'Site' },
      host: { web: { head: [{ script: { text: "const r = '<div id=\"exact-root\"></div>';" } }] } } }));
    const built = spawnSync(process.execPath, ['host/web-js/build.mjs', app, '--out', out, '--inline'], { cwd: root, env: { ...process.env, EXACT_APP_DIR: app }, encoding: 'utf8' });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    const shell = readFileSync(resolve(out, 'shell.html'), 'utf8'), page = readFileSync(resolve(out, 'index.html'), 'utf8');
    const inline = `<script type="module" data-entry="./${builtFile(out, 'app.js')}">`;
    expect(shell.includes(inline) && shell.indexOf(inline) > shell.lastIndexOf(entry)).toBe(true);
    // The head entry once and before the document; the document in the root; the checkpoint in the entry's place,
    // which names the entry the capture script loads, so the bundle is not in the page, once or twice.
    const document = page.indexOf('The note, rendered.'), checkpoint = page.indexOf('<script type="application/vnd.exact.checkpoint"');
    expect([page.split(entry).length - 1, page.indexOf(entry) < document, document < checkpoint, page.includes(inline),
      page.split('<script type="application/vnd.exact.checkpoint"').length - 1]).toEqual([1, true, true, false, 1]);
    expect(page.slice(0, document).lastIndexOf('<div id="exact-root">')).toBeGreaterThan(page.indexOf(entry) + entry.length - 1);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 300_000);

// A title holds text, not tags: an app named "Learn <script> tags" is written `&lt;script>` in the shell's title, and a
// head entry's attribute value holding `<script>` is escaped too; the renderers find the shell's own root and entry.
test('an app whose name or head entry spells a script tag renders its pages in the shell\'s own root', () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-site-title-')), app = resolve(dir, 'site'), out = resolve(app, 'dist');
  try {
    site(app, 'shape Note\n  text: string\n\nroutes nav\n  home "/" render=build\n\ncomponent App\n  resource note = note("rendered") as shape Note\n  view\n    column\n      text note.text\n');
    writeFileSync(resolve(app, 'app.json'), JSON.stringify({ name: 'Learn <script> tags', app: { id: 'test.site', name: 'Site' },
      host: { web: { head: [{ meta: { name: 'keywords', content: '<script>, </script>' } }] } } }));
    const built = spawnSync(process.execPath, ['host/web/build.mjs', app, '--out', out], { cwd: root, env: { ...process.env, EXACT_APP_DIR: app }, encoding: 'utf8' });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    const shell = readFileSync(resolve(out, 'shell.html'), 'utf8'), page = readFileSync(resolve(out, 'index.html'), 'utf8');
    expect(shell).toContain('<title>Learn &lt;script> tags</title>');
    expect(shell).toContain('<meta name="keywords" content="&lt;script&gt;, &lt;/script&gt;">');
    const document = page.indexOf('The note, rendered.');
    expect([document > page.lastIndexOf('<div id="exact-root">'), page.split('<div id="exact-root">').length - 1, page.includes('<title>Learn &lt;script&gt; tags</title>')]).toEqual([true, 1, true]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 300_000);

// The build puts its compiled Markdown before the shell's own entry: not before a head entry that is a module script,
// nor inside a head script's text that spells one (render.mjs `markdownAt`).
test("compiled Markdown goes before the shell's own entry, whatever the head's scripts are or spell", async () => {
  const { markdownAt } = await import('./render.mjs');
  const entry = '<script type="module" data-entry src="./app.js"></script>', inline = '<script type="module" data-entry="./app.js">const a = "<script type=\\"module\\">";</script>';
  const head = '<script type="module" src="https://cdn.example/x.js"></script>\n<script>const s = \'<script type="module" src="./app.js">\';</script>\n';
  for (const tail of [entry, inline]) {
    const shell = `<!doctype html>\n<html lang="en">\n<title>T</title>\n<meta name="viewport" content="x">\n<style>p{}</style>\n${head}<div id="exact-root"></div>\n${tail}\n`;
    expect(markdownAt(shell)).toBe(shell.lastIndexOf(tail));
  }
});

// The render realm runs no code from a string by any route (#128): the module's `Function` and `eval` throw Hermes's
// error, and a function's own `constructor` throws the realm's EvalError, as under the render server's CSP.
test('a page rendered at build runs no code from a string, by any name or through a function\'s constructor', () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
  const dir = mkdtempSync(resolve(root, 'target/site-strings-')), app = resolve(dir, 'site'), out = resolve(app, 'dist');
  try {
    site(app, 'shape Note\n  text: string\n\nroutes nav\n  home "/" render=build\n\ncomponent App\n  resource note = note("x") as shape Note\n  view\n    column\n      text note.text\n');
    // A module that names only `eval`, never `globalThis`: its `eval` is the stand-in all the same (build.mjs `scopedModule`).
    // A parameter named `globalThis` stands between no `eval` and the stand-in.
    writeFileSync(resolve(app, 'evals.ts'), "export const direct = () => eval('6*7'), indirect = () => (0, eval)('6*7');\n"
      + "export function captured(globalThis: { eval(source: string): unknown }) { return eval('1'); }\n"
      + "export function probe(globalThis: null) { return typeof eval; }\n");
    // CommonJS permits a binding named `eval`; its call must stay local.
    const local = resolve(app, 'node_modules/local-eval');
    mkdirSync(local, { recursive: true });
    writeFileSync(resolve(local, 'package.json'), JSON.stringify({ name: 'local-eval', version: '1.0.0', main: 'index.cjs', types: 'index.d.ts' }));
    writeFileSync(resolve(local, 'index.cjs'), `exports.probe = function(eval) { return eval('1'); };`);
    writeFileSync(resolve(local, 'index.d.ts'), 'export function probe(evaluate: (source: string) => number): number;\n');
    writeFileSync(resolve(app, 'app.ts'), "import type { Answer, Result, Sources } from './app.contract.d.ts';\nimport { captured, direct, indirect, probe } from './evals';\nimport { probe as local } from 'local-eval';\nexport const appId = 'test.site';\nexport const grants = '';\n"
      + "const tried = (f: () => unknown) => { try { return `ran ${String(f())}`; } catch (e) { return (e as Error).name; } };\n"
      + "const F = Function;\n"
      + "const sources: Sources = { note: (): Result<'note'> => ({ text: [tried(() => new Function('return 1')()), tried(() => new F('return 1')()), tried(() => Reflect.construct(Function, ['return 1'])), tried(() => globalThis.Function('return 1')),\n"
      + "  tried(direct), tried(indirect), tried(() => globalThis.eval('1')), tried(() => captured({ eval: () => 1 })), String(Function instanceof Function), String(eval instanceof Function),\n"
      + "  tried(() => probe(null)), tried(() => local(() => 42)),\n"

      + "  tried(() => new ((function () {}).constructor as FunctionConstructor)('return 1')())].join(' ') }) };\n"
      + 'export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);\n');
    const built = spawnSync(process.execPath, ['host/web/build.mjs', app, '--out', out], { cwd: root, env: { ...process.env, EXACT_APP_DIR: app }, encoding: 'utf8' });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    // Free uses of `Function` and `eval` reach the module's stand-ins; a bound `eval` stays local.
    expect(readFileSync(resolve(out, 'index.html'), 'utf8')).toContain(`>${Array(8).fill('SyntaxError').join(' ')} true true ran function ran 42 EvalError<`);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 300_000);

test('authored names cannot capture the eval stand-in in rendered pages or the page bundle', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/eval-bindings-'));
  const app = resolve(dir, 'site'), out = resolve(app, 'dist');
  let browser, server;
  try {
    site(app, `shape Note
  text: string

routes nav
  home "/" render=build

component App
  state phase: string = "build"
  resource note = note(phase) as shape Note
  action probe
    phase = "page"
  view
    column
      text note.text testId="result"
      button "Probe" press=probe testId="probe"
`);
    const probes = {
      'parameter.js': `export function probe(__exact_eval) { try { return eval('1'); } catch { return -1; } }`,
      'module.js': `const __exact_eval = () => 42;
export function probe() { try { return eval('1'); } catch { return -1; } }`,
      'nested.js': `export function probe() {
  const __exact_eval = () => 42;
  return ((__exact_eval1) => {
    const __exact_eval2 = () => 44;
    try { return (0, eval)('1'); } catch { return -1; }
  })(() => 43);
}`,
      'escaped.js': `export function probe(\\u005f_exact_eval, \\u005f_exact_eval1) {
  try { return eval('1'); } catch { return -1; }
}`,
      'reference.js': `export function probe() {
  try { eval('1'); } catch {}
  return typeof __exact_eval;
}`,
      'jsx-member.jsx': `/** @jsx createElement */
/** @jsxRuntime classic */
const createElement = type => typeof type;
export function probe(__exact_eval) {
  try { eval('1'); } catch {}
  try { return <__exact_eval1.Child />; } catch (e) { return e.name; }
}`,
      'jsx-bare.jsx': `/** @jsx createElement */
/** @jsxRuntime classic */
const createElement = type => typeof type;
export function probe(__exact_eval) {
  try { eval('1'); } catch {}
  try { return <__exact_eval1 />; } catch (e) { return e.name; }
}`,
      'jsx-unsuffixed.jsx': `/** @jsx createElement */
/** @jsxRuntime classic */
const createElement = type => typeof type;
export function probe() {
  try { eval('1'); } catch {}
  try { return <__exact_eval.Child />; } catch (e) { return e.name; }
}`,
      'jsx-factory.jsx': `/** @jsx __exact_eval1 */
/** @jsxRuntime classic */
export function probe(__exact_eval) {
  try { return <div />; } catch (e) { return e.name; }
}`,
      'jsx-fragment.jsx': `/** @jsx createElement */
/** @jsxFrag __exact_eval1 */
/** @jsxRuntime classic */
const createElement = type => typeof type;
export function probe(__exact_eval) {
  try { return <></>; } catch (e) { return e.name; }
}`,
    };
    for (const [name, code] of Object.entries(probes)) {
      if (!name.endsWith('.jsx')) { writeFileSync(resolve(app, name), code); continue; }
      const pkg = name.slice(0, -4), at = resolve(app, 'node_modules', pkg);
      mkdirSync(at, { recursive: true });
      writeFileSync(resolve(at, 'package.json'), JSON.stringify({ name: pkg, version: '1.0.0', main: 'index.jsx', types: 'index.d.ts' }));
      writeFileSync(resolve(at, 'index.jsx'), code);
      writeFileSync(resolve(at, 'index.d.ts'), 'export function probe(evaluate?: () => number): string;\n');
    }
    writeFileSync(resolve(app, 'app.ts'), `import type { Answer } from './app.contract.d.ts';
import { probe as parameter } from './parameter.js';
import { probe as module } from './module.js';
import { probe as nested } from './nested.js';
import { probe as escaped } from './escaped.js';
import { probe as reference } from './reference.js';
import { probe as jsxMember } from 'jsx-member';
import { probe as jsxBare } from 'jsx-bare';
import { probe as jsxUnsuffixed } from 'jsx-unsuffixed';
import { probe as jsxFactory } from 'jsx-factory';
import { probe as jsxFragment } from 'jsx-fragment';
export const appId = 'test.site';
export const grants = '';
export const answer: Answer = (_source, [phase]) => ({ text: [phase, parameter(() => 42), module(), nested(),
  escaped(() => 42, () => 43), reference(), jsxMember(() => 42), jsxBare(() => 42), jsxUnsuffixed(),
  jsxFactory(() => 42), jsxFragment(() => 42)].join(' ') });
`);
    const built = spawnSync(process.execPath, ['host/web-js/build.mjs', app, '--out', out], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: app }, encoding: 'utf8',
    });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    const result = '-1 -1 -1 -1 undefined ReferenceError ReferenceError ReferenceError ReferenceError ReferenceError';
    const html = readFileSync(resolve(out, 'index.html'), 'utf8');
    expect(/data-testid="result"[^>]*>([^<]*)</.exec(html)?.[1]).toBe(`build ${result}`);
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const path = new URL(request.url).pathname;
      return new Response(Bun.file(resolve(out, path === '/' ? 'index.html' : path.slice(1))));
    } });
    browser = await suiteBrowser();
    const page = await browser.newPage(), errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(server.url.href + '?agent=1');
    await page.waitForFunction(() => globalThis.exact?.ready);
    await page.evaluate(async () => { await exact.ready; });
    expect(errors).toEqual([]);
    await page.getByTestId('probe').click();
    await page.waitForFunction(expected => document.querySelector('[data-testid=result]')?.textContent === expected, `page ${result}`);
    expect(errors).toEqual([]);
  } finally {
    await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true });
  }
}, 60000);

function site(app, source) {
  mkdirSync(app, { recursive: true });
  writeFileSync(resolve(app, 'app.json'), JSON.stringify({ name: 'Site', app: { id: 'test.site', name: 'Site' } }));
  writeFileSync(resolve(app, 'app.contract'), source);
  writeFileSync(resolve(app, 'app.ts'), "import type { Answer, Result, Sources } from './app.contract.d.ts';\nexport const appId = 'test.site';\nexport const grants = '';\n"
    + "const sources: Sources = { note: ([what]): Result<'note'> => ({ text: `The note, ${what}.` }) };\n"
    + 'export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);\n');
}
const generationIn = (body) => /<base href="\/\.exact\/gen\/([0-9a-f]{32})\/">/.exec(body)?.[1];

test('source capture skips nested dot directories in apps and mounts and names refused links', async () => {
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/hidden-capture-'));
  const app = resolve(dir, 'app'), shared = resolve(dir, 'shared'), out = resolve(dir, 'dist');
  const build = () => spawnSync(process.execPath, ['host/web-js/build.mjs', app, '--out', out, '--render', 'none'],
    { cwd: root, encoding: 'utf8', env: process.env });
  let browser, server;
  try {
    site(app, 'shape Note\n  text: string\n\ncomponent App\n  resource note = note("ready") as shape Note\n  view\n    text note.text testId="note"\n');
    mkdirSync(shared);
    const manifest = JSON.parse(readFileSync(resolve(app, 'app.json'), 'utf8'));
    manifest.typescript = { sources: { shared: '../shared' } };
    writeFileSync(resolve(app, 'app.json'), JSON.stringify(manifest));
    writeFileSync(resolve(shared, '.settings.json'), '{}');
    writeFileSync(resolve(shared, 'model.ts'), "export const word = 'ready';\n");
    const source = readFileSync(resolve(app, 'app.ts'), 'utf8');
    writeFileSync(resolve(app, 'app.ts'), "import { word } from './shared/model';\n" + source.replace('`The note, ${what}.`', '`The note, ${word}.`'));
    for (const from of [app, shared]) {
      for (const cache of ['backend/.elixir_ls/build/lib/app', 'server/.venv/bin']) {
        const at = resolve(from, cache);
        mkdirSync(at, { recursive: true });
        mkdirSync(resolve(from, 'backend/priv'), { recursive: true });
        writeFileSync(resolve(at, 'cache.json'), '{}');
        symlinkSync(resolve(from, 'backend/priv'), resolve(at, 'priv'));
        symlinkSync('missing-python', resolve(at, 'python'));
      }
    }
    const built = build();
    expect(built.status, built.stdout + built.stderr).toBe(0);
    expect(existsSync(resolve(out, '.gen/typescript/shared/.settings.json'))).toBe(true);
    for (const from of ['', 'shared']) for (const cache of ['backend/.elixir_ls', 'server/.venv']) {
      expect(existsSync(resolve(out, '.gen/typescript', from, cache))).toBe(false);
    }
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const path = new URL(request.url).pathname;
      return new Response(Bun.file(resolve(out, path === '/' ? 'index.html' : path.slice(1))));
    } });
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.getByTestId('note').waitFor();
    expect(await page.getByTestId('note').innerText()).toBe('The note, ready.');
    await browser.close(); browser = null;
    const link = resolve(app, 'backend/helper.ts');
    symlinkSync('../app.ts', link);
    const refused = build();
    expect(refused.status).toBe(1);
    expect(refused.stderr).toContain(`error: app: source links are not captured: ${link}`);
    expect(refused.stderr).not.toContain('failed the type check');
    rmSync(link);
    writeFileSync(resolve(app, 'app.ts'), source + '\nconst bad: number = "wrong";\n');
    const mistyped = build();
    expect(mistyped.status).toBe(1);
    expect(mistyped.stderr).toContain('failed the type check');
  } finally { await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true }); }
}, 60000);

/** serve.mjs on `out` as the `serve` verb runs it (`--loopback`), on a free port: `get(path)` a navigation's answer,
 * `renders(path)` how many pages of it serve said it rendered, `heard(done)` once what it said satisfies `done`. */
async function serving(root, out, { fromEnv = false } = {}) {
  const probe = createServer();
  await new Promise(ok => probe.listen(0, '127.0.0.1', ok));
  const port = probe.address().port;
  await new Promise(ok => probe.close(ok));
  // `fromEnv`: the port as a host gives `npm start` one, in `PORT` (LLP 1153.001 D3).
  const child = spawnOwned(process.execPath, ['host/web/serve.mjs', out, ...(fromEnv ? [] : ['--port', String(port)]), '--loopback'],
    { cwd: root, stdio: ['ignore', 'pipe', 'pipe'], env: { ...process.env, PORT: fromEnv ? String(port) : '' } });
  let said = '';
  child.stdout.on('data', (data) => { said += data; });
  child.stderr.on('data', (data) => { said += data; });
  // Its output is the event; bounded so a failure names what it said.
  const heard = (done) => new Promise((ok, no) => {
    const timer = setTimeout(() => { child.stdout.off('data', check); no(new Error(`serve did not say it: ${said}`)); }, 30_000);
    const check = () => { if (done(said)) { clearTimeout(timer); child.stdout.off('data', check); ok(); } };
    child.stdout.on('data', check); check();
  });
  await heard((text) => text.includes(`http://127.0.0.1:${port}/`));
  const get = async (path) => {
    const r = await fetch(`http://127.0.0.1:${port}${path}`, { headers: { accept: 'text/html' } });
    return { status: r.status, cache: r.headers.get('cache-control'), retryAfter: r.headers.get('retry-after'), body: await r.text() };
  };
  const renders = (path) => (said.match(new RegExp(`render ${path} 200 `, 'g')) ?? []).length;
  return { port, get, renders, heard, stop: () => child.stop(5_000) };
}

test('Snapback mutation success types check final answers through helpers and preserve ordinary resource values', async () => {
  const {configure, check} = await import('../../js/bake/src/typescript.mjs');
  const {parseSync} = await import('rolldown/utils');
  const root = resolve(import.meta.dir, '../..');
  const libraries = resolve(dirname(fileURLToPath(import.meta.resolve(`@typescript/typescript-${process.platform}-${process.arch}/package.json`))), 'lib');
  const dir = mkdtempSync(resolve(root, 'target/snapback-refusals-'));
  const declarations = `export type MutationSource = "post" | "post-note" | "count";
export interface SourceMap {
  post: {args: []; result: {ok: boolean; message: string}};
  "post-note": {args: []; result: {ok: boolean; message: string}};
  status: {args: []; result: {ok: boolean; message: string}};
  count: {args: []; result: number};
}
export type Source = keyof SourceMap;
export type Result<S extends Source> = SourceMap[S]['result'];
export type Sources = {[S in Source]: () => Result<S> | Promise<Result<S>>};
export type Answer = <S extends Source>(source: S) => Result<S> | Promise<Result<S>>;
`;
  const imports = "import type {Answer, Sources, Result} from './app.contract.d.ts';\n";
  const source = body => imports + `function refused(why: {code:string;message:string}) {return new Error(why.message);}
const sources: Sources = {
  post: () => {${body}},
  "post-note": () => ({ok:true, message:'Posted'}),
  status: () => ({ok:false, message:'Loading'}),
  count: () => 1,
};
export const answer: Answer = (name) => sources[name]();
`;
  const compile = async (body, snapback = true) => {
    writeFileSync(resolve(dir, 'app.ts'), source(body));
    writeFileSync(resolve(dir, '__exact_entry.ts'), "export * from './app.ts';\n");
    writeFileSync(resolve(dir, 'app.contract.d.ts'), declarations);
    writeFileSync(resolve(dir, '__exact_paths.json'), JSON.stringify({app:dir, mounts:snapback ? [['adapter',resolve(root,'snapback4/online')]] : []}));
    configure(dir, false, parseSync); // as build.mjs configures a stage
    return check(dir, resolve(libraries, 'tsc'), libraries);
  };
  try {
    for (const body of [
      "return {ok:false, message:'No room'};",
      "const value = {ok:false, message:'No room'}; const alias = value; return alias;",
      "const value = {ok:false, message:'No room'}; const {ok,message} = value; return {ok,message};",
      "function identity<T>(value:T) {return value;} return identity({ok:false, message:'No room'});",
      "return Promise.resolve({ok:false, message:'No room'});",
      "function helper() {return {ok:false, message:'No room'};} return helper();",
    ]) {
      let failure;
      try {await compile(body);} catch(error) {failure=error;}
      expect(failure?.message, body).toContain('throw refused(');
    }
    await compile("function helper() {return {ok:false, message:'Handled'};} const intermediate = helper(); return {ok:true, message:intermediate.message};");
    await compile("function helper(): Result<'post'> {return {ok:true, message:'Posted'};} return helper();");
    await compile("const result = {ok:true as const, message:'Posted'}; return result;");
    await compile("throw refused({code:'FULL', message:'No room'});");
    await compile("return {ok:false, message:'Ordinary app result'};", false);
    expect(readFileSync(resolve(dir,'app.contract.d.ts'),'utf8')).toBe(declarations);
  } finally {rmSync(dir, {recursive: true, force: true});}
}, 30_000);


test('package declarations over the cap are named by package, size and the captured file that reached them', async () => {
  const { stagePackages } = await import('../../js/bake/src/typescript.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-declarations-cap-')), app = resolve(dir, 'app'), stage = resolve(dir, 'stage');
  const write = (path, text) => { mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, text); };
  try {
    // A web page's source beside app.ts imports a package whose declarations import a heavy one.
    write(resolve(app, 'node_modules/diagram/package.json'), '{"name":"diagram","version":"1.0.0","types":"index.d.ts"}');
    write(resolve(app, 'node_modules/diagram/index.d.ts'), "export * from 'heavy';\n");
    write(resolve(app, 'node_modules/heavy/package.json'), '{"name":"heavy","version":"1.0.0","types":"index.d.ts"}');
    write(resolve(app, 'node_modules/heavy/index.d.ts'), 'export type Heavy = 1;\n'.repeat(3 << 20));
    for (const root of [app, stage]) write(resolve(root, 'page/main.ts'), "import { Heavy } from 'diagram';\nexport const h: Heavy = 1;\n");
    write(resolve(stage, '__exact_paths.json'), JSON.stringify({ app, mounts: [] }));
    expect(() => stagePackages(stage)).toThrow(/^package declarations exceed 64 MiB: heavy 69\.0 MiB \(through diagram, from page\/main\.ts\), diagram 0\.0 MiB \(from page\/main\.ts\)\. .*whose name starts with a dot$/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('the web module exports the source app.json\'s rust.data names, in the grammar it copies (LLP 1153.003 D2)', () => {
  const data = (fields) => rustDataOf({ rust: { data: { crate: 'mail-data', type: 'mail_data::Mail', ...fields } } });
  expect(data({})).toEqual({ krate: 'mail-data', ty: 'mail_data::Mail', lib: 'mail_data', source: 'mail_data::Mail::default()' });
  for (const source of ['mail_data::Mail', 'mail_data::Mail::new()', 'mail_data::Mail::new(true)', 'mail_data::Mail::new(3, "a, b")',
    "mail_data::Mail::new(',', ')')", 'mail_data::Mail::new(\'"\', "/*keep*/")', 'mail_data::Mail::new(1e-3, 2_000u32, -4.5f64)']) {
    expect(data({ source }).source).toBe(source);
  }
  // A local, a reference, a nested call, a builder chain, another type, or a bake's own constructor names nothing
  // the module can copy.
  for (const source of ['mail_data::Mail::new(path)', 'source', 'mail_data::Mail::from(&config)', 'mail_data::Mail::new(mail_data::Mode::Fast)',
    'mail_data::Mail::new(0..end)', 'mail_data::Mail::new(false).with_auth(true)', 'other_data::Mail', 'mail_data::Mail::for_bake()']) {
    expect(() => data({ source })).toThrow("a source built from anything else is a module of its own (rust.module.package)");
  }
  expect(() => data({ type: 'Mail' })).toThrow('rust.data.type is a path');
  expect(() => data({ mode: 'off' })).toThrow('not mode');
  expect(rustDataOf({ rust: { mode: 'off' } })).toBeNull();
  // An app that names neither a module nor a data crate has no Rust module, and is told.
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-module-source-'));
  try {
    writeFileSync(resolve(dir, 'app.json'), '{}');
    expect(() => buildModule('mail', resolve(dir, 'out'), false, dir)).toThrow('its web build bakes with no Rust data source');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// LLP 1153.003 D2: the JS target runs only the Rust source app.json's rust.data names, so a web crate that links the
// app's data crate without it is refused with the line to add, not built into a page that answers nothing.
test('a web crate linking a data crate app.json does not name is refused with the rust.data line', () => {
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(tmpdir(), 'exact-undeclared-')), out = resolve(dir, 'dist');
  const write = (path, text) => { mkdirSync(dirname(resolve(dir, path)), { recursive: true }); writeFileSync(resolve(dir, path), text); };
  try {
    write('app.json', JSON.stringify({ name: 'Pins', app: { id: 'test.pins', name: 'Pins' }, rust: 'off' }));
    write('app.contract', 'component App\n  resource message = message() as shape string\n  view\n    column\n      text message\n');
    write('data/Cargo.toml', '[package]\nname = "pins-data"\nversion = "0.1.0"\nedition = "2021"\n');
    write('web/Cargo.toml', '[package]\nname = "pins-web"\nversion = "0.1.0"\nedition = "2021"\n\n[dependencies]\npins-data = { path = "../data" }\n');
    write('web/build.rs', 'fn main() { let _ = contract::bake(plan, pins_data::Pins::new(3)); }\n');
    const built = spawnSync(process.execPath, ['host/web-js/build.mjs', dir, '--out', out, '--render', 'none'], { cwd: root, encoding: 'utf8', env: { ...process.env, EXACT_APP_DIR: dir } });
    expect(built.status, built.stdout + built.stderr).toBe(1);
    expect(built.stderr).toContain('web/Cargo.toml links pins-data, a crate beside app.json, and app.json names no rust.data: the web build runs only the Rust source rust.data names');
    expect(built.stderr).toContain('\n  "rust": { "mode": "off", "data": { "crate": "pins-data", "type": "pins_data::<Type>", "source": "pins_data::<Type>::new(…)" } }\n');
    const manifest = JSON.parse(readFileSync(resolve(dir, 'app.json'), 'utf8'));
    expect(undeclaredRustData('pins', dir, manifest)).toContain('links pins-data');
    // A module of the app's own answers instead, and a web crate that links none of the app's crates has no Rust source.
    expect(undeclaredRustData('pins', dir, { ...manifest, rust: { module: { package: 'pins-logic' } } })).toBeNull();
    write('web/Cargo.toml', '[package]\nname = "pins-web"\nversion = "0.1.0"\nedition = "2021"\n\n[dependencies]\ncontract = { path = "../../contract/cli" }\n');
    expect(undeclaredRustData('pins', dir, manifest)).toBeNull();
    // A source the build script defines and composes with app.ts links no crate, and is refused all the same.
    write('web/build.rs', 'mod pins { pub struct Pins; }\nfn main() { exact_js_bake::build_mixed_with(std::path::Path::new(".."), "web", &pins::Pins, &[], |m| todo!()).unwrap(); }\n');
    const inline = undeclaredRustData('pins', dir, manifest);
    expect(inline).toContain('pins: web/build.rs composes a Rust source with app.ts (exact_js_bake::build_mixed_with), and app.json names no rust.data');
    expect(inline).toContain('"data": { "crate": "pins-data", "type": "pins_data::<Type>"');
    rmSync(resolve(dir, 'data'), { recursive: true });
    expect(undeclaredRustData('pins', dir, manifest)).toContain('"data": { "crate": "<crate>", "type": "<library>::<Type>"');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('a mixed contract build leaves the Cargo provider types even when checking fails', async () => {
  const root = resolve(import.meta.dir, '../..');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/mixed-provider-types-'));
  const { patchLines } = await import('../../scripts/app.mjs');
  const env = { ...process.env, EXACT_APP_DIR: dir, CARGO_TARGET_DIR: resolve(dir, 'target'), CARGO_NET_OFFLINE: 'true', HERMES_LEAN_SYS_OFFLINE: '1' };
  const write = (name, text) => { mkdirSync(dirname(resolve(dir, name)), { recursive: true }); writeFileSync(resolve(dir, name), text); };
  const run = () => spawnSync(process.execPath, [resolve(root, 'scripts/exact.mjs'), 'contract', 'build', 'app.contract'], { cwd: dir, env, encoding: 'utf8', timeout: 240_000 });
  const declarations = () => readFileSync(resolve(dir, 'app.contract.d.ts'), 'utf8');
  try {
    write('Cargo.toml', `[workspace]\nmembers = ["data", "web"]\nresolver = "2"\n[profile.dev]\ndebug = 0\n[patch.crates-io]\n${patchLines(dir).join('\n')}\n`);
    write('data/Cargo.toml', `[package]\nname = "provider-data"\nversion = "0.1.0"\nedition = "2021"\n[dependencies]\nexact-runner = { path = ${JSON.stringify(resolve(root, 'runner'))} }\n`);
    const data = `pub const RUST_SOURCES: &[&str] = &["rust"];
pub struct Rust;
impl exact_runner::DataSource for Rust {
    fn app_id(&self) -> &str { "test.providers" }
    fn query(&mut self, _: &str, _: &[exact_runner::Value]) -> Result<exact_runner::Value, exact_runner::DataError> {
        Ok(exact_runner::Value::str("Rust"))
    }
}
`;
    write('data/src/lib.rs', data);
    write('web/Cargo.toml', `[package]\nname = "provider-web"\nversion = "0.1.0"\nedition = "2021"\n[build-dependencies]\nprovider-data = { path = "../data" }\nexact-js-bake = { path = ${JSON.stringify(resolve(root, 'js/bake'))} }\nexact-data = { path = ${JSON.stringify(resolve(root, 'data'))} }\n`);
    write('web/src/lib.rs', '');
    write('web/build.rs', `fn main() {
    exact_js_bake::build_mixed_with(
        std::path::Path::new(".."), "web", &provider_data::Rust, provider_data::RUST_SOURCES,
        |javascript| Box::new(exact_data::Mixed::new(javascript, provider_data::Rust,
            if provider_data::RUST_SOURCES.is_empty() { &["javascript", "rust"] } else { &["javascript"] },
            provider_data::RUST_SOURCES).unwrap()),
    ).unwrap();
}
`);
    const manifest = { name: 'Provider', app: { id: 'test.providers', name: 'Provider' }, deploy: { store: { web: '0' } } };
    write('app.json', JSON.stringify({ ...manifest, rust: { data: { crate: 'provider-data', type: 'provider_data::Rust', source: 'provider_data::Rust' } } }));
    write('app.contract', 'component App\n  resource javascript = javascript() as shape string\n  resource rust = rust() as shape string\n  view\n    column\n      text javascript\n      text rust\n');
    const source = `import type { TypeScriptSources, TypeScriptAnswer } from './app.contract.d.ts';
export const grants = '';
const sources: TypeScriptSources = { javascript: () => 'TypeScript' };
export const answer: TypeScriptAnswer = (source, args, store, storage, native) => sources[source](args, store, storage, native);
`;
    write('app.ts', source);
    const locked = spawnSync('cargo', ['generate-lockfile', '--offline'], { cwd: dir, env, encoding: 'utf8', timeout: 60_000 });
    expect(locked.status, locked.stdout + locked.stderr).toBe(0);
    let checked = run();
    expect(checked.status, checked.stdout + checked.stderr).toBe(0);
    expect(declarations()).toContain('Exclude<Source, "rust">');
    checked = run();
    expect(checked.status, checked.stdout + checked.stderr).toBe(0);
    write('app.ts', source.replace("{ javascript: () => 'TypeScript' }", '{}'));
    checked = run();
    expect(checked.status).toBe(1);
    expect(checked.stdout + checked.stderr).toContain("Property 'javascript' is missing");
    expect(declarations()).toContain('Exclude<Source, "rust">');
    write('app.ts', source);
    write('data/src/lib.rs', data.replace('&["rust"];', '&[];'));
    checked = run();
    expect(checked.status).toBe(1);
    expect(checked.stdout + checked.stderr).toContain("Property 'rust' is missing");
    expect(declarations()).toContain('export type TypeScriptSource = Source;');
    write('app.ts', source.replace("javascript: () => 'TypeScript'", "javascript: () => 'TypeScript', rust: () => 'Moved'"));
    checked = run();
    expect(checked.status, checked.stdout + checked.stderr).toBe(0);
    write('data/src/lib.rs', data);
    write('app.ts', source.replace("{ javascript: () => 'TypeScript' }", '{}'));
    checked = run();
    expect(checked.status).toBe(1);
    expect(checked.stdout + checked.stderr).toContain("Property 'javascript' is missing");
    expect(declarations()).toContain('Exclude<Source, "rust">');
    // LLP 1153.003 D2: the JS target runs only the source rust.data names, so a build that composes a source
    // app.json does not name is refused, not type-checked against declarations that leave its sources to app.ts.
    write('app.json', JSON.stringify(manifest));
    checked = run();
    expect(checked.status).toBe(1);
    expect(checked.stdout + checked.stderr).toContain('web/build.rs composes a Rust source with app.ts (exact_js_bake::build_mixed_with), and app.json names no rust.data');
    expect(checked.stdout + checked.stderr).toContain('"data": { "crate": "provider-data", "type": "provider_data::<Type>"');
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 300_000);
