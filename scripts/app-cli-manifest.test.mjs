import { expect, test } from 'bun:test';
import { parseManifest } from './app-cli.mjs';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';

test('an outside app attaches on the same simulator with --sim or EXACT_SIM', () => {
  const root = resolve(import.meta.dir, '..'), parent = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-attach-cli-')));
  const dir = resolve(parent, 'app'), sdk = resolve(parent, 'sdk');
  try {
    mkdirSync(dir); mkdirSync(resolve(sdk, 'scripts'), { recursive: true });
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    text "Attach"\n');
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Attach', app: { id: 'com.example.attach', name: 'Attach' }, before_build: ['must-not-build'] }));
    writeFileSync(resolve(sdk, 'scripts/agent.mjs'), `
      import { parseFlags } from ${JSON.stringify(resolve(root, 'scripts/agent-launch.mjs'))};
      import { resolveApp } from ${JSON.stringify(resolve(root, 'scripts/app.mjs'))};
      const { flags, rest } = parseFlags(process.argv.slice(2));
      console.log(JSON.stringify({ dir: resolveApp(flags.app, { useOutput: false }).dir,
        sim: flags.sim ?? process.env.EXACT_SIM, attach: flags.attach, rest,
        exact: process.env.EXACT, exact2: process.env.EXACT2 }));
      process.exitCode = 3;
    `);
    const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('EXACT')));
    for (const explicit of [false, true]) {
      const args = ['agent', 'ios', ...explicit ? ['--sim', 'our-simulator'] : [], '--attach', 'tree'];
      const wrapper = `import { run } from ${JSON.stringify(resolve(root, 'scripts/app-cli.mjs'))};
        process.exit(await run(${JSON.stringify({ dir, name: 'app', exact2: sdk, args })}));`;
      const result = spawnSync(process.execPath, ['-e', wrapper], { cwd: parent,
        env: { ...env, ...explicit ? {} : { EXACT_SIM: 'our-simulator' } }, encoding: 'utf8' });
      expect(result.status, result.stderr).toBe(3);
      expect(result.stderr).toBe('');
      expect(JSON.parse(result.stdout)).toEqual({ dir, sim: 'our-simulator', attach: true,
        rest: ['ios', 'tree'], exact: sdk, exact2: sdk });
    }
  } finally { rmSync(parent, { recursive: true, force: true }); }
});

test('build and driver children see backend, private output and build-check environment changes', () => {
  const root = resolve(import.meta.dir, '..'), parent = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-child-env-')));
  const dir = resolve(parent, 'app'), sdk = resolve(parent, 'sdk'), record = resolve(parent, 'build.json');
  const outputs = { EXACT_APPLE_OUTPUT: resolve(parent, 'apple'), EXACT_WINDOWS_OUTPUT: resolve(parent, 'windows'),
    EXACT_BAKE_OUTPUT: resolve(parent, 'bake'), EXACT_WEB_TYPES: 'private' };
  const keys = [...Object.keys(outputs), 'EXACT_BACKEND_ORIGIN', 'EXACT_BACKEND_PROVENANCE',
    'EXACT_BUILD_CHECK_OUT', 'EXACT_BUILD_CHECKED', 'EXACT_APP_DIR', 'EXACT', 'EXACT2', 'HERMES_LEAN_SYS_OFFLINE'];
  const snapshot = `Object.fromEntries(${JSON.stringify(keys)}.filter(key => key in process.env).map(key => [key, process.env[key]]))`;
  try {
    mkdirSync(dir); mkdirSync(resolve(sdk, 'scripts'), { recursive: true });
    mkdirSync(resolve(sdk, 'node_modules/.bin'), { recursive: true });
    writeFileSync(resolve(sdk, 'node_modules/.bin/rolldown'), '');
    writeFileSync(resolve(sdk, 'scripts/help.mjs'), 'export const guardFlags = () => null;');
    writeFileSync(resolve(sdk, 'scripts/app.mjs'), 'export const resolveApp = dir => ({ dir });');
    writeFileSync(resolve(sdk, 'scripts/web-build.mjs'), `export const privateNativeBuild = () => ({
      env: ${JSON.stringify(outputs)}, seed: async () => {}, close: () => {}
    });`);
    writeFileSync(resolve(sdk, 'scripts/agent-build.mjs'), `
      import { writeFileSync } from 'node:fs';
      writeFileSync(${JSON.stringify(record)}, JSON.stringify(${snapshot}));
      if (process.env.EXACT_BUILD_CHECK_OUT) writeFileSync(process.env.EXACT_BUILD_CHECK_OUT, 'checked-build');
    `);
    writeFileSync(resolve(sdk, 'scripts/agent.mjs'), `console.log(JSON.stringify(${snapshot}));`);
    const args = ['agent', 'macos', '--backend', 'https://backend.example', 'tree'];
    const wrapper = `import { run } from ${JSON.stringify(resolve(root, 'scripts/app-cli.mjs'))};
      process.exit(await run(${JSON.stringify({ dir, name: 'app', exact2: sdk, args })}));`;
    const env = Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('EXACT')));
    const result = spawnSync(process.execPath, ['-e', wrapper], { cwd: parent, encoding: 'utf8',
      env: { ...env, EXACT_BUILD_CHECKED: 'stale-build', EXACT_BUILD_CHECK_OUT: 'stale-path', HERMES_LEAN_SYS_OFFLINE: '0' } });
    expect(result.status, result.stderr).toBe(0);
    expect(result.stderr).toBe('');
    const built = JSON.parse(readFileSync(record, 'utf8')), driven = JSON.parse(result.stdout);
    const common = { ...outputs, EXACT_BACKEND_ORIGIN: 'https://backend.example',
      EXACT_BACKEND_PROVENANCE: JSON.stringify({ origin: 'https://backend.example', source: '--backend' }),
      EXACT_APP_DIR: dir, EXACT: sdk, EXACT2: sdk, HERMES_LEAN_SYS_OFFLINE: '1' };
    expect({ built, driven }).toEqual({
      built: { ...common, EXACT_BUILD_CHECK_OUT: expect.stringMatching(/\/exact-build-check-\d+$/) },
      driven: { ...common, EXACT_BUILD_CHECKED: 'checked-build' },
    });
  } finally { rmSync(parent, { recursive: true, force: true }); }
});

test('the former CLI verb refuses with the new command, including help', async () => {
  const { spawnSync } = await import('node:child_process');
  const { fileURLToPath } = await import('node:url');
  const root = fileURLToPath(new URL('..', import.meta.url));
  for (const args of [['tamber'], ['tamber', '--help']]) {
    const wrapper = `const { run } = await import('./scripts/app-cli.mjs'); process.exit(await run({ dir: process.cwd(), name: 'fixture', exact2: process.cwd(), args: ${JSON.stringify(args)} }))`;
    for (const command of [['scripts/exact.mjs', ...args], ['-e', wrapper]]) {
      const result = spawnSync(process.execPath, command, { cwd: root, encoding: 'utf8' });
      expect(result.status).toBe(2);
      expect(result.stderr).toContain('rename the tamber verb to forma');
    }
  }
});

// A UTF-8 byte-order mark, which some Windows editors write, is named with its fix (expo/exact-build#190).
test('app.json with a byte-order mark is refused by name', () => {
  expect(parseManifest('{"app":{"id":"com.example.a"}}')).toEqual({ app: { id: 'com.example.a' } });
  expect(() => parseManifest('﻿{"app":{}}')).toThrow('byte-order mark (U+FEFF), which JSON does not allow; save it as UTF-8 without a BOM');
  expect(() => parseManifest('{')).toThrow();
});

// scripts/app.mjs keeps the same check for readManifest and gameDefaults.
test('the SDK side names a byte-order mark the same way', async () => {
  const { parseManifest: sdk } = await import('./app.mjs');
  expect(() => sdk('﻿{}')).toThrow('byte-order mark (U+FEFF)');
});

test('lock reports a missing Android verification file and leaves the Maven lock for review', () => {
  const dir = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-lock-report-')));
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Lock', app: { name: 'Lock', id: 'dev.exact.lock' }, host: { android: { maven: ['androidx.window:window-java:1.3.0'] } } }));
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    text "Lock"\n');
    writeFileSync(resolve(dir, 'gradle.lockfile'), 'committed lock\n');
    const result = spawnSync(process.execPath, [resolve(import.meta.dir, 'app-cli.mjs'), '--lock', dir], { encoding: 'utf8' });
    expect(result.status, result.stderr).toBe(0);
    expect(result.stdout).toContain('gradle-verification.xml is missing');
    expect(result.stdout).toContain('--write-locks');
    expect(readFileSync(resolve(dir, 'gradle.lockfile'), 'utf8')).toBe('committed lock\n');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
