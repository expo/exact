import { expect, test } from 'bun:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { need } from '../../../scripts/coverage.mjs';

const root = resolve(import.meta.dir, '../../..');
const swift = process.platform === 'darwin'
  ? spawnSync('xcrun', ['swiftc', '--version'], { encoding: 'utf8' })
  : null;
const check = need('swift', swift && swift.status !== 0
  ? (swift.stderr || swift.error?.message || `exit ${swift.status}`).trim()
  : null, { test: test.skipIf(process.platform !== 'darwin') });

function run(command, args) {
  const result = spawnSync(command, args, { cwd: root, encoding: 'utf8' });
  expect(result.status, `${result.stdout}${result.stderr}`).toBe(0);
  return result;
}

check('LocalNetworkModule handles per-waiter resolve timers, stopped browser handlers, and sync destroy', () => {
  const build = mkdtempSync(resolve(tmpdir(), 'exact-local-network-'));
  try {
    run('xcrun', ['swiftc', '-emit-library', '-emit-module', '-parse-as-library', '-module-name', 'ExactPackage',
      resolve(root, 'host/apple/modules/ExactPackage.swift'), '-o', resolve(build, 'libExactPackage.dylib')]);
    run('xcrun', ['swiftc', '-I', build, '-L', build, '-lExactPackage',
      resolve(root, 'packages/local-network/apple/LocalNetwork.swift'),
      resolve(import.meta.dir, 'apple-lifecycle.swift'), '-o', resolve(build, 'local-network-test')]);
    const result = spawnSync(resolve(build, 'local-network-test'), [], {
      cwd: root, encoding: 'utf8', env: { ...process.env, DYLD_LIBRARY_PATH: build },
    });
    expect(`${result.stdout}${result.stderr}`).toBe('local-network lifecycle ok\n');
    expect(result.status).toBe(0);
  } finally {
    rmSync(build, { recursive: true, force: true });
  }
}, 30000);
