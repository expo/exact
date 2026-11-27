import { expect, test } from 'bun:test';
import { spawn } from 'node:child_process';
import { chmodSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { fileURLToPath } from 'node:url';

const ROOT = fileURLToPath(new URL('..', import.meta.url));

// verify runs `contract verify` beside the device run, and its signal listener took the place of the signal's
// default: Ctrl-C ended only that child, and verify carried the device run to its end. It ends now, with the code.
test('a signal while contract verify runs ends verify with the signal\'s code', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-verify-signal-'));
  try {
    // Keep the spawned child alive deterministically without racing cargo startup or fast exit.
    const fakeCargo = join(dir, 'cargo');
    writeFileSync(fakeCargo, '#!/bin/sh\nsleep 30\n');
    chmodSync(fakeCargo, 0o755);
    const script = `import { contractVerify } from './scripts/verify-semantics.mjs';
contractVerify({ dir: '/nonexistent-app' }, false);
process.stdout.write('ready\\n');
setInterval(() => {}, 1000); // the device run, still going`;
    const child = spawn(process.execPath, ['-e', script], {
      cwd: ROOT,
      env: { ...process.env, PATH: `${dir}:${process.env.PATH ?? ''}` },
      stdio: ['ignore', 'pipe', 'ignore'],
    });
    const ended = new Promise((done) => child.on('exit', (code, signal) => done({ code, signal })));
    await new Promise((ready, fail) => {
      let out = '';
      child.stdout.on('data', (chunk) => {
        out += chunk.toString();
        if (out.includes('ready\n')) ready();
      });
      child.on('exit', (code, signal) => fail(new Error(`child exited before ready: code=${code} signal=${signal}`)));
    });
    child.kill('SIGINT');
    const result = await Promise.race([ended, Bun.sleep(15000).then(() => 'still running')]);
    if (result === 'still running') child.kill('SIGKILL');
    expect(result).toEqual({ code: 130, signal: null });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}, 30000);
