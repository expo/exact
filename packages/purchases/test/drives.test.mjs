import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { relative, resolve } from 'node:path';

const root = resolve(import.meta.dir, '../../..');

test.skipIf(process.platform === 'win32')('the native forecast fixture uses the baker in CARGO_TARGET_DIR', () => {
  const target = resolve(root, 'target');
  mkdirSync(target, { recursive: true });
  const dir = mkdtempSync(resolve(target, 'purchases-baker-'));
  const called = resolve(dir, 'called.json');
  try {
    const bin = resolve(dir, 'host-dev/exact-js-bake');
    mkdirSync(resolve(dir, 'host-dev'));
    writeFileSync(bin, `#!${process.execPath}\nawait Bun.write(process.env.PURCHASES_BAKE_CALL, JSON.stringify(process.argv.slice(2)));\n`);
    chmodSync(bin, 0o755);
    // The drive asks Cargo to bring the baker up to date first; this cargo records the request and builds nothing.
    const cargo = resolve(dir, 'bin/cargo'), built = resolve(dir, 'cargo.json');
    mkdirSync(resolve(dir, 'bin'));
    writeFileSync(cargo, `#!${process.execPath}\nawait Bun.write(${JSON.stringify(built)}, JSON.stringify(process.argv.slice(2)));\n`);
    chmodSync(cargo, 0o755);
    for (const cargoTarget of [dir, relative(root, dir)]) {
      // With no drive selected, setup bakes the fixture without launching an app.
      const result = spawnSync(process.execPath, ['packages/purchases/test/drives.mjs', 'macos', '--only', '0'], {
        cwd: root, encoding: 'utf8',
        env: { ...process.env, PATH: `${resolve(dir, 'bin')}:${process.env.PATH}`, CARGO_TARGET_DIR: cargoTarget, PURCHASES_BAKE_CALL: called },
      });
      assert.equal(result.status, 0, result.stdout + result.stderr);
      assert.deepEqual(JSON.parse(readFileSync(built, 'utf8')).slice(-2), ['-p', 'exact-js-bake'], 'the drive did not build the baker');
      rmSync(built);
      assert.ok(existsSync(called), 'the forecast fixture did not use the private baker');
      const args = JSON.parse(readFileSync(called, 'utf8'));
      assert.equal(args[1], '--out');
      assert.equal(args[2], resolve(args[0], '../baked'));
      rmSync(called);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60000);
