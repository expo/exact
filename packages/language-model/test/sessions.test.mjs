import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';

test.skipIf(process.platform !== 'darwin')('the Apple module keeps a session\'s bookkeeping across responses', () => {
  const root = resolve(import.meta.dir, '../../..');
  mkdirSync(join(root, 'target'), { recursive: true });
  const out = mkdtempSync(join(root, 'target/language-model-sessions-'));
  const run = (command, args) => {
    const result = spawnSync(command, args, { cwd: root, encoding: 'utf8', timeout: 120_000 });
    assert.equal(result.status, 0, `${command}: ${result.error ?? result.signal ?? ''}\n${result.stdout}\n${result.stderr}`);
  };
  try {
    run('xcrun', ['swiftc', '-parse-as-library', '-emit-library', '-static', '-module-name', 'ExactPackage',
      '-emit-module', '-emit-module-path', join(out, 'ExactPackage.swiftmodule'),
      'host/apple/modules/ExactPackage.swift', '-o', join(out, 'libExactPackage.a')]);
    const apple = resolve(import.meta.dir, '../apple');
    run('xcrun', ['swiftc', '-parse-as-library', '-I', out, '-L', out, '-lExactPackage',
      ...['LanguageModel.swift', 'LanguageModelApple.swift', 'LanguageModelSchema.swift'].map(f => join(apple, f)),
      resolve(import.meta.dir, 'Sessions.swift'), '-o', join(out, 'sessions')]);
    run(join(out, 'sessions'), []);
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
}, 240_000);
