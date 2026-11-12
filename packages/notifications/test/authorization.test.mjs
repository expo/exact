import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

test.skipIf(process.platform !== 'darwin')('posting asks only while notification authorization is undetermined', () => {
  const root = resolve(import.meta.dir, '../../..');
  mkdirSync(join(root, 'target'), { recursive: true });
  const out = mkdtempSync(join(root, 'target/notification-authorization-'));
  const run = (command, args) => {
    const result = spawnSync(command, args, { cwd: root, encoding: 'utf8', timeout: 60_000 });
    assert.equal(result.status, 0, `${command}: ${result.error ?? result.signal ?? ''}\n${result.stdout}\n${result.stderr}`);
    return result.stdout;
  };
  try {
    run('xcrun', ['swiftc', '-parse-as-library', '-emit-library', '-static', '-module-name', 'ExactPackage',
      '-emit-module', '-emit-module-path', join(out, 'ExactPackage.swiftmodule'),
      'host/apple/modules/ExactPackage.swift', '-o', join(out, 'libExactPackage.a')]);
    // A same-file extension exercises the private post without requiring an app bundle or system permission.
    const source = readFileSync(resolve(import.meta.dir, '../apple/Notifications.swift'), 'utf8');
    const checks = readFileSync(resolve(import.meta.dir, 'Authorization.swift'), 'utf8');
    writeFileSync(join(out, 'Authorization.swift'), `${source}\n${checks}`);
    run('xcrun', ['swiftc', '-parse-as-library', '-I', out, '-L', out, '-lExactPackage',
      join(out, 'Authorization.swift'), '-o', join(out, 'authorization')]);
    console.log(run(join(out, 'authorization'), []));
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
}, 120_000);
