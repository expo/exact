import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';

test.skipIf(process.platform !== 'darwin')('a poll answers or counts dropped every sample after its cursor', () => {
  const root = resolve(import.meta.dir, '../../..');
  mkdirSync(join(root, 'target'), { recursive: true });
  const out = mkdtempSync(join(root, 'target/altimeter-batch-'));
  const run = (command, args) => {
    const result = spawnSync(command, args, { cwd: root, encoding: 'utf8', timeout: 60_000 });
    assert.equal(result.status, 0, `${command}: ${result.error ?? result.signal ?? ''}\n${result.stdout}\n${result.stderr}`);
  };
  try {
    run('xcrun', ['swiftc', '-parse-as-library', '-emit-library', '-static', '-module-name', 'ExactPackage',
      '-emit-module', '-emit-module-path', join(out, 'ExactPackage.swiftmodule'),
      'host/apple/modules/ExactPackage.swift', '-o', join(out, 'libExactPackage.a')]);
    run('xcrun', ['swiftc', '-parse-as-library', '-I', out, '-L', out, '-lExactPackage',
      resolve(import.meta.dir, '../apple/Altimeter.swift'), resolve(import.meta.dir, 'Batch.swift'), '-o', join(out, 'batch')]);
    run(join(out, 'batch'), []);
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
}, 120_000);
