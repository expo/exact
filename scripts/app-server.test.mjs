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
    const children = [], spawn = Bun.spawn, groups = new Map();
    const processes = await import('node:child_process'), spawnSyncReal = processes.spawnSync;
    // Each handle is the server's supervisor; its child, the keeper, has the pid that is the server's group's id.
    const keeperOf = (child) => Number(spawnSyncReal('pgrep', ['-P', String(child.pid)], { encoding: 'utf8' }).stdout.trim().split('\\n')[0]) || null;
    const groupOf = (child) => { if (!groups.get(child)) groups.set(child, keeperOf(child)); return groups.get(child); };
    spyOn(Bun, 'spawn').mockImplementation((...args) => {
      const child = spawn(...args); children.push(child);
      (async () => { while (!groups.get(child) && child.exitCode === null && child.signalCode === null) { groups.set(child, keeperOf(child)); await Bun.sleep(5); } })();
      return child;
    });
    mock.module('node:child_process', () => ({ ...processes, spawnSync: (command, args, options) => command === 'ps' && args.includes('command=') && children.some((child) => child.pid === Number(args.at(-1)))
      ? { stdout: 'bun server.ts' } : spawnSyncReal(command, args, options) }));
    const { appServer, startServer, withAppServer, driveServer } = await import(resolve(scripts, 'verify.mjs'));
    const groupAlive = (child) => { try { process.kill(-groupOf(child), 0); return true; } catch (e) { return e.code !== 'ESRCH'; } };
    const serverPid = (child = children.at(-1)) => Number(spawnSyncReal('pgrep', ['-g', String(groupOf(child))], { encoding: 'utf8' }).stdout.trim().split('\\n')[0]);
    // Ended: a killed process the supervisor held is a zombie until its new parent reaps it, after the supervisor exits.
    const ended = async (pid) => { for (let i = 0; i < 100; i++) { try { process.kill(pid, 0); } catch (e) { if (e.code === 'ESRCH') return true; } await Bun.sleep(20); } return false; };
    const crash = async (running, signal) => { process.kill(serverPid(), signal); while (running.exited() == null) await Bun.sleep(10); };
    process.env.DB = resolve(dir, 'inherited.db');
    try { ${body} }
    finally { for (const child of children) { for (const pid of [-groupOf(child), groupOf(child), child.pid]) try { if (pid) process.kill(pid, 'SIGKILL'); } catch {} await child.exited; } }
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

// A drive's server has no owner process to stop its supervisor.
test('a drive server\'s supervisor ends once the server and its group have', () => {
  stubServerRun(`
    const running = await driveServer(dir, 'one', () => {});
    process.kill(serverPid(), 'SIGKILL');
    assert.equal(await children[0].exited, 137);
    await running.stop();
    assert.equal(existsSync(resolve(dir, '.exact/agent-server')), false);
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
      if (signal === 'SIGKILL' && children[0] && pid === -groupOf(children[0])) {
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
      return { pid: child.pid, get exitCode() { return child.exitCode; }, get signalCode() { return child.signalCode; }, exited: child.exited,
        stdout: child.stdout, kill: (signal) => child.kill(signal), stderr: { async *[Symbol.asyncIterator]() { await gate; yield new TextEncoder().encode('error: previous child\\n'); drained(); } } };
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
    // The group outlives SIGKILL, as a member stuck in the kernel would.
    const kill = process.kill.bind(process), now = Date.now.bind(Date);
    let stuck = true, time = now();
    const signal = spyOn(process, 'kill').mockImplementation((pid, kind) => stuck && kind === 'SIGKILL' ? true : kill(pid, kind));
    const date = spyOn(Date, 'now').mockImplementation(() => time += 1000);
    try {
      await assert.rejects(running.stop(), /shutdown did not finish: process group .* still alive or port .* still in use/);
      assert.equal(groupAlive(children[0]), true);
      assert.equal(readFileSync(resolve(dir, '.exact/test/db.sqlite'), 'utf8'), 'held');
      assert.equal(existsSync(resolve(dir, '.exact/test/owner')), true);
      assert.equal(given, 0);
    } finally { stuck = false; signal.mockRestore(); date.mockRestore(); await running.stop(); }
    assert.equal(groupAlive(children[0]), false);
    assert.equal(given, 1);
  `);
}, 30000);

test('failed initial startup retains the port turn when shutdown also fails', () => {
  stubServerRun(`
    delete manifest.server; save();
    // It never answers, and its group outlives SIGKILL.
    writeFileSync(resolve(dir, 'server.ts'), "import { writeFileSync } from 'node:fs'; process.on('SIGTERM', () => {}); writeFileSync(process.env.DB, 'held'); setInterval(() => {}, 1000);\\n");
    const db = resolve(dir, '.exact/test/db.sqlite'), kill = process.kill.bind(process), now = Date.now.bind(Date);
    let stuck = true, time = now();
    const signal = spyOn(process, 'kill').mockImplementation((pid, kind) => stuck && kind === 'SIGKILL' ? true : kill(pid, kind));
    spyOn(Date, 'now').mockImplementation(() => existsSync(db) ? time += 11000 : now());
    try {
      await assert.rejects(startServer(dir, appServer(dir), 'test'), /shutdown did not finish/);
      assert.equal(readFileSync(db, 'utf8'), 'held');
      assert.equal(existsSync(resolve(dir, '.exact/test/owner')), true);
      assert.equal(given, 0, 'a surviving launch still holds the port turn');
    } finally { stuck = false; signal.mockRestore(); }
  `);
});

test('shutdown probes only the managed address when another address holds the same port', () => {
  stubServerRun(`
    const other = Bun.serve({hostname:'::1', port:0, fetch:()=>new Response('other')});
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
    await crash(running, 'SIGTERM');
    try {
      assert.throws(()=>running.check(), /exited.*SIGTERM.*during the test/);
      assert.equal(running.exited(), 143);
      rmSync(ready);
      await running.beforeTest();
      assert.equal(children.length, 2, 'a signal exit before the first test also restarts');
    } finally { await running.stop(); }
  `);
});

test('a signal exit during startup reports death without waiting for the readiness deadline', () => {
  stubServerRun(`
    writeFileSync(resolve(dir, 'server.ts'), 'process.kill(process.pid, "SIGTERM");');
    const started = Date.now();
    await assert.rejects(startServer(dir, appServer(dir), 'test'), /exited.*SIGTERM.*before it answered/);
    assert.ok(Date.now() - started < 5000, 'the death ends readiness before its 10 s deadline');
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

// Windows: the supervisor and the kernel32 job calls (processes.mjs) are substituted; the job is a table entry.
const windowsJobs = `
    Object.defineProperty(process, 'platform', {value:'win32'});
    mock.module('node:child_process',()=>({...processes,spawnSync:(cmd)=>{throw new Error('no process-table query or taskkill: '+cmd);}}));
    mock.module('node:net',()=>({createServer:()=>({once(){},listen(options,done){done();},close(done){done();}})}));
    const real=await import(resolve(scripts,'processes.mjs'));
    // jobs: name to {active, stuck}; a handle is the name. joins: the supervisor creates its job and says it started.
    const jobs=new Map(), calls=[]; let joins=true, finish, out, launches=[], launched=null;
    const child={pid:65000,exitCode:null,signalCode:null,stdout:new ReadableStream({start(c){out=c;}}),stderr:new ReadableStream({start(c){c.close();}}),
      exited:new Promise(done=>{finish=done;}),kill(signal){calls.push('kill '+signal);exit();}};
    const exit=()=>{if(child.exitCode!=null)return;child.exitCode=1;out.close();finish(1);};
    spyOn(Bun,'spawn').mockImplementation((args,options)=>{
      launches.push({args,options});
      launched='exact-app-server-'+args[5];
      if(joins){jobs.set(launched,{active:3,stuck:false});out.enqueue(new TextEncoder().encode('{"started":true,"group":65000}\\n'));}
      return child;
    });
    mock.module(resolve(scripts,'processes.mjs'),()=>({...real,
      openJob:(name)=>{calls.push('open');return jobs.has(name)?name:null;},
      jobActive:(handle)=>jobs.get(handle).active,
      jobHas:(handle,pid)=>jobs.get(handle)?.holds===pid,
      endJob:(handle)=>{calls.push('end');const job=jobs.get(handle);if(job.stuck)return;job.active=0;if(handle===launched)exit();},
      // The job goes with its last handle, once it has no process left.
      closeJob:(handle)=>{calls.push('close');if(!jobs.get(handle)?.active)jobs.delete(handle);},
    }));
`;

test.each(['test', 'drive'])('Windows %s runs the server in its supervisor\'s job and stops it by ending the job', (mode) => {
  stubServerRun(windowsJobs + `
    let probes=0;
    spyOn(globalThis,'fetch').mockImplementation(async()=>{if(probes++===0)throw new Error('not yet');return new Response('ok');});
    const running=await ${mode === 'test' ? "startServer(dir,appServer(dir),'test')" : "driveServer(dir,'one',()=>{})"};
    assert.equal(launches.length,1);
    assert.equal(launches[0].args[2],'supervise');
    assert.equal(launches[0].args[3],${mode === 'test' ? "'1'" : "'0'"},'a drive\\'s server has no owner');
    assert.equal(launches[0].options.stdin,${mode === 'test' ? "'pipe'" : "'ignore'"},'an owner\\'s end ends its server through the supervisor\\'s stdin');
    assert.equal(launches[0].options.detached,true,'Bun\\'s own job would end the supervisor with this process');
    assert.deepEqual(calls,['open'],'the job is opened once its supervisor has started the server');
    await running.stop();
    assert.deepEqual(calls.slice(1),['end','close'],'one call ends every member; the handle closes after');
    assert.equal(given,${mode === 'test' ? 1 : 0});
  `);
});

test.each(['test', 'drive'])('Windows %s refuses a launch whose supervisor exits before its job exists', (mode) => {
  stubServerRun(windowsJobs + `
    spyOn(globalThis,'fetch').mockImplementation(async()=>{throw new Error('not ready');});
    joins=false; setTimeout(exit,50);
    await assert.rejects(${mode === 'test' ? "startServer(dir,appServer(dir),'test')" : "driveServer(dir,'one',()=>{})"}, /exited \\(code 1\\) before it answered/);
    assert.equal(existsSync(resolve(dir,'.exact/test/owner')),false);
    assert.equal(given,${mode === 'test' ? 1 : 0});
    assert.equal(existsSync(resolve(dir,'.exact/agent-server')),false);
  `);
});

test('Windows: the last drive out stops a server another drive started, by the job its record names', () => {
  stubServerRun(windowsJobs + `
    // The drive that started it has gone. Its record names supervisor 65001, whose job holds the listener, pid 7000.
    const lease=resolve(dir,'.exact/agent-server'), {realpathSync}=await import('node:fs');
    mkdirSync(lease,{recursive:true});
    writeFileSync(resolve(lease,'server-65001'),JSON.stringify({directory:realpathSync(dir),origin:'http://127.0.0.1:4790',db:null,token:'earlier'}));
    jobs.set('exact-app-server-earlier',{active:3,stuck:false,holds:7000});
    mock.module('node:child_process',()=>({...processes,spawnSync:(cmd,args)=>{
      assert.equal(cmd,'powershell.exe');
      const query=args.at(-1);
      if(query.includes('Get-NetTCPConnection'))return {status:0,stdout:'7000\\r\\n'};
      if(query.includes('ProcessId=65001'))return {status:0,stdout:'bun processes.mjs supervise 0 '+lease+' bun server.ts\\r\\n'};
      return {status:0,stdout:''};
    }}));
    const kill=process.kill.bind(process);
    spyOn(process,'kill').mockImplementation((pid,signal)=>pid===65001&&!signal?true:kill(pid,signal));
    spyOn(globalThis,'fetch').mockImplementation(async()=>new Response('ok'));
    const running=await driveServer(dir,'one',()=>{});
    assert.equal(launches.length,0,'the drive uses the server that answers');
    await running.stop();
    assert.equal(jobs.get('exact-app-server-earlier'),undefined,'its job is ended and closed');
    assert.deepEqual(calls,['open','close','open','end','close'],'the record\\'s job is found, then opened to be ended');
    assert.equal(existsSync(lease),false);
  `);
});

test('Windows shutdown that a job member outlives keeps the database, owner and port turn, and can be retried', () => {
  stubServerRun(windowsJobs + `
    delete manifest.server; save();
    writeFileSync(resolve(dir,'server.ts'),'const db=process.env.DB;');
    let probes=0;
    spyOn(globalThis,'fetch').mockImplementation(async()=>{if(probes++===0)throw new Error('not yet');return new Response('ok');});
    const running=await startServer(dir,appServer(dir),'test'), db=resolve(dir,'.exact/test/db.sqlite');
    writeFileSync(db,'held');
    jobs.get(launched).stuck=true;
    const now=Date.now.bind(Date); let time=now();
    const date=spyOn(Date,'now').mockImplementation(()=>time+=500);
    try {
      await assert.rejects(running.stop(),/shutdown did not finish: the job of supervisor 65000 is still alive/);
      assert.equal(readFileSync(db,'utf8'),'held');
      assert.equal(existsSync(resolve(dir,'.exact/test/owner')),true);
      assert.equal(given,0,'failed shutdown retains the port turn');
      assert.equal(calls.includes('close'),false,'the handle stays open for a retry');
    } finally { date.mockRestore(); jobs.get(launched).stuck=false; await running.stop(); }
    assert.equal(existsSync(db),false);
    assert.equal(given,1);
    assert.equal(calls.at(-1),'close');
  `);
});

test('shutdown stops a worker the server forks while handling SIGTERM, after the server exits', () => {
  stubServerRun(`
    delete manifest.server; save();
    writeFileSync(resolve(dir, 'worker.ts'), "import { writeFileSync } from 'node:fs'; process.on('SIGTERM', () => {}); writeFileSync(process.env.DB, 'worker'); setInterval(() => {}, 1000);\\n");
    // The server exits as soon as its worker is forked, before any observation of the process table could see it.
    writeFileSync(resolve(dir, 'server.ts'), "const db = process.env.DB;\\nprocess.on('SIGTERM', () => { writeFileSync('worker.pid', String(Bun.spawn([process.execPath, 'worker.ts'], { stdio: ['ignore', 'ignore', 'inherit'] }).pid)); process.exit(0); });\\n" + serving);
    const running = await startServer(dir, appServer(dir), 'test'), db = resolve(dir, '.exact/test/db.sqlite');
    await running.stop();
    const worker = Number(readFileSync(resolve(dir, 'worker.pid'), 'utf8'));
    assert.ok(await ended(worker), 'the worker is stopped with its server');
    assert.equal(groupAlive(children[0]), false);
    assert.equal(existsSync(db), false, 'its database is removed after it ends');
    assert.equal(existsSync(resolve(dir, '.exact/test/owner')), false);
    assert.equal(given, 1);
    assert.equal(await children[0].exited, 0, 'the supervisor reports the server ending on its own');
  `);
}, 30_000);

test('a server group outlives its exited leader under its supervisor, and is refused without one', () => {
  stubServerRun(`
    delete manifest.server; save();
    // The launcher exits once its worker answers, leaving the worker in its group.
    writeFileSync(resolve(dir, 'worker.ts'), "const db = process.env.DB; process.on('SIGTERM', () => {}); writeFileSync('worker.pid', String(process.pid));\\n" + serving);
    writeFileSync(resolve(dir, 'server.ts'), "const db = process.env.DB;\\nBun.spawn([process.execPath, 'worker.ts'], { stdio: ['ignore', 'ignore', 'inherit'] }).unref(); await Bun.sleep(1000); process.exit(0);\\n");
    const running = await startServer(dir, appServer(dir), 'test'), db = resolve(dir, '.exact/test/db.sqlite');
    while (running.exited() == null) await Bun.sleep(10);
    const worker = Number(readFileSync(resolve(dir, 'worker.pid'), 'utf8'));
    writeFileSync(db, 'held');
    // The group's id is the supervisor's pid, which no other process can take while it runs.
    assert.throws(() => running.check(), /exited \\(code 0\\) during the test/);
    await running.stop();
    assert.ok(await ended(worker), 'the worker left behind is stopped');
    assert.equal(existsSync(db), false);
    assert.equal(given, 1);

    rmSync(ready); rmSync(resolve(dir, 'worker.pid'));
    const again = await startServer(dir, appServer(dir), 'test');
    while (again.exited() == null) await Bun.sleep(10);
    writeFileSync(db, 'held');
    // Without its supervisor, a group of that id may be another program's.
    process.kill(groupOf(children[1]), 'SIGKILL'); await children[1].exited;
    try {
      await assert.rejects(again.stop(), /cannot verify ownership of process group \\d+: its keeper exited/);
      assert.equal(readFileSync(db, 'utf8'), 'held');
      assert.equal(existsSync(resolve(dir, '.exact/test/owner')), true);
      assert.equal(given, 1, 'uncertain ownership retains the port turn');
    } finally { process.kill(-groupOf(children[1]), 'SIGKILL'); while (groupAlive(children[1])) await Bun.sleep(10); }
    // On Linux the supervisor also held what left the group, and it ended without saying it had ended that.
    if (process.platform === 'linux') {
      await assert.rejects(again.stop(), /cannot verify that the keeper of process group \\d+ ended every process the server started/);
      assert.equal(given, 1);
    } else { await again.stop(); assert.equal(given, 2); }
  `);
}, 30_000);

test('shutdown stops a launcher that respawns its worker, and the worker of a parent that exited at once', () => {
  stubServerRun(`
    delete manifest.server; save();
    const keep = "process.on('SIGTERM', () => {}); setInterval(() => {}, 1000);";
    writeFileSync(resolve(dir, 'orphan.ts'), "import { writeFileSync } from 'node:fs'; writeFileSync('orphan.pid', String(process.pid)); " + keep + "\\n");
    // The orphan's parent exits as soon as it has forked, before the server answers.
    writeFileSync(resolve(dir, 'parent.ts'), "Bun.spawn([process.execPath, 'orphan.ts'], { stdio: ['ignore', 'ignore', 'inherit'] }).unref(); process.exit(0);\\n");
    writeFileSync(resolve(dir, 'worker.ts'), "import { appendFileSync } from 'node:fs'; appendFileSync('workers', process.pid + '\\\\n'); " + keep + "\\n");
    writeFileSync(resolve(dir, 'server.ts'), "const db = process.env.DB; process.on('SIGTERM', () => {});\\nBun.spawn([process.execPath, 'parent.ts'], { stdio: ['ignore', 'ignore', 'inherit'] });\\nconst work = () => Bun.spawn([process.execPath, 'worker.ts'], { stdio: ['ignore', 'ignore', 'inherit'], onExit: work });\\nwork();\\n" + serving);
    const workers = () => existsSync(resolve(dir, 'workers')) ? readFileSync(resolve(dir, 'workers'), 'utf8').trim().split('\\n').map(Number) : [];
    const running = await startServer(dir, appServer(dir), 'test');
    while (!existsSync(resolve(dir, 'orphan.pid')) || !workers().length) await Bun.sleep(10);
    process.kill(workers()[0], 'SIGKILL');
    while (workers().length < 2) await Bun.sleep(10);
    await running.stop();
    for (const pid of [Number(readFileSync(resolve(dir, 'orphan.pid'), 'utf8')), ...workers()])
      assert.ok(await ended(pid), 'process ' + pid + ' is stopped with its server');
    assert.equal(groupAlive(children[0]), false);
    assert.equal(existsSync(resolve(dir, '.exact/test/db.sqlite')), false);
    assert.equal(given, 1);
  `);
}, 30_000);

test.skipIf(process.platform !== 'linux').each(['a worker', 'the server'])('on Linux shutdown stops %s that left the server\'s process group', (who) => {
  stubServerRun(`
    // setsid: a new session and process group, as a daemon makes (Bun's detached).
    const daemon = "import { writeFileSync } from 'node:fs'; process.on('SIGTERM', () => {}); writeFileSync('daemon.pid', String(process.pid));\\n";
    if (${JSON.stringify(who)} === 'a worker') {
      writeFileSync(resolve(dir, 'daemon.ts'), daemon + "setInterval(() => {}, 1000);\\n");
      writeFileSync(resolve(dir, 'server.ts'), "Bun.spawn([process.execPath, 'daemon.ts'], { detached: true, stdio: ['ignore', 'ignore', 'inherit'] }).unref();\\n" + serving);
    } else {
      writeFileSync(resolve(dir, 'server.ts'), daemon + serving);
      manifest.commands.server = ['setsid', ...manifest.commands.server]; save();
    }
    const running = await startServer(dir, appServer(dir), 'test');
    while (!existsSync(resolve(dir, 'daemon.pid'))) await Bun.sleep(10);
    const pid = Number(readFileSync(resolve(dir, 'daemon.pid'), 'utf8'));
    try {
      assert.notEqual(Number(spawnSyncReal('ps', ['-o', 'pgid=', '-p', String(pid)], { encoding: 'utf8' }).stdout.trim()), groupOf(children[0]), 'it left the group');
      await running.stop();
      assert.ok(await ended(pid), 'the supervisor, its subreaper, stops it');
      assert.equal(given, 1);
    } finally { try { process.kill(pid, 'SIGKILL'); } catch {} }
  `);
}, 30_000);

// A real owner in a process group of its own, whose whole group is then killed, as a CI timeout kills a job's.
const ownerProcess = `
    const ownerScript = (call) => [
      "import { mock, spyOn } from 'bun:test';",
      "import { existsSync, writeFileSync } from 'node:fs';",
      "mock.module(" + JSON.stringify(scripts + '/port-lock.mjs') + ", () => ({ holderOf: async () => null, takePort: async () => () => {} }));",
      "spyOn(globalThis, 'fetch').mockImplementation(async () => { if (!existsSync('ready.json')) throw new Error('not ready'); return new Response('ready'); });",
      "const { appServer, startServer, driveServer } = await import(" + JSON.stringify(scripts + '/verify.mjs') + ");",
      "await " + call + ";",
      "writeFileSync('owned', 'yes'); setInterval(() => {}, 1000);",
    ].join('\\n');
    const killedOwner = async (call) => {
      writeFileSync(resolve(dir, 'owner.mjs'), ownerScript(call));
      const owner = Bun.spawn([process.execPath, 'owner.mjs'], { cwd: dir, detached: true, stdio: ['ignore', 'ignore', 'inherit'] });
      while (!existsSync(resolve(dir, 'owned'))) { assert.equal(owner.exitCode, null, 'the owner runs'); await Bun.sleep(10); }
      process.kill(-owner.pid, 'SIGKILL'); await owner.exited;
    };
    writeFileSync(resolve(dir, 'worker.ts'), "import { writeFileSync } from 'node:fs'; process.on('SIGTERM', () => {}); writeFileSync('worker.pid', String(process.pid)); setInterval(() => {}, 1000);\\n");
    const worker = () => Number(readFileSync(resolve(dir, 'worker.pid'), 'utf8'));
`;

test('a test server ends when its owner\'s whole process group is killed, and the next run starts clean', () => {
  stubServerRun(ownerProcess + `
    delete manifest.server; save();
    writeFileSync(resolve(dir, 'server.ts'), "const db = process.env.DB;\\nBun.spawn([process.execPath, 'worker.ts'], { stdio: ['ignore', 'ignore', 'inherit'] });\\n" + serving);
    await killedOwner("startServer(process.cwd(), appServer(process.cwd()), 'test')");
    assert.ok(await ended(worker()), 'the supervisor, in a session of its own, ends the server when its owner is gone');
    const run = resolve(dir, '.exact/test');
    for (let i = 0; i < 100 && !readdirSync(run).some((f) => f.startsWith('ended-')); i++) await Bun.sleep(20);
    assert.ok(readdirSync(run).some((f) => f.startsWith('ended-')), 'its keeper records the end');
    rmSync(ready);
    const running = await startServer(dir, appServer(dir), 'test');
    await running.stop();
    assert.equal(given, 1);
    assert.deepEqual(existsSync(resolve(dir, '.exact/test')) ? readdirSync(resolve(dir, '.exact/test')) : [], []);
  `);
}, 30_000);

// Unrecorded only on Linux: on macOS the keeper holds nothing outside the group, so no record is asked for.
test.each(process.platform === 'linux' ? ['recorded', 'unrecorded'] : ['recorded'])('a drive\'s server outlives its drive\'s killed process group; the next drive stops it, its end %s', (record) => {
  stubServerRun(ownerProcess + `
    // The listener lets the next drive prove the server is this checkout's (lsof, the group, the keeper's cwd).
    writeFileSync(resolve(dir, 'server.ts'), "Bun.serve({ hostname: '127.0.0.1', port: Number(process.env.PORT), fetch: () => new Response('ok') });\\nBun.spawn([process.execPath, 'worker.ts'], { stdio: ['ignore', 'ignore', 'inherit'] });\\n" + serving);
    await killedOwner("driveServer(process.cwd(), 'one', () => {})");
    const lease = resolve(dir, '.exact/agent-server'), group = Number(readdirSync(lease).find((f) => f.startsWith('server-')).slice(7));
    try {
      await Bun.sleep(300);
      process.kill(worker(), 0);
      if (${JSON.stringify(record)} === 'unrecorded') mkdirSync(resolve(lease, 'ended-' + group));
      const running = await driveServer(dir, 'one', () => {});
      if (${JSON.stringify(record)} === 'unrecorded') {
        // The keeper cannot write its record: what it held outside the group is not known to have ended.
        await assert.rejects(running.stop(), new RegExp('cannot verify that the keeper of process group ' + group + ' ended every process'));
      } else {
        await running.stop();
        assert.ok(await ended(worker()), 'the last drive out stops it');
        assert.equal(existsSync(lease), false, 'its record of the end read and removed with the lease');
      }
    } finally { for (const pid of [-group, worker()]) try { process.kill(pid, 'SIGKILL'); } catch {} }
  `);
}, 30_000);

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
        await crash(running, signal);
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
      await crash(server, 'SIGKILL');
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
        if(++tests===2){const keeper=Number(require('node:child_process').spawnSync('pgrep',['-P',String(children[0].pid)],{encoding:'utf8'}).stdout.trim());process.kill(-keeper,'SIGKILL');while(running.exited()==null)await Bun.sleep(10);}
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
