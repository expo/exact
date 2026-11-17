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

function run(command, args, env = process.env) {
  const result = spawnSync(command, args, { cwd: root, env, encoding: 'utf8' });
  expect(`${result.stdout}${result.stderr}`).toBe(result.status === 0 ? `${result.stdout}${result.stderr}` : '');
  return result;
}

check('module destruction waits for an ended recognition transcript writer', () => {
  const build = mkdtempSync(resolve(tmpdir(), 'exact-speech-writer-'));
  try {
    run('xcrun', ['swiftc', '-emit-library', '-emit-module', '-parse-as-library', '-module-name', 'ExactPackage',
      resolve(root, 'host/apple/modules/ExactPackage.swift'), '-o', resolve(build, 'libExactPackage.dylib')]);
    run('xcrun', ['swiftc', '-I', build, '-L', build, '-lExactPackage',
      resolve(root, 'packages/speech-recognition/apple/SpeechRecognition.swift'),
      resolve(import.meta.dir, 'apple-writer-lifetime.swift'), '-o', resolve(build, 'writer-lifetime')]);
    const result = spawnSync(resolve(build, 'writer-lifetime'), [], {
      cwd: root, encoding: 'utf8', env: { ...process.env, DYLD_LIBRARY_PATH: build },
    });
    expect(`${result.stdout}${result.stderr}`).toBe('final transcript written; directory closed\n');
    expect(result.status).toBe(0);
  } finally {
    rmSync(build, { recursive: true, force: true });
  }
});
