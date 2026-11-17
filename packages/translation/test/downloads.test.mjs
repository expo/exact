import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';

test.skipIf(process.platform !== 'darwin')('a pair has one download sheet, which each create for it waits on', () => {
  const root = resolve(import.meta.dir, '../../..');
  mkdirSync(join(root, 'target'), { recursive: true });
  const out = mkdtempSync(join(root, 'target/translation-downloads-'));
  const run = (command, args) => {
    const result = spawnSync(command, args, { cwd: root, encoding: 'utf8', timeout: 120_000 });
    assert.equal(result.status, 0, `${command}: ${result.error ?? result.signal ?? ''}\n${result.stdout}\n${result.stderr}`);
  };
  try {
    run('xcrun', ['swiftc', '-parse-as-library', '-emit-library', '-static', '-module-name', 'ExactPackage',
      '-emit-module', '-emit-module-path', join(out, 'ExactPackage.swiftmodule'),
      'host/apple/modules/ExactPackage.swift', '-o', join(out, 'libExactPackage.a')]);
    // A same-file extension reaches the module's private download bookkeeping.
    const apple = resolve(import.meta.dir, '../apple');
    const source = readFileSync(join(apple, 'Translation.swift'), 'utf8');
    const checks = readFileSync(resolve(import.meta.dir, 'Downloads.swift'), 'utf8');
    writeFileSync(join(out, 'Translation.swift'), `${source}\n${checks}`);
    run('xcrun', ['swiftc', '-parse-as-library', '-I', out, '-L', out, '-lExactPackage',
      join(out, 'Translation.swift'), join(apple, 'Hosted.swift'), join(apple, 'Languages.swift'), '-o', join(out, 'downloads')]);
    run(join(out, 'downloads'), []);
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
}, 240_000);
