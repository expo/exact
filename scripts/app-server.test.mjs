// App server discovery, command forwarding, ownership and restart recovery.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve, toNamespacedPath } from 'node:path';
import { appServer, serverChoice, serverUrl } from './app-server.mjs';

// LLP 1116.006 D3: the server an app calls, found where the rest pattern and the builders put it.
// LLP 1116.008 D1: an entry that does not read DB (itself, or in a module it imports) is refused in one line naming it.
test('the app\'s server: its entry, the URL app.ts calls, and why it cannot be run on an empty database', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-server-'));
  const put = (files) => { rmSync(dir, { recursive: true, force: true }); for (const [path, text] of Object.entries(files)) { mkdirSync(resolve(dir, path, '..'), { recursive: true }); writeFileSync(resolve(dir, path), text); } };
  const reads = "const db = new Database(process.env.DB ?? 'data.db');\n";
  try {
    put({ 'app.ts': "const API = 'http://127.0.0.1:6810'; // a phone reaches http://192.168.1.2:6810\n", 'server/index.ts': reads });
    assert.deepEqual(appServer(dir), { entry: 'server/index.ts', args: [], database: true, url: 'http://127.0.0.1:6810', port: 6810, why: null, refusal: null, settings: {} });
    put({ 'app.ts': "export const API = 'http://127.0.0.1:8787';\n", 'server.ts': reads, 'srv/main.ts': reads, 'app.json': JSON.stringify({ commands: { server: ['bun', '--watch', 'srv/main.ts'] } }) });
    assert.equal(appServer(dir).entry, 'srv/main.ts');
    put({ 'app.ts': "import type { Answer } from './app.contract.d.ts';\nimport { API } from './api';\n", 'api.ts': "export const API = 'http://localhost:3000/v1/';\n", 'server/server.ts': 'const DB = Bun.env.DB;\n' });
    assert.deepEqual([serverUrl(dir), appServer(dir).port, appServer(dir).refusal], ['http://localhost:3000/v1', 3000, null]);
    put({ 'app.ts': "const API = 'http://127.0.0.1:8787';\n", 'server.ts': "new Database('data.db');\n" });
    assert.match(appServer(dir).refusal, /^server\.ts does not read its database's path from DB, so it cannot be run on an empty one: write new Database\(process\.env\.DB \?\? 'data\.db'\) in it .*"commands": \{ "server": \["bun", "server\/index\.ts"\] \}$/);
    assert.equal(appServer(dir).why, null);
    // The entry app.json names is the one it looked at, and says so.
    put({ 'app.ts': "const API = 'http://127.0.0.1:8787';\n", 'server.ts': reads, 'srv/main.ts': 'Bun.serve({});\n', 'app.json': JSON.stringify({ commands: { server: ['bun', 'srv/main.ts'] } }) });
    assert.match(appServer(dir).refusal, /^srv\/main\.ts \(app\.json's commands\.server\) does not read /);
    // A server another program runs is run as written, told DB as Bun's is; its file is what the DB check reads (LLP 1156.001 D5).
    put({ 'app.ts': "const API = 'http://127.0.0.1:8787';\n", 'server.py': 'import os\ndb = sqlite3.connect(os.environ["DB"])\n', 'app.json': JSON.stringify({ commands: { server: ['python3', '-u', 'server.py'] } }) });
    assert.deepEqual([appServer(dir).entry, appServer(dir).command, appServer(dir).refusal], ['server.py', ['python3', '-u', 'server.py'], null]);
    put({ 'app.ts': "const API = 'http://127.0.0.1:8787';\n", 'server.py': 'db = sqlite3.connect("data.db")\n', 'app.json': JSON.stringify({ commands: { server: ['python3', 'server.py'] } }) });
    assert.match(appServer(dir).refusal, /^server\.py \(app\.json's commands\.server\) does not read .*os\.environ\["DB"\] in Python\), or say it keeps none in app\.json: "server": \{ "database": false \}$/);
    // A command with no file of the app to read (a module path) cannot be told to use the test's database: refused,
    // unless the app says its server keeps none.
    put({ 'app.ts': "const API = 'http://127.0.0.1:8787';\n", 'app.json': JSON.stringify({ commands: { server: ['uvicorn', 'api.main:app', '--port', '8787'] } }) });
    assert.match(appServer(dir).refusal, /^app\.json's commands\.server \(uvicorn api\.main:app --port 8787\) names no file of the app to read/);
    put({ 'app.ts': "const API = 'http://127.0.0.1:8787';\n", 'app.json': JSON.stringify({ commands: { server: ['uvicorn', 'api.main:app'] }, server: { database: false } }) });
    assert.deepEqual([appServer(dir).entry, appServer(dir).refusal], ['uvicorn api.main:app', null]);
    put({ 'app.ts': "const API = 'https://api.example.com';\n", 'server.ts': reads });
    assert.match(appServer(dir).why, /^server\.ts is left alone: app\.ts calls no local URL/);
    put({ 'app.ts': "const API = 'http://127.0.0.1:8787';\n" });
    assert.equal(appServer(dir), null);
    // A setting app.ts reads (LLP 1153 D4), as the build inlines it: the environment's, else .env.local's, else .env's,
    // else the default; a value that is no local URL leaves the server alone, though the default it overrides is one.
    const setting = { 'app.ts': "export const API = import.meta.env.API_BASE_URL ?? 'http://127.0.0.1:8080';\nexport const grants = `net.fetch ${API}`;\n", 'app.json': JSON.stringify({ env: ['API_BASE_URL'], server: { database: false } }), 'server.ts': 'Bun.serve({});\n' };
    put(setting);
    assert.equal(serverUrl(dir, {}), 'http://127.0.0.1:8080');
    // An app whose backend is one socket names its origin alone: the server is there.
    put({ ...setting, 'app.ts': "const CONVEX = 'ws://127.0.0.1:3210';\nexport const grants = `net.websocket ${CONVEX}`;\n" });
    assert.equal(serverUrl(dir, {}), 'http://127.0.0.1:3210');
    // A socket elsewhere does not stand for the server when the app names an http: one, whichever comes first.
    put({ ...setting, 'app.ts': "const FEED = 'ws://127.0.0.1:4791';\nconst API = 'http://127.0.0.1:8080';\nexport const grants = `net.websocket ${FEED}\\nnet.fetch ${API}`;\n" });
    assert.equal(serverUrl(dir, {}), 'http://127.0.0.1:8080');
    // …in a module app.ts imports, too.
    put({ ...setting, 'app.ts': "import { API } from './api';\nconst FEED = 'ws://127.0.0.1:4791';\nexport const grants = `net.websocket ${FEED}\\nnet.fetch ${API}`;\n", 'api.ts': "export const API = 'http://127.0.0.1:8090';\n" });
    assert.equal(serverUrl(dir, {}), 'http://127.0.0.1:8090');
    put({ ...setting, '.env': 'API_BASE_URL=http://127.0.0.1:8787\n' });
    assert.deepEqual([appServer(dir, {}).url, appServer(dir, {}).refusal], ['http://127.0.0.1:8787', null]);
    put({ ...setting, '.env': 'API_BASE_URL=http://127.0.0.1:8787\n', '.env.local': 'API_BASE_URL=http://localhost:9000\n' });
    assert.equal(serverUrl(dir, {}), 'http://localhost:9000');
    assert.equal(serverUrl(dir, { API_BASE_URL: 'http://127.0.0.1:7000' }), 'http://127.0.0.1:7000');
    // A comment naming another local URL is not code, and a setting's value wins over a constant before it; the
    // server's line says which origin it took and why.
    put({ ...setting, 'app.ts': "// The server listens on http://127.0.0.1:8787 by default.\nconst API = import.meta.env.API_BASE_URL ?? 'http://127.0.0.1:8787';\nexport function load() { return fetch(`${API}/items`); }\n" });
    assert.deepEqual([serverUrl(dir, { API_BASE_URL: 'http://127.0.0.1:9100' }), appServer(dir, { API_BASE_URL: 'http://127.0.0.1:9100' }).note], ['http://127.0.0.1:9100', undefined]);
    assert.equal(serverUrl(dir, {}), 'http://127.0.0.1:8787');
    put({ ...setting, 'app.ts': "const FALLBACK = 'http://127.0.0.1:8787'; /* or http://localhost:1 */\nconst API = import.meta.env.API_BASE_URL ?? 'http://127.0.0.1:8787';\nexport const grants = `net.fetch ${API} ${FALLBACK}`;\n" });
    assert.deepEqual([serverChoice(dir, { API_BASE_URL: 'http://127.0.0.1:9100' })], [{ url: 'http://127.0.0.1:9100', note: "the app's code names http://127.0.0.1:9100, http://127.0.0.1:8787: the server is started at http://127.0.0.1:9100, the value of API_BASE_URL" }]);
    put({ ...setting, 'app.ts': "const API = 'http://127.0.0.1:8080'; // not http://localhost:1\nconst OTHER = 'http://localhost:9000';\nexport const grants = `net.fetch ${API} ${OTHER}`;\n" });
    assert.deepEqual(serverChoice(dir, {}), { url: 'http://127.0.0.1:8080', note: "the app's code names http://127.0.0.1:8080, http://localhost:9000: the server is started at http://127.0.0.1:8080, the first in app.ts's code" });
    put(setting);
    assert.equal(appServer(dir, { API_BASE_URL: 'https://api.example.com' }).url, null);
    assert.match(appServer(dir, { API_BASE_URL: 'https://api.example.com' }).why, /^server\.ts is left alone: app\.ts calls no local URL/);
    // The spellings of reading DB, and a root shim that only imports the backend that reads it (book-club, polls, event-rsvp).
    for (const spelling of ['Bun.env.DB', 'process.env["DB"]', "process.env['DB'] ?? 'x.db'", 'env.DB', 'process.env?.DB', "const { DB } = process.env;", "const { DB: path = 'x.db', PORT } = Bun.env;", 'start(process.env.DB)']) {
      put({ 'app.ts': "const API = 'http://127.0.0.1:8787';\n", 'server.ts': `${spelling}\n` });
      assert.equal(appServer(dir).refusal, null, spelling);
    }
    for (const reader of ["process.env.DB_PATH", 'const DB = "data.db"', '// the DB is data.db']) {
      put({ 'app.ts': "const API = 'http://127.0.0.1:8787';\n", 'server.ts': `${reader}\n` });
      assert.match(appServer(dir).refusal, /^server\.ts does not read/, reader);
    }
    put({ 'app.ts': "const API = 'http://127.0.0.1:8787';\n", 'server.ts': "import './server/index.ts';\n", 'server/index.ts': "import { serve } from './polls';\nserve();\n", 'server/polls.ts': reads });
    assert.equal(appServer(dir).refusal, null);
    put({ 'app.ts': "const API = 'http://127.0.0.1:8787';\n", 'server.ts': "export * from './a';\n", 'a.ts': "import('./b');\n", 'b.ts': "import './a';\nconst x = require('./c');\n", 'c.js': 'new Database("data.db");\n' });
    assert.match(appServer(dir).refusal, /^server\.ts does not read/); // the cycle ends
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// A real child records its arguments and environment. The port lock, HTTP readiness and process listing are stubbed,
// in a separate process so Bun's module mock cannot replace the lock for other tests.
function stubServerRun(body) {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-server-command-'));
  const script = `
    import assert from 'node:assert/strict';
    import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
    import { resolve } from 'node:path';
    import { spawnSync } from 'node:child_process';
    import { mock, spyOn } from 'bun:test';
    const dir = ${JSON.stringify(dir)}, scripts = ${JSON.stringify(import.meta.dir)};
    const argv = ['--world', 'transcript', '--label', 'two words', '--reset-on-identify'];
    const manifest = { name: 'Server test', app: { id: 'dev.exact.server-test', name: 'Server test' }, commands: { server: ['bun', 'server.ts', ...argv] }, server: { database: false } };
    const save = () => writeFileSync(resolve(dir, 'app.json'), JSON.stringify(manifest));
    save();
    writeFileSync(resolve(dir, 'app.contract'), 'component App\\n  view\\n    text "Test"\\n');
    writeFileSync(resolve(dir, 'app.ts'), 'export const origin = "http://127.0.0.1:4790";');
    const ready = resolve(dir, 'ready.json');
    const serving = ${JSON.stringify("import { writeFileSync } from 'node:fs';\nwriteFileSync('ready.json', JSON.stringify({ argv: process.argv.slice(2), db: process.env['D' + 'B'] ?? null, port: process.env.PORT }));\nsetInterval(() => {}, 1000);\n")};
    writeFileSync(resolve(dir, 'server.ts'), serving);
    const record = () => JSON.parse(readFileSync(ready, 'utf8'));
    let taken = 0, given = 0;
    mock.module(resolve(scripts, 'port-lock.mjs'), () => ({ holderOf: async () => null, takePort: async (port) => {
      assert.equal(port, 4790); taken++; return () => { given++; };
    } }));
    spyOn(globalThis, 'fetch').mockImplementation(async () => {
      if (!existsSync(ready)) throw new Error('not ready');
      return new Response('ready');
    });
    const children = [], spawn = Bun.spawn;
    spyOn(Bun, 'spawn').mockImplementation((...args) => { const child = spawn(...args); children.push(child); return child; });
    const processes = await import('node:child_process'), spawnSyncReal = processes.spawnSync;
    mock.module('node:child_process', () => ({ ...processes, spawnSync: (command, args, options) => command === 'ps' && !args.includes('pid=,pgid=,stat=,lstart=')
      ? { stdout: children.some((child) => child.pid === Number(args.at(-1))) ? 'bun server.ts' : '' }
      : spawnSyncReal(command, args, options) }));
    const { appServer, startServer, withAppServer, driveServer } = await import(resolve(scripts, 'verify.mjs'));
    process.env.DB = resolve(dir, 'inherited.db');
    try { ${body} }
    finally { for (const child of children) { try { process.kill(-child.pid, 'SIGKILL'); } catch {} await child.exited; } }
    writeFileSync(resolve(dir, 'completed'), 'yes');
  `;
  try {
    writeFileSync(resolve(dir, 'test.mjs'), script); const r = spawnSync(process.execPath, [resolve(dir, 'test.mjs')], { encoding: 'utf8', timeout: 20_000 });
    assert.equal(r.status, 0, r.stdout + r.stderr + (r.error ?? ''));
    assert.equal(existsSync(resolve(dir, 'completed')), true, 'the subprocess completed its assertions');
  } finally { rmSync(dir, { recursive: true, force: true }); }
}

test('server command arguments reach the child in order, with DB and PORT for a database server', () => {
  stubServerRun(`
    delete manifest.server; save();
    writeFileSync(resolve(dir, 'server.ts'), "import { writeFileSync as write } from 'node:fs'; write(process.env.DB, 'scratch');\\n" + serving);
    const server = appServer(dir);
    assert.equal(server.refusal, null);
    const running = await startServer(dir, server, 'test');
    const db = resolve(dir, '.exact/test/db.sqlite');
    assert.deepEqual(record(), { argv, db, port: '4790' });
    assert.equal(readFileSync(db, 'utf8'), 'scratch');
    assert.equal(readFileSync(resolve(dir, '.exact/test/owner'), 'utf8').trim(), String(process.pid));
    await running.stop();
    assert.notEqual(await children[0].exited, 0);
    assert.equal(existsSync(resolve(dir, '.exact/test')), false);
    assert.deepEqual([taken, given], [1, 1]);
  `);
}, 30_000);

// LLP 1153 D4: a server that imports app.ts (the site pattern's) reads its settings as the build has them, though
// Bun's own .env loading under NODE_ENV=test skips .env.local, so it listens where discovery looks.
test('the settings app.ts reads reach the server as the build has them', () => {
  stubServerRun(`
    manifest.env = ['API_BASE_URL']; save();
    delete process.env.API_BASE_URL;
    process.env.NODE_ENV = 'test';
    writeFileSync(resolve(dir, 'app.ts'), "export const origin = import.meta.env.API_BASE_URL ?? 'http://127.0.0.1:8080';\\n");
    writeFileSync(resolve(dir, '.env'), 'API_BASE_URL=http://127.0.0.1:8787\\n');
    writeFileSync(resolve(dir, '.env.local'), 'API_BASE_URL=http://127.0.0.1:4790\\n');
    // As site's server.ts does: its address is app.ts's, read through Bun's import.meta.env.
    writeFileSync(resolve(dir, 'server.ts'), "import { writeFileSync } from 'node:fs';\\nimport { origin } from './app.ts';\\nwriteFileSync('ready.json', JSON.stringify({ api: origin, port: process.env.PORT }));\\nsetInterval(() => {}, 1000);\\n");
    // Discovery read under one environment; the server is given what it read, whatever this process inherited.
    const server = appServer(dir, { ...process.env });
    process.env.API_BASE_URL = 'http://127.0.0.1:8080';
    assert.equal(server.url, 'http://127.0.0.1:4790');
    const running = await startServer(dir, server, 'test');
    assert.deepEqual(record(), { api: 'http://127.0.0.1:4790', port: '4790' });
    await running.stop();
  `);
}, 30_000);

test('database: false omits inherited DB, preserves database files, and keeps refusal and cleanup behavior', () => {
  stubServerRun(`
    const server = appServer(dir);
    assert.equal(server.refusal, null);
    const runDir = resolve(dir, '.exact/test');
    mkdirSync(runDir, { recursive: true });
    const files = ['db.sqlite', 'db.sqlite-wal', 'db.sqlite-shm', 'db.sqlite-journal'];
    for (const file of files) writeFileSync(resolve(runDir, file), 'untouched');
    await assert.rejects(withAppServer(dir, async () => {
      assert.deepEqual(record(), { argv, db: null, port: '4790' });
      throw new Error('test failed');
    }, { verb: 'test', optional: true }), /test failed/);
    for (const file of files) assert.equal(readFileSync(resolve(runDir, file), 'utf8'), 'untouched');
    assert.equal(existsSync(resolve(runDir, 'owner')), false);
    assert.notEqual(await children[0].exited, 0);
    assert.deepEqual([taken, given], [1, 1]);
    assert.equal(await withAppServer(dir, () => assert.fail('an answering server must be refused'), { verb: 'test' }), 1);
    assert.equal(children.length, 1);
    assert.deepEqual([taken, given], [2, 2]);
    rmSync(ready);
    rmSync(runDir, { recursive: true });
    const running = await startServer(dir, server, 'test');
    assert.deepEqual(record(), { argv, db: null, port: '4790' });
    assert.deepEqual(readdirSync(runDir), ['owner']);
    await running.stop();
    assert.equal(existsSync(runDir), false);
    delete manifest.server; save();
    assert.match(appServer(dir).refusal, /does not read its database's path from DB/);
    assert.equal(await withAppServer(dir, () => assert.fail('a database server without DB must be refused'), { optional: true }), 1);
    assert.equal(children.length, 2);
  `);
}, 30_000);

test('agent --test starts and stops the app server on web, ios and macos, and reuses its parent server', () => {
  stubServerRun(`
    const { main } = await import(resolve(scripts, 'agent.mjs'));
    const args = ['--app', dir, '--test', resolve(dir, 'app.test.contract')];
    for (const host of ['web', 'ios', 'macos']) {
      const code = await main([host, ...args], { runTests: async (options) => {
        assert.equal(options.host, host);
        assert.equal(options.file, args[3]);
        assert.deepEqual(record(), { argv, db: null, port: '4790' });
        assert.equal(children.at(-1).exitCode, null);
        return { results: [], passed: 1, failed: 0 };
      } });
      assert.equal(code, 0);
      assert.notEqual(await children.at(-1).exited, 0);
      assert.equal(existsSync(resolve(dir, '.exact/test')), false);
      rmSync(ready);
    }
    const result = await withAppServer(dir, async () => {
      const source = 'const { main } = await import(' + JSON.stringify(resolve(scripts, 'agent.mjs')) + ');' +
        'process.exit(await main(' + JSON.stringify(['web', ...args]) + ', { runTests: async () => ({ results: [], passed: 1, failed: 0 }) }));';
      const child = spawnSync(process.execPath, ['-e', source], { encoding: 'utf8', timeout: 10_000 });
      assert.equal(child.status, 0, child.stdout + child.stderr);
      assert.equal(children.at(-1).exitCode, null, 'the parent server still runs');
      return 0;
    }, { verb: 'test', optional: true });
    assert.equal(result, 0);
    assert.notEqual(await children.at(-1).exited, 0);
    assert.deepEqual([taken, given], [4, 4]);
    rmSync(ready);
    rmSync(resolve(dir, 'server.ts'));
    assert.equal(await main(['web', ...args], { runTests: async () => ({ results: [], passed: 0, failed: 1 }) }), 1);
    assert.equal(children.length, 4, 'an app with no server still runs its tests');
  `);
}, 30_000);

test('an agent drive passes server arguments without DB or a scratch database', () => {
  stubServerRun(`
    const lease = resolve(dir, '.exact/agent-server');
    mkdirSync(lease, { recursive: true });
    const old = resolve(lease, 'db.sqlite');
    writeFileSync(old, 'untouched');
    const running = await driveServer(dir, 'one', () => {}, { fresh: true });
    assert.deepEqual(record(), { argv, db: null, port: '4790' });
    assert.equal(existsSync(resolve(dir, '.exact/agent/one')), false);
    await running.stop();
    assert.notEqual(await children[0].exited, 0);
    assert.equal(readFileSync(old, 'utf8'), 'untouched');
    assert.deepEqual(readdirSync(lease), ['db.sqlite']);
  `);
});

test('fixture drives isolate checkouts, refuse foreign servers and clean overlapping ports', async () => {
  const parent = mkdtempSync(resolve(tmpdir(), 'exact-fixture-checkouts-')), running = [], fixturePids = new Map();
  const { backendOriginSelection } = await import('../js/bake/src/typescript.mjs');
  const env = { ...process.env }; delete env.PORT; delete env.EXACT_BACKEND_ORIGIN;
  const script = `import { driveServer, appServer } from ${JSON.stringify(resolve(import.meta.dir, 'verify.mjs'))};
let running;
try {
  running = await driveServer(process.argv[1], 'fixture', () => {});
  console.log(JSON.stringify({ origin: appServer(process.argv[1]).url }));
  for await (const line of console) break;
} catch (e) { console.log(JSON.stringify({ error: e.message })); }
finally { await running?.stop(); }
process.exit(0);`;
  const drive = async (dir, extra = {}) => {
    const child = Bun.spawn([process.execPath, '-e', script, dir], { env: { ...env, ...extra }, stdin: 'pipe', stdout: 'pipe', stderr: 'pipe' });
    running.push(child);
    const reader = child.stdout.values()[Symbol.asyncIterator](), first = await reader.next();
    const result = JSON.parse(new TextDecoder().decode(first.value));
    if (result.origin) for (const file of readdirSync(resolve(dir, '.exact/agent-server')).filter(f => /^server-\d+$/.test(f))) {
      fixturePids.set(Number(file.slice(7)), realpathSync(dir));
    }
    return { ...result, child, stop: async () => { child.stdin.write('\n'); await child.stdin.flush(); await child.exited; } };
  };
  const fixtures = [], ports = new Set();
  try {
    for (let i = 0; fixtures.length < 2 && i < 100; i++) {
      const dir = resolve(parent, `checkout-${i}`);
      mkdirSync(resolve(dir, 'fixture'), { recursive: true });
      writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ commands: { server: ['bun', 'fixture/server.ts'] }, server: { database: false } }));
      writeFileSync(resolve(dir, 'app.ts'), 'export const origin = "http://127.0.0.1:4790";\n');
      const selected = backendOriginSelection(dir, {}), port = Number(new URL(selected.origin).port);
      assert.equal(backendOriginSelection(dir, {}).origin, selected.origin);
      if (ports.has(port)) continue;
      let probe; try { probe = Bun.serve({ hostname: '127.0.0.1', port, fetch: () => new Response('') }); } catch { continue; }
      probe.stop(true); ports.add(port);
      writeFileSync(resolve(dir, 'fixture/server.ts'), `Bun.serve({hostname:'127.0.0.1', port:Number(process.env.PORT), fetch:()=>Response.json({world:${JSON.stringify(dir)}})});\n`);
      fixtures.push(dir);
    }
    assert.equal(fixtures.length, 2, 'two unused checkout ports');
    const [a, b] = await Promise.all(fixtures.map(dir => drive(dir)));
    assert.notEqual(a.origin, b.origin);
    assert.deepEqual(await Promise.all([a, b].map(x => fetch(x.origin).then(r => r.json()))), fixtures.map(world => ({ world })));
    const conflict = await drive(fixtures[1], { EXACT_BACKEND_ORIGIN: a.origin });
    assert.match(conflict.error, /fixture does not belong.*--backend.*PORT=/);
    await conflict.child.exited;
    // A matching command and cwd are insufficient without the launch record.
    const lease = resolve(fixtures[0], '.exact/agent-server');
    const record = readdirSync(lease).find(f => /^server-\d+$/.test(f)), path = resolve(lease, record), held = readFileSync(path, 'utf8');
    await b.stop();
    const foreignLease = resolve(fixtures[1], '.exact/agent-server');
    mkdirSync(foreignLease, { recursive: true });
    const copied = resolve(foreignLease, record);
    for (const text of [held, JSON.stringify({ ...JSON.parse(held), directory: realpathSync(fixtures[1]), origin: a.origin }), 'null']) {
      writeFileSync(copied, text);
      try {
        const refused = await drive(fixtures[1], { EXACT_BACKEND_ORIGIN: a.origin });
        assert.match(refused.error, /fixture does not belong/);
        await refused.child.exited;
        assert.equal((await fetch(a.origin).then(r => r.json())).world, fixtures[0], 'refusal cleanup leaves the foreign server running');
        assert.equal(readFileSync(copied, 'utf8'), text, 'an unowned live process record is left alone');
      } finally { rmSync(copied, { force: true }); }
    }
    rmSync(path);
    try {
      const unknown = await drive(fixtures[0]);
      assert.match(unknown.error, /fixture does not belong/);
      await unknown.child.exited;
      assert.equal((await fetch(a.origin).then(r => r.json())).world, fixtures[0]);
    } finally { writeFileSync(path, held); }
    const parallel = await drive(fixtures[0], { EXACT_BACKEND_ORIGIN: b.origin });
    assert.equal(parallel.origin, b.origin);
    assert.equal((await fetch(parallel.origin).then(r => r.json())).world, fixtures[0]);
    const shared = await drive(fixtures[0]);
    assert.equal(shared.origin, a.origin);
    await a.stop();
    assert.equal((await fetch(shared.origin).then(r => r.json())).world, fixtures[0]);
    await shared.stop();
    assert.equal((await fetch(a.origin).then(r => r.json())).world, fixtures[0], 'the other port keeps this checkout leased');
    await parallel.stop();
    for (const origin of [a.origin, parallel.origin]) {
      assert.equal(await fetch(origin, { signal: AbortSignal.timeout(1000) }).then(() => true, () => false), false, 'the last drive stops every owned port');
    }
    assert.equal(existsSync(lease), false, 'both server records are removed');
  } finally {
    for (const child of running) { if (child.exitCode == null) child.kill('SIGTERM'); await child.exited; }
    for (const [pid, dir] of fixturePids) {
      const cwd = spawnSync('lsof', ['-a', '-p', String(pid), '-d', 'cwd', '-Fn'], { encoding: 'utf8' }).stdout?.match(/^n(.+)$/m)?.[1];
      if (cwd === dir) try { process.kill(pid, 'SIGTERM'); } catch {}
    }
    rmSync(parent, { recursive: true, force: true });
  }
}, 30000);


// A failed restart belongs to that test; the next test can retry without giving up the lease.
test('a failed restart keeps ownership, reports its last error and recovers on the next test', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-server-restart-'));
  const probe = Bun.serve({ port: 0, fetch: () => new Response('') }), port = probe.port;
  probe.stop(true);
  const { startServer } = await import('./app-server.mjs');
  const server = { entry: 'server.ts', args: [], database: true, port, url: `http://127.0.0.1:${port}` };
  writeFileSync(resolve(dir, 'server.ts'), `import { existsSync } from 'node:fs';
if (existsSync('.exact/fail')) { console.error('error: restart refused'); process.exit(23); }
if (existsSync(process.env.DB)) throw new Error('old database survived');
await Bun.write(process.env.DB, 'written');
Bun.serve({ port: Number(process.env.PORT), fetch: () => new Response('ok') });`);
  let running;
  try {
    running = await startServer(dir, server, 'test');
    await running.beforeTest();
    writeFileSync(resolve(dir, '.exact/fail'), '');
    await assert.rejects(running.beforeTest(), /exited \(code 23\).*error: restart refused/);
    assert.equal(readFileSync(resolve(dir, '.exact/test/owner'), 'utf8').trim(), String(process.pid));
    rmSync(resolve(dir, '.exact/fail'));
    await running.beforeTest();
    assert.equal(await fetch(server.url).then(r => r.text()), 'ok');
    assert.equal(readFileSync(resolve(dir, '.exact/test/db.sqlite'), 'utf8'), 'written');
  } finally { await running?.stop(); rmSync(dir, { recursive: true, force: true }); }
});

test('stopping a memory server leaves verify’s report directory available after failed captures', () => {
  stubServerRun(`
    const running = await startServer(dir, appServer(dir), 'verify');
    await running.stop();
    writeFileSync(resolve(dir, '.exact/verify/last.json'), '{}');
    assert.equal(existsSync(resolve(dir, '.exact/verify/owner')), false);
  `);
});


// Delay delivery of SIGKILL to a real descendant so the cleanup race is deterministic.
test('shutdown waits for a delayed descendant before clearing storage or launching again', () => {
  stubServerRun(`
    delete manifest.server; save();
    writeFileSync(resolve(dir, 'worker.ts'), "import { writeFileSync } from 'node:fs';\\nprocess.on('SIGTERM', () => {});\\nwriteFileSync(process.env.DB, 'held');\\nwriteFileSync('ready.json', '{}');\\nsetInterval(() => {}, 1000);\\n");
    writeFileSync(resolve(dir, 'server.ts'), "Bun.spawn([process.execPath, 'worker.ts'], {env: process.env, stdout:'ignore', stderr:'inherit'});\\nsetInterval(() => {}, 1000);\\n");
    const kill = process.kill.bind(process);
    let release, killed;
    const forced = new Promise(done => { killed = done; });
    spyOn(process, 'kill').mockImplementation((pid, signal) => {
      if (signal === 'SIGKILL' && pid === -children[0]?.pid) {
        release = () => kill(pid, signal); killed(); return true;
      }
      return kill(pid, signal);
    });
    const running = await startServer(dir, appServer(dir), 'test');
    const db = resolve(dir, '.exact/test/db.sqlite');
    await running.beforeTest();
    const restarting = running.beforeTest();
    await forced;
    // Let the already-exited launcher resolve without releasing its descendant.
    await new Promise(done => setImmediate(done));
    try {
      assert.equal(readFileSync(db, 'utf8'), 'held', 'the old process still owns the database');
      assert.equal(children.length, 1, 'no replacement before the descendant exits');
    } finally { release(); await restarting; await running.stop(); }
  `);
}, 30_000);

test('a launch ignores stderr still draining from the previous child', () => {
  stubServerRun(`
    let drain, drained;
    const gate = new Promise(done => { drain = done; });
    const finished = new Promise(done => { drained = done; });
    // Keep the first reader pending after its child exits, as a pipe with buffered data may be.
    spyOn(Bun, 'spawn').mockImplementation((...args) => {
      const child = spawn(...args); children.push(child);
      if (children.length !== 1) return child;
      return { pid: child.pid, get exitCode() { return child.exitCode; }, exited: child.exited,
        stderr: { async *[Symbol.asyncIterator]() { await gate; yield new TextEncoder().encode('error: previous child\\n'); drained(); } } };
    });
    const running = await startServer(dir, appServer(dir), 'test');
    await running.beforeTest();
    rmSync(ready);
    await running.beforeTest();
    drain(); await finished;
    assert.equal(running.tail(), '', 'only this launch may supply its diagnostic');
    await running.stop();
  `);
});

test.each(['127.0.0.1', '::1'])('shutdown keeps storage and ownership until the port is free: %s', (host) => {
  stubServerRun(`
    delete manifest.server; save();
    writeFileSync(resolve(dir, 'server.ts'), "import { writeFileSync } from 'node:fs'; writeFileSync(process.env.DB, 'held');\\n" + serving);
    const probe = Bun.serve({hostname:${JSON.stringify(host)}, port:0, fetch:()=>new Response('held')}), port = probe.port;
    const server = {...appServer(dir), port, url:'http://'+${JSON.stringify(host === '::1' ? '[::1]' : host)}+':'+port};
    // The lock and readiness are stubbed, but the port probe must observe this real listener.
    mock.module(resolve(scripts, 'port-lock.mjs'), () => ({takePort:async()=>()=>{given++;}}));
    const running = await startServer(dir, server, 'test');
    const net = await import('node:net'), create = net.createServer;
    let checked; const probed = new Promise(done => {checked=done;});
    mock.module('node:net',()=>({...net,createServer:()=>{const p=create();p.once('error',checked);p.once('close',checked);return p;}}));
    let stopped = false;
    const stopping = running.stop().then(()=>{stopped=true;});
    await Promise.race([probed, stopping]);
    await new Promise(done=>setImmediate(done));
    try {
      assert.equal(stopped, false, 'a live listener prevents shutdown completion');
      assert.equal(readFileSync(resolve(dir, '.exact/test/db.sqlite'), 'utf8'), 'held');
      assert.equal(existsSync(resolve(dir, '.exact/test/owner')), true);
      assert.equal(given, 0);
    } finally {probe.stop(true); await stopping;}
    assert.equal(existsSync(resolve(dir, '.exact/test/owner')), false);
    assert.equal(given, 1);
  `);
});

test('a shutdown that cannot finish refuses cleanup and can be retried', () => {
  stubServerRun(`
    delete manifest.server; save();
    writeFileSync(resolve(dir, 'server.ts'), "import { writeFileSync } from 'node:fs'; process.on('SIGTERM', () => {}); writeFileSync(process.env.DB, 'held');\\n" + serving);
    const running = await startServer(dir, appServer(dir), 'test');
    const kill = process.kill.bind(process), now = Date.now.bind(Date);
    let forced = false, time = now();
    const signal = spyOn(process, 'kill').mockImplementation((pid, kind) => {
      if (kind === 'SIGKILL') {forced=true;return kill(pid,kind);}
      if (forced && kind === 0) return true;
      return kill(pid,kind);
    });
    const table = spawnSyncReal('ps',['-A','-o','pid=,pgid=,stat=,lstart='],{encoding:'utf8'});
    mock.module('node:child_process',()=>({...processes,spawnSync:(cmd,args,opts)=>forced && cmd==='ps' && args.includes('pid=,pgid=,stat=,lstart=') ? table : spawnSyncReal(cmd,args,opts)}));
    const date = spyOn(Date, 'now').mockImplementation(()=>forced ? (time+=4000) : now());
    try {
      await assert.rejects(running.stop(), /shutdown did not finish: process group .* still alive or port .* still in use/);
      assert.equal(readFileSync(resolve(dir, '.exact/test/db.sqlite'), 'utf8'), 'held');
      assert.equal(existsSync(resolve(dir, '.exact/test/owner')), true);
      assert.equal(given, 0);
    } finally {forced=false;signal.mockRestore(); date.mockRestore(); await running.stop();}
    assert.equal(given, 1);
  `);
}, 30000);


test('failed initial startup retains the port turn when shutdown also fails', () => {
  stubServerRun(`
    delete manifest.server; save();
    mock.module('node:child_process',()=>({...processes,spawnSync:()=>({status:0,stdout:'65000 65000 S original'})}));
    let time=0;
    spyOn(Date,'now').mockImplementation(()=>time+=11000);
    spyOn(Bun,'sleep').mockImplementation(async()=>{});
    spyOn(globalThis,'fetch').mockImplementation(async()=>{throw new Error('not ready');});
    spyOn(process,'kill').mockImplementation(()=>true);
    spyOn(Bun,'spawn').mockImplementation((cmd, options)=>{
      writeFileSync(options.env.DB,'held');
      return {pid:65000,exitCode:null,signalCode:null,stderr:new ReadableStream({start(c){c.close();}}),exited:new Promise(()=>{})};
    });
    await assert.rejects(startServer(dir,appServer(dir),'test'), /shutdown did not finish/);
    assert.equal(readFileSync(resolve(dir,'.exact/test/db.sqlite'),'utf8'),'held');
    assert.equal(existsSync(resolve(dir,'.exact/test/owner')),true);
    assert.equal(given,0,'a surviving launch still holds the port turn');
  `);
});

test('shutdown probes only the managed address when another address holds the same port', () => {
  stubServerRun(`
    const other = Bun.serve({hostname:'::', port:0, fetch:()=>new Response('other')});
    const managed = Bun.serve({hostname:'127.0.0.1', port:other.port, fetch:()=>new Response('managed')});
    managed.stop(true);
    mock.module(resolve(scripts, 'port-lock.mjs'), () => ({takePort:async()=>()=>{given++;}}));
    const server = {...appServer(dir), port:other.port, url:'http://127.0.0.1:'+other.port};
    const running = await startServer(dir, server, 'test');
    try { await running.stop(); assert.equal(given, 1); }
    finally { other.stop(true); await running.stop(); }
  `);
});

test('a signal exit fails the current test and is reported by exited', () => {
  stubServerRun(`
    const running = await startServer(dir, appServer(dir), 'test');
    process.kill(children[0].pid, 'SIGTERM');
    const code = await children[0].exited;
    try {
      assert.throws(()=>running.check(), /exited.*SIGTERM.*during the test/);
      assert.equal(running.exited(), code);
      rmSync(ready);
      await running.beforeTest();
      assert.equal(children.length, 2, 'a signal exit before the first test also restarts');
    } finally { await running.stop(); }
  `);
});

test('a signal exit during startup reports death without waiting for the readiness deadline', () => {
  stubServerRun(`
    writeFileSync(resolve(dir, 'server.ts'), 'process.kill(process.pid, "SIGTERM");');
    const sleep = Bun.sleep;
    spyOn(Bun, 'sleep').mockImplementation(async(ms)=>{
      if(children[0]?.signalCode) throw new Error('readiness polled after the signal exit');
      await sleep(ms);
    });
    await assert.rejects(startServer(dir, appServer(dir), 'test'), /exited.*SIGTERM.*before it answered/);
  `);
});

test('a database delete failure keeps stop retryable and retains its exit handler', () => {
  stubServerRun(`
    delete manifest.server; save();
    const running = await startServer(dir, appServer(dir), 'test');
    const db = resolve(dir, '.exact/test/db.sqlite');
    writeFileSync(db, 'held');
    const fs = await import('node:fs'), remove = fs.rmSync;
    let locked = true;
    mock.module('node:fs', () => ({...fs, rmSync:(path, options)=>{
      if(path === db && locked) throw Object.assign(new Error('database locked'), {code:'EBUSY'});
      return remove(path, options);
    }}));
    const handlers = process.listenerCount('exit');
    try {
      await assert.rejects(running.stop(), /database locked/);
      assert.equal(process.listenerCount('exit'), handlers, 'failed cleanup keeps the exit handler');
      assert.equal(readFileSync(db, 'utf8'), 'held');
      assert.equal(existsSync(resolve(dir, '.exact/test/owner')), true);
      assert.equal(given, 0);
    } finally { locked = false; await running.stop(); }
    assert.equal(existsSync(db), false);
    assert.equal(existsSync(resolve(dir, '.exact/test/owner')), false);
    assert.equal(given, 1, 'retry releases the turn');
    assert.equal(process.listenerCount('exit'), handlers - 1);
    await running.stop();
    assert.equal(given, 1);
  `);
});

test.each(['test', 'drive'])('Windows identification failure stops the spawned child and releases %s ownership', (mode) => {
  stubServerRun(`
    Object.defineProperty(process, 'platform', {value:'win32'});
    spyOn(globalThis,'fetch').mockImplementation(async()=>{throw new Error('not ready');});
    mock.module('node:child_process',()=>({...processes,spawnSync:(cmd)=>{
      assert.equal(cmd,'powershell.exe'); return {status:1,stdout:'',stderr:'query failed'};
    }}));
    let ended, killed=0;
    const child={pid:65000,exitCode:null,signalCode:null,stderr:new ReadableStream({start(c){c.close();}}),
      exited:new Promise(done=>{ended=done;}),kill(signal){assert.equal(signal,'SIGKILL');killed++;this.signalCode=signal;ended(137);}};
    spyOn(Bun,'spawn').mockImplementation(()=>child);
    await assert.rejects(${mode === 'test' ? "startServer(dir,appServer(dir),'test')" : "driveServer(dir,'one',()=>{})"}, /cannot read the Windows process table/);
    assert.equal(killed,1,'the unidentified launch is killed through its child handle');
    assert.equal(existsSync(resolve(dir,'.exact/test/owner')),false);
    assert.equal(given,${mode === 'test' ? 1 : 0});
    assert.equal(existsSync(resolve(dir,'.exact/agent-server')),false);
  `);
});

test.each(['test', 'drive'])('Windows %s launch waits for its root to appear in CIM', (mode) => {
  stubServerRun(`
    Object.defineProperty(process, 'platform', {value:'win32'});
    let reads=0, ended, killed=0;
    const child={pid:65000,exitCode:null,signalCode:null,stderr:new ReadableStream({start(c){c.close();}}),
      exited:new Promise(done=>{ended=done;}),kill(){killed++;this.signalCode='SIGKILL';ended(137);}};
    spyOn(Bun,'spawn').mockImplementation(()=>child);
    mock.module('node:child_process',()=>({...processes,spawnSync:(cmd,args)=>{
      if(cmd==='powershell.exe')return {status:0,stdout:'1 0 1'+(++reads > 1 && !child.signalCode ? '\\n65000 1 2' : '')};
      assert.equal(cmd,'taskkill'); child.signalCode='SIGKILL';ended(137);return {status:0,stdout:''};
    }}));
    mock.module('node:net',()=>({createServer:()=>({once(){},listen(options,done){done();},close(done){done();}})}));
    let probes=0;
    spyOn(globalThis,'fetch').mockImplementation(async()=>{if(probes++===0)throw new Error('not yet');return new Response('ok');});
    const running=await ${mode === 'test' ? "startServer(dir,appServer(dir),'test')" : "driveServer(dir,'one',()=>{})"};
    await running.stop();
    assert.ok(reads > 1);
    assert.equal(killed,0,'the root was identified before readiness');
    assert.equal(given,${mode === 'test' ? 1 : 0});
  `);
});

test.each([false, true])('Windows never adopts an orphan older than its reused parent PID: newborn=%s', (newborn) => {
  stubServerRun(`
    Object.defineProperty(process, 'platform', {value:'win32'});
    const root=65000, worker=65001, orphan=65002, leaf=65003, born=65004;
    const stamp=638987654321000000n;
    const table=new Map([[root,{parent:1,created:stamp}], [worker,{parent:root,created:stamp+1n}],
      [orphan,{parent:root,created:stamp-1n}], [leaf,{parent:orphan,created:stamp+2n}]]);
    const kills=[]; let ended;
    mock.module('node:child_process',()=>({...processes,spawnSync:(cmd,args)=>{
      if(cmd==='powershell.exe')return {status:0,stdout:[...table].map(([pid,p])=>[pid,p.parent,p.created].join(' ')).join('\\n')};
      assert.equal(cmd,'taskkill');
      const pid=Number(args[args.indexOf('/PID')+1]);
      const selected=new Set([pid]);
      if(args.includes('/T')) {
        let added;
        do {added=false;for(const [id,p] of table)if(selected.has(p.parent) && !selected.has(id)){selected.add(id);added=true;}} while(added);
      }
      for(const id of selected) {
        assert.ok(id !== orphan && id !== leaf, 'a tree kill reaches the unrelated older orphan');
        kills.push(id);table.delete(id);
        if(id===worker && ${newborn})table.set(born,{parent:root,created:stamp+3n});
        if(id===root){child.exitCode=0;ended(0);}
      }
      return {status:0,stdout:''};
    }}));
    mock.module('node:net',()=>({createServer:()=>({once(){},listen(options,done){done();},close(done){done();}})}));
    const child={pid:root,exitCode:null,signalCode:null,stderr:new ReadableStream({start(c){c.close();}}),exited:new Promise(done=>{ended=done;})};
    spyOn(Bun,'spawn').mockImplementation(()=>child);
    let probes=0;
    spyOn(globalThis,'fetch').mockImplementation(async()=>{if(probes++===0)throw new Error('not yet');return new Response('ok');});
    const running=await startServer(dir,appServer(dir),'test');
    await running.stop();
    assert.deepEqual(kills,${newborn ? '[worker,born,root]' : '[worker,root]'}, 'only owned descendants are stopped, leaves first');
    assert.equal(table.has(orphan),true);
    assert.equal(table.has(leaf),true);
    assert.equal(given,1);
  `);
});

test.each([false, true])('Windows shutdown checks descendant creation times after ancestors exit: reused=%s', (reused) => {
  stubServerRun(`
    Object.defineProperty(process, 'platform', {value:'win32'});
    const root=65000, worker=65001, leaf=65002, foreign=65003;
    let table=new Map([[root,1],[worker,root],[leaf,worker],[foreign,1]]), ended;
    const kills=[]; let reusedRoot=false,reusedWorker=false;
    spyOn(process, 'kill').mockImplementation((pid, signal)=>{
      assert.equal(signal, 0);
      if(!table.has(pid)) throw Object.assign(new Error('gone'), {code:'ESRCH'});
      return true;
    });
    mock.module('node:child_process',()=>({...processes,spawnSync:(cmd,args)=>{
      if(cmd==='powershell.exe') return {status:0,stdout:[...table].map(([pid,parent])=>[pid,parent,pid===root && reusedRoot || pid===worker && reusedWorker ? '638987654321000001' : '638987654321000000'].join(' ')).join('\\n')};
      assert.equal(cmd,'taskkill'); assert.ok(!args.includes('/T') && args.includes('/F'));
      const pid=Number(args[args.indexOf('/PID')+1]); kills.push(pid);
      table.delete(pid); if(pid===worker)table.delete(leaf);
      return {status:0,stdout:''};
    }}));
    mock.module('node:net',()=>({createServer:()=>({once(){},listen(options,done){done();},close(done){done();}})}));
    const child={pid:root,exitCode:null,signalCode:null,stderr:new ReadableStream({start(c){c.close();}}),exited:new Promise(done=>{ended=done;})};
    spyOn(Bun,'spawn').mockImplementation(()=>child);
    writeFileSync(ready,'{}');
    let probes=0;
    spyOn(globalThis,'fetch').mockImplementation(async()=>{if(probes++===0)throw new Error('not yet');return new Response('ok');});
    const running=await startServer(dir,appServer(dir),'test');
    // Startup's readiness walk observed the descendants while the launcher lived.
    await running.beforeTest();
    table.delete(root); table.delete(worker); child.exitCode=0; ended(0);
    if(${reused}){reusedRoot=true;reusedWorker=true;table.set(root,1);table.set(worker,1);table.set(65004,worker);}
    await running.stop();
    assert.ok(kills.includes(leaf), 'the orphan is killed independently of the launcher');
    assert.equal(table.has(leaf),false);
    assert.equal(table.has(foreign),true,'an unrelated process is left alone');
    if(${reused}){assert.equal(table.has(root),true,'a reused launcher PID is left alone');assert.equal(table.has(worker),true,'a reused worker PID is left alone');assert.ok(!kills.includes(65004),'a reused worker grants no authority over its children');}
  `);
});

test('Windows shutdown rechecks a PID reused after an earlier kill', () => {
  stubServerRun(`
    Object.defineProperty(process, 'platform', {value:'win32'});
    const root=65000, worker=65001, leaf=65002, foreign=65003;
    let table=new Map([[root,{parent:1,created:'638987654321000000'}],[worker,{parent:root,created:'638987654321000000'}],[leaf,{parent:worker,created:'638987654321000000'}],[foreign,{parent:1,created:'638987654321000000'}]]), ended;
    const kills=[];
    mock.module('node:child_process',()=>({...processes,spawnSync:(cmd,args)=>{
      if(cmd==='powershell.exe')return {status:0,stdout:[...table].map(([pid,p])=>[pid,p.parent,p.created].join(' ')).join('\\n')};
      assert.equal(cmd,'taskkill');
      const pid=Number(args[args.indexOf('/PID')+1]);kills.push(pid);table.delete(pid);
      if(pid===leaf)table.set(worker,{parent:1,created:'638987654321000001'});
      if(pid===root){child.exitCode=0;ended(0);}
      return {status:0,stdout:''};
    }}));
    mock.module('node:net',()=>({createServer:()=>({once(){},listen(options,done){done();},close(done){done();}})}));
    const child={pid:root,exitCode:null,signalCode:null,stderr:new ReadableStream({start(c){c.close();}}),exited:new Promise(done=>{ended=done;})};
    spyOn(Bun,'spawn').mockImplementation(()=>child);
    let probes=0;
    spyOn(globalThis,'fetch').mockImplementation(async()=>{if(probes++===0)throw new Error('not yet');return new Response('ok');});
    const running=await startServer(dir,appServer(dir),'test');
    await running.stop();
    assert.deepEqual(kills,[leaf,root],'a preceding kill can release a descendant PID for reuse');
    assert.equal(table.get(worker)?.created,'638987654321000001','the unrelated replacement survives');
    assert.equal(given,1);
  `);
});

test.each(['before shutdown', 'during shutdown'])('Windows stops an owned replacement reusing an observed PID %s', (when) => {
  stubServerRun(`
    Object.defineProperty(process, 'platform', {value:'win32'});
    const root=65000, worker=65001, stamp=638987654321000000n;
    const table=new Map([[1,{parent:0,created:stamp-1n}],[root,{parent:1,created:stamp}],[worker,{parent:root,created:stamp+1n}]]);
    const kills=[]; let ended;
    mock.module('node:child_process',()=>({...processes,spawnSync:(cmd,args)=>{
      if(cmd==='powershell.exe')return {status:0,stdout:[...table].map(([pid,p])=>[pid,p.parent,p.created].join(' ')).join('\\n')};
      assert.equal(cmd,'taskkill'); assert.ok(!args.includes('/T'));
      const pid=Number(args[args.indexOf('/PID')+1]);
      const created=table.get(pid).created;
      kills.push([pid,String(created)]);table.delete(pid);
      if(pid===worker && created===stamp+1n && ${when === 'during shutdown'})table.set(worker,{parent:root,created:stamp+2n});
      if(pid===root){child.exitCode=0;ended(0);}
      return {status:0,stdout:''};
    }}));
    mock.module('node:net',()=>({createServer:()=>({once(){},listen(options,done){done();},close(done){done();}})}));
    const child={pid:root,exitCode:null,signalCode:null,stderr:new ReadableStream({start(c){c.close();}}),exited:new Promise(done=>{ended=done;})};
    spyOn(Bun,'spawn').mockImplementation(()=>child);
    let probes=0;
    spyOn(globalThis,'fetch').mockImplementation(async()=>{if(probes++===0)throw new Error('not yet');return new Response('ok');});
    const running=await startServer(dir,appServer(dir),'test');
    if(${when === 'before shutdown'})table.set(worker,{parent:root,created:stamp+2n});
    await running.stop();
    assert.deepEqual(kills,${when === 'during shutdown' ? "[[worker,String(stamp+1n)],[worker,String(stamp+2n)],[root,String(stamp)]]" : "[[worker,String(stamp+2n)],[root,String(stamp)]]"});
    assert.deepEqual([...table.keys()],[1],'only the unrelated process survives cleanup');
    assert.equal(given,1);
  `);
});

test.each(['SIGTERM', 'SIGKILL'])('Windows repeated worker replacements respect the %s shutdown deadline', (phase) => {
  stubServerRun(`
    Object.defineProperty(process, 'platform', {value:'win32'});
    delete manifest.server;save();
    writeFileSync(resolve(dir,'server.ts'),'const db=process.env.DB;');
    const root=65000, worker=65001, stamp=638987654321000000n;
    const table=new Map([[1,{parent:0,created:stamp-1n}],[root,{parent:1,created:stamp}],[worker,{parent:root,created:stamp+1n}]]);
    let ended, time=0, replacements=0, respawning=true;
    spyOn(Date,'now').mockImplementation(()=>time);
    spyOn(Bun,'sleep').mockImplementation(async(ms)=>{time+=ms;});
    mock.module('node:child_process',()=>({...processes,spawnSync:(cmd,args)=>{
      if(cmd==='powershell.exe')return {status:0,stdout:[...table].map(([pid,p])=>[pid,p.parent,p.created].join(' ')).join('\\n')};
      assert.equal(cmd,'taskkill');
      const pid=Number(args[args.indexOf('/PID')+1]);
      if(respawning && ${phase === 'SIGKILL'} && time<3000)return {status:0,stdout:''};
      const created=table.get(pid).created;table.delete(pid);
      if(pid===worker && respawning){
        if(++replacements>20)throw new Error('replacement loop exceeded the shutdown deadline');
        time+=250;table.set(worker,{parent:root,created:created+1n});
      }
      if(pid===root){child.exitCode=0;ended(0);}
      return {status:0,stdout:''};
    }}));
    mock.module('node:net',()=>({createServer:()=>({once(){},listen(options,done){done();},close(done){done();}})}));
    const child={pid:root,exitCode:null,signalCode:null,stderr:new ReadableStream({start(c){c.close();}}),exited:new Promise(done=>{ended=done;})};
    spyOn(Bun,'spawn').mockImplementation(()=>child);
    let probes=0;
    spyOn(globalThis,'fetch').mockImplementation(async()=>{if(probes++===0)throw new Error('not yet');return new Response('ok');});
    const running=await startServer(dir,appServer(dir),'test'), db=resolve(dir,'.exact/test/db.sqlite');
    writeFileSync(db,'held');
    try {
      await assert.rejects(running.stop(),/shutdown did not finish: process group 65000/);
      assert.equal(time,${phase === 'SIGTERM' ? 3000 : 6000},'the phase deadline bounds the kill loop');
      assert.equal(replacements,12,'repeated replacements were observed');
      assert.equal(table.has(root),true,'the respawning launcher survives the failed shutdown');
      assert.equal(readFileSync(db,'utf8'),'held');
      assert.equal(existsSync(resolve(dir,'.exact/test/owner')),true);
      assert.equal(given,0,'failed shutdown retains the port turn');
    } finally {respawning=false;await running.stop();}
    assert.equal(existsSync(db),false);
    assert.equal(existsSync(resolve(dir,'.exact/test/owner')),false);
    assert.equal(given,1,'a successful retry releases the port turn');
  `);
});

test('POSIX shutdown retains storage and its port turn after an unobserved worker handoff', () => {
  stubServerRun(`
    delete manifest.server;save();
    writeFileSync(resolve(dir,'server.ts'),'const db=process.env.DB;');
    const root=65000, worker=65001;
    let table=root+' '+root+' S original', ended;
    mock.module('node:child_process',()=>({...processes,spawnSync:()=>({status:0,stdout:table})}));
    const child={pid:root,exitCode:null,signalCode:null,stderr:new ReadableStream({start(c){c.close();}}),exited:new Promise(done=>{ended=done;})};
    spyOn(Bun,'spawn').mockImplementation(()=>child);
    spyOn(process,'kill').mockImplementation((pid,signal)=>{
      assert.equal(pid,-root);assert.equal(signal,'SIGTERM');
      table=worker+' '+root+' S worker';child.exitCode=0;ended(0);return true;
    });
    mock.module('node:net',()=>({createServer:()=>({once(){},listen(options,done){done();},close(done){done();}})}));
    let probes=0;
    spyOn(globalThis,'fetch').mockImplementation(async()=>{if(probes++===0)throw new Error('not yet');return new Response('ok');});
    const running=await startServer(dir,appServer(dir),'test'), db=resolve(dir,'.exact/test/db.sqlite');
    writeFileSync(db,'held');
    try {
      await assert.rejects(running.stop(),/cannot verify.*process group 65000/);
      assert.equal(readFileSync(db,'utf8'),'held');
      assert.equal(existsSync(resolve(dir,'.exact/test/owner')),true);
      assert.equal(given,0,'an unknown live group retains the port turn');
    } finally {table='';await running.stop();}
  `);
});

test('shutdown refuses a reused POSIX process group after its observed members exit', () => {
  stubServerRun(`
    const root=65000, foreign=65001;
    let reused=false, ended;
    const kills=[];
    mock.module('node:child_process',()=>({...processes,spawnSync:(cmd,args)=>({status:0,stdout:args.includes('pid=,pgid=,stat=,lstart=')
      ? root+' '+root+' S '+(reused?'new':'original')+'\\n'+(reused?foreign+' '+root+' S new':'') : ''})}));
    spyOn(process,'kill').mockImplementation((pid,signal)=>{
      if(signal===0)return true;
      kills.push([pid,signal]);
    });
    const child={pid:root,exitCode:null,signalCode:null,stderr:new ReadableStream({start(c){c.close();}}),exited:new Promise(done=>{ended=done;})};
    spyOn(Bun,'spawn').mockImplementation(()=>child);
    mock.module('node:net',()=>({createServer:()=>({once(){},listen(options,done){done();},close(done){done();}})}));
    let probes=0;
    spyOn(globalThis,'fetch').mockImplementation(async()=>{if(probes++===0)throw new Error('not yet');return new Response('ok');});
    const running=await startServer(dir,appServer(dir),'test');
    reused=true;child.exitCode=0;ended(0);
    let time=0;spyOn(Date,'now').mockImplementation(()=>time+=4000);
    try {
      await assert.rejects(running.stop(),/cannot verify.*process group 65000/);
      assert.deepEqual(kills,[],'the new group has no observed member from our launch');
      assert.equal(given,0,'uncertain ownership retains the port turn');
    } finally {reused=false;mock.module('node:child_process',()=>({...processes,spawnSync:()=>({status:0,stdout:''})}));await running.stop();}
  `);
});

test('an agent drive retains its server record until its address is free', () => {
  stubServerRun(`
    const probe=Bun.serve({hostname:'127.0.0.1',port:0,fetch:()=>new Response('held')});
    writeFileSync(resolve(dir,'app.ts'),'export const API="http://127.0.0.1:'+probe.port+'";');
    const running=await driveServer(dir,'one',()=>{}), lease=resolve(dir,'.exact/agent-server');
    const net=await import('node:net'), create=net.createServer;
    let checked;const probed=new Promise(done=>{checked=done;});
    mock.module('node:net',()=>({...net,createServer:()=>{const p=create();p.once('error',checked);p.once('close',checked);return p;}}));
    let stopped=false;const stopping=running.stop().then(()=>{stopped=true;});
    await Promise.race([probed,stopping]);
    await new Promise(done=>setImmediate(done));
    try {
      assert.equal(stopped,false,'the drive waits on its own server address');
      assert.ok(readdirSync(lease).some(f=>f.startsWith('server-')),'ownership survives while the address is held');
    } finally {probe.stop(true);await stopping;}
    assert.equal(existsSync(lease),false);
  `);
});


test('unexpected server exits survive later healthy restarts and intentional stops', () => {
  stubServerRun(`
    const running = await startServer(dir, appServer(dir), 'verify');
    try {
      for (const signal of ['SIGTERM','SIGKILL']) {
        process.kill(children.at(-1).pid,signal);
        await children.at(-1).exited;
        rmSync(ready);
        await running.beforeTest();
      }
      assert.deepEqual(running.exits(),[
        'server.ts exited (signal SIGTERM) during the run',
        'server.ts exited (signal SIGKILL) during the run',
      ]);
      assert.equal(running.exited(),null,'the replacement is healthy');
      await running.stop();
      assert.equal(running.exits().length,2,'intentional shutdown is not a crash');
    } finally {await running.stop();}
  `);
});

test('withAppServer fails after a crash even when its replacement passes', () => {
  stubServerRun(`
    let running;
    const code = await withAppServer(dir, async (server) => {
      running = server;
      await server.beforeTest(); server.check();
      process.kill(children.at(-1).pid, 'SIGKILL');
      await children.at(-1).exited;
      rmSync(ready);
      await server.beforeTest(); server.check();
      assert.equal(children.length, 2, 'the replacement passed');
      return 0;
    }, {verb:'test'});
    assert.equal(given, 1, 'the verdict follows cleanup');
    assert.equal(running.exits().length, 1);
    assert.equal(code, 1, 'a recovered crash still fails the run');
  `);
});

// The public command, compiler, runner and HTTP server run. A beforeTest wrapper triggers the crash;
// host I/O and evidence writes are substituted.
test('agent --test reports a crash between passing tests on a replacement server', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-agent-server-crash-')), script = resolve(dir, 'stub.mjs');
  const probe = Bun.serve({hostname:'127.0.0.1',port:0,fetch:()=>new Response('reserved')});
  const port = probe.port; probe.stop(true);
  writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    text "Test"\n');
  writeFileSync(resolve(dir, 'app.ts'), `export const API="http://127.0.0.1:${port}";`);
  writeFileSync(resolve(dir, 'app.test.contract'), 'test "early"\ntest "late"\n');
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({name:'Server crash',app:{id:'dev.exact.server-crash',name:'Server crash'},server:{database:false}}));
  writeFileSync(resolve(dir, 'server.ts'), 'Bun.serve({hostname:"127.0.0.1",port:Number(process.env.PORT),fetch:()=>new Response("ready")});');
  writeFileSync(script, `
    import assert from 'node:assert/strict';
    import {mock,spyOn} from 'bun:test';
    import {writeFileSync} from 'node:fs';
    const at=p=>${JSON.stringify(import.meta.dir)}+'/'+p, dir=${JSON.stringify(dir)};
    const children=[], spawn=Bun.spawn;
    spyOn(Bun,'spawn').mockImplementation((...args)=>{const child=spawn(...args);children.push(child);return child;});
    const server={...await import(at('app-server.mjs'))};
    mock.module(at('app-server.mjs'),()=>({...server,withAppServer:(dir,fn,options)=>server.withAppServer(dir,async(running)=>{
      let tests=0;
      const before=running.beforeTest;
      running.beforeTest=async()=>{
        if(++tests===2){children[0].kill('SIGKILL');await children[0].exited;}
        await before();
      };
      return fn(running);
    },options)}));
    let opened=0;
    mock.module(at('agent-session.mjs'),()=>({clockSpan(){},open:async()=>{
      if(++opened===2){assert.equal(children.length,2);writeFileSync(dir+'/replacement','passed');}
      return {host:'web',now:0,carrier:{reset:async()=>{}},clock:async()=>({}),
        state:async()=>({mutationRequests:0,resources:{},background:{done:0,failed:0}}),
        op:async()=>({clock:0,nodes:[],lines:[]}),logs:async()=>({lines:[]}),close:async()=>{}};
    }}));
    mock.module(at('evidence.mjs'),()=>({recordRun(){}}));
  `);
  try {
    const r = spawnSync(process.execPath, ['--preload', script, resolve(import.meta.dir, 'agent.mjs'), 'web', '--browser', 'firefox', '--app', dir, '--test', resolve(dir, 'app.test.contract')], {encoding:'utf8',timeout:20000});
    assert.equal(existsSync(resolve(dir, 'replacement')), true, r.stdout+r.stderr);
    assert.equal(readFileSync(resolve(dir, 'replacement'), 'utf8'), 'passed', r.stdout+r.stderr);
    assert.match(r.stdout, /2 passed, 0 failed/);
    assert.equal(existsSync(resolve(dir, '.exact/test/owner')), false, 'cleanup completed');
    assert.equal(r.status, 1, r.stdout+r.stderr);
    assert.match(r.stderr, /test: server\.ts exited \(signal SIGKILL\) during the run/);
  } finally {rmSync(dir,{recursive:true,force:true});}
}, 30_000);


test.each(['SIGTERM', 'exit'])('a throwing server cleanup cannot interrupt %s', (event) => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-server-signal-'));
  try {
    const script = resolve(dir, 'test.mjs');
    writeFileSync(script, `
      import {whenEnded} from ${JSON.stringify(resolve(import.meta.dir, 'app-server.mjs'))};
      import {writeFileSync} from 'node:fs';
      const marker=${JSON.stringify(resolve(dir, 'cleaned'))};
      whenEnded(()=>{writeFileSync(marker,'attempted');throw new Error('cannot verify server ownership');});
      process.on('uncaughtException',()=>{});
      process.on('exit',()=>writeFileSync(marker+'-after','yes'));
      ${event === 'SIGTERM' ? "setTimeout(()=>process.exit(99),250);process.kill(process.pid,'SIGTERM');" : 'process.exit(17);'}
    `);
    const r = spawnSync(process.execPath, [script], {encoding:'utf8',timeout:3000,killSignal:'SIGKILL'});
    assert.equal(existsSync(resolve(dir, 'cleaned')),true,'cleanup was attempted');
    assert.equal(r.status,event === 'SIGTERM' ? 143 : 17,r.stdout+r.stderr+String(r.error ?? ''));
    assert.equal(existsSync(resolve(dir, 'cleaned-after')),true,'later exit listeners still run');
  } finally {rmSync(dir,{recursive:true,force:true});}
});
