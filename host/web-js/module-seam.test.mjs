import { test, expect } from 'bun:test';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { extname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { open } from '../../scripts/agent.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';

// The module seam's general pieces on the JS target (LLP 1133.002 D14; LLP
// 1133.008 D8 item 9; LLP 1133.009 D10 items 3, 7, 10-12), through an
// interface outside the tree, as LLP 1133.001 D8 asks: `@fixture/sensor`,
// which is its own substitute, with a web implementation that records what
// it is told, installed into an app `exact new` made outside the repo and
// driven by the agent.

const root = resolve(import.meta.dir, '../..');

function moduleServer() {
  return Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/') return new Response('<div id="exact-root"></div>', { headers: { 'content-type': 'text/html' } });
    if (path.endsWith('/names.js')) return new Response('export const sourceTypes = {};', { headers: { 'content-type': 'text/javascript' } });
    if (path.endsWith('/admission-data.js')) return new Response("import { createGrantSet } from './admission.js'; export const tsGrantSet = createGrantSet(null);", { headers: { 'content-type': 'text/javascript' } });
    if (path === '/fixture-source.js') return new Response(`export const appId = 'test.owned';
      export const answer = () => __exact_module({ extension: 'sensor', owner: '@fixture/sensor/own', source: 'reading', args: { track: 'a' } });`, { headers: { 'content-type': 'text/javascript' } });
    if (path.endsWith('/ts-data.js')) return new Response(readFileSync(resolve(root, 'host/web-js/ts-data.js'), 'utf8')
      .replace('__APP_TS__', '/fixture-source.js').replace('__AUTH_IMPORT__', '').replace('__AUTH_INSTALL__', '').replace('__FETCH_MODULE__', './ts-fetch.js'), { headers: { 'content-type': 'text/javascript' } });
    const local = resolve(root, '.' + path);
    const file = existsSync(local) ? local : resolve(root, 'host/web', path.split('/').at(-1));
    return existsSync(file) ? new Response(hostFixtureFile(file), { headers: { 'content-type': 'text/javascript' } }) : new Response('Not found', { status: 404 });
  } });
}

test.each([false, true])('a view validates and delivers its agent vocabulary (substitute source: %p)', async withSource => {
  const server = moduleServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async withSource => {
      const rt = await import('/host/web-js/rt.js');
      const { modules } = await import('/host/web-js/modules.js');
      rt.clock.agent = true;
      const seen = [];
      const loader = async () => ({
        implementation: () => ({ preferred(values) { seen.push(values); } }),
        roster: { 'words-panel': { view: class {} } },
      });
      const words = { heard: 'string', levels: 'number[]', enabled: 'bool', status: 'string' };
      const rows = withSource ? { status: ['words.status', false, { default: { available: false }, agent: words }, { available: 'b' }] } : {};
      modules(rows, { words: loader }, { 'words-panel': ['words', false, [], words] });
      const prefer = extension => exact.extensions.prefer({ extension });
      const before = prefer({ 'words.heard': 'hello' });
      if (before.error) throw new Error(before.error);
      await exact.packageViews.make('words-panel', {});
      const initial = seen.at(-1);
      const errors = [
        prefer({ 'words.heard': 7 }), prefer({ 'words.levels': [1, 'two'] }),
        prefer({ 'words.enabled': 'true' }), prefer({ 'words.unknown': 'hello' }),
        prefer({ 'unknown.heard': 'hello' }),
      ];
      await prefer({ 'words.heard': 'again', 'words.levels': [1, 2], 'words.enabled': true });
      const updated = seen.at(-1);
      await prefer({ 'words.heard': 'clear' });
      const cleared = seen.at(-1), count = seen.length;
      const source = withSource ? await prefer({ 'words.status': { available: true } }) : null;
      const sourceError = withSource ? prefer({ 'words.status': 'a string' }) : null;
      return { before, initial, updated, errors, cleared, source, sourceError, sourceTold: count !== seen.length, state: exact.extensions.state() };
    }, withSource);
    expect(result.before.error).toBeUndefined();
    expect(result.initial).toEqual({ heard: 'hello' });
    expect(result.updated).toEqual({ heard: 'again', levels: [1, 2], enabled: true });
    expect(result.errors.map(e => e.error)).toEqual([
      'prefer extension: `words.heard` is a string',
      'prefer extension: `words.levels` is a number[]',
      'prefer extension: `words.enabled` is a bool',
      `prefer extension: no source \`words.unknown\` in this app; its module sources are ${withSource ? 'words.status' : 'none'}`,
      `prefer extension: no source \`unknown.heard\` in this app; its module sources are ${withSource ? 'words.status' : 'none'}`,
    ]);
    expect(result.cleared).toEqual({ levels: [1, 2], enabled: true });
    expect(result.sourceTold).toBe(false);
    if (withSource) {
      expect(result.source.error).toBeUndefined();
      expect(result.sourceError.error).toContain('type its JSON');
    }
    expect(result.state.words.preferred).toEqual({ ...result.cleared, ...(withSource ? { status: { available: true } } : {}) });
  } finally { await browser?.close(); server.stop(true); }
});

test.each([false, true])('implementation live readers refresh and share their provider lifetime (data code: %p)', async wrapper => {
  const server = moduleServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    await page.evaluate(async wrapper => {
      const rt = await import('/host/web-js/rt.js');
      const { install } = await import('/host/web-js/ts-data.js');
      const { modules } = await import('/host/web-js/modules.js');
      rt.clock.agent = true;
      install(rt.data);
      let value = 1, changed;
      const releases = [], own = { implementation: true, owner: '@fixture/sensor/own', implementationTargets: ['web'] };
      modules({
        ...(!wrapper ? { common: ['sensor.reading', false, { implementation: true }, 'n', 'never', { track: 's' }, true] } : {}),
        owned: ['sensor.reading', false, own, 'n', 'never', { track: 's' }, true],
      }, { sensor: async () => ({ implementation: context => {
        changed = context.changed;
        return { reading: () => value, release: name => releases.push([name]), releaseArgs: (name, args) => releases.push([name, args]) };
      } }) });
      let owned, common, ownScope, commonScope;
      const track = rt.sig('a');
      rt.mount(() => {
        commonScope = rt.scope(() => { common = rt.res('shared', wrapper ? 'owned' : 'common', () => [['a']], undefined, undefined, 'n', 0); rt.effect(() => common()); });
        ownScope = rt.scope(() => { owned = rt.res('own', wrapper ? 'wrapped' : 'owned', () => wrapper ? [] : [[track()]], undefined, undefined, 'n', 0); rt.effect(() => owned()); });
      });
      globalThis.fixture = { rt, owned, common, releases, ownScope, commonScope, track, change: () => { value++; changed('sensor.reading'); } };
    }, wrapper);
    await page.waitForFunction(() => fixture.owned() === 1 && fixture.common() === 1);
    await page.evaluate(() => fixture.change());
    await page.waitForFunction(() => fixture.owned() === 2 && fixture.common() === 2, null, { timeout: 3000 });
    if (!wrapper) {
      await page.evaluate(() => fixture.rt.commit(() => fixture.rt.W(fixture.track, 'b')));
      await page.waitForFunction(() => fixture.owned.r.settled?.[0]?.[0] === 'b' && !fixture.owned.r.ticket);
      expect(await page.evaluate(() => fixture.releases)).toEqual([]);
    }
    await page.evaluate(() => fixture.rt.commit(() => fixture.rt.end(fixture.ownScope)));
    await page.evaluate(() => Promise.resolve());
    expect(await page.evaluate(() => fixture.releases)).toEqual(wrapper ? [] : [['reading', { track: 'b' }]]);
    await page.evaluate(() => fixture.change());
    await page.waitForFunction(() => fixture.common() === 3);
    await page.evaluate(() => fixture.rt.commit(() => fixture.rt.end(fixture.commonScope)));
    await page.evaluate(() => Promise.resolve());
    expect(await page.evaluate(() => fixture.releases.filter(r => r.length === 1))).toEqual([['reading']]);
  } finally { await browser.close(); server.stop(true); }
});

test.each([false, true])('implementation names keep their defaults and refuse off-target sends (agent: %p)', async agent => {
  const server = moduleServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async agent => {
      const rt = await import('/host/web-js/rt.js');
      const { modules } = await import('/host/web-js/modules.js');
      rt.clock.agent = agent;
      let calls = 0;
      const own = { implementation: true, owner: '@fixture/sensor/apple', implementationTargets: ['ios'] };
      modules({
        reading: ['sensor.reading', false, { ...own, default: { value: 0 }, targets: { web: { value: 7 } } }, { value: 'n' }],
        sample: ['sensor.sample', true, own, { value: 'n' }],
        reset: ['sensor.reset', true, { ...own, effect: true }, 'u'],
      }, { sensor: async () => ({ implementation: () => ({
        reading() { calls++; return { value: 9 }; },
        sample() { calls++; return { value: 9 }; },
        reset() { calls++; },
      }) }) });
      const call = (name, send) => exact.extensions.call({ extension: 'sensor', owner: '@fixture/sensor/apple', [send ? 'send' : 'source']: name, args: {} });
      return { reading: await call('reading', false), sample: await call('sample', true), reset: await call('reset', true), calls };
    }, agent);
    expect(result).toEqual({ reading: { value: 7 }, sample: { error: 'NotSupportedError' }, reset: { error: 'NotSupportedError' }, calls: 0 });
  } finally { await browser?.close(); server.stop(true); }
});

test.each([false, true])('implementation owners separate calls and explicit overrides use substitutes (agent: %p)', async agent => {
  const server = moduleServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async agent => {
      const rt = await import('/host/web-js/rt.js');
      const { modules } = await import('/host/web-js/modules.js');
      rt.clock.agent = agent;
      let instances = 0, calls = 0;
      const own = { implementation: true, implementationTargets: ['web'] };
      modules({
        shared: ['sensor.reading', false, { default: 3 }, 'n'],
        first: ['sensor.reading', false, { ...own, owner: '@fixture/first', default: 1 }, 'n'],
        second: ['sensor.reading', false, { ...own, owner: '@fixture/second', implementationOverrides: ['web'], default: 2 }, 'n'],
        send: ['sensor.sample', true, { ...own, owner: '@fixture/second', implementationOverrides: ['web'] }, 'n'],
      }, { sensor: async () => ({ implementation: () => {
        instances++;
        return { reading() { calls++; return 9; }, sample() { calls++; return 9; } };
      } }) });
      const call = (owner, name = 'reading', send = false) => exact.extensions.call({ extension: 'sensor', ...(owner ? { owner } : {}), [send ? 'send' : 'source']: name });
      return { shared: await call(), first: await call('@fixture/first'), second: await call('@fixture/second'), send: await call('@fixture/second', 'sample', true), instances, calls, state: exact.extensions.state() };
    }, agent);
    expect(result.shared).toBe(agent ? 3 : 9);
    expect(result.first).toBe(9);
    expect(result.second).toBe(2);
    expect(result.send).toEqual({ error: 'NotSupportedError' });
    expect(result.instances).toBe(1);
    expect(result.calls).toBe(agent ? 1 : 2);
    expect(result.state.sensor.names['reading#@fixture/second']).toEqual({ owner: '@fixture/second', route: 'substitute' });
  } finally { await browser?.close(); server.stop(true); }
});

test('a data-code reader asleep under its when stays listed and lets its provider go', async () => {
  const server = moduleServer(), browser = await suiteBrowser(), page = await browser.newPage();
  try {
    await page.goto(server.url.href);
    await page.evaluate(async () => {
      const rt = await import('/host/web-js/rt.js');
      const { install } = await import('/host/web-js/ts-data.js');
      const { modules } = await import('/host/web-js/modules.js');
      rt.clock.agent = true;
      install(rt.data);
      const releases = [], open = rt.sig(true), own = { implementation: true, owner: '@fixture/sensor/own', implementationTargets: ['web'] };
      modules({ owned: ['sensor.reading', false, own, 'n', 'never', { track: 's' }, true] },
        { sensor: async () => ({ implementation: () => ({ reading: () => 1, release: name => releases.push(name) }) }) });
      const asked = () => [];
      asked.gate = () => open();
      let read;
      rt.mount(() => { read = rt.res('own', 'wrapped', asked, undefined, undefined, 'n', 0); rt.effect(() => read()); });
      globalThis.fixture = { rt, read, open, releases };
    });
    await page.waitForFunction(() => fixture.read() === 1);
    await page.evaluate(() => fixture.rt.commit(() => fixture.rt.W(fixture.open, false)));
    await page.evaluate(() => Promise.resolve());
    expect(await page.evaluate(() => ({ readers: exact.extensions.state().sensor.readers, releases: fixture.releases })))
      .toEqual({ readers: { 'reading#@fixture/sensor/own': ['own'] }, releases: ['reading'] });
  } finally { await browser.close(); server.stop(true); }
});

test('an implementation-owned ticket names its declared request', async () => {
  const server = moduleServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const holds = await page.evaluate(async () => {
      const rt = await import('/host/web-js/rt.js');
      const { modules } = await import('/host/web-js/modules.js');
      rt.clock.agent = true;
      const own = { implementation: true, owner: '@fixture/sensor/apple', implementationTargets: ['web'] };
      modules({ sample: ['sensor.sample', true, { ...own, choose: true }, 'n'] }, { sensor: async () => ({ implementation: () => ({ sample: () => 1 }) }) });
      exact.extensions.call({ extension: 'sensor', owner: '@fixture/sensor/apple', send: 'sample', args: { count: 2 } });
      await new Promise(r => setTimeout(r, 0));
      return exact.extensions.holds();
    });
    expect(holds.map(h => [h.name, h.device.args.request])).toEqual([['sensor.sample#@fixture/sensor/apple', 'sample']]);
  } finally { await browser?.close(); server.stop(true); }
});

const run = (cmd, args, cwd) => {
  const r = spawnSync(cmd, args, { cwd, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  if (r.status !== 0) throw new Error(`${cmd} ${args.join(' ')}: ${r.stdout}\n${r.stderr}`);
};

const INTERFACE = {
  kind: 'interface', name: '@fixture/sensor', version: '1.0.0',
  grants: ['device.microphone'], substitute: 'implementation',
  permission: { name: 'microphone', status: 'sensorStatus', field: 'state', denied: 'NotAllowedError' },
  shapes: {
    SensorStatus: { available: 'bool', state: 'enum<granted|prompt|denied|unknown>' },
    Reading: { id: 'string', level: 'number', muted: 'bool' },
  },
  names: [
    { source: 'sensorStatus', live: true, answers: 'SensorStatus', substitute: { default: { available: true } } },
    { send: 'open', request: true, origin: 'app', foreground: true, options: { slow: 'option<bool>' }, answers: 'Reading', handles: { id: 'track' } },
    { source: 'level', live: true, poll: { option: 'interval', default: 100 }, options: { track: 'string', interval: 'option<number>' }, answers: 'Reading' },
  ],
};

// Each call is recorded on the page (`__sensor`); `open(slow=true)` answers after 5 s of wall time, so a second
// open overtakes it (300 ms was sometimes over before a loaded machine's next tap, and nothing was overtaken; a
// settle waits for the answer either way). A level reads the session's clock.
const IMPLEMENTATION = `export function implementation({ agent, now }) {
  const seen = globalThis.__sensor = { agent, opened: [], levels: [], releases: [], disposed: [], preferred: [] };
  let n = 0;
  return {
    open(args) {
      const id = 't' + ++n;
      seen.opened.push(id);
      const reading = { id, level: 0, muted: false };
      return args.slow ? new Promise(done => setTimeout(() => done(reading), 5000)) : reading;
    },
    level(args, extra) { seen.levels.push([args, now(), extra?.preferred ?? null]); return { id: args.track, level: now(), muted: false }; },
    releaseArgs(name, args) { seen.releases.push([name, args]); },
    release(name) { seen.releases.push([name]); },
    dispose(ids) { seen.disposed.push(...ids); },
    preferred(values) { seen.preferred.push(values); },
  };
}
`;

const CONTRACT = `use open, level, sensorStatus, SensorStatus, Reading from "@fixture/sensor"

component Fixture
  state track = "a"
  resource status = sensorStatus() as shape SensorStatus else empty()
  resource meter = level(track=track, interval=100) as shape Reading
  mutation opened as shape Reading
  action start
    send opened = open()
  action startSlow
    send opened = open(slow=true)
  action moveB
    track = "b"
  view
    column gap=8 padding=16
      text status.state testId="status"
      text \`\${meter.id} \${meter.level} \${meter.muted}\` testId="meter"
      match opened
        case some(r)
          text r.id testId="opened"
        case none
          text "none" testId="opened"
      button press=start testId="start"
        text "Open"
      button press=startSlow testId="slow"
        text "Open slowly"
      button press=moveB testId="move"
        text "Track b"
`;

/** The interface package and the app, outside the tree; the app's built page. */
function fixture() {
  const dir = mkdtempSync(join(tmpdir(), 'exact-module-seam-')), pkg = join(dir, 'sensor'), app = join(dir, 'seamfixture');
  mkdirSync(join(pkg, 'web'), { recursive: true });
  writeFileSync(join(pkg, 'package.json'), JSON.stringify({ name: '@fixture/sensor', version: '1.0.0', exports: { '.': './exact.json', './web': './web/exact.json' } }));
  writeFileSync(join(pkg, 'exact.json'), JSON.stringify(INTERFACE));
  writeFileSync(join(pkg, 'web/exact.json'), JSON.stringify({ kind: 'implementation', implements: '@fixture/sensor@^1.0', platform: 'web', targets: ['web'], sources: { javascript: ['index.js'] } }));
  writeFileSync(join(pkg, 'web/index.js'), IMPLEMENTATION);
  run('bun', [resolve(root, 'scripts/exact.mjs'), 'new', app], root);
  writeFileSync(join(app, 'app.contract'), CONTRACT);
  rmSync(join(app, 'app.test.contract'), { force: true });
  const id = JSON.parse(readFileSync(join(app, 'app.json'), 'utf8')).app.id;
  writeFileSync(join(app, 'app.ts'), `export const appId = '${id}';\nexport const grants = 'device.microphone purpose.microphone';\nexport const answer = () => { throw new Error('no data source'); };\n`);
  mkdirSync(join(app, 'strings'), { recursive: true });
  writeFileSync(join(app, 'strings/en.json'), JSON.stringify({ 'purpose.microphone': 'The seam fixture reads a level.' }));
  const manifest = JSON.parse(readFileSync(join(app, 'package.json'), 'utf8'));
  manifest.dependencies = { ...manifest.dependencies, '@fixture/sensor': `file:${pkg}` };
  writeFileSync(join(app, 'package.json'), JSON.stringify(manifest, null, 2));
  run('bun', ['install', '--offline'], app);
  run('bun', ['exact.mjs', 'web-build', '--render', 'none'], app);
  return { dir, dist: join(app, 'target/web-dist') };
}

function serveFixture(dist, opener = open) {
  const types = { '.js': 'text/javascript', '.html': 'text/html', '.wasm': 'application/wasm', '.json': 'application/json', '.css': 'text/css' };
  const missing = [];
  const server = Bun.serve({ port: 0, async fetch(req) {
    const url = new URL(req.url), asked = url.pathname;
    const file = Bun.file(join(dist, asked === '/' ? 'index.html' : asked.slice(1)));
    const response = (await file.exists())
      ? new Response(file, { headers: { 'content-type': types[extname(asked)] ?? 'text/html' } })
      // Bun otherwise adds this header when sending, after the diagnostic reads it.
      : new Response('not found', { status: 404, headers: { 'content-type': 'text/plain;charset=utf-8' } });
    if (response.status === 404) missing.push(
      `${req.method} ${asked}${url.search} -> ${response.status}\n` +
      `  file: ${file.name}\n  content-type: ${response.headers.get('content-type')}\n` +
      `  referer: ${req.headers.get('referer') ?? '(none)'}`,
    );
    return response;
  } });
  return {
    server,
    async open() {
      try {
        return await opener({ host: 'web', url: `http://127.0.0.1:${server.port}/`, webDist: dist });
      } catch (error) {
        throw new Error(`${error.message}\nStatic server at ${server.url} (${dist}):\n` +
          (missing.join('\n') || 'no missing resources'), { cause: error });
      }
    },
  };
}

test('a failed seam fixture boot names missing static resources', async () => {
  const dist = mkdtempSync(join(tmpdir(), 'exact-module-seam-missing-'));
  const cause = new Error('fixture page could not boot');
  const served = serveFixture(dist, async ({ host, url, webDist }) => {
    expect(host).toBe('web');
    expect(webDist).toBe(dist);
    const response = await fetch(new URL('/missing-seam.js?probe=1', url), { headers: { referer: url } });
    expect(response.status).toBe(404);
    expect(response.headers.get('content-type')).toBe('text/plain;charset=utf-8');
    expect(await response.text()).toBe('not found');
    throw cause;
  });
  try {
    const failure = await served.open().then(() => null, error => error);
    expect(failure).toBeInstanceOf(Error);
    expect(failure.message).toContain(cause.message);
    expect(failure.message).toContain('GET /missing-seam.js?probe=1 -> 404');
    expect(failure.message).toContain(join(dist, 'missing-seam.js'));
    expect(failure.message).toContain('content-type: text/plain;charset=utf-8');
    expect(failure.message).toContain(`referer: http://127.0.0.1:${served.server.port}/`);
    expect(failure.cause).toBe(cause);
  } finally {
    served.server.stop(true);
    rmSync(dist, { recursive: true, force: true });
  }
});

test('an implementation that is its own substitute answers under the agent, behind admission and its ticket', async () => {
  const { dir, dist } = fixture();
  const served = serveFixture(dist);
  let s;
  try {
    s = await served.open();
    const seen = () => s.carrier.evaluate('globalThis.__sensor ?? null');
    const texts = async () => {
      const { nodes } = await s.tree(), byId = new Map(nodes.map(n => [n.id, n]));
      const text = n => n.props?.text ?? n.children.map(c => text(byId.get(c))).join('');
      return Object.fromEntries(nodes.filter(n => n.props?.testId).map(n => [n.props.testId, text(n)]));
    };
    await s.clock('settle');
    // The permission is the runner's under the agent: its status is not asked of the implementation.
    expect((await texts()).status).toBe('prompt');
    const extensions = (await s.state()).extensions.sensor;
    expect(extensions).toMatchObject({ agent: true, permission: 'prompt' });
    expect((await seen()).agent).toBe(true);
    expect((await texts()).meter).toBe('a 0 false');

    // A request while `prompt` waits on a ticket before the implementation hears of it.
    await s.tap('start');
    await s.clock('settle');
    expect((await seen()).opened).toEqual([]);
    const ticket = (await s.state()).pending.find(p => p.device?.capability === 'permission').ticket;
    await s.op({ op: 'tap', ticket, choice: 'allow' });
    await s.clock('settle');
    expect((await seen()).opened).toEqual(['t1']);
    expect((await texts()).opened).toBe('t1');

    // The preferred value reaches the implementation and is laid over its answer.
    await s.prefer({ extension: 'sensor.level={"muted":true}' });
    await s.clock('settle');
    expect((await seen()).preferred.at(-1).level).toEqual({ muted: true });
    expect((await texts()).meter).toMatch(/ true$/);

    // A jump across many periods polls once, and its reply waits for the answer.
    const before = (await seen()).levels.length;
    await s.clock('+1000');
    expect((await seen()).levels.length).toBe(before + 1);
    expect(Number((await texts()).meter.split(' ')[1])).toBeGreaterThanOrEqual(1000);

    // A reader's new arguments go to the implementation before the old group is released.
    await s.tap('move');
    await s.clock('settle');
    expect((await seen()).releases).toEqual([['level', { track: 'a', interval: 100 }]]);
    expect((await texts()).meter).toMatch(/^b /);

    // A reply overtaken before it landed is dropped, and its handles disposed of.
    await s.tap('slow');
    await s.tap('start');
    // Settle once: it waits for the slow reply, in flight for 5 s, which is dropped as it lands
    await s.clock('settle');
    expect((await seen()).disposed).toEqual(['t2']);
    expect((await texts()).opened).toBe('t3');
  } finally {
    await s?.close?.();
    served.server.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 600_000);

// A page's app-scoped list (LLP 1133.030 D12 item 2) in a real page: what a send, a due time and a DOM event see.
test.each(['send', 'extension', 'key', 'frame'])('an app list is current where %s reads it', async mode => {
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/') return new Response('<div id="exact-root"></div>', { headers: { 'content-type': 'text/html' } });
    if (path.endsWith('/names.js')) return new Response('export const sourceTypes = {};', { headers: { 'content-type': 'text/javascript' } });
    const local = resolve(root, '.' + path);
    const file = existsSync(local) ? local : resolve(root, 'host/web', path.split('/').at(-1));
    return existsSync(file) ? new Response(hostFixtureFile(file), { headers: { 'content-type': 'text/javascript' } }) : new Response('Not found', { status: 404 });
  } });
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(`${server.url}?agent&epoch=100000&storage=one`);
    await page.evaluate(async mode => {
      const rt = await import('/host/web-js/rt.js');
      const { modules } = await import('/host/web-js/modules.js');
      rt.clock.agent = true; rt.data.appId = 'review-list';
      const rows = {}, list = extension => ({ state: 'alarms', key: 'tag', scope: 'app', due: {
        after: 'duration', into: 'at', fires: { state: `${extension}-fired` },
      }, tap: { choices: { stop: { retire: true } } } });
      for (const extension of (mode === 'extension' ? ['first', 'second'] : ['first'])) {
        rows[extension] = [`${extension}.schedule`, true, { effect: true, list: list(extension) }, null, 'always', {}];
        rows[`${extension}Read`] = [`${extension}.read`, false, { listed: { state: 'alarms', fields: ['tag', 'state', 'delivered'] } }, ['[', {}], '', {}];
      }
      rows.patch = ['first.patch', true, { effect: true, update: { state: 'alarms', key: 'tag', set: { a: true } } }, null, 'always', {}];
      modules(rows);
      globalThis.schedule = (tag, extension = 'first') => exact.extensions.call({ extension, send: 'schedule', args: { tag, duration: 10 } });
      globalThis.entries = extension => exact.extensions.call({ extension, source: 'read' });
      globalThis.rt = rt;
    }, mode);
    if (mode === 'send') {
      // The runner applies a send, or refuses it, before the call returns.
      expect(await page.evaluate(() => {
        rt.Hosts.first('app', { tag: 'a', duration: 10 });
        const listed = exact.extensions.declared().alarms.map(e => e.tag);
        rt.Hosts.patch('app', { tag: 'a' });
        const patched = exact.extensions.declared().alarms[0].a;
        rt.Hosts.patch('app', { tag: 'missing' });
        return { listed, patched, refused: rt.journal.at(-1) };
      })).toEqual({ listed: ['a'], patched: true, refused: 't=0 patch: refused: NotFoundError' });
    } else if (mode === 'extension') {
      await page.evaluate(() => schedule('same')); await page.evaluate(() => schedule('same', 'second'));
      await page.evaluate(() => { rt.advance(10); });
      expect(await page.evaluate(() => entries('first'))).toEqual([{ tag: 'same', state: 'first-fired', delivered: true }]);
      expect(await page.evaluate(() => entries('second'))).toEqual([{ tag: 'same', state: 'second-fired', delivered: true }]);
      expect(await page.evaluate(() => exact.extensions.declared().alarms.map(e => e.state))).toEqual(['second-fired']);
    } else if (mode === 'key') {
      // The due entry fires before the event's handler runs, which decides the default as it returns.
      await page.evaluate(() => schedule('key'));
      expect(await page.evaluate(async () => {
        const { on, onKey } = await import('/host/web-js/events.js');
        const input = document.createElement('input'); document.body.append(input);
        const seen = [], announced = [];
        exact.announceModule = key => { if (key === 'first.read') announced.push(exact.extensions.declared().alarms[0].delivered); };
        rt.every(10, () => { seen.push(exact.extensions.declared().alarms[0].delivered); return true; }, true, 'due');
        on(input, 'keydown', () => { seen.push('key'); rt.Hosts.preventDefault(); rt.Hosts.stopPropagation(); }, onKey);
        on(document.body, 'keydown', () => seen.push('bubble'), onKey);
        rt.clock.now = 10;
        const event = new KeyboardEvent('keydown', { key: 'a', bubbles: true, cancelable: true });
        input.dispatchEvent(event);
        return { seen, prevented: event.defaultPrevented, announced, due: rt.journal.filter(l => l.includes(': due')).length };
      })).toEqual({ seen: [true, 'key'], prevented: true, announced: [true], due: 1 });
    } else {
      expect(await page.evaluate(async () => {
        const callbacks = [], seen = [];
        performance.now = () => 0;
        globalThis.requestAnimationFrame = callback => { callbacks.push(callback); return callbacks.length; };
        globalThis.cancelAnimationFrame = () => {};
        const { timerWaits } = await import('/host/web-js/timer-waits.js'); timerWaits();
        rt.every(0, () => { seen.push('wait'); return true; }, true, 'wait', 1);
        rt.frames(() => { seen.push('frame'); return true; }, 'frame');
        rt.clock.agent = false;
        rt.paint();
        callbacks.shift()(performance.now());
        rt.clock.agent = true;
        return seen;
      })).toEqual(['wait', 'frame']);
    }
  } finally { await browser?.close(); server.stop(true); }
}, 30000);
