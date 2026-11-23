import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, realpathSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { settingValues } from '../js/bake/src/__exact_env.mjs';
import { driveBuildEnv } from './agent-build.mjs';
import { resolveApp } from './app.mjs';

const launch = name => ({ name, launch: true });

function withApp(env, run) {
  const dir = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-agent-build-env-')));
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'launch-probe', app: { id: 'com.exact.launch-probe', name: 'Launch Probe' }, env }));
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    text "hello"\n');
    run(dir);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}

test('a drive build keeps file and build settings and leaves shell launch settings to the launch (#1336)', () => {
  withApp([launch('API_TOKEN'), launch('SESSION'), 'API_BASE_URL'], dir => {
    writeFileSync(resolve(dir, '.env'), 'SESSION=from-dotenv\n');
    const app = resolveApp(dir);
    const shell = { PATH: '/usr/bin', API_TOKEN: 'from-shell', SESSION: 'from-dotenv', API_BASE_URL: 'https://api.test', EXACT_BUN_LOADED: 'SESSION' };
    const env = driveBuildEnv(app, shell);
    assert.deepEqual(env, { ...shell, EXACT_LAUNCH_OMIT: 'API_TOKEN' });
    assert.deepEqual(settingValues(dir, ['API_TOKEN', 'SESSION'], env), { API_TOKEN: null, SESSION: { value: 'from-dotenv', source: '.env' } });
    assert.deepEqual(driveBuildEnv(app, shell, { PATH: '/usr/bin' }), shell);
  });
});

test('Windows build omissions match manifest, launch and Bun-loaded names regardless of case', () => {
  const app = { manifest: { env: [launch('API_TOKEN'), launch('SESSION')] } };
  const env = { Api_Token: 'from-shell', Session: 'from-dotenv', exact_bun_loaded: 'sEsSiOn' };
  const launched = { api_token: 'from-launch', SESSION: 'from-dotenv' };
  assert.deepEqual(driveBuildEnv(app, env, launched, { windows: true }), { ...env, EXACT_LAUNCH_OMIT: 'API_TOKEN' });
  assert.deepEqual(driveBuildEnv(app, env, launched), env);
  const empty = { ...env, Api_Token: '' };
  assert.deepEqual(driveBuildEnv(app, empty, { ...launched, api_token: '' }, { windows: true }), empty);
});

// Keep the module mock in a separate Bun process so the other build tests use real children.
function checkSpawns(names, script) {
  withApp(names.map(launch), dir => {
    const result = spawnSync(process.execPath, ['--no-env-file', '-e', `
      import assert from 'node:assert/strict';
      import { mock } from 'bun:test';
      import * as processes from 'node:child_process';
      import { EventEmitter } from 'node:events';
      import { PassThrough } from 'node:stream';
      import { resolve } from 'node:path';
      const dir = ${JSON.stringify(dir)}, calls = [];
      delete process.env.EXACT_MAC_BIN;
      delete process.env.EXACT_WINDOWS_BIN;
      delete process.env.SIMCTL_CHILD_API_TOKEN;
      delete process.env.SIMCTL_CHILD_SESSION;
      delete process.env.EXACT_LAUNCH_OMIT;
      let onSpawn = () => {};
      mock.module('node:child_process', () => ({ ...processes, spawn(command, args, options) {
        calls.push({ command, args, ...options });
        onSpawn(command, args);
        const child = Object.assign(new EventEmitter(), {
          pid: process.pid, stdout: new PassThrough(), stderr: new PassThrough(), unref() {},
        });
        queueMicrotask(() => { child.emit('exit', 0); child.emit('close', 0); });
        return child;
      } }));
      const { buildFirst, makeOverride, prebuild, windowsBuildFirst } = await import('./scripts/agent-build.mjs');
      const { resolveApp } = await import('./scripts/app.mjs');
      const app = resolveApp(dir), quiet = { say() {} };
      ${script}
    `], {
      cwd: resolve(import.meta.dir, '..'), encoding: 'utf8', timeout: 20_000,
      env: { ...process.env, CARGO_TARGET_DIR: resolve(dir, 'target'), EXACT_APP_DIR: dir, EXACT_BAKE_OUTPUT: resolve(dir, 'baked'), EXACT_BUN_LOADED: '',
        EXACT_UPDATE_TRUST: 'development', EXACT_VARIANT: '', EXACT_WINDOWS_OUTPUT: resolve(dir, 'dist-windows') },
    });
    assert.equal(result.status, 0, result.stdout + result.stderr + (result.error ?? ''));
  });
}

test('macOS drive rebuilds give the Apple builder its Cargo target and name the bake omission', () => {
  checkSpawns(['CARGO_TARGET_DIR'], `
    assert.equal(await buildFirst('macos', app, quiet), 0);
    assert.equal(calls.length, 1);
    assert.equal(calls[0].args[0], resolve('host/apple/build.mjs'));
    assert.equal(calls[0].env.CARGO_TARGET_DIR, resolve(dir, 'target'));
    assert.equal(calls[0].env.EXACT_LAUNCH_OMIT, 'CARGO_TARGET_DIR');
  `);
});

test('iOS drive rebuilds name the setting the simulator launch receives', () => {
  checkSpawns(['API_TOKEN', 'SESSION'], `
    process.env.API_TOKEN = 'build-only';
    process.env.SESSION = 'from-shell';
    process.env.SIMCTL_CHILD_SESSION = 'from-launch';
    assert.equal(await buildFirst('ios', app, quiet), 0);
    assert.equal(calls.length, 1);
    assert.equal(calls[0].args[0], resolve('host/apple/build.mjs'));
    assert.ok(calls[0].args.includes('--ios'));
    assert.equal(calls[0].env.API_TOKEN, 'build-only');
    assert.equal(calls[0].env.SESSION, 'from-shell');
    assert.equal(calls[0].env.SIMCTL_CHILD_SESSION, 'from-launch');
    assert.equal(calls[0].env.EXACT_LAUNCH_OMIT, 'SESSION');
  `);
});

test('Windows drive rebuilds preserve the Cargo target and name mixed-case omissions', () => {
  checkSpawns(['API_TOKEN', 'CARGO_TARGET_DIR'], `
    delete process.env.API_TOKEN;
    process.env.Api_Token = 'windows-launch';
    assert.equal(await windowsBuildFirst(app, quiet), 0);
    assert.equal(calls.length, 1);
    assert.equal(calls[0].args[0], resolve('host/windows/build.mjs'));
    assert.equal(calls[0].env.CARGO_TARGET_DIR, resolve(dir, 'target'));
    assert.equal(Object.entries(calls[0].env).find(([name]) => name.toUpperCase() === 'API_TOKEN')?.[1], 'windows-launch');
    assert.equal(calls[0].env.EXACT_LAUNCH_OMIT, 'API_TOKEN,CARGO_TARGET_DIR');
  `);
});

test.each(['macos', 'windows'])('a %s drive rebuild keeps launch settings under production trust', platform => {
  checkSpawns(['API_TOKEN'], `
    const host = ${JSON.stringify(platform)};
    process.env.API_TOKEN = 'from-shell';
    process.env.EXACT_UPDATE_TRUST = 'production';
    process.env.EXACT_LAUNCH_OMIT = 'API_TOKEN';
    const result = host === 'windows' ? await windowsBuildFirst(app, quiet) : await buildFirst(host, app, quiet);
    assert.equal(result, 0);
    assert.equal(calls.length, 1);
    const { args, env } = calls[0];
    assert.equal(args[0], resolve(host === 'windows' ? 'host/windows/build.mjs' : 'host/apple/build.mjs'));
    assert.equal(env.EXACT_UPDATE_TRUST, 'production');
    assert.equal(env.API_TOKEN, 'from-shell');
    assert.equal(env.EXACT_LAUNCH_OMIT, undefined);
  `);
});

test.each([
  ['macOS --run', ['--run'], 'development', 'API_TOKEN,CARGO_TARGET_DIR'],
  ['iOS simulator', ['--ios'], 'development', 'API_TOKEN'],
  ['--release', ['--release'], 'development', ''],
  ['--distribution', ['--distribution'], 'development', ''],
  ['--appstore', ['--appstore'], 'development', ''],
  ['--archive', ['--archive', 'app.ipa'], 'development', ''],
  ['production trust', [], 'production', ''],
  ['--embed', ['--embed'], 'development', ''],
  ['--device', ['--device'], 'development', ''],
  ['--tvos', ['--tvos'], 'development', ''],
])('Apple prebuild preserves the child environment for %s', (_label, flags, trust, omit) => {
  checkSpawns(['API_TOKEN', 'CARGO_TARGET_DIR'], `
    process.env.API_TOKEN = 'from-shell';
    process.env.SIMCTL_CHILD_API_TOKEN = 'from-launch';
    process.env.EXACT_UPDATE_TRUST = ${JSON.stringify(trust)};
    assert.equal(await prebuild(app, [...${JSON.stringify(flags)}, '--prebuild'], quiet), 0);
    assert.equal(calls.length, 1);
    const { args, env } = calls[0];
    assert.ok(args.includes(resolve('host/apple/build.mjs')));
    assert.equal(env.CARGO_TARGET_DIR, resolve(dir, 'target'));
    assert.equal(env.API_TOKEN, 'from-shell');
    assert.equal(env.SIMCTL_CHILD_API_TOKEN, 'from-launch');
    assert.equal(env.EXACT_LAUNCH_OMIT ?? '', ${JSON.stringify(omit)});
  `);
});

test.each(['macos', 'ios'])('a plan-only %s bake names omissions only when baking the app', host => {
  checkSpawns(['API_TOKEN', 'CARGO_TARGET_DIR'], `
    const { mkdirSync, writeFileSync } = await import('node:fs');
    const { createHash } = await import('node:crypto');
    const { isDeepStrictEqual } = await import('node:util');
    const { bakeOutput } = await import('./scripts/app.mjs');
    const { bakeTool, linkedBakeDir } = await import('./scripts/linked-plan.mjs');
    const platform = ${JSON.stringify(host)}, target = platform === 'macos' ? 'aarch64-apple-darwin' : 'aarch64-apple-ios-sim';
    process.env.API_TOKEN = 'from-shell';
    process.env.SIMCTL_CHILD_API_TOKEN = 'from-launch';
    const linked = linkedBakeDir({ EXACT_BAKE_OUTPUT: bakeOutput(app) }, platform, target);
    const plan = 'built plan', sha256 = createHash('sha256').update(plan).digest('hex');
    const receipt = resolve(dir, 'receipt.json');
    mkdirSync(linked, { recursive: true });
    writeFileSync(resolve(linked, 'app.plan'), plan);
    writeFileSync(resolve(linked, 'app.module.json'), '{}');
    writeFileSync(receipt, JSON.stringify({ target, build: { compat: { inputs: { platform }, embedded: { plan: { sha256 } } } } }));
    onSpawn = (command, args) => {
      if (command !== bakeTool.built) return;
      mkdirSync(args[2], { recursive: true });
      writeFileSync(resolve(args[2], 'app.module.json'), '{}');
    };
    assert.deepEqual(await makeOverride(platform, app, { receipt, changed: ['app.contract'] }, { log: resolve(dir, 'plan.log') }), { made: true, output: '' });
    assert.deepEqual(calls.map(call => call.command), [bakeTool.command, bakeTool.built]);
    assert.equal(calls[0].env.CARGO_TARGET_DIR, resolve(dir, 'target'));
    assert.ok(isDeepStrictEqual(calls[0].env, bakeTool.options().env), 'Cargo received a different environment');
    assert.equal(calls[1].env.CARGO_TARGET_DIR, resolve(dir, 'target'));
    assert.equal(calls[1].env.API_TOKEN, 'from-shell');
    assert.equal(calls[1].env.EXACT_LAUNCH_OMIT, platform === 'macos' ? 'API_TOKEN,CARGO_TARGET_DIR' : 'API_TOKEN');
  `);
});
