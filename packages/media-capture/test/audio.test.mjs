import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, rmSync } from 'node:fs';
import { join, resolve } from 'node:path';

test.skipIf(process.platform !== 'darwin')('disabled audio tracks record silence independently of their shared input', () => {
  const root = resolve(import.meta.dir, '../../..');
  mkdirSync(join(root, 'target'), { recursive: true });
  const out = mkdtempSync(join(root, 'target/media-capture-audio-'));
  const run = (command, args) => {
    const result = spawnSync(command, args, { cwd: root, encoding: 'utf8', timeout: 120_000 });
    assert.equal(result.status, 0, `${command}: ${result.error ?? result.signal ?? ''}\n${result.stdout}\n${result.stderr}`);
  };
  try {
    run('xcrun', ['swiftc', '-parse-as-library', '-emit-library', '-static', '-module-name', 'ExactPackage',
      '-emit-module', '-emit-module-path', join(out, 'ExactPackage.swiftmodule'),
      'host/apple/modules/ExactPackage.swift', '-o', join(out, 'libExactPackage.a')]);
    run('xcrun', ['swiftc', '-sanitize=thread', '-parse-as-library', '-I', out, '-L', out, '-lExactPackage',
      'packages/media-capture/apple/MediaCapture.swift', 'packages/media-capture/apple/VideoCapture.swift',
      'packages/recording/apple/Recording.swift', 'packages/media-capture/test/Audio.swift', '-o', join(out, 'audio')]);
    run(join(out, 'audio'), [out]);
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
}, 240_000);
