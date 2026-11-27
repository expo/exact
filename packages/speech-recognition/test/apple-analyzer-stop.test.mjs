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

check('AnalyzerEngine mid-flight stop, abort, and destroy settle and release the track lease', () => {
  const build = mkdtempSync(resolve(tmpdir(), 'exact-speech-analyzer-stop-'));
  try {
    run('xcrun', ['swiftc', '-emit-library', '-emit-module', '-parse-as-library', '-module-name', 'ExactPackage',
      resolve(root, 'host/apple/modules/ExactPackage.swift'), '-o', resolve(build, 'libExactPackage.dylib')]);
    run('xcrun', ['swiftc', '-I', build, '-L', build, '-lExactPackage',
      resolve(root, 'packages/speech-recognition/apple/SpeechRecognition.swift'),
      resolve(import.meta.dir, 'apple-analyzer-stop.swift'), '-o', resolve(build, 'analyzer-stop-test')]);
    const result = spawnSync(resolve(build, 'analyzer-stop-test'), [], {
      cwd: root, encoding: 'utf8', env: { ...process.env, DYLD_LIBRARY_PATH: build },
    });
    expect(`${result.stdout}${result.stderr}`).toBe('analyzer stop+abort ok\n');
    expect(result.status).toBe(0);
  } finally {
    rmSync(build, { recursive: true, force: true });
  }
}, 30000);
