// The server address a drive records for the app's server (scripts/verify.mjs `recordedServerAddress`): the URL
// appServer accepted, kept as written, its port read as the URL's.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, readdirSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { appServer, driveServer, recordedServerAddress } from './verify.mjs';

test('recorded server addresses preserve accepted default ports and paths, refusing credentials and queries', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-server-address-'));
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ server: { database: false } }));
    writeFileSync(resolve(dir, 'server.ts'), 'Bun.serve({});\n');
    for (const [url, port] of [
      ['http://127.0.0.1:80', 80], ['http://127.0.0.1:080', 80], ['https://localhost:443', 443],
      ['https://[::1]:0443/api', 443], ['http://localhost:80/health', 80], ['http://127.0.0.1:8080/api/v1/', 8080],
    ]) {
      writeFileSync(resolve(dir, 'app.ts'), `export const API = ${JSON.stringify(url)};\n`);
      const accepted = appServer(dir, {});
      assert.equal(accepted.url, url.replace(/\/$/, ''));
      assert.equal(accepted.port, port);
      assert.deepEqual(recordedServerAddress(accepted.url), { url: accepted.url, port });
    }
    for (const url of [null, {}, 'file:///tmp/server', 'ftp://localhost:80', 'http://user:pw@localhost:80',
      'http://@localhost:80', 'http://localhost:80/?', 'http://localhost:80/?q=x', 'http://localhost:80/#',
      'http://localhost:80/#x', 'http://:80', 'http://localhost:65536']) assert.equal(recordedServerAddress(url), null);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('a fresh drive refuses an answering orphan server without ending it or keeping a lease', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-server-orphan-'));
  const probe = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch: () => new Response('') }), port = probe.port;
  probe.stop(true);
  const origin = `http://127.0.0.1:${port}`, lease = resolve(dir, '.exact/agent-server');
  const db = resolve(dir, 'db.sqlite');
  writeFileSync(resolve(dir, 'app.ts'), `export const API = '${origin}';\n`);
  writeFileSync(db, 'kept');
  writeFileSync(resolve(dir, 'server.ts'), `const db = process.env.DB;
    Bun.serve({ hostname: '127.0.0.1', port: ${port}, fetch: () => new Response(db) });
    console.log('ready');`);
  const child = Bun.spawn([process.execPath, 'server.ts'], { cwd: dir, env: { ...process.env, DB: db }, stdout: 'pipe', stderr: 'inherit' });
  try {
    const lines = child.stdout.values()[Symbol.asyncIterator]();
    assert.equal(new TextDecoder().decode((await lines.next()).value).trim(), 'ready');
    mkdirSync(lease, { recursive: true });
    const record = `server-${child.pid}`, contents = JSON.stringify({ directory: realpathSync(dir), origin, db });
    writeFileSync(resolve(lease, record), contents);
    const events = ['exit', 'SIGINT', 'SIGTERM', 'SIGHUP'], listeners = events.map(e => process.listenerCount(e));
    await assert.rejects(driveServer(dir, 'fresh', () => {}, { fresh: true }), /but something already answers at/);
    assert.deepEqual(events.map(e => process.listenerCount(e)), listeners);
    assert.equal(child.exitCode, null, 'refusal leaves the orphan server alive');
    assert.equal(await fetch(origin).then(r => r.text()), db);
    assert.deepEqual(readdirSync(lease), [record]);
    assert.equal(readFileSync(resolve(lease, record), 'utf8'), contents);
    assert.equal(readFileSync(db, 'utf8'), 'kept');
  } finally {
    child.kill();
    await child.exited;
    rmSync(dir, { recursive: true, force: true });
  }
}, 10_000);
