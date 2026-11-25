import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';

test.skipIf(process.platform !== 'darwin')('macOS readText selects the same first plain-text item as read', () => {
  const root = resolve(import.meta.dir, '../../..');
  mkdirSync(join(root, 'target'), { recursive: true });
  const out = mkdtempSync(join(root, 'target/clipboard-read-'));
  const run = (command, args) => {
    const result = spawnSync(command, args, { cwd: root, env: { ...process.env, EXACT_DEVICE: '' }, encoding: 'utf8', timeout: 120_000 });
    expect(result.status, `${command}: ${result.error ?? result.signal ?? ''}\n${result.stdout}\n${result.stderr}`).toBe(0);
  };
  try {
    run('xcrun', ['swiftc', '-parse-as-library', '-emit-library', '-static', '-module-name', 'ExactPackage',
      '-emit-module', '-emit-module-path', join(out, 'ExactPackage.swiftmodule'),
      'host/apple/modules/ExactPackage.swift', '-o', join(out, 'libExactPackage.a')]);
    run('xcrun', ['swiftc', '-parse-as-library', '-I', out, '-L', out, '-lExactPackage',
      'packages/clipboard/apple/Clipboard.swift', 'packages/clipboard/test/Read.swift', '-o', join(out, 'read')]);
    run(join(out, 'read'), []);
  } finally { rmSync(out, { recursive: true, force: true }); }
}, 240_000);
