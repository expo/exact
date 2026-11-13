import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';

test.skipIf(process.platform !== 'darwin')('an index that names no slot answers NotFoundError rather than trapping', () => {
  const root = resolve(import.meta.dir, '../../..');
  mkdirSync(join(root, 'target'), { recursive: true });
  const out = mkdtempSync(join(root, 'target/gamepad-indices-'));
  const run = (command, args) => {
    const result = spawnSync(command, args, { cwd: root, encoding: 'utf8', timeout: 60_000 });
    assert.equal(result.status, 0, `${command}: ${result.error ?? result.signal ?? ''}\n${result.stdout}\n${result.stderr}`);
  };
  try {
    run('xcrun', ['swiftc', '-parse-as-library', '-emit-library', '-static', '-module-name', 'ExactPackage',
      '-emit-module', '-emit-module-path', join(out, 'ExactPackage.swiftmodule'),
      'host/apple/modules/ExactPackage.swift', '-o', join(out, 'libExactPackage.a')]);
    run('xcrun', ['swiftc', '-parse-as-library', '-I', out, '-L', out, '-lExactPackage',
      resolve(import.meta.dir, '../apple/Gamepad.swift'), resolve(import.meta.dir, 'Indices.swift'), '-o', join(out, 'indices')]);
    run(join(out, 'indices'), []);
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
}, 120_000);
