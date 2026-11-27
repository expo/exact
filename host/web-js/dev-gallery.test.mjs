// The variant gallery on the JS dev loop (host/web-js/dev-gallery.mjs, the
// design tool's G3): nothing is made until a page asks, through the loop's
// gate; then `contract gallery`'s source is built into its own dist and
// served at /__dev/gallery/ with its own stream, which registers with
// `page: 'gallery'`; a kept design edit reaches the app as a batch and the
// gallery as a build, neither stream carrying the other's frames; the
// gallery stops a while after its last page goes, and with the loop.
// On a copy of apps/library-gate under the repo's target (its packages are
// the repo's), with an id of its own, so the gallery's cache is its own.
import { afterEach, expect, test } from 'bun:test';
import { spawnOwned, stopAll } from '../../scripts/test-children.mjs';
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createServer, request } from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { drivable } from '../../scripts/coverage.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { whenRendered } from '../web/engagement.js';
import { frameBuildQueue, galleryCache } from './dev-gallery.mjs';
import { FRAME_ANCESTORS } from '../../scripts/forma-server.mjs';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(fileURLToPath(new URL('../..', import.meta.url)));
afterEach(() => stopAll());

/** A copy of library-gate's contract and design with id `id`, under the repo's target. */
function copy(id) {
  const dir = resolve(ROOT, 'target', `dev-gallery-test-${process.pid}`, 'app');
  rmSync(dir, { recursive: true, force: true });
  for (const f of ['app.contract', 'app.forma']) cpSync(resolve(ROOT, 'apps/library-gate', f), join(dir, f), { recursive: true });
  // An asset of the app's: the gallery's are the app's.
  mkdirSync(join(dir, 'assets'), { recursive: true });
  writeFileSync(join(dir, 'assets', 'note.txt'), 'the app\'s asset\n');
  const manifest = JSON.parse(readFileSync(resolve(ROOT, 'apps/library-gate/app.json'), 'utf8'));
  manifest.app.id = id; manifest.id = id;
  // The copy has no data crate (its contract asks for no data), so its manifest names none (LLP 1153.003 D2).
  delete manifest.rust;
  writeFileSync(join(dir, 'app.json'), JSON.stringify(manifest, null, 2));
  return dir;
}

async function devLoop(dir, env = {}, args = []) {
  const listener = createServer();
  await new Promise((ok) => listener.listen(0, '127.0.0.1', ok));
  const port = listener.address().port;
  await new Promise((ok) => listener.close(ok));
  // The build's lock and kept generations go beside its dist (serve.mjs `replaceBuild`): the dist sits in a directory of its own.
  const dist = join(mkdtempSync(join(tmpdir(), 'exact-dev-gallery-')), 'dist');
  const dev = spawnOwned(process.execPath, [resolve(ROOT, 'host/web/dev.mjs'), '--app', dir, '--port', String(port), ...args], { cwd: ROOT, env: { ...process.env, EXACT_WEB_DIST: dist, ...env }, stdio: ['ignore', 'pipe', 'pipe'] });
  let out = '';
  dev.stderr.on('data', (d) => { out = (out + d).slice(-20000); });
  await new Promise((ok, fail) => {
    dev.stdout.on('data', (d) => { out = (out + d).slice(-20000); if (out.includes('plan ready') && out.includes(`127.0.0.1:${port}/`)) ok(); });
    dev.on('exit', (code) => fail(new Error(`the dev loop exited ${code}: ${out}`)));
  });
  return { base: `http://127.0.0.1:${port}`, port, dev, said: () => out, stop: async () => { await dev.stop(30_000); rmSync(dirname(dist), { recursive: true, force: true }); } };
}

/** An SSE stream's frames, as they come; `close` ends it. */
function frames(url) {
  const got = [];
  let buffered = '';
  const req = request(url, (res) => res.on('data', (d) => {
    buffered += d;
    const parts = buffered.split('\n\n');
    buffered = parts.pop();
    for (const p of parts) for (const line of p.split('\n')) if (line.startsWith('data: ')) got.push(JSON.parse(line.slice(6)));
  }));
  req.on('error', () => {});
  req.end();
  return { got, close: () => req.destroy() };
}

const until = async (what, ms = 300_000) => {
  const end = Date.now() + ms;
  while (!await what()) { if (Date.now() > end) throw new Error('timed out'); await Bun.sleep(100); }
};

test('the gallery: made only once a page asks through the gate, its own build, stream and registration; a kept design edit is a batch to the app and a build to the gallery, never crossing; stopped when idle and with the loop', async () => {
  const id = `com.exact.dev-gallery-test-${process.pid}`;
  const dir = copy(id);
  const cache = galleryCache({ dir, id }, ROOT);
  rmSync(cache, { recursive: true, force: true });
  const loop = await devLoop(dir, { EXACT_DEV_GALLERY_IDLE_MS: '3000' });
  try {
    // A refused origin starts nothing.
    const refused = await new Promise((ok) => request({ host: '127.0.0.1', port: loop.port, path: '/__dev/gallery/', headers: { host: 'evil.example' } }, (res) => { res.resume(); ok(res.statusCode); }).end());
    expect(refused).toBe(421);
    await Bun.sleep(500);
    expect(existsSync(join(cache, 'live/app.contract'))).toBe(false);
    // The first page waits on the gallery's stream while it builds.
    expect(await (await fetch(`${loop.base}/__dev/gallery/`)).text()).toContain('Building the gallery');
    const gallery = frames(`${loop.base}/__dev/gallery/stream`);
    await until(() => gallery.got.some((f) => f.reload >= 1));
    const nonce = gallery.got[0].nonce;
    expect(typeof nonce).toBe('string');
    expect(existsSync(join(cache, 'live/app.contract'))).toBe(true);
    expect(lstatSync(join(cache, 'live/assets')).isDirectory()).toBe(true);
    expect(readFileSync(join(cache, 'live/assets/note.txt'), 'utf8')).toBe("the app's asset\n");
    // Its page: its files under its path, its own dev client.
    const page = await (await fetch(`${loop.base}/__dev/gallery/`)).text();
    expect(page).toContain('<base href="/__dev/gallery/">');
    // Framed only by the design tool's origin and its own, as every page of the loop (frame-ancestors).
    expect((await fetch(`${loop.base}/__dev/gallery/`)).headers.get('content-security-policy')).toBe(FRAME_ANCESTORS);
    expect(page).toContain("new EventSource('/__dev/gallery/stream')");
    expect(page).toContain("page:'gallery'");
    expect(page).not.toContain("'navigate']");
    const script = /src="\.\/([^"]+\.js)"/.exec(page)?.[1];
    expect((await fetch(`${loop.base}/__dev/gallery/${script}`)).status).toBe(200);
    expect(await (await fetch(`${loop.base}/__dev/gallery/assets/note.txt`)).text()).toBe("the app's asset\n");
    // Its registration, listed as it said it.
    const reg = await fetch(`${loop.base}/__dev/report`, { method: 'POST', body: JSON.stringify({ nonce, kind: 'register', id: 'web-js-gallery-test', host: 'web-js', capabilities: ['pin', 'inspect', 'hold'], plan: null, page: 'gallery' }) });
    expect(reg.status).toBeLessThan(300);
    const { token } = await (await fetch(`${loop.base}/__forma/token`)).json();
    const hello = (await (await fetch(`${loop.base}/__forma`, { method: 'POST', headers: { 'x-exact-forma-token': token }, body: '{"method":"hello"}' })).json()).ok;
    expect(hello.builds.find((b) => b.id === 'web-js-gallery-test')?.page).toBe('gallery');
    // A kept design edit: a batch to the app, a build to the gallery; neither stream carries the other's.
    const app = frames(`${loop.base}/__dev/page`);
    await until(() => app.got.length > 0);
    const [appBefore, galleryBefore] = [app.got.length, gallery.got.length];
    const builtBefore = Math.max(...gallery.got.map((f) => f.reload ?? 0));
    const forma = readFileSync(join(dir, 'app.forma'), 'utf8');
    writeFileSync(join(dir, 'app.forma'), forma.replace('#ffffff', '#fefefe'));
    await until(() => app.got.slice(appBefore).some((f) => f.kind === 'forma-batch'));
    await until(() => gallery.got.slice(galleryBefore).some((f) => f.reload > builtBefore));
    expect(app.got.slice(appBefore).filter((f) => f.reload !== undefined || f.error)).toEqual([]);
    expect(gallery.got.slice(galleryBefore).filter((f) => f.kind === 'forma-batch' || f.error)).toEqual([]);
    app.close();
    // Its last page gone: its build stopped a while after (EXACT_DEV_GALLERY_IDLE_MS).
    gallery.close();
    await until(() => loop.said().includes('gallery: no page for a while'), 30_000);
  } finally {
    await loop.stop();
  }
  rmSync(resolve(dir, '..'), { recursive: true, force: true });
  rmSync(cache, { recursive: true, force: true });
}, 900_000);

/** The gallery's processes running now (its generator or its build), by pid. */
const galleryProcesses = (cache) => spawnSync('ps', ['-axo', 'pid=,command='], { encoding: 'utf8' }).stdout.split('\n')
  .filter((l) => l.includes(cache) && !l.includes('ps -axo')).map((l) => Number(l.trim().split(/\s+/)[0])).filter(Boolean);
const alive = (pid) => { try { process.kill(pid, 0); return true; } catch { return false; } };

test("a gallery that can't be made says so on its stream only, and is made once it can be; the loop interrupted mid-build leaves none of its processes", async () => {
  const id = `com.exact.dev-gallery-fail-${process.pid}`;
  const dir = copy(id);
  const cache = galleryCache({ dir, id }, ROOT);
  rmSync(cache, { recursive: true, force: true });
  // Its source's directory a file: `contract gallery` refuses.
  mkdirSync(cache, { recursive: true });
  writeFileSync(join(cache, 'source'), 'not a directory\n');
  const loop = await devLoop(dir);
  try {
    const app = frames(`${loop.base}/__dev/page`);
    const gallery = frames(`${loop.base}/__dev/gallery/stream`);
    await until(() => gallery.got.some((f) => f.error));
    expect(app.got.filter((f) => f.error)).toEqual([]);
    // A page asking again tries again (and fails again, said again, still on its stream only).
    const errors = gallery.got.filter((f) => f.error).length;
    await fetch(`${loop.base}/__dev/gallery/`);
    await until(() => gallery.got.filter((f) => f.error).length > errors);
    expect(app.got.filter((f) => f.error)).toEqual([]);
    // Fixed: the next page asks again, and it is made.
    rmSync(join(cache, 'source'), { force: true });
    await fetch(`${loop.base}/__dev/gallery/`);
    await until(() => gallery.got.some((f) => f.reload >= 1));
    gallery.close(); app.close();
    // A build under way (an edit makes it again), and the loop alone interrupted: its stop takes the gallery's with it.
    const again = frames(`${loop.base}/__dev/gallery/stream`);
    writeFileSync(join(dir, 'app.contract'), readFileSync(join(dir, 'app.contract'), 'utf8') + '\n');
    let seen = [];
    await until(() => (seen = galleryProcesses(cache)).length > 0, 120_000);
    loop.dev.kill('SIGINT');
    await new Promise((ok) => (loop.dev.exitCode !== null ? ok() : loop.dev.once('exit', ok)));
    again.close();
    expect(seen.filter(alive)).toEqual([]);
  } finally {
    await loop.stop();
  }
  rmSync(resolve(dir, '..'), { recursive: true, force: true });
  rmSync(cache, { recursive: true, force: true });
}, 900_000);

test("the gallery is told of a kept design only after the app's own publication: each call deferred past the batch", () => {
  const s = readFileSync(resolve(ROOT, 'host/web-js/dev.mjs'), 'utf8');
  const calls = s.split('\n').filter((l) => l.includes('gallery.designKept('));
  expect(calls.length).toBe(3);
  for (const l of calls) expect(l).toContain('setImmediate(() => gallery.designKept(');
  // A reload's: after the push.
  expect(s).toContain('push({ reload: seq, revision: logicRevision }); gallery.appChanged();');
});

/** A stand-in root and target: a `contract` that writes the gallery's source (or waits, holding a child of its
 * own), and a builder that waits holding one; each records its pid and its child's. */
function standIns(stall, parent = tmpdir()) {
  const dir = mkdtempSync(join(parent, 'exact-gallery-life-'));
  const pids = join(dir, 'pids'), buildEnv = join(dir, 'build-env.json'), home = join(dir, 'home');
  const env = { ...process.env, HOME: home, USERPROFILE: home, EXACT_TOOLS_ORIGIN: 'none', CARGO_TARGET_DIR: join(dir, 'target') };
  writeFileSync(pids, '');
  const hold = `sleep 30 & echo $! >> ${pids}; echo $$ >> ${pids};`;
  mkdirSync(join(dir, 'target/debug'), { recursive: true });
  writeFileSync(join(dir, 'src.rs'), '');
  writeFileSync(join(dir, 'target/debug/contract'), `#!/bin/sh\n${stall === 'generate' ? `${hold} sleep 30` : 'mkdir -p "$4"; echo "// Generated" > "$4/app.contract"; echo "{}" > "$4/app.json"; echo made'}\n`, { mode: 0o755 });
  writeFileSync(join(dir, 'target/debug/contract.d'), `${join(dir, 'target/debug/contract')}: ${join(dir, 'src.rs')}\n`);
  mkdirSync(join(dir, 'root/host/web-js'), { recursive: true });
  writeFileSync(join(dir, 'root/host/web-js/build.mjs'), `
    const { spawn } = require('node:child_process');
    const fs = require('node:fs');
    const { HOME, USERPROFILE, EXACT_TOOLS_ORIGIN } = process.env;
    fs.writeFileSync(${JSON.stringify(buildEnv)}, JSON.stringify({ HOME, USERPROFILE, EXACT_TOOLS_ORIGIN }));
    const c = spawn('sleep', ['30']);
    fs.appendFileSync(${JSON.stringify(pids)}, c.pid + '\\n' + process.pid + '\\n');
    setTimeout(() => {}, 30000);
  `);
  mkdirSync(join(dir, 'app'), { recursive: true });
  return { dir, env, buildEnv, pids: () => readFileSync(pids, 'utf8').split('\n').filter(Boolean).map(Number) };
}
const fakeRes = () => { const handlers = {}; return { writeHead() {}, write() {}, end() {}, on: (e, f) => { handlers[e] = f; }, close: () => handlers.close?.() }; };
const fakeReq = () => { const handlers = {}; return { method: 'GET', headers: {}, on: (e, f) => { handlers[e] = f; }, close: () => handlers.close?.() }; };

test('every process the gallery starts, a child of its generator or its builder included, stops with it: at stop, at either stage, and when its last page has been gone a while', async () => {
  const { galleryLoop } = await import('./dev-gallery.mjs');
  const parent = mkdtempSync(join(tmpdir(), 'exact-gallery-parent-'));
  mkdirSync(join(parent, 'node_modules'));
  // ('build, res only': its page gone with only the response's close, as under Bun on a handed-off socket.)
  try {
    for (const [stage, by] of [['generate', null], ['build', 'req'], ['build', 'res']]) {
      const s = standIns(stage, parent);
      const loop = galleryLoop({ app: { dir: join(s.dir, 'app'), id: `life-${stage}` }, root: join(s.dir, 'root'), appDist: join(s.dir, 'dist'), env: s.env, client: () => '', log: () => {}, idleMs: 200 });
      const stale = join(loop.cache, 'previous-run');
      mkdirSync(loop.cache, { recursive: true });
      writeFileSync(stale, '');
      rmSync(loop.cache, { recursive: true, force: true });
      try {
        const req = fakeReq(), res = fakeRes();
        await loop.handle(req, res, new URL('http://x/__dev/gallery/stream'));
        await until(() => s.pids().length >= 2, 30_000);
        const started = s.pids();
        expect(started.every(alive)).toBe(true);
        if (stage === 'generate') await loop.stop();
        else {
          expect(JSON.parse(readFileSync(s.buildEnv, 'utf8'))).toEqual({ HOME: join(s.dir, 'home'), USERPROFILE: join(s.dir, 'home'), EXACT_TOOLS_ORIGIN: 'none' });
          (by === 'req' ? req : res).close();
        }
        await until(() => !started.some(alive), 10_000);
        expect(started.filter(alive)).toEqual([]);
        expect(existsSync(stale)).toBe(false);
      } finally { await loop.stop(); rmSync(loop.cache, { recursive: true, force: true }); rmSync(s.dir, { recursive: true, force: true }); }
      expect(existsSync(loop.cache)).toBe(false);
    }
  } finally { rmSync(parent, { recursive: true, force: true }); }
}, 120_000);

test("an inspect table asked with no page of the gallery open is a process of the gallery's, stopped with it after the quiet period", async () => {
  const { galleryLoop } = await import('./dev-gallery.mjs');
  const s = standIns('generate');
  const pids = join(s.dir, 'table-pids');
  writeFileSync(pids, '');
  writeFileSync(join(s.dir, 'table'), `#!/bin/sh\nsleep 30 & echo $! >> ${pids}; echo $$ >> ${pids}; sleep 30\n`, { mode: 0o755 });
  // (A quiet period longer than a loaded machine takes from the table writing its pids to the check that they run:
  // at 200 ms the gallery could already have stopped them, 1 run in 5 on main.)
  const loop = galleryLoop({ app: { dir: join(s.dir, 'app'), id: 'life-table' }, root: join(s.dir, 'root'), appDist: join(s.dir, 'dist'), env: s.env, client: () => '', log: () => {}, idleMs: 3000, tableTool: () => ({ cmd: join(s.dir, 'table'), pre: [] }) });
  rmSync(loop.cache, { recursive: true, force: true });
  mkdirSync(join(loop.cache, 'dist'), { recursive: true });
  writeFileSync(join(loop.cache, 'dist', 'app.plan'), 'a plan');
  void loop.handle({ method: 'GET', headers: {}, on() {} }, fakeRes(), new URL('http://x/__dev/gallery/dev-inspect.json'));
  const started = () => readFileSync(pids, 'utf8').split('\n').filter(Boolean).map(Number);
  await until(() => started().length >= 2, 30_000);
  expect(started().every(alive)).toBe(true);
  await until(() => !started().some(alive), 10_000);
  rmSync(loop.cache, { recursive: true, force: true });
  rmSync(s.dir, { recursive: true, force: true });
}, 120_000);

test('each frame URL reloads its own document and recovers from its own errors', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-frame-pages-'));
  mkdirSync(join(dir, 'frames'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Canvas', app: { name: 'Canvas', id: 'test.frame-pages' } }));
  const samples = join(dir, 'node_modules/preview-data');
  mkdirSync(samples, { recursive: true });
  writeFileSync(join(samples, 'package.json'), JSON.stringify({ name: 'preview-data', version: '1.0.0', type: 'module', main: 'index.js', types: 'index.d.ts' }));
  writeFileSync(join(samples, 'index.d.ts'), 'export const prefix: string;');
  writeFileSync(join(samples, 'index.js'), 'export const prefix = "Sample ";');
  const app = `routes nav
  home "/"
component App
  view
    Card(title="Full app")
component Card
  props
    title: string
  resource label = caption(title) as shape string
  resource detail = detail() as shape string
  state count = 0
  action add
    count = count + 1
  view
    column
      text title
      text label
      text detail
      button press=add
        text count.toString()
`;
  const source = title => `use Card from "../app.contract"
component Frame
  view
    Card(title=${JSON.stringify(title)})
`;
  const put = (name, content) => writeFileSync(join(dir, 'frames', name + '.contract'), content);
  writeFileSync(join(dir, 'app.contract'), app);
  writeFileSync(join(dir, 'app.ts'), `import type { Answer, Sources } from './app.contract.d.ts';
export const grants = '';
const sources: Sources = { caption: ([title]) => 'Live ' + title, detail: () => 'Live detail' };
export const answer: Answer = (name, args, store, storage) => sources[name](args, store, storage);
`);
  put('card', source('Card A'));
  put('other', source('Card B'));
  let loop, browser;
  try {
    loop = await devLoop(dir);
    browser = await suiteBrowser('chromium');
    const context = await browser.newContext();
    const [a, b, full] = await Promise.all([context.newPage(), context.newPage(), context.newPage()]);
    const open = async (page, path, title) => {
      await page.goto(loop.base + path);
      await page.getByText(title, { exact: true }).waitFor({ timeout: 120_000 });
    };
    await Promise.all([open(a, '/__dev/frames/card/', 'Card A'), open(b, '/__dev/frames/other/', 'Card B'), open(full, '/', 'Full app')]);
    const count = b.getByRole('button');
    await count.click();
    await b.getByRole('button', { name: '1' }).waitFor();
    await b.evaluate(() => { window.frameDocument = 'kept'; });
    await full.evaluate(() => { window.appDocument = 'kept'; });
    put('card', source('Card A edited'));
    await a.getByText('Card A edited', { exact: true }).waitFor({ timeout: 60_000 });
    expect(await b.evaluate(() => window.frameDocument)).toBe('kept');
    expect(await full.evaluate(() => window.appDocument)).toBe('kept');
    expect(await count.textContent()).toBe('1');

    const fixture = join(dir, 'frames/card.fixture.mts');
    const sample = `import type { TypeScriptSources } from '../app.contract.d.ts';
import { prefix } from 'preview-data';
export default { caption: ([title]) => prefix + title } satisfies Partial<TypeScriptSources>;`;
    const inputs = join(dir, '.exact/typescript-inputs.json');
    const originalInputs = readFileSync(inputs, 'utf8');
    writeFileSync(fixture, sample);
    await a.getByText('Sample Card A edited', { exact: true }).waitFor({ timeout: 60_000 });
    await a.getByText('Live detail', { exact: true }).waitFor();
    expect(await b.getByText('Live Card B', { exact: true }).count()).toBe(1);
    writeFileSync(join(samples, 'index.js'), 'export const prefix = "Edited ";');
    await a.getByText('Edited Card A edited', { exact: true }).waitFor({ timeout: 60_000 });
    writeFileSync(fixture, sample.replace('prefix + title', '42'));
    const fixtureError = a.locator('pre').filter({ hasText: 'card.fixture.mts' });
    await fixtureError.waitFor({ timeout: 60_000 });
    expect(await fixtureError.textContent()).toContain('number');
    expect(await b.evaluate(() => window.frameDocument)).toBe('kept');
    expect(await full.evaluate(() => window.appDocument)).toBe('kept');
    rmSync(fixture);
    await a.getByText('Live Card A edited', { exact: true }).waitFor({ timeout: 60_000 });
    expect(readFileSync(inputs, 'utf8')).toBe(originalInputs);

    put('other', source('Card B').replace('"Card B"', 'unknownTitle'));
    const error = b.locator('pre').filter({ hasText: 'unknownTitle' });
    await error.waitFor({ timeout: 60_000 });
    expect(await error.textContent()).toContain('other.contract:');
    expect(await a.locator('pre').filter({ hasText: 'unknownTitle' }).count()).toBe(0);
    put('other', source('Card B'));
    await error.waitFor({ state: 'detached', timeout: 60_000 });
    expect(await b.evaluate(() => window.frameDocument)).toBe('kept');
    expect(await count.textContent()).toBe('1');

    writeFileSync(join(dir, 'app.contract'), app.replace('      text title', '      text "Shared edit"\n      text title'));
    await Promise.all([a, b, full].map(page => page.getByText('Shared edit', { exact: true }).waitFor({ timeout: 60_000 })));
    expect((await fetch(loop.base + '/__dev/frames/absent/')).status).toBe(404);
  } catch (error) {
    throw new Error(`${error.message}\n${loop?.said() ?? ''}`, { cause: error });
  } finally {
    await browser?.close();
    await loop?.stop();
    rmSync(dir, { recursive: true, force: true });
  }
}, 300_000);

test('two frame clients pin independently, pick their own source and take their own value batches', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-frame-channel-'));
  mkdirSync(join(dir, 'frames'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Canvas', app: { name: 'Canvas', id: 'test.frame-channel' } }));
  writeFileSync(join(dir, 'app.contract'), 'component App\n  view\n    text "App"\n');
  const theme = join(dir, 'app.forma'), source = 'forma canvas\n\naxes\n  tone = warm | cool\n\ntheme\n  ink = #123456\n';
  writeFileSync(theme, source);
  const frame = 'component Frame\n  view\n    text "Pick me" margin-top=50 color="ink" testId="pick"\n      :hover\n        opacity=0.5\n';
  for (const [name, prefix] of [['a', ''], ['b', '// Frame B\n'], ['late', '']]) writeFileSync(join(dir, 'frames', name + '.contract'), prefix + frame);
  const loop = await devLoop(dir);
  let browser;
  try {
    browser = await suiteBrowser('chromium');
    const context = await browser.newContext();
    const paths = ['/__dev/frames/a/', '/__dev/frames/b/', '/'];
    const messages = [[], []];
    const app = await context.newPage(), cdp = await context.newCDPSession(app), streams = new Map();
    await cdp.send('Network.enable');
    cdp.on('Network.requestWillBeSent', ({ requestId, request }) => {
      const i = paths.slice(0, 2).findIndex(path => request.url === loop.base + path + '__dev/stream');
      if (i >= 0) streams.set(requestId, i);
    });
    cdp.on('Network.eventSourceMessageReceived', ({ requestId, data }) => {
      if (streams.has(requestId)) messages[streams.get(requestId)].push(JSON.parse(data));
    });
    await app.goto(loop.base);
    await app.getByText('App', { exact: true }).waitFor();
    await app.evaluate(paths => {
      for (const [i, src] of paths.entries()) document.body.append(Object.assign(document.createElement('iframe'), { src, name: 'frame' + i }));
    }, paths.slice(0, 2));
    await until(() => app.frame('frame0') && app.frame('frame1'));
    const a = app.frame('frame0'), b = app.frame('frame1');
    await Promise.all([a, b].map(f => f.getByText('Pick me', { exact: true }).waitFor({ timeout: 120_000 })));
    const { token } = await (await fetch(loop.base + '/__forma/token')).json();
    const call = async (body, path = '/__forma') => (await (await fetch(loop.base + path, { method: 'POST',
      headers: { 'x-exact-forma-token': token }, body: JSON.stringify(body) })).json());
    let builds;
    await until(async () => { builds = (await call({ method: 'hello' })).ok.builds; return paths.every(p => builds.some(row => row.status?.location === p)); }, 10_000);
    const id = path => builds.find(row => row.status.location === path).id;
    const digest = builds.find(r => r.id === id(paths[1])).planDigest;
    expect(digest).toBeString();
    expect(builds.find(r => r.id === id(paths[0])).planDigest).toBe(digest);
    const size = page => page.evaluate(() => getComputedStyle(document.documentElement).fontSize);
    await call({ after: 0, wait: false, client: 'tool' }, '/__forma/events');
    const otherSizes = await Promise.all([size(b), size(app)]);
    expect((await call({ method: 'pin', client: 'tool', build: id(paths[0]), pin: { rootFontSize: 29 } })).ok).toBeTruthy();
    await until(async () => await size(a) === '29px');
    expect(await Promise.all([size(b), size(app)])).toEqual(otherSizes);
    expect((await call({ method: 'preview', client: 'tool', build: id(paths[1]), axis: 'tone', value: 'cool' })).ok).toBeTruthy();
    await until(() => messages[1].some(m => m.kind === 'pin' && m.pin?.mode?.tone === 'cool'));
    expect(messages[0].some(m => m.pin?.mode?.tone === 'cool')).toBe(false);
    for (const [i, page] of [a, b].entries()) {
      await page.evaluate(() => { window.originalFrame = true; });
      messages[i].length = 0;
    }
    writeFileSync(theme, source.replace('#123456', '#654321'));
    await until(() => loop.said().includes('the batch stands'), 30_000);
    for (const [i, page] of [a, b].entries()) {
      await until(async () => await page.getByTestId('pick').evaluate(el => getComputedStyle(el).color) === 'rgb(101, 67, 33)', 30_000);
      expect(messages[i].some(m => m.kind === 'forma-batch' && m.tokens.some(t => t.name === 'ink'))).toBe(true);
      expect(messages[i].some(m => m.reload)).toBe(false);
      expect(await page.evaluate(() => window.originalFrame)).toBe(true);
    }
    const files = (await call({ method: 'hello' })).ok.files;
    const scrub = await call({ method: 'scrub', client: 'tool', files, ops: [{ op: 'set-token', name: 'ink', value: '#123123' }] });
    expect(scrub.ok?.scrub).toBeString();
    const color = page => page.getByTestId('pick').evaluate(el => getComputedStyle(el).color);
    await until(async () => (await Promise.all([color(a), color(b)])).every(c => c === 'rgb(18, 49, 35)'));
    const late = await context.newPage();
    await late.goto(loop.base + '/__dev/frames/late/');
    await late.getByTestId('pick').waitFor();
    await until(async () => await color(late) === 'rgb(18, 49, 35)', 8_000);
    await late.close();
    expect((await call({ method: 'scrub-cancel', client: 'tool', scrub: scrub.ok.scrub })).ok?.ended).toBe('cancelled');
    await until(async () => (await Promise.all([color(a), color(b)])).every(c => c === 'rgb(101, 67, 33)'));
    const keep = await call({ method: 'scrub', client: 'tool', files, ops: [{ op: 'set-token', name: 'ink', value: '#123123' }] });
    const snapshot = async i => (await fetch(loop.base + paths[i] + '__dev/design')).json();
    const beforeRelease = await Promise.all([snapshot(0), snapshot(1)]);
    expect((await call({ method: 'scrub-release', client: 'tool', scrub: keep.ok.scrub })).ok?.scrub).toBe(keep.ok.scrub);
    await until(async () => readFileSync(theme, 'utf8').includes('#123123') && (await Promise.all([color(a), color(b)])).every(c => c === 'rgb(18, 49, 35)'));
    for (const [i, page] of [a, b].entries()) {
      await until(async () => (await snapshot(i)).scrub?.rev > beforeRelease[i].scrub.rev, 8_000);
      const { structure, design, tokens } = beforeRelease[i];
      expect(await snapshot(i)).toMatchObject({ structure, design, tokens });
      const cursor = await call({ after: 0, wait: false, client: 'tool' }, '/__forma/events');
      const build = id(paths[i]);
      expect((await call({ method: 'inspect', client: 'tool', build, on: true })).ok).toBeTruthy();
      await page.locator('#exact-inspect').waitFor();
      const table = await (await fetch(loop.base + paths[i] + '__dev/dev-inspect.json')).json();
      expect(table.plan).toBe(digest);
      await page.getByTestId('pick').scrollIntoViewIfNeeded();
      const box = await page.getByTestId('pick').boundingBox();
      await app.mouse.click(box.x + 3, box.y + box.height / 2);
      let picked;
      await until(async () => { const e = await call({ after: cursor.seq, epoch: cursor.epoch, wait: false, client: 'tool' }, '/__forma/events'); return picked = e.events?.find(e => e.event === 'selection' && e.build === build); }, 8_000);
      expect(picked.readOnly).toBeNull();
      expect(picked.source).toMatchObject({ file: 'frames/' + ['a','b'][i] + '.contract', line: 3 + i, component: 'Frame' });
      expect(picked.source.col).toBeGreaterThan(0);
      const at = [{ file: picked.source.file, line: picked.source.line, col: picked.source.col }];
      const outlined = await call({ method: 'outline', client: 'tool', at });
      expect(outlined.ok?.builds).toContain(build);
      const site = await page.getByTestId('pick').getAttribute('data-site');
      expect((await call({ method: 'hold', client: 'tool', build, site: Number(site), state: 'hover', plan: digest })).ok?.held).toBe(true);
      await until(async () => (await call({ method: 'hello' })).ok.builds.find(b => b.id === build)?.held?.state === 'hover', 5_000);
      expect((await call({ method: 'hold', client: 'tool', build, state: null })).ok?.held).toBe(true);
    }
    const survivors = [id(paths[0]), id('/')].sort();
    await app.locator('iframe[name="frame1"]').evaluate(node => node.remove());
    await until(async () => (await call({ method: 'hello' })).ok.builds.length === 2);
    expect((await call({ method: 'hello' })).ok.builds.map(row => row.id).sort()).toEqual(survivors);
  } finally {
    await browser?.close();
    await loop.stop();
    rmSync(dir, { recursive: true, force: true });
  }
}, 300_000);

test('a frame rebuilds for a sound baked into its plan, both retry an unknown input, and public assets stay current without a frame build', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-frame-inputs-'));
  const entry = 'component Frame\n  view\n    text "Preview"\n';
  mkdirSync(join(dir, 'frames'));
  mkdirSync(join(dir, 'assets'));
  writeFileSync(join(dir, 'app.contract'), 'component App\n  view\n    text "App"\n');
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Inputs', app: { name: 'Inputs', id: 'test.frame-inputs' } }));
  writeFileSync(join(dir, 'frames/a.contract'), 'sound "assets/click.wav"\n\n' + entry);
  writeFileSync(join(dir, 'frames/b.contract'), entry);
  const sound = join(dir, 'assets/click.wav'), note = join(dir, 'assets/card.svg');
  const card = width => `<svg xmlns="http://www.w3.org/2000/svg" width="${width}" height="12"/>`;
  cpSync(join(ROOT, 'apps/metronome/assets/click-high.wav'), sound);
  writeFileSync(note, card(17));
  let loop, browser;
  const streams = [];
  try {
    loop = await devLoop(dir);
    const pages = ['a', 'b'].map(name => loop.base + '/__dev/frames/' + name + '/');
    streams.push(...pages.map(url => frames(url + '__dev/stream')));
    await until(() => streams.every(s => s.got.some(m => m.reload === 1)));
    const plans = () => Promise.all(pages.map(async url => Buffer.from(await (await fetch(url + 'app.plan')).arrayBuffer())));
    const before = await plans();
    browser = await suiteBrowser('chromium');
    const page = await (await browser.newContext()).newPage();
    await page.addInitScript(() => addEventListener('DOMContentLoaded', () => {
      const image = document.body.appendChild(new Image());
      image.id = 'dynamic-asset';
      image.src = ['/assets/card', 'svg'].join('.');
    }));
    await page.goto(pages[0]);
    const displayed = width => page.waitForFunction(w => document.getElementById('dynamic-asset')?.naturalWidth === w, width);
    await displayed(17);
    const mark = () => loop.said().length;
    const started = at => [...loop.said().slice(at).matchAll(/^([ab]): building$/gm)].map(m => m[1]).sort();
    let at = mark();
    writeFileSync(note, card(23));
    await displayed(23);
    expect(await Promise.all(pages.map(async url => (await fetch(url + 'assets/card.svg')).text()))).toEqual([card(23), card(23)]);
    await until(() => loop.said().slice(at).includes('edit → plan ready'));
    expect(started(at)).toEqual([]);
    await until(() => streams[0].got.some(m => m.reload === 2));

    at = mark();
    const oldSound = readFileSync(sound);
    cpSync(join(ROOT, 'apps/metronome/assets/click-low.wav'), sound);
    const servedSounds = () => Promise.all(pages.map(async url => Buffer.from(await (await fetch(url + 'assets/click.wav')).arrayBuffer())));
    expect(await servedSounds()).toEqual([oldSound, readFileSync(sound)]);
    await until(async () => !(await plans())[0].equals(before[0]));
    expect(await servedSounds()).toEqual([readFileSync(sound), readFileSync(sound)]);
    await until(() => loop.said().slice(at).includes('edit → plan ready'));
    expect(started(at)).toEqual(['a']);
    expect((await plans())[1]).toEqual(before[1]);

    at = mark();
    writeFileSync(join(dir, 'new-input.dat'), 'new');
    await until(() => started(at).length >= 2);
    expect(started(at)).toEqual(['a', 'b']);
  } catch (error) {
    throw new Error(`${error.message}\n${loop?.said() ?? ''}`, { cause: error });
  } finally {
    await browser?.close();
    for (const stream of streams) stream.close();
    await loop?.stop();
    rmSync(dir, { recursive: true, force: true });
  }
}, 120_000);

test('fifty frames share build slots, favor open streams and read the latest waiting edit once', async () => {
  const queue = frameBuildQueue(3), builds = [], finish = [], values = new Map(), held = new Set();
  let active = 0, peak = 0;
  const jobs = Array.from({ length: 50 }, (_, id) => async () => {
    peak = Math.max(peak, ++active);
    builds.push([id, values.get(id)]);
    await new Promise(resolve => finish.push(resolve));
    --active;
  });
  const ask = id => queue.add(jobs[id], () => held.has(id));
  jobs.forEach((_, id) => ask(id));
  await until(() => builds.length === 3);
  held.add(49);
  for (let value = 1; value <= 5; ++value) { values.set(49, value); ask(49); }
  queue.remove(jobs[9]);
  finish.shift()();
  await until(() => builds.length === 4);
  expect(builds[3]).toEqual([49, 5]);
  while (active) { finish.splice(0).forEach(done => done()); await Bun.sleep(0); }
  expect(peak).toBe(3);
  expect(builds.map(([id]) => id).sort((a, b) => a - b)).toEqual(jobs.map((_, id) => id).filter(id => id !== 9));
});

test('a streamless request advances while a connected frame keeps asking to build', async () => {
  const queue = frameBuildQueue(1), calls = [];
  let release;
  queue.add(async () => { await new Promise(done => { release = done; }); }, () => true);
  queue.add(async () => { calls.push('waiting'); }, () => false);
  const edit = async () => { calls.push('editing'); if (calls.length < 5) queue.add(edit, () => true); };
  queue.add(edit, () => true);
  release();
  await until(() => calls.includes('waiting'));
  expect(calls.slice(0, 2)).toEqual(['editing', 'waiting']);
});

async function listedBuilds(base) {
  const { token } = await (await fetch(base + '/__forma/token')).json();
  return async () => (await (await fetch(base + '/__forma', {
    method: 'POST', headers: { 'x-exact-forma-token': token }, body: '{"method":"hello"}',
  })).json()).ok.builds;
}

for (const gallery of [true, false]) test(`${gallery ? 'real gallery' : 'numbered wasm'} pages report their rendered build`, async () => {
  const engines = await drivable(['chromium', 'webkit']);
  if (!engines.length) return;
  const dir = gallery ? copy(`test.render-gallery-${process.pid}`) : 'theme-fixture';
  const loop = gallery ? await devLoop(dir) : await devLoop(dir, {
    EXACT_TEST_TIMING: '1', EXACT_DEV_LISTEN_DELAY_MS: 'generation',
  }, ['--wasm']);
  try {
    const list = await listedBuilds(loop.base), path = gallery ? '/__dev/gallery/' : '/';
    for (const engine of engines) {
      const browser = await suiteBrowser(engine);
      let connect;
      try {
        const page = await browser.newPage();
        if (!gallery) {
          const waiting = new Promise(resolve => { connect = resolve; });
          await page.route(/\/__dev$/, async route => { await waiting; await route.continue(); });
        }
        const registration = page.waitForRequest(req => req.url().endsWith('/__dev/report') && req.postDataJSON()?.kind === 'register', { timeout: 90_000 });
        await page.goto(loop.base + path);
        if (connect) {
          await page.waitForFunction(() => document.getElementById('exact-root')?.dataset.bootMs !== undefined);
          await page.evaluate(() => new Promise(resolve => requestAnimationFrame(() => requestAnimationFrame(resolve))));
          connect();
        }
        const { id } = (await registration).postDataJSON();
        await until(async () => (await list()).some(row => row.id === id && row.status?.location === path && row.status.rendered === 1 && row.planDigest && (gallery ? row.page === 'gallery' : row.host === 'web' && row.plan)), 90_000);
      } finally { connect?.(); await browser.close(); }
    }
  } finally {
    await loop.stop();
    if (gallery) rmSync(dirname(dir), { recursive: true, force: true });
  }
}, 900_000);

for (const engine of await drivable(['chromium', 'webkit'])) test(`${engine}: the app and two frames report only builds rendered in their own viewports`, async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-frame-render-'));
  mkdirSync(join(dir, 'frames'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Render', app: { name: 'Render', id: 'test.frame-render' } }));
  writeFileSync(join(dir, 'app.forma'), 'forma canvas\n\ntheme\n  ink = #123456\n');
  const entry = text => `component App\n  view\n    text "${text}" color="ink" testId="words"\n`;
  for (const name of ['app', 'frames/a', 'frames/b']) writeFileSync(join(dir, name + '.contract'), entry(name));
  const parent = createServer((_, res) => { res.writeHead(200, { 'content-type': 'text/html' }); res.end('<!doctype html><body></body>'); });
  await new Promise(ok => parent.listen(0, '127.0.0.1', ok));
  const port = parent.address().port;
  let browser, loop;
  try {
    loop = await devLoop(dir, { EXACT_TOOL_PORT: String(port) });
    const list = await listedBuilds(loop.base);
    const paths = ['/', '/__dev/frames/a/', '/__dev/frames/b/'];
    const rows = async () => { const all = await list(); return paths.map(path => all.find(row => row.status?.location === path)); };
    browser = await suiteBrowser(engine);
    const page = await browser.newPage();
    await page.goto(loop.base);
    await until(async () => (await rows())[0]?.status.rendered === 1, 30_000);
    const canvas = await browser.newPage();
    const reports = [];
    canvas.on('request', r => {
      if (!r.url().endsWith('/__dev/report')) return;
      const body = JSON.parse(r.postData());
      if (body.rendered !== undefined) reports.push([r.frame().name(), body.rendered]);
    });
    await canvas.goto(`http://127.0.0.1:${port}/`);
    // scale(0) keeps the layout viewport. A document loaded under display:none has none.
    await canvas.evaluate(([a, b]) => {
      for (const [name, src, style] of [['a', a, 'transform:scale(0)'], ['b', b, 'display:none']]) {
        const frame = Object.assign(document.createElement('iframe'), { src, name });
        frame.style.cssText = style;
        document.body.append(frame);
      }
    }, paths.slice(1).map(path => loop.base + path));
    await until(async () => { const [, a, b] = await rows(); return a?.status.rendered === 1 && b; }, 60_000);
    const viewport = frame => canvas.frame(frame).evaluate(() => [innerWidth, innerHeight, document.visibilityState]);
    expect(await viewport('a')).toEqual([300, 150, 'visible']);
    expect(await viewport('b')).toEqual([0, 0, 'visible']);
    expect((await rows()).map(row => row.status.rendered)).toEqual([1, 1, undefined]);
    const old = (await rows())[2];
    writeFileSync(join(dir, 'frames/b.contract'), entry('rebuilt'));
    await until(async () => { const b = (await rows())[2]; return b && b.planDigest !== old.planDigest; }, 60_000);
    expect((await rows()).map(row => row.status.rendered)).toEqual([1, 1, undefined]);

    const frameB = canvas.locator('iframe[name=b]');
    await frameB.evaluate(el => el.style.display = '');
    await until(async () => (await rows())[2]?.status.rendered === 2, 15_000);
    expect(reports).not.toContainEqual(['b', 1]);
    expect(new Set((await rows()).map(row => row.id)).size).toBe(3);

    const frame = canvas.frame('b'), color = () => getComputedStyle(document.querySelector('[data-testid=words]')).color;
    const before = await frame.evaluate(color);
    writeFileSync(join(dir, 'app.forma'), 'forma canvas\n\ntheme\n  ink = #654321\n');
    await until(async () => await frame.evaluate(color) !== before, 30_000).catch(async error => {
      throw new Error(`The frame's ink stayed ${await frame.evaluate(color)} after the edit.\n${loop.said()}`, { cause: error });
    });
    writeFileSync(join(dir, 'frames/b.contract'), 'component App\n  view\n    text unknownValue\n');
    await canvas.frame('b').locator('pre').filter({ hasText: 'unknownValue' }).waitFor();
    expect((await rows()).map(row => row.status.rendered)).toEqual([1, 1, 2]);

    // A report already in flight can arrive after the embedder hides the frame.
    await canvas.route('**/__dev/report', async route => {
      if (route.request().frame().name() === 'b' && route.request().postDataJSON()?.rendered === 3)
        await frameB.evaluate(el => el.style.display = 'none');
      await route.continue();
    });
    writeFileSync(join(dir, 'frames/b.contract'), entry('recovered'));
    await until(async () => (await rows())[2]?.status.rendered === 3, 60_000);
    expect(await frameB.evaluate(el => el.style.display)).toBe('none');
  } finally {
    await browser?.close(); await loop?.stop(); parent.close(); rmSync(dir, { recursive: true, force: true });
  }
}, 180_000);

for (const engine of await drivable(['chromium', 'webkit'])) test(`${engine}: replacing a pending render and detaching its document cancel the old report`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage();
    const receipts = await page.evaluate(async source => {
      const seen = [];
      const child = () => {
        const frame = document.body.appendChild(document.createElement('iframe'));
        frame.contentDocument.documentElement.style.height = '100%';
        const watch = frame.contentWindow.eval('(' + source + ')');
        return { frame, watch };
      };
      const old = child();
      old.watch(() => seen.push('detached'));
      old.frame.remove();
      const fresh = child();
      const cancel = fresh.watch(() => seen.push('superseded'));
      cancel();
      await new Promise(resolve => fresh.watch(() => { seen.push('fresh'); resolve(); }));
      return seen;
    }, whenRendered.toString());
    expect(receipts).toEqual(['fresh']);
  } finally { await browser.close(); }
}, 30_000);

for (const engine of await drivable(['chromium', 'webkit'])) test(`${engine}: a viewport collapsed after commit waits for both dimensions`, async () => {
  const browser = await suiteBrowser(engine);
  try {
    const page = await browser.newPage();
    const reports = await page.evaluate(async source => {
      const watch = eval('(' + source + ')'), reports = [];
      for (const dimension of ['width', 'height']) {
        const frame = document.body.appendChild(document.createElement('iframe'));
        const child = frame.contentWindow;
        const rendered = new Promise(resolve => child.eval('(' + source + ')')(() => {
          reports.push([child.innerWidth, child.innerHeight]);
          resolve();
        }));
        frame.style[dimension] = '0px';
        await new Promise(resolve => watch(resolve));
        frame.style[dimension] = '';
        await rendered;
        frame.remove();
      }
      return reports;
    }, whenRendered.toString());
    expect(reports).toEqual([[300, 150], [300, 150]]);
  } finally { await browser.close(); }
}, 30_000);

for (const engine of await drivable(['chromium', 'webkit'])) test(`${engine}: cross-origin frames with visibility:hidden, a placeholder or a zero-height root report`, async () => {
  const documentAt = html => Bun.serve({ hostname: '127.0.0.1', port: 0, fetch: () => new Response(html, { headers: { 'content-type': 'text/html' } }) });
  const frame = documentAt(`<!doctype html><html><style>html,body{margin:0;height:0}aside{position:fixed;width:80px;height:40px}</style><body><aside>words</aside><script>
    (${whenRendered.toString()})(() => parent.postMessage(name + ':rendered', '*'));
    </script></body></html>`);
  const tool = documentAt(`<!doctype html><script>window.receipts=[];onmessage=e=>receipts.push(e.data)</script>
    <iframe name="hidden" style="visibility:hidden" src="${frame.url}"></iframe>
    <iframe name="covered" src="${frame.url}"></iframe><div style="position:fixed;inset:0;background:white"></div>`);
  let browser;
  try {
    browser = await suiteBrowser(engine);
    const page = await browser.newPage();
    await page.goto(tool.url.href);
    await page.waitForFunction(() => receipts.includes('hidden:rendered') && receipts.includes('covered:rendered'));
    expect(await page.frame('covered').evaluate(() => [
      document.documentElement.getBoundingClientRect().height, document.querySelector('aside').getBoundingClientRect().height,
    ])).toEqual([0, 40]);
    const hidden = page.frame('hidden');
    await page.locator('iframe[name=hidden]').evaluate(el => el.style.transform = 'scale(0)');
    await hidden.evaluate(source => {
      document.documentElement.style.display = 'none';
      eval('(' + source + ')')(() => parent.postMessage('boxless:rendered', '*'));
    }, whenRendered.toString());
    await page.waitForFunction(() => receipts.includes('boxless:rendered'));
  } finally { await browser?.close(); tool.stop(true); frame.stop(true); }
}, 45_000);
