import { test, expect } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { claimDevLoop } from './dev-startup.mjs';
import { discoverDevLoop, parseLoopbackDevURL } from './dev-discovery.mjs';

test('parseLoopbackDevURL accepts loopback http URLs and rejects remote or non-http URLs', () => {
  expect(parseLoopbackDevURL('http://127.0.0.1:8765')).toEqual({ href: 'http://127.0.0.1:8765/', host: '127.0.0.1', port: 8765 });
  expect(parseLoopbackDevURL('http://localhost:9000/foo')).toEqual({ href: 'http://localhost:9000/', host: '127.0.0.1', port: 9000 });
  expect(parseLoopbackDevURL('http://[::1]:8765/')).toEqual({ href: 'http://[::1]:8765/', host: '::1', port: 8765 });
  expect(parseLoopbackDevURL('https://127.0.0.1:8765/')).toBeNull();
  expect(parseLoopbackDevURL('http://192.168.1.5:8765/')).toBeNull();
  expect(parseLoopbackDevURL(null)).toBeNull();
});

test('discoverDevLoop requires a held OS flock, matching PID, and listening loopback server', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-dev-discover-'));
  const server = createServer();
  await new Promise((ok) => server.listen(0, '127.0.0.1', ok));
  const { port } = server.address();
  let claim = null;
  try {
    claim = claimDevLoop(dir, 'js');
    // Before URL is written (starting state): ignored.
    expect(await discoverDevLoop(dir)).toBeNull();
    claim.url(`http://127.0.0.1:${port}`);
    // Held lock + live PID + listening port: discovered.
    const found = await discoverDevLoop(dir);
    expect(found).toEqual({ target: 'js', pid: process.pid, url: `http://127.0.0.1:${port}/`, port });
    // Listening check rejects closed port even if lock is held.
    expect(await discoverDevLoop(dir, { probeFn: async () => false })).toBeNull();
    // Simulate SIGKILL: release flock while leaving both JSON and .lock on disk with our live PID.
    claim.close();
    claim = null;
    mkdirSync(resolve(dir, '.exact'), { recursive: true });
    writeFileSync(resolve(dir, '.exact/web-js-loop.json'), JSON.stringify({ pid: process.pid, url: `http://127.0.0.1:${port}` }));
    writeFileSync(resolve(dir, '.exact/web-js-loop.json.lock'), `${process.pid}\n`);
    expect(await discoverDevLoop(dir)).toBeNull();
    // Symlink record is ignored.
    const target = resolve(dir, 'other.json');
    writeFileSync(target, JSON.stringify({ pid: process.pid, url: `http://127.0.0.1:${port}` }));
    rmSync(resolve(dir, '.exact/web-js-loop.json'), { force: true });
    symlinkSync(target, resolve(dir, '.exact/web-js-loop.json'));
    expect(await discoverDevLoop(dir, { lockHeldFn: () => true })).toBeNull();
  } finally {
    claim?.close();
    server.close();
    rmSync(dir, { recursive: true, force: true });
  }
});

test('discoverDevLoop prefers JS loop over wasm loop', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-dev-pref-'));
  let jsClaim = null, wasmClaim = null;
  try {
    wasmClaim = claimDevLoop(dir, 'wasm');
    wasmClaim.url('http://127.0.0.1:8766');
    expect((await discoverDevLoop(dir, { probe: false })).target).toBe('wasm');
    jsClaim = claimDevLoop(dir, 'js');
    jsClaim.url('http://127.0.0.1:8765');
    expect((await discoverDevLoop(dir, { probe: false })).target).toBe('js');
  } finally {
    jsClaim?.close();
    wasmClaim?.close();
    rmSync(dir, { recursive: true, force: true });
  }
});
