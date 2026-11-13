// The server address a drive records for the app's server (scripts/verify.mjs `recordedServerAddress`): the URL
// appServer accepted, kept as written, its port read as the URL's.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { appServer, recordedServerAddress } from './verify.mjs';

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
