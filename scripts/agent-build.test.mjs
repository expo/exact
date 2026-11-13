// A native drive's build first (agent-build.mjs, LLP 1116.002 D4): the decision an app's exact.mjs makes before
// `test ios|macos` and `agent ios|macos`, on the driver's own record, with no simulator and no build run.
import { afterEach, beforeEach, test, expect } from 'bun:test';
import assert from 'node:assert/strict';
import { execFileSync, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { chmodSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, renameSync, rmSync, statSync, symlinkSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, delimiter, dirname, relative, resolve } from 'node:path';
import { appleArtifacts, appleRunBinary } from '../host/apple/build.mjs';
import { notBuildInput, receiptChanges, refuseBinaryBackend } from './agent-launch.mjs';
import { moduleGraph } from './module-graph.mjs';
import { directoryInputSnapshot } from './cargo-dep-info.mjs';
import { buildArgs, buildFirst, buildReason, childFailure, failureSummary, linuxRebuild, lockHolder, logged, nativeBuild, overrideDir, planOverride, planSource, planSources, ownVerb, prebuild, rebuildCommand, refuseBuild } from './agent-build.mjs';
import { openStdio } from './agent-stdio.mjs';
import { bakeOutput, hermesBundle, hermesTarget, linuxBinary, resolveApp, windowsFile, PINNED_RUST } from './app.mjs';
import { recordBuiltSettings } from '../js/bake/src/__exact_env.mjs';
import { writePackagedReceipt } from '../host/windows/build.mjs';

test('Mac drives use the assembled app bundle even without catalogs or widgets, and the sample host its own', () => {
  const { dir } = scratchApp('ats-drive');
  try {
    const app = resolveApp(dir);
    const paths = appleArtifacts(app);
    assert.equal(appleRunBinary(app), resolve(paths.bundle, 'Contents/MacOS', paths.executable));
    const host = appleArtifacts(app, { host: true });
    assert.equal(appleRunBinary(app, true), resolve(host.bundle, 'Contents/MacOS', 'ExactHostMac'));
    assert.match(host.bundle, / Host\.app$/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('a child failure keeps stderr when stdout ends with successful build progress', () => {
  assert.match(childFailure({ stderr: 'error: simulator installation failed\n', stdout: 'Building for production...\n[1/2] ExactIOS\nBuild complete!\n' }), /simulator installation failed/);
  assert.match(childFailure({ error: new Error('spawn ENOENT') }), /spawn ENOENT/);
  assert.equal(childFailure({ stdout: 'error: compiler failed\n' }), 'error: compiler failed');
});

test('a child failure retains errors from either stream past warnings and progress', () => {
  assert.equal(childFailure({ stderr: 'warning: deprecated flag\n', stdout: 'Building...\nerror: could not find sdk\n' }), 'error: could not find sdk');
  assert.equal(childFailure({ stderr: 'error: simulator installation failed\nwarning: one\nwarning: two\nwarning: three\n', stdout: 'Build complete!\n' }), 'error: simulator installation failed');
});

test('a metrics build summary does not claim that a drive was planned', () => {
  const summary = failureSummary('error: missing sdk\n', { host: 'ios', code: 1, log: 'size.log', beforeDrive: false });
  assert.match(summary, /the iOS build failed \(exit 1\); its whole log: size.log/);
  assert.doesNotMatch(summary, /driven/);
});

/** An outside app, as `exact new` lays one out, with only what the decision reads. */
// Each case starts with no target of the environment's: a command a case expects names one only when it is set
// (`ownVerb`), and the cases that set one say so.
const inheritedTarget = process.env.CARGO_TARGET_DIR;
beforeEach(() => { delete process.env.CARGO_TARGET_DIR; });
afterEach(() => { if (inheritedTarget === undefined) delete process.env.CARGO_TARGET_DIR; else process.env.CARGO_TARGET_DIR = inheritedTarget; });

function scratchApp(name) {
  const dir = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-agent-build-')));
  const write = (path, text = '') => { mkdirSync(dirname(resolve(dir, path)), { recursive: true }); writeFileSync(resolve(dir, path), text); };
  write('app.contract', 'component App\n  view\n    text "hello"\n');
  write('app.json', JSON.stringify({ name, app: { id: `com.exact.${name}`, name } }));
  for (const kind of ['apple', 'web']) write(`${kind}/Cargo.toml`, `[package]\nname="${name}-${kind}"\nversion="0.1.0"\n`);
  write('exact.mjs', "// …\nconst built = await run(['scripts/agent-build.mjs', host]);\n");
  return { dir, write };
}

test('Apple rebuilds carry a path-selected app into the child without changing the parent environment', () => {
  const { dir } = scratchApp('field-log');
  const script = `
    import assert from 'node:assert/strict';
    import { mock } from 'bun:test';
    import * as processes from 'node:child_process';
    import { resolveApp } from './scripts/app.mjs';
    const spawn = processes.spawn;
    const dir = ${JSON.stringify(dir)}, builds = [];
    mock.module('node:child_process', () => ({ ...processes, spawn(command, args, options) {
      assert.ok(args[0].endsWith('/host/apple/build.mjs'));
      builds.push(args.slice(1));
      const probe = 'import { resolveApp } from "./scripts/app.mjs"; '
        + 'console.log(resolveApp(' + JSON.stringify(args.find(arg => arg.endsWith('-apple'))) + ').dir)';
      return spawn(command, ['-e', probe], options);
    } }));
    const { buildFirst } = await import('./scripts/agent-build.mjs');
    delete process.env.EXACT_MAC_BIN;
    for (const inherited of [undefined, ${JSON.stringify(resolve(import.meta.dir, '../apps/theme-fixture'))}]) {
      if (inherited === undefined) delete process.env.EXACT_APP_DIR;
      else process.env.EXACT_APP_DIR = inherited;
      const app = resolveApp(dir);
      for (const host of ['ios', 'macos']) {
        assert.equal(await buildFirst(host, app, { say: line => console.error(line), wait: () => {} }), 0);
        const log = await Bun.file(dir + '/.exact/' + host + '-build.log').text();
        assert.equal(log.trim(), dir);
        assert.equal(process.env.EXACT_APP_DIR, inherited);
      }
    }
    assert.equal(builds.length, 4);
  `;
  try {
    const r = spawnSync(process.execPath, ['-e', script], { cwd: resolve(import.meta.dir, '..'), encoding: 'utf8', timeout: 20_000 });
    assert.equal(r.status, 0, r.stdout + r.stderr + (r.error ?? ''));
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('macOS freshness checks the full product receipt when the bundle carries a reduced receipt', () => {
  const previous = process.env.EXACT_APP_DIR, { dir, write } = scratchApp('ats-drive');
  try {
    process.env.EXACT_APP_DIR = dir;
    const app = resolveApp('ats-drive'), source = resolve(dir, 'source.swift');
    write('source.swift', 'before');
    const paths = appleArtifacts(app), executable = resolve(paths.bundle, 'Contents/MacOS', paths.executable);
    mkdirSync(dirname(executable), { recursive: true }); writeFileSync(executable, '');
    const receipt = resolve(paths.products, 'receipt.json'), time = new Date(Date.now() + 60_000);
    mkdirSync(dirname(receipt), { recursive: true });
    writeFileSync(receipt, JSON.stringify({ target: paths.target, started: time.toISOString(), build: { trust: 'development', binary: { sha256: 'dev',
      inputs: [{ name: 'source.swift', path: source, sha256: createHash('sha256').update('before').digest('hex') }], missing: [], directories: [] } } }));
    utimesSync(receipt, time, time);
    const bundled = resolve(paths.bundle, 'Contents/Resources/receipt.json');
    mkdirSync(dirname(bundled), { recursive: true });
    const shipped = JSON.parse(readFileSync(receipt, 'utf8')); shipped.build.binary = { sha256: 'dev' };
    writeFileSync(bundled, JSON.stringify(shipped));
    assert.equal(nativeBuild('macos', app).receipt, receipt);
    assert.deepEqual(nativeBuild('macos', app).changed, []);
    write('source.swift', 'after');
    assert.deepEqual(nativeBuild('macos', app).changed, ['source.swift']);
    write('source.swift', 'before');
  } finally {
    if (previous === undefined) delete process.env.EXACT_APP_DIR; else process.env.EXACT_APP_DIR = previous;
    rmSync(dir, { recursive: true, force: true });
  }
});

test('native drives refuse a baked backend that the drive did not select', async () => {
  const { dir, write } = scratchApp('backend-drive');
  const saved = { ...process.env };
  try {
    for (const key of ['EXACT_BACKEND_ORIGIN', 'EXACT_UPDATE_TRUST', 'PORT']) delete process.env[key];
    const literal = 'http://127.0.0.1:4400', other = 'http://127.0.0.1:4412';
    write('app.ts', `// const origin = 'http://comment.invalid';\nexport const origin = \`${literal}/\` as const;\n`);
    const app = resolveApp(dir), paths = appleArtifacts(app), receipt = resolve(paths.products, 'receipt.json');
    const bundled = resolve(paths.bundle, 'Contents/Resources/receipt.json');
    const ios = resolve(appleArtifacts(app, { destination: 'ios-simulator' }).bundle, 'receipt.json');
    write(relative(dir, appleRunBinary(app)), '');
    const time = new Date(Date.now() + 60_000);
    const record = origin => {
      for (const path of [receipt, bundled, ios]) {
        write(relative(dir, path), JSON.stringify({ target: paths.target, build: { trust: 'development', backendOrigin: origin } }));
        utimesSync(path, time, time);
      }
    };
    const check = () => {
      assert.deepEqual(receiptChanges(receipt, app), []);
      for (const host of ['macos', 'ios']) {
        const state = nativeBuild(host, app);
        assert.deepEqual(state.changed, []);
        assert.doesNotThrow(() => refuseBuild(host, state, app));
      }
    };
    record(literal); check();
    const replaced = JSON.parse(readFileSync(bundled, 'utf8'));
    replaced.build.backendOrigin = other;
    writeFileSync(bundled, JSON.stringify(replaced));
    assert.deepEqual(nativeBuild('macos', app).changed, [bundled]);
    record(other);
    for (const checked of [null, createHash('sha256').update(readFileSync(receipt)).digest('hex')]) {
      assert.deepEqual(receiptChanges(receipt, app, { hashed: !!checked }), []);
      for (const host of ['macos', 'ios']) assert.throws(() => refuseBuild(host, nativeBuild(host, app, { checked }), app), /baked backend.*4412.*app.ts origin.*4400/);
    }
    const unexpected = () => assert.fail('a refused backend must neither build nor bake');
    await assert.rejects(buildFirst('macos', app, { wait: () => {}, build: unexpected, bake: unexpected }), /baked backend/);
    const { open } = await import('./agent.mjs');
    for (const host of ['macos', 'ios']) {
      await assert.rejects(open({ host, app: dir, plan: resolve(dir, 'replacement.plan'), onProcess: unexpected }), /baked backend/);
    }
    // --backend sets this variable; selecting another origin never authorizes the baked one.
    process.env.EXACT_BACKEND_ORIGIN = `${other}/`; check();
    process.env.EXACT_BACKEND_ORIGIN = literal;
    assert.throws(() => refuseBuild('macos', nativeBuild('macos', app), app), /baked backend.*EXACT_BACKEND_ORIGIN/);
    delete process.env.EXACT_BACKEND_ORIGIN;
    write('.exact/backend-origin', other); check();
    rmSync(resolve(dir, '.exact/backend-origin'));
    // A checkout's fixture port is also the bake's default selection.
    write('app.ts', `const origin = '${literal}';\n`);
    write('app.json', JSON.stringify({ ...app.manifest, commands: { server: ['bun', 'fixture/server.mjs', '--port', '4412'] } }));
    check();
    record(literal);
    assert.throws(() => refuseBuild('macos', nativeBuild('macos', app), app), /baked backend.*commands.server --port/);
    record(undefined);
    assert.deepEqual(receiptChanges(receipt, app), []);
    assert.throws(() => refuseBuild('macos', nativeBuild('macos', app), app), /receipt has no backend origin; run .*exact\.mjs (mac|ios)/);
    for (const path of [receipt, bundled, ios]) rmSync(path);
    for (const host of ['macos', 'ios']) assert.throws(() => refuseBuild(host, nativeBuild(host, app), app), /receipt has no backend origin; run .*exact\.mjs (mac|ios)/);
    write('app.ts', "const origin = 'https://example.com/app';\n");
    record(other);
    for (const host of ['macos', 'ios']) assert.throws(() => refuseBuild(host, nativeBuild(host, app), app), /baked backend.*example.com/);
    write('app.ts', `// const origin = '${literal}';\nconst config = { origin: '${literal}' };\n`);
    check();
  } finally {
    for (const key of ['EXACT_BACKEND_ORIGIN', 'EXACT_UPDATE_TRUST', 'PORT']) {
      if (saved[key] === undefined) delete process.env[key]; else process.env[key] = saved[key];
    }
    rmSync(dir, { recursive: true, force: true });
  }
});

test('the backend command reads a literal declaration rather than a commented origin', () => {
  mkdirSync(resolve(import.meta.dir, '../target'), { recursive: true });
  const dir = mkdtempSync(resolve(import.meta.dir, '../target/exact-backend-'));
  try {
    const app = resolve(dir, 'app'), server = resolve(dir, 'node_modules/snapback4/bin/snapback4.js');
    mkdirSync(app);
    mkdirSync(dirname(server), { recursive: true });
    writeFileSync(server, 'console.log(JSON.stringify(process.argv.slice(2)));\n');
    writeFileSync(resolve(app, 'app.json'), '{}');
    const env = { ...process.env };
    delete env.EXACT_BACKEND_ORIGIN;
    const run = () => spawnSync(process.execPath, [resolve(import.meta.dir, 'exact.mjs'), 'backend', app], { encoding: 'utf8', env });
    writeFileSync(resolve(app, 'app.ts'), "// const origin = 'http://comment.invalid';\nconst origin = ('http://127.0.0.1:4410' as const);\n");
    const selected = run();
    assert.equal(selected.status, 0, selected.stderr);
    assert.deepEqual(JSON.parse(selected.stdout), ['dev', '--port', '4410']);
    writeFileSync(resolve(app, 'app.ts'), "// const origin = 'http://comment.invalid';\n");
    const absent = run();
    assert.equal(absent.status, 1);
    assert.match(absent.stderr, /needs a literal const origin/);
    assert.equal(absent.stdout, '');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('an app launcher selects a native backend before checking its build', () => {
  const { dir, write } = scratchApp('backend-launcher');
  const literal = 'http://127.0.0.1:4400', selected = 'http://127.0.0.1:4412';
  const app = resolveApp(dir), mac = appleArtifacts(app), ios = appleArtifacts(app, { destination: 'ios-simulator' });
  write('app.ts', `const origin = '${literal}';\n`);
  write(relative(dir, appleRunBinary(app)), '');
  const receipt = JSON.stringify({ target: mac.target, build: { trust: 'development', backendOrigin: selected } });
  for (const path of [resolve(mac.products, 'receipt.json'), resolve(mac.bundle, 'Contents/Resources/receipt.json'), resolve(ios.bundle, 'receipt.json')]) {
    write(relative(dir, path), receipt);
    const time = new Date(Date.now() + 60_000);
    utimesSync(path, time, time);
  }
  const script = `
    import { mock } from 'bun:test';
    import * as processes from 'node:child_process';
    const spawn = processes.spawn;
    mock.module(${JSON.stringify(resolve(import.meta.dir, 'app-shells.mjs'))}, () => ({
      platformOf: () => null,
    }));
    mock.module('node:child_process', () => ({ ...processes, spawn(command, args, options) {
      if (args[0].endsWith('/scripts/agent.mjs')) {
        return spawn(command, ['-e', 'console.log(process.env.EXACT_BACKEND_ORIGIN)'], options);
      }
      return spawn(command, args, options);
    } }));
    const { run } = await import('./scripts/app-cli.mjs');
    process.exitCode = await run({ dir: ${JSON.stringify(dir)}, name: 'backend-launcher',
      exact2: ${JSON.stringify(resolve(import.meta.dir, '..'))}, args: process.argv.slice(1) });
  `;
  const env = { ...process.env };
  for (const key of ['EXACT_BACKEND_ORIGIN', 'EXACT_MAC_BIN', 'EXACT_BUILD_CHECKED', 'PORT']) delete env[key];
  try {
    for (const host of ['macos', 'ios']) {
      const drive = backend => spawnSync(process.execPath, ['-e', script, 'agent', host, ...(backend ? ['--backend', backend] : []), 'tree'], {
        cwd: resolve(import.meta.dir, '..'), encoding: 'utf8', env, timeout: 15_000,
      });
      for (const backend of [undefined, literal]) {
        const refused = drive(backend);
        assert.equal(refused.status, 1, refused.stdout + refused.stderr);
        assert.match(refused.stderr, /baked backend.*4412.*4400/);
        assert.equal(refused.stdout, '');
      }
      const overridden = drive(selected);
      assert.equal(overridden.status, 0, overridden.stdout + overridden.stderr);
      assert.equal(overridden.stdout.trim(), selected);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('stdio drives check backend receipts before desktop spawns or phone installation', async () => {
  const { dir, write } = scratchApp('backend-stdio');
  const saved = { ...process.env };
  try {
    for (const key of ['EXACT_BACKEND_ORIGIN', 'EXACT_UPDATE_TRUST', 'PORT']) delete process.env[key];
    write('app.ts', "const origin = 'http://127.0.0.1:4400';\n");
    const app = resolveApp(dir), other = 'http://127.0.0.1:4412';
    const bin = resolve(dir, 'probe.app/Contents/MacOS/probe'), backend = { backendOrigin: other };
    write(relative(dir, bin), 'never run');
    write('baked/app.module.json', JSON.stringify(backend));
    recordBuiltSettings(bin, app, resolve(dir, 'baked'));
    // The next bake alone must not change the backend recorded for an existing executable.
    write('baked/app.module.json', JSON.stringify({ backendOrigin: 'http://127.0.0.1:4400' }));
    write('probe.app/Contents/Resources/receipt.json', JSON.stringify({ build: backend }));
    const phone = appleArtifacts(app, { destination: 'ios' }).bundle;
    write(relative(dir, resolve(phone, 'receipt.json')), JSON.stringify({ build: backend }));
    const product = `${app.crate('windows')}.exe`, dll = resolve(dirname(bin), 'backend.dll');
    write(relative(dir, resolve(dirname(bin), windowsFile(app, product))), readFileSync(bin));
    write(relative(dir, dll), 'selected backend module');
    writePackagedReceipt(app, { ...backend, products: [product, dll].map(path => ({ path,
      sha256: createHash('sha256').update(readFileSync(resolve(dirname(bin), windowsFile(app, path)))).digest('hex') })) }, dirname(bin));
    for (const host of ['macos', 'windows', 'linux', 'android']) {
      process.env[`EXACT_${host === 'macos' ? 'MAC' : host.toUpperCase()}_BIN`] = bin;
      await assert.rejects(openStdio({ host, app: dir, onProcess: () => assert.fail('refusal must precede spawn') }), /baked backend.*4412.*4400/);
    }
    await assert.rejects(openStdio({ host: 'ios', device: true, app: dir }), /baked backend.*4412.*4400/);
    process.env.EXACT_BACKEND_ORIGIN = other;
    for (const options of [{}, { packaged: true }, { receipt: resolve(phone, 'receipt.json') }]) {
      assert.doesNotThrow(() => refuseBinaryBackend(bin, app, options));
    }
    write(relative(dir, dll), 'a different backend module');
    assert.throws(() => refuseBinaryBackend(bin, app, { packaged: true }), /windows build is stale: .*backend.dll.*run .*windows/);
    await assert.rejects(openStdio({ host: 'windows', app: dir, onProcess: () => assert.fail('refusal must precede spawn') }), /windows build is stale: .*backend.dll.*run .*windows/);
    write(relative(dir, bin), 'another executable without a bound record');
    for (const options of [{}, { packaged: true }]) {
      assert.throws(() => refuseBinaryBackend(bin, app, options), /receipt has no backend origin/);
    }
  } finally {
    for (const key of ['EXACT_BACKEND_ORIGIN', 'EXACT_UPDATE_TRUST', 'PORT', 'EXACT_MAC_BIN', 'EXACT_WINDOWS_BIN', 'EXACT_LINUX_BIN', 'EXACT_ANDROID_BIN']) {
      if (saved[key] === undefined) delete process.env[key]; else process.env[key] = saved[key];
    }
    rmSync(dir, { recursive: true, force: true });
  }
});

// The bake cache keeps one record per target; a directory packaged before the next build keeps its own.
test('a Windows package keeps its backend after the same target is packaged elsewhere for another', () => {
  const { dir, write } = scratchApp('backend-packages');
  const saved = process.env.EXACT_BACKEND_ORIGIN;
  try {
    write('app.ts', "const origin = 'http://127.0.0.1:4400';\n");
    const app = resolveApp(dir), exe = `${app.crate('windows')}.exe`;
    const sha = text => createHash('sha256').update(text).digest('hex');
    const pack = (into, origin) => {
      const products = [[exe, `executable for ${origin}`], ['backend.dll', `module for ${origin}`]].map(([path, text]) => {
        write(`${into}/${windowsFile(app, path)}`, text);
        return { path: resolve(app.target, 'gpu-dev', path), sha256: sha(text) };
      });
      const build = { version: 1, backendOrigin: origin, compat: { inputs: { app: app.id } }, products };
      // What buildBake leaves in the cache: the target's one record, which the next build replaces.
      write(relative(dir, resolve(bakeOutput(app), 'windows-probe.build.json')), JSON.stringify(build));
      writePackagedReceipt(app, build, resolve(dir, into));
      return resolve(dir, into, windowsFile(app, exe));
    };
    const a = pack('a', 'http://127.0.0.1:4412'), b = pack('b', 'http://127.0.0.1:4400');
    process.env.EXACT_BACKEND_ORIGIN = 'http://127.0.0.1:4412';
    assert.doesNotThrow(() => refuseBinaryBackend(a, app, { packaged: true }));
    assert.throws(() => refuseBinaryBackend(b, app, { packaged: true }), /baked backend http:\/\/127\.0\.0\.1:4400 differs/);
    // A's receipt is bound to A's executable and DLL, not to whatever the directory now holds.
    write('a/backend.dll', 'module for http://127.0.0.1:4400');
    assert.throws(() => refuseBinaryBackend(a, app, { packaged: true }), /windows build is stale: .*backend\.dll/);
    write(`a/${windowsFile(app, exe)}`, 'executable for http://127.0.0.1:4400');
    assert.throws(() => refuseBinaryBackend(a, app, { packaged: true }), /receipt has no backend origin; run bun host\/windows\/build\.mjs/);
  } finally {
    if (saved === undefined) delete process.env.EXACT_BACKEND_ORIGIN; else process.env.EXACT_BACKEND_ORIGIN = saved;
    rmSync(dir, { recursive: true, force: true });
  }
});

// A bare `cargo build` of the Linux host writes no `<executable>.settings.json`, so a refusal for want of one names
// the build that writes it, and what that build records after Cargo satisfies the check.
test('a Linux build without a backend record is told the build that writes one', async () => {
  const { dir, write } = scratchApp('fixture-backend'), saved = { ...process.env };
  try {
    rmSync(resolve(dir, 'exact.mjs')); // exact2's own build, not an app's verb
    write('app.ts', "const origin = 'http://127.0.0.1:4400';\n");
    Object.assign(process.env, { EXACT_APP_DIR: dir });
    for (const key of ['EXACT_BACKEND_ORIGIN', 'EXACT_LINUX_BIN', 'CARGO_TARGET_DIR']) delete process.env[key];
    const app = resolveApp('fixture-backend'), bin = linuxBinary(app);
    assert.equal(linuxRebuild(app), 'bun scripts/build-linux.mjs fixture-backend');
    write(relative(dir, bin), 'linked by cargo');
    await assert.rejects(openStdio({ host: 'linux', app: 'fixture-backend', onProcess: () => assert.fail('refusal must precede spawn') }),
      { message: 'the native build receipt has no backend origin; run bun scripts/build-linux.mjs fixture-backend before driving this app' });
    // What scripts/build-linux.mjs does once Cargo has linked: the record the bake's module names, bound to the executable.
    const out = resolve(app.target, 'host-dev/build/fixture-backend-linux-0/out');
    write(relative(dir, resolve(out, 'app.module.json')), JSON.stringify({ backendOrigin: 'http://127.0.0.1:4400' }));
    write(relative(dir, `${bin}.d`), `${bin}: ${resolve(out, 'app.hbc')}\n`);
    recordBuiltSettings(bin, app);
    assert.doesNotThrow(() => refuseBinaryBackend(bin, app));
  } finally {
    for (const key of ['EXACT_APP_DIR', 'EXACT_BACKEND_ORIGIN', 'EXACT_LINUX_BIN', 'CARGO_TARGET_DIR']) {
      if (saved[key] === undefined) delete process.env[key]; else process.env[key] = saved[key];
    }
    rmSync(dir, { recursive: true, force: true });
  }
});

test('missing native products name their build command before checking backend metadata', async () => {
  const { dir, write } = scratchApp('backend-missing'), saved = { ...process.env };
  try {
    write('app.ts', "const origin = 'http://127.0.0.1:4400';\n");
    for (const host of ['macos', 'windows', 'linux', 'android']) {
      const key = `EXACT_${host === 'macos' ? 'MAC' : host.toUpperCase()}_BIN`;
      delete process.env[key];
      await assert.rejects(openStdio({ host, app: dir }), /run .* first/);
      process.env[key] = resolve(dir, 'absent');
      await assert.rejects(openStdio({ host, app: dir }), host === 'macos' ? /EXACT_MAC_BIN names no file/ : /run .* first/);
    }
    await assert.rejects(openStdio({ host: 'ios', device: true, app: dir }), /run bun host\/apple\/build.mjs --device first/);
    // The Canvas reader's drive runs the APK, whose build is the one named; the headless one names the Linux host's.
    delete process.env.EXACT_ANDROID_BIN;
    await assert.rejects(openStdio({ host: 'android', app: dir, reader: true }), /bun exact\.mjs android\) first/);
    await assert.rejects(openStdio({ host: 'android', app: dir, reader: false }), /bun exact\.mjs android --headless\) first/);
  } finally {
    for (const key of ['EXACT_MAC_BIN', 'EXACT_WINDOWS_BIN', 'EXACT_LINUX_BIN', 'EXACT_ANDROID_BIN']) {
      if (saved[key] === undefined) delete process.env[key]; else process.env[key] = saved[key];
    }
    rmSync(dir, { recursive: true, force: true });
  }
});

test('a release published after a development build must be rebuilt before an agent drive', () => {
  const previous = process.env.EXACT_APP_DIR, { dir, write } = scratchApp('release-after-dev');
  try {
    process.env.EXACT_APP_DIR = dir;
    const app = resolveApp('release-after-dev'), paths = appleArtifacts(app), receipt = resolve(paths.products, 'receipt.json');
    const bundled = resolve(paths.bundle, 'Contents/Resources/receipt.json'), binary = appleRunBinary(app);
    const development = { built: new Date(Date.now() + 60_000).toISOString(), target: paths.target, composition: paths.composition,
      build: { trust: 'development', binary: { sha256: 'dev', inputs: [], missing: [], directories: [] } } };
    write(relative(dir, binary), 'development executable');
    write(relative(dir, receipt), JSON.stringify(development));
    write(relative(dir, bundled), JSON.stringify(development));
    assert.deepEqual(nativeBuild('macos', app).changed, []);
    const release = { ...development, build: { trust: 'production', binary: { sha256: 'release' } } };
    write(relative(dir, bundled), JSON.stringify(release));
    write(relative(dir, binary), 'production executable');
    const stale = nativeBuild('macos', app);
    assert.ok(stale.changed.includes(bundled), 'the development source receipt cannot approve a production bundle');
    assert.throws(() => refuseBuild('macos', stale, app), /build is stale/);
    write(relative(dir, bundled), JSON.stringify(development));
    write(relative(dir, binary), 'rebuilt development executable');
    assert.deepEqual(nativeBuild('macos', app).changed, []);
  } finally {
    if (previous === undefined) delete process.env.EXACT_APP_DIR; else process.env.EXACT_APP_DIR = previous;
    rmSync(dir, { recursive: true, force: true });
  }
});

// Both of a Linux drive's refusals (no build, a stale one) name this command (agent-stdio.mjs).
test('a Linux drive is told the build that writes where it reads', () => {
  const previous = { app: process.env.EXACT_APP_DIR, target: process.env.CARGO_TARGET_DIR };
  const { dir } = scratchApp('field-log');
  // An app directory inside this checkout, under the target/ a build makes and git ignores
  mkdirSync(resolve(import.meta.dir, '../target'), { recursive: true });
  const inside = mkdtempSync(resolve(import.meta.dir, '../target/agent-build-inside-'));
  const target = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-agent-target-')));
  try {
    process.env.EXACT_APP_DIR = dir;
    delete process.env.CARGO_TARGET_DIR;
    assert.equal(linuxRebuild(resolveApp('field-log')), `(cd '${dir}' && bun exact.mjs linux)`);
    // A target the environment names is resolved where the drive ran; the command runs in the app, so it names it whole.
    process.env.CARGO_TARGET_DIR = relative(process.cwd(), target);
    const app = resolveApp('field-log');
    assert.equal(app.target, target);
    assert.equal(linuxRebuild(app), `(cd '${dir}' && CARGO_TARGET_DIR='${target}' bun exact.mjs linux)`);
    // An Apple host's command names it too.
    assert.equal(rebuildCommand(app, 'ios'), `(cd '${dir}' && CARGO_TARGET_DIR='${target}' bun exact.mjs ios)`);
    // An app inside this checkout has no verb of its own, an exact.mjs or not.
    writeFileSync(resolve(inside, 'exact.mjs'), '');
    assert.equal(ownVerb({ dir: inside }, 'linux'), null);
  } finally {
    for (const [name, value] of [['EXACT_APP_DIR', previous.app], ['CARGO_TARGET_DIR', previous.target]]) {
      if (value === undefined) delete process.env[name]; else process.env[name] = value;
    }
    for (const made of [dir, inside, target]) rmSync(made, { recursive: true, force: true });
  }
});

// Each refusal reached through the driver, and its command run as written, by an exact.mjs that says where it ran and
// which target it was given: the one the drive reads.
test('a Linux drive refused for want of a current build names a command that builds where it reads', async () => {
  const previous = { app: process.env.EXACT_APP_DIR, target: process.env.CARGO_TARGET_DIR, bin: process.env.EXACT_LINUX_BIN };
  const { dir, write } = scratchApp('field-log');
  const target = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-agent-target-')));
  try {
    Object.assign(process.env, { EXACT_APP_DIR: dir, CARGO_TARGET_DIR: target });
    delete process.env.EXACT_LINUX_BIN;
    const app = resolveApp('field-log'), bin = linuxBinary(app), command = linuxRebuild(app);
    await assert.rejects(openStdio({ host: 'linux', app: 'field-log' }), { message: `run ${command} first` });
    // A build a minute older than a source its dep-info names
    mkdirSync(dirname(bin), { recursive: true });
    writeFileSync(bin, '');
    writeFileSync(`${bin}.d`, `${bin}: ${resolve(dir, 'app.contract')}\n`);
    const now = new Date(), past = new Date(now.getTime() - 60_000);
    utimesSync(resolve(dir, 'app.contract'), now, now);
    utimesSync(bin, past, past);
    await assert.rejects(openStdio({ host: 'linux', app: 'field-log' }), (e) => e.message.includes('linux build is stale') && e.message.includes(`; run ${command}`));
    const ran = resolve(dir, 'ran.json');
    write('exact.mjs', `import { writeFileSync } from 'node:fs';\nwriteFileSync(${JSON.stringify(ran)}, JSON.stringify({ cwd: process.cwd(), target: process.env.CARGO_TARGET_DIR, verb: process.argv[2] }));\n`);
    // From a shell whose target is relative, as the drive's was before it resolved it
    execFileSync('sh', ['-c', command], { env: { ...process.env, CARGO_TARGET_DIR: relative(process.cwd(), target) } });
    assert.deepEqual(JSON.parse(readFileSync(ran, 'utf8')), { cwd: dir, target, verb: 'linux' });
    assert.ok(bin.startsWith(`${target}/`), bin);
  } finally {
    for (const [name, value] of [['EXACT_APP_DIR', previous.app], ['CARGO_TARGET_DIR', previous.target], ['EXACT_LINUX_BIN', previous.bin]]) {
      if (value === undefined) delete process.env[name]; else process.env[name] = value;
    }
    rmSync(dir, { recursive: true, force: true });
    rmSync(target, { recursive: true, force: true });
  }
});

test('a missing or stale build is built first, said in one line; a fresh one runs nothing and says nothing', async () => {
  const previous = process.env.EXACT_APP_DIR, macBin = process.env.EXACT_MAC_BIN;
  const { dir, write } = scratchApp('field-log');
  try {
    process.env.EXACT_APP_DIR = dir;
    delete process.env.EXACT_MAC_BIN;
    const app = resolveApp('field-log');
    const built = new Date(Date.now() + 10_000), edited = new Date(+built + 10_000), before = new Date(Date.now() - 60_000);
    for (const file of ['app.contract', 'app.json', 'exact.mjs', 'apple/Cargo.toml', 'web/Cargo.toml']) utimesSync(resolve(dir, file), before, before);
    const said = [], builds = [];
    const drive = (host, { code = 0, output = '' } = {}) => buildFirst(host, app, {
      say: line => said.push(line), wait: () => {}, build: async (args, log) => (builds.push({ args, log }), { code, output }),
    });

    // No build yet: built first, for either host, by the command `bun exact.mjs ios|mac` runs.
    for (const host of ['ios', 'macos']) assert.equal(buildReason(nativeBuild(host, app), app), 'there is no build yet');
    assert.throws(() => refuseBuild('ios', nativeBuild('ios', app), app), new RegExp(`^Error: run \\(cd '${dir}' && bun exact\\.mjs ios\\) first$`));
    assert.equal(await drive('ios'), 0);
    // The drive installs, keeping a Release build it replaces: the build installs nothing.
    assert.deepEqual(builds.map(b => b.args), [['--ios', 'field-log-apple', '--no-install']]);
    assert.equal(builds[0].log, resolve(dir, '.exact/ios-build.log'));
    assert.equal(said.length, 1);
    assert.match(said[0], /^building for iOS: there is no build yet; its log: \S*\.exact\/ios-build\.log$/);
    assert.deepEqual(buildArgs('macos', app), ['field-log-apple']);

    // A build its receipt shows fresh: nothing runs, nothing is said.
    const bundle = appleArtifacts(app, { destination: 'ios-simulator' }).bundle, receipt = resolve(bundle, 'receipt.json');
    mkdirSync(bundle, { recursive: true });
    writeFileSync(receipt, JSON.stringify({ target: 'aarch64-apple-ios-sim' }));
    utimesSync(receipt, built, built);
    said.length = builds.length = 0;
    assert.equal(buildReason(nativeBuild('ios', app), app), null);
    assert.doesNotThrow(() => refuseBuild('ios', nativeBuild('ios', app), app));
    assert.equal(await drive('ios'), 0);
    assert.deepEqual([said, builds], [[], []]);

    // A source edited since: the same refusal the driver makes, and the build first instead, naming what changed.
    utimesSync(resolve(dir, 'app.contract'), edited, edited);
    assert.deepEqual(nativeBuild('ios', app).changed.map(p => resolve(p)), [resolve(dir, 'app.contract')]);
    assert.throws(() => refuseBuild('ios', nativeBuild('ios', app), app), /^Error: ios build is stale: .*app\.contract changed since .*; run \(cd .* && bun exact\.mjs ios\)$/);
    assert.equal(await drive('ios'), 0);
    // (A contract edit alone is a plan-only edit, which a build with no module receipt to check its bake against still rebuilds.)
    assert.deepEqual([said, builds.map(b => b.args)], [['building for iOS: the build is older than its sources (app.contract changed), and the build has no module receipt to check a new plan against; its log: ' + said[0].split('its log: ')[1]], [['--ios', 'field-log-apple', '--no-install']]]);

    // A scratch file in .exact/ (the build's log among them) is not a source.
    utimesSync(resolve(dir, 'app.contract'), before, before);
    write('.exact/ios-build.log', 'Compiling …');
    write('.exact/notes.json', '{}');
    assert.equal(buildReason(nativeBuild('ios', app), app), null);

    // A failed build fails the command with what it said: from its first error, then where its whole log is.
    utimesSync(resolve(dir, 'app.contract'), edited, edited);
    said.length = builds.length = 0;
    const output = '   Compiling exact-kernel v0.1.0\n   Compiling field-log-apple v0.1.0\nerror[E0425]: cannot find value `x` in this scope\n --> src/lib.rs:3:5\nerror: could not compile `field-log-apple`\ncargo rustc exited 101\n';
    assert.equal(await drive('ios', { code: 101, output }), 101);
    assert.equal(said.length, 2);
    assert.ok(said[1].startsWith('error[E0425]: cannot find value `x` in this scope\n --> src/lib.rs:3:5\n'), said[1]);
    assert.ok(!said[1].includes('Compiling'), 'the summary leaves out the progress');
    assert.match(said[1], /\ncargo rustc exited 101\nthe iOS build failed \(exit 101\), so nothing was driven; its whole log: \S*\.exact\/ios-build\.log$/);
    assert.match(failureSummary('a\nb\nc\n', { host: 'macos', code: 1, log: 'x.log' }), /^a\nb\nc\nthe macOS build failed \(exit 1\)/, 'no error line: the tail');

    // macOS: its binary is the record; a binary EXACT_MAC_BIN names is the caller's, never built.
    said.length = builds.length = 0;
    assert.equal(await drive('macos'), 0);
    assert.deepEqual([said.length, builds.map(b => b.args)], [1, [['field-log-apple']]]);
    assert.match(said[0], /^building for macOS: there is no build yet/);
    mkdirSync(dirname(appleRunBinary(app)), { recursive: true });
    writeFileSync(appleRunBinary(app), '');
    assert.equal(buildReason(nativeBuild('macos', app), app), null, 'a Mac binary without a receipt is not refused');
    process.env.EXACT_MAC_BIN = '/nowhere/bin';
    said.length = builds.length = 0;
    assert.equal(await buildFirst('mac', app, { say: line => said.push(line), wait: () => assert.fail('waited'), build: () => assert.fail('built') }), 0);
    assert.deepEqual(said, []);

    // An exact.mjs from before test and agent built first is told how to get that.
    assert.equal(rebuildCommand(app, 'ios'), `(cd '${dir}' && bun exact.mjs ios)`);
    write('exact.mjs', "const cli = resolve(EXACT, 'scripts/app-cli.mjs');\n"); // the launcher (LLP 1116.006 D7)
    assert.equal(rebuildCommand(app, 'ios'), `(cd '${dir}' && bun exact.mjs ios)`);
    write('exact.mjs', '// an older one\n');
    assert.equal(rebuildCommand(app, 'mac'), `(cd '${dir}' && bun exact.mjs mac) (after bun exact.mjs update, test and agent build it themselves)`);
  } finally {
    if (previous === undefined) delete process.env.EXACT_APP_DIR; else process.env.EXACT_APP_DIR = previous;
    if (macBin === undefined) delete process.env.EXACT_MAC_BIN; else process.env.EXACT_MAC_BIN = macBin;
    rmSync(dir, { recursive: true, force: true });
  }
});

// LLP 1116.008 D2: never a stale build. The receipt is written at the build's end, so a file edited while the build ran
// is older than it, though the build read the file before the edit (polls: a prebuild tested the old UI); the receipt's
// `started` bounds it. And a build another process claims between the check and this one is waited for, not failed on.
test('a source edited while the build ran is stale; a build claimed meanwhile is waited for, then the check is made again', async () => {
  const previous = process.env.EXACT_APP_DIR, macBin = process.env.EXACT_MAC_BIN;
  const { dir, write } = scratchApp('race-log');
  try {
    process.env.EXACT_APP_DIR = dir;
    delete process.env.EXACT_MAC_BIN;
    const app = resolveApp('race-log');
    const at = seconds => new Date(Date.now() + seconds * 1000), stamp = (file, seconds) => utimesSync(resolve(dir, file), at(seconds), at(seconds));
    for (const file of ['app.contract', 'app.json', 'exact.mjs', 'apple/Cargo.toml', 'web/Cargo.toml']) stamp(file, -60);
    const bundle = appleArtifacts(app, { destination: 'ios-simulator' }).bundle, receipt = resolve(bundle, 'receipt.json');
    mkdirSync(bundle, { recursive: true });
    // Built from t-20 s to t+10 s; the contract edited at t-5 s, while it ran.
    const built = (started) => { writeFileSync(receipt, JSON.stringify({ target: 'aarch64-apple-ios-sim', ...(started ? { started: at(-20).toISOString() } : {}) })); utimesSync(receipt, at(10), at(10)); };
    stamp('app.contract', -5);
    built(false);
    assert.deepEqual(nativeBuild('ios', app).changed, [], 'a receipt without `started` is trusted by its time, as it was');
    built(true);
    assert.deepEqual(nativeBuild('ios', app).changed.map(p => resolve(p)), [resolve(dir, 'app.contract')]);
    assert.match(buildReason(nativeBuild('ios', app), app), /^the build is older than its sources \(app\.contract changed\)$/);
    stamp('app.contract', -60); // read before the build began: fresh
    assert.deepEqual(nativeBuild('ios', app).changed, []);

    // Two drives started together: the second's build finds the claim taken, waits for the first's, and finds it fresh.
    stamp('app.contract', 60);
    const said = [], builds = [];
    let waits = 0;
    const code = await buildFirst('ios', app, {
      say: line => said.push(line), wait: () => { waits++; },
      build: async (args) => { builds.push(args); stamp('app.contract', -60); built(true); return { code: 1, output: `Error: Apple build busy for com.exact.race-log: {"pid":1} (${dir}/.apple-build.lock); remove this lock explicitly\n` }; },
    });
    assert.deepEqual([code, builds.length, waits], [0, 1, 2]);
    assert.equal(said.length, 1);
    assert.match(said[0], /^building for iOS: the build is older than its sources/);
    // Another failure is still the build's: said, with its code.
    stamp('app.contract', 120);
    said.length = 0;
    assert.equal(await buildFirst('ios', app, { say: line => said.push(line), wait: () => {}, build: async () => ({ code: 101, output: 'error: could not compile\n' }) }), 101);
    assert.match(said.at(-1), /the iOS build failed \(exit 101\)/);
  } finally {
    if (previous === undefined) delete process.env.EXACT_APP_DIR; else process.env.EXACT_APP_DIR = previous;
    if (macBin === undefined) delete process.env.EXACT_MAC_BIN; else process.env.EXACT_MAC_BIN = macBin;
    rmSync(dir, { recursive: true, force: true });
  }
});

// The Apple build signs a copy of the GPU dylib and the app's bake reads it, so the archive's dep-info names a file the
// build itself wrote after it started. Cargo leaves that dep-info as it was when a relink names the same paths, so its
// time says nothing: the receipt's digest of the product decides, and every other input keeps the build's start.
test('a product the build wrote and then read is judged by its recorded bytes; any other input by the build start', () => {
  const previous = process.env.EXACT_APP_DIR, macBin = process.env.EXACT_MAC_BIN, targetDir = process.env.CARGO_TARGET_DIR, cwd = process.cwd();
  const { dir, write } = scratchApp('gpu-log');
  try {
    process.env.EXACT_APP_DIR = dir;
    delete process.env.EXACT_MAC_BIN; delete process.env.CARGO_TARGET_DIR;
    const app = resolveApp('gpu-log');
    const at = seconds => new Date(Date.now() + seconds * 1000), stamp = (file, seconds) => utimesSync(file, at(seconds), at(seconds));
    for (const file of ['app.contract', 'app.json', 'exact.mjs', 'apple/Cargo.toml', 'web/Cargo.toml']) stamp(resolve(dir, file), -120);
    const bundle = appleArtifacts(app, { destination: 'ios-simulator' }).bundle, receipt = resolve(bundle, 'receipt.json');
    mkdirSync(bundle, { recursive: true });
    const profile = resolve(app.target, 'aarch64-apple-ios-sim', 'host-dev'), dylib = resolve(profile, 'signed/libgpu_log_gpu.dylib');
    const info = resolve(profile, 'libgpu_log_apple.d'), generated = resolve(profile, 'generated.contract');
    const sha256 = text => createHash('sha256').update(text).digest('hex');
    const changed = () => nativeBuild('ios', app).changed.map(p => resolve(p)).sort();
    // The first build, t-100 s to t-70 s, wrote the dep-info at t-80 s.
    write(dylib.slice(dir.length + 1), 'signed v1');
    write(generated.slice(dir.length + 1), 'component Generated');
    writeFileSync(info, `${profile}/libgpu_log_apple.a: ${resolve(dir, 'app.contract')} ${generated} ${dylib}\n`);
    stamp(info, -80); stamp(generated, -110);
    // A GPU source edited, and built again from t-20 s to t+10 s: the copy signed again at t-10 s, the archive relinked,
    // and the dep-info, naming the same paths, left as it was.
    writeFileSync(dylib, 'signed v2'); stamp(dylib, -10);
    writeFileSync(receipt, JSON.stringify({ target: 'aarch64-apple-ios-sim', started: at(-20).toISOString(), build: { products: [{ path: dylib, bytes: 9, sha256: sha256('signed v2') }] } }));
    stamp(receipt, 10);
    assert.deepEqual(changed(), []);
    // Other bytes, though put back with the time it had: not what the bundle holds.
    writeFileSync(dylib, 'signed v3'); stamp(dylib, -10);
    assert.deepEqual(changed(), [dylib]);
    rmSync(dylib);
    assert.deepEqual(changed(), [dylib]);
    write(dylib.slice(dir.length + 1), 'signed v2');
    assert.deepEqual(changed(), []);
    // A source under the profile directory that the receipt does not name as a product keeps the build's start.
    stamp(generated, -5);
    assert.deepEqual(changed(), [generated]);
    stamp(generated, -110); stamp(resolve(dir, 'app.contract'), -5);
    assert.deepEqual(changed(), [resolve(dir, 'app.contract')]);
    stamp(resolve(dir, 'app.contract'), -120);
    // `build.dep-info-basedir` writes the paths relative to the workspace Cargo ran in, and the drive runs from
    // elsewhere (here neither this checkout nor the workspace): both rules read them against that workspace.
    const rel = path => relative(app.workspace, path);
    writeFileSync(info, `${rel(profile)}/libgpu_log_apple.a: ${rel(resolve(dir, 'app.contract'))} ${rel(generated)} ${rel(dylib)}\n`);
    stamp(info, -80);
    process.chdir(dirname(dir));
    assert.notEqual(realpathSync(process.cwd()), app.workspace);
    assert.deepEqual(changed(), []);
    writeFileSync(dylib, 'signed v3'); stamp(dylib, -10);
    assert.deepEqual(changed(), [dylib]);
    writeFileSync(dylib, 'signed v2'); stamp(dylib, -10); stamp(generated, -5);
    assert.deepEqual(changed(), [generated]);
    stamp(generated, -110);
    // Assets watched through a directory need no individual dep-info entry.
    const model = resolve(dir, 'assets/crate.model');
    write('assets/crate.model', 'baked model'); stamp(model, -10);
    const record = JSON.parse(readFileSync(receipt, 'utf8'));
    record.build.products.push({path:model, bytes:11, sha256:sha256('baked model'), input:true});
    record.build.watchedDirectories = [directoryInputSnapshot(dirname(model))];
    writeFileSync(info, readFileSync(info, 'utf8').trimEnd() + ` ${dirname(model)}\n`);
    writeFileSync(receipt, JSON.stringify(record)); stamp(receipt, 10);
    assert.deepEqual(changed(), []);
    writeFileSync(model, 'other model'); stamp(model, -110);
    assert.deepEqual(changed(), [model]);
    rmSync(model);
    assert.deepEqual(changed(), [model]);
    write('assets/crate.model', 'baked model');
    assert.deepEqual(changed(), []);
    write('assets/authored.model', 'authored'); stamp(resolve(dir, 'assets/authored.model'), -110);
    assert.deepEqual(changed(), [dirname(model)]);
    rmSync(resolve(dir, 'assets/authored.model'));
    assert.deepEqual(changed(), []);
  } finally {
    process.chdir(cwd);
    if (previous === undefined) delete process.env.EXACT_APP_DIR; else process.env.EXACT_APP_DIR = previous;
    if (macBin === undefined) delete process.env.EXACT_MAC_BIN; else process.env.EXACT_MAC_BIN = macBin;
    if (targetDir !== undefined) process.env.CARGO_TARGET_DIR = targetDir;
    rmSync(dir, { recursive: true, force: true });
  }
});

test('generated shell outputs a cold game build records are judged by their bytes; authored inputs keep their mtime', () => {
  const { dir, write } = scratchApp('shell-output');
  try {
    const target = resolve(dir, 'target');
    const app = { dir, workspace: resolve(dir, '.shells'), target, manifest: { game: {} }, crate: () => 'shell-output-apple' };
    const before = new Date(Date.now() - 60_000), started = new Date(+before + 20_000), generated = new Date(+started + 10_000);
    const stamp = (path, at) => utimesSync(resolve(dir, path), at, at);
    const digest = path => createHash('sha256').update(readFileSync(resolve(dir, path))).digest('hex');
    const outputs = ['app.json', 'keys.json', 'surfaces.json', 'apple/build.rs', 'apple/src/lib.rs'].map(path => `.shells/${path}`);
    write('.shells/Cargo.toml', '[workspace]');
    write('logic/src/lib.rs', 'pub struct Game;');
    write('.shells/apple/src/extra.rs', 'pub const EXTRA: u32 = 1;');
    for (const path of outputs) write(path, path.endsWith('.json') ? '{}' : '// generated shell');
    for (const path of ['app.json', 'app.contract', 'exact.mjs', 'apple/Cargo.toml', 'web/Cargo.toml', '.shells/Cargo.toml', 'logic/src/lib.rs', '.shells/apple/src/extra.rs']) stamp(path, before);
    for (const path of outputs) stamp(path, generated);
    const sources = [...outputs, 'app.contract', 'logic/src/lib.rs', '.shells/apple/src/extra.rs'];
    write('target/aarch64-apple-ios-sim/host-dev/libshell_output_apple.d', `archive: ${sources.map(path => resolve(dir, path)).join(' ')}\n`);
    const inputs = ['logic/src/lib.rs'].map(path => ({ path: resolve(dir, path), name: resolve(dir, path), sha256: digest(path) }));
    const products = outputs.map(path => ({ path: resolve(dir, path), bytes: readFileSync(resolve(dir, path)).length, sha256: digest(path), input: true }));
    const receipt = resolve(target, 'receipt.json');
    const record = products => write('target/receipt.json', JSON.stringify({ target: 'aarch64-apple-ios-sim', started: started.toISOString(), build: {
      binary: { inputs, missing: [], directories: [] }, products,
    } }));
    record(products);
    const changed = () => receiptChanges(receipt, app).map(path => resolve(path)).sort();
    assert.deepEqual(changed(), [], 'files produced during the cold build are not later source edits');
    for (const path of ['app.json', 'app.contract', 'logic/src/lib.rs', '.shells/apple/src/extra.rs']) {
      stamp(path, generated);
      assert.ok(changed().includes(resolve(dir, path)), `mtime still checks ${path}`);
      stamp(path, before);
    }
    for (const path of [...outputs, 'logic/src/lib.rs']) {
      const previous = readFileSync(resolve(dir, path));
      write(path, 'changed bytes'); stamp(path, before);
      assert.ok(changed().includes(resolve(dir, path)), `the recorded digest still checks ${path}`);
      write(path, previous); stamp(path, path.startsWith('.shells/') ? generated : before);
    }
    record([]);
    assert.deepEqual(changed(), outputs.map(path => resolve(dir, path)).sort(), 'outputs the receipt did not record keep the mtime bound');
    record(products);
    rmSync(resolve(dir, '.shells/surfaces.json'));
    assert.ok(changed().includes(resolve(dir, '.shells/surfaces.json')), 'a missing declaration still needs a build');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('a build already running ends before the check: the drive neither races it nor is refused as busy', async () => {
  const previous = process.env.EXACT_APP_DIR;
  const { dir } = scratchApp('busy-log');
  try {
    process.env.EXACT_APP_DIR = dir;
    const app = resolveApp('busy-log'), lock = appleArtifacts(app).lock;
    mkdirSync(dirname(lock), { recursive: true });
    // This process holds the claim, as a background `bun exact.mjs ios` would, and lets it go in a moment.
    writeFileSync(lock, JSON.stringify({ pid: process.pid, app: app.id, source: dir, started: new Date().toISOString() }) + '\n');
    setTimeout(() => rmSync(lock), 400);
    const said = [], started = Date.now();
    const code = await buildFirst('ios', app, { say: line => said.push(line), build: async () => ({ code: 0, output: '' }) });
    assert.equal(code, 0);
    assert.ok(Date.now() - started >= 350, 'it waited for the claim');
    assert.deepEqual(said.map(l => l.split(':')[0]), ['waiting for the Apple build of busy-log already running', 'building for iOS']);
  } finally {
    if (previous === undefined) delete process.env.EXACT_APP_DIR; else process.env.EXACT_APP_DIR = previous;
    rmSync(dir, { recursive: true, force: true });
  }
});

test('a plan-only edit runs its bake on the existing build; any other edit, or a newer real build, retires it', async () => {
  const previous = process.env.EXACT_APP_DIR, trust = process.env.EXACT_UPDATE_TRUST;
  const { dir, write } = scratchApp('plan-log');
  try {
    process.env.EXACT_APP_DIR = dir;
    delete process.env.EXACT_UPDATE_TRUST;
    const app = resolveApp('plan-log');
    const at = offset => new Date(Date.now() + offset * 1000), stamp = (file, offset) => utimesSync(resolve(dir, file), at(offset), at(offset));
    // app.ts and what it imports, type-only imports included; a backend beside it that the app reaches over HTTP.
    write('app.ts', "import type { Book } from './types';\nimport { shelf } from './lib/model.js';\nimport seed from './lib/seed.json';\nimport { money } from './lib/format.mjs';\nexport const grants = '';\n");
    write('types.ts', 'export type Book = { title: string };\n');
    write('lib/model.ts', "export const shelf = [] as const;\n");
    write('lib/seed.json', '[]\n');
    write('lib/format.mjs', "import { digits } from './digits.mjs';\nexport const money = n => digits(n);\n");
    write('lib/digits.mjs', 'export const digits = n => String(n);\n');
    // A helper script beside the app that app.ts never imports (an export, a seeding script).
    write('scripts/export.mjs', 'console.log(1);\n');
    write('server/index.ts', "import { Database } from 'bun:sqlite';\nimport { routes } from './routes';\n");
    write('server/routes.ts', 'export const routes = {};\n');
    write('server/fixtures.json', '{}\n');
    for (const file of ['app.contract', 'app.ts', 'types.ts', 'lib/model.ts', 'lib/seed.json', 'lib/format.mjs', 'lib/digits.mjs', 'scripts/export.mjs', 'server/index.ts', 'server/routes.ts', 'server/fixtures.json', 'app.json', 'exact.mjs', 'apple/Cargo.toml', 'web/Cargo.toml']) stamp(file, -60);
    // The build: its receipt names the plan its build script baked, beside which is the module receipt it paired.
    const target = 'aarch64-apple-ios-sim', out = resolve(app.target, target, 'host-dev/build/plan-log-apple-0123/out');
    const module = { version: 1, appId: 'com.exact.plan-log', grants: '', abi: 1, surfaces: {}, bytecodeVersion: 96 };
    mkdirSync(out, { recursive: true });
    writeFileSync(resolve(out, 'app.plan'), 'plan one');
    writeFileSync(resolve(out, 'app.module.json'), JSON.stringify(module));
    // The archive's dep-info also names every source the bake captured (its rerun-if-changed), the backend's among them.
    writeFileSync(resolve(app.target, target, `host-dev/lib${app.crate('ios').replace(/-/g, '_')}.d`), `${resolve(app.target, target, 'host-dev/libplan_log_apple.a')}: ${out}/app.plan ${out}/app.hbc ${['app.contract', 'app.ts', 'server/index.ts', 'server/fixtures.json'].map(f => resolve(dir, f)).join(' ')}\n`);
    const bundle = appleArtifacts(app, { destination: 'ios-simulator' }).bundle, receipt = resolve(bundle, 'receipt.json');
    const build = (inputs = {}, offset = 10, built = 'one') => {
      mkdirSync(bundle, { recursive: true });
      writeFileSync(receipt, JSON.stringify({ built, target, build: { compat: { inputs: { linked: ['markdown', 'router', 'io'], rustMode: 'off', dataCrate: null, ...inputs },
        embedded: { plan: { sha256: createHash('sha256').update('plan one').digest('hex') } } }, graph: { uses: ['router'] } } }));
      utimesSync(receipt, at(offset), at(offset));
    };
    build();
    const said = [], builds = [], bakes = [];
    let baked = { ...module }, bakeFails = null, bakeSays = '', buildSays = '';
    const bake = async (a, into) => {
      bakes.push(into);
      if (bakeFails) return { code: 1, output: bakeFails };
      mkdirSync(into);
      for (const [file, text] of [['app.plan', 'plan two'], ['app.hbc', 'bytecode'], ['app.module.json', JSON.stringify(baked)]]) writeFileSync(resolve(into, file), text);
      return { code: 0, output: bakeSays };
    };
    const drive = (code = 0) => buildFirst('ios', app, { say: line => said.push(line), wait: () => {}, bake,
      build: async args => (builds.push(args), { code, output: buildSays }) });
    const reset = () => { said.length = builds.length = bakes.length = 0; };
    const kept = () => planOverride('ios', app, nativeBuild('ios', app));

    // What app.ts imports is the plan's; what it does not import is no input at all (LLP 1116.003 D1.1).
    assert.deepEqual([...moduleGraph(dir, app.manifest)].map(p => p.slice(dir.length + 1)).sort(), ['app.ts', 'lib/digits.mjs', 'lib/format.mjs', 'lib/model.ts', 'lib/seed.json', 'types.ts']);
    // A script app.ts never imports is no input, a `.mjs` as a `.ts`; one it imports, through another, is.
    const input = notBuildInput(dir);
    assert.deepEqual(['scripts/export.mjs', 'lib/format.mjs', 'lib/digits.mjs', 'server/index.ts'].map(f => input(resolve(dir, f))), [true, false, false, true]);
    // Only the Contract and what app.ts imports, and nothing a native build reads, is a plan-only edit.
    for (const [path, only] of [['app.contract', true], ['app.ts', true], ['types.ts', true], ['lib/model.ts', true], ['lib/seed.json', true], ['screens/list.contract', true],
      ['server/index.ts', false], ['server/fixtures.json', false], ['app.json', false], ['app.contract.d.ts', false],
      ['assets/data.ts', false], ['apple/src/lib.rs', false], ['web/x.ts', false], ['fonts/a.ttf', false], ['../outside.ts', false]]) assert.equal(planSource(resolve(dir, path), app), only, path);
    write('data/Cargo.toml', '[package]\nname = "plan-log-data"\n');
    assert.equal(planSource(resolve(dir, 'data/answers.ts'), app), false, "a Rust crate's own files are its build's");
    rmSync(resolve(dir, 'data'), { recursive: true });

    // A fresh build: nothing runs, nothing is kept, the driver runs the build's own plan. An edit to the backend,
    // or to the declarations a web build writes beside app.ts, leaves it fresh (round 5's book-club rebuilt for server/index.ts).
    write('server/index.ts', "import { Database } from 'bun:sqlite';\nimport { routes } from './routes';\nexport const port = 9610;\n");
    write('app.contract.d.ts', 'export type Answer = unknown;\n');
    write('seed.json', '[{"table":"owners","row":{"person":"dev:alice"}}]\n');
    write('.exact/seed-log.json', '{}\n');
    stamp('server/index.ts', 15); stamp('app.contract.d.ts', 15);
    stamp('seed.json', 15); stamp('.exact/seed-log.json', 15);
    assert.deepEqual(nativeBuild('ios', app).changed, []);
    assert.equal(await drive(), 0);
    assert.deepEqual([said, builds, bakes, kept()], [[], [], [], null]);

    // A contract edit: baked and kept beside the build, in one line, and no build; the driver runs it. The
    // compiler's warnings the bake said follow it, one line each the first time, then a count naming their file.
    write('app.contract', 'component App\n  view\n    text "hello again" color="#336699"\n');
    stamp('app.contract', 20);
    const warning = 'app.contract:3:31 warning [lower-colour-one-scheme] `#336699` on `color` stays the same in dark mode';
    bakeSays = `${warning}\n{"version":1}\n`;
    assert.throws(() => refuseBuild('ios', nativeBuild('ios', app), app), /ios build is stale/, 'before the bake, the driver refuses as before');
    assert.equal(await drive(), 0);
    assert.deepEqual([said.slice(0, 2), builds, bakes.length], [['iOS: app.contract changed; running the new plan on the existing build (no native rebuild). Only the agent\'s drives run it: a launch outside them (`simctl launch`, the home screen) runs the installed plan, so `bun exact.mjs ios` before timing one', warning], [], 1]);
    assert.equal(said.length, 3);
    assert.match(said[2], /^compiler: 1 warning; \S+\/\.exact\/warnings-ios\.txt lists them all$/);
    bakeSays = '';
    assert.deepEqual(kept(), { plan: resolve(overrideDir('ios', app), 'app.plan'), dir: overrideDir('ios', app) });
    assert.equal(readFileSync(kept().plan, 'utf8'), 'plan two');
    assert.ok(!existsSync(bakes[0]), 'the bake is staged, then moved beside the build');

    // Driven again with nothing edited: it runs as it is, saying nothing.
    reset();
    assert.equal(await drive(), 0);
    assert.deepEqual([said, builds, bakes], [[], [], []]);

    // The TypeScript edited since: what is kept is no longer the bake of these sources; the driver refuses it, test and agent bake again.
    write('app.ts', "export const grants = '';\nexport const more = 1;\n");
    stamp('app.ts', 30);
    assert.equal(kept(), null);
    assert.throws(() => refuseBuild('ios', nativeBuild('ios', app), app), /ios build is stale/);
    reset();
    assert.equal(await drive(), 0);
    assert.deepEqual([said.length, builds.length, bakes.length], [1, 0, 1]);
    assert.match(said[0], /^iOS: (app\.contract, app\.ts|app\.ts, app\.contract) changed; running the new plan on the existing build \(no native rebuild\)\. Only the agent's drives run it: .*`bun exact\.mjs ios` before timing one$/);
    assert.ok(kept());

    // A newer real build retires it: fresh, the build's own plan runs; a later touch of the same sources does not bring it back.
    build({}, 40, 'two');
    assert.equal(kept(), null);
    stamp('app.contract', 50);
    assert.deepEqual(nativeBuild('ios', app).changed.map(p => resolve(p)), [resolve(dir, 'app.contract')]);
    assert.equal(kept(), null, 'kept for another build: never run as fresh');

    // Anything else edited is built, and what was kept goes with it; the warnings Cargo said for the bake's
    // build script follow, in the compiler's own form: in full, since the bake before said none.
    reset();
    stamp('app.json', 60);
    buildSays = `   Compiling plan-log-apple v0.1.0\nwarning: plan-log-apple@0.1.0: ${warning}\nhost/apple: built\n`;
    assert.equal(await drive(), 0);
    assert.deepEqual([bakes.length, builds.length, said[1], said.length], [0, 1, warning, 3]);
    // Built again over the same warning: its count alone.
    reset();
    stamp('app.json', 70);
    assert.equal(await drive(), 0);
    buildSays = '';
    assert.equal(said.length, 2);
    assert.match(said[1], /^compiler: 1 warning, as before \(\S+\/warnings-ios\.txt lists them; /);
    assert.match(said[0], /^building for iOS: the build is older than its sources \((app\.contract, app\.json|app\.json, app\.contract) changed\); its log: /);
    assert.ok(!existsSync(overrideDir('ios', app)));

    // What a build must still make: new grants, a Rust data crate, a build that links only what its plan used.
    for (const [setup, why] of [
      [() => { baked = { ...module, grants: 'net.fetch https://api.example' }; }, 'app.ts changed its grants, which only a build admits'],
      [() => build({ dataCrate: 'plan-log-data' }, 70), 'the app has a Rust data crate, whose plan only a build bakes'],
      [() => build({ linked: ['router', 'io'] }, 70), 'the build links only what its plan used (--link plan)'],
    ]) {
      build({}, 70); baked = { ...module }; setup(); reset();
      stamp('app.json', -60); stamp('app.contract', 80);
      assert.equal(await drive(), 0);
      assert.deepEqual([builds.length, kept()], [1, null], why);
      assert.match(said[0], new RegExp(`^building for iOS: the build is older than its sources \\(app\\.contract changed\\), and ${why.replace(/[().]/g, '\\$&')}; its log: `));
    }

    // A bake the compiler refuses fails the command with what it said, as the build would have; nothing is built or driven.
    build({}, 70); baked = { ...module }; reset();
    bakeFails = 'TypeScript bake: app.contract:3:5: unknown tag `txet`';
    assert.equal(await drive(), 1);
    assert.deepEqual([builds.length, kept()], [0, null]);
    assert.match(said[0], /^TypeScript bake: app\.contract:3:5: unknown tag `txet`\nthe iOS plan did not bake \(app\.contract changed\), so nothing was driven; its whole log: \S*\.exact\/ios-plan\.log$/);
  } finally {
    if (previous === undefined) delete process.env.EXACT_APP_DIR; else process.env.EXACT_APP_DIR = previous;
    if (trust === undefined) delete process.env.EXACT_UPDATE_TRUST; else process.env.EXACT_UPDATE_TRUST = trust;
    rmSync(dir, { recursive: true, force: true });
  }
});

// A drive's compiler warnings keep the compiler's order, the symbols only the web draws blank one line at the end;
// the file beside the app lists each (the euxy port's header warning was one of 43, behind 21 of them).
// A background build ends apart from every shell, so the next build or drive says once how it failed (the euxy port's
// `--prebuild --release`, failed by an edit, said so only in its log).
test('a failed background build is said once by the next drive, from its part of the log', async () => {
  const previous = process.env.EXACT_APP_DIR;
  const { dir, write } = scratchApp('prebuilt-log');
  try {
    process.env.EXACT_APP_DIR = dir;
    const app = resolveApp('prebuilt-log'), said = [];
    // The drive rebuilds, writing the build log anew.
    const drive = () => buildFirst('ios', app, { say: line => said.push(line), wait: () => {}, build: async (args, log) => (writeFileSync(log, 'rebuilt'), { code: 0, output: '' }) });
    const build = 'app: the bake failed:\napp.contract:14:10 [syntax-expected] expected `)`, found `like`\n';
    write('.exact/ios-build.log', `${build}   Compiling the bake tool\n`);
    write('.exact/ios-prebuild.json', JSON.stringify({ args: ['--ios', 'prebuilt-log-apple', '--release'], started: '08:02:54', pid: 1, exit: 1, bytes: build.length }));
    await drive();
    assert.match(said[0], /^app: the bake failed:\napp\.contract:14:10 \[syntax-expected\] expected `\)`, found `like`\nthe background iOS build \(--prebuild --ios --release, started 08:02:54\) failed \(exit 1\); its whole log: \S*\.exact\/ios-prebuild-failed\.log$/);
    assert.equal(readFileSync(resolve(dir, '.exact/ios-prebuild-failed.log'), 'utf8'), build, 'its part, kept from the rebuild');
    assert.ok(!existsSync(resolve(dir, '.exact/ios-prebuild.json')), 'said once');
    said.length = 0;
    await drive();
    assert.ok(!said.some(line => line.includes('background')), said.join('\n'));
    // Changed sources explain the older failure; signing or linking may still fail.
    write('app.ts', 'export const grants = \'\';');
    const sourceRecord = () => ({ id: 'old', args: ['--ios'], started: '08:02:54', pid: 1, exit: 1, bytes: build.length,
      sources: { ...planSources(app), [resolve(dir, 'app.json')]: createHash('sha256').update(readFileSync(resolve(dir, 'app.json'))).digest('hex') } });
    write('.exact/ios-build.log', build);
    write('.exact/ios-prebuild.json', JSON.stringify(sourceRecord()));
    said.length = 0;
    await drive();
    assert.ok(said.some(line => line.includes('bake failed')), 'a failure with matching sources is still reported');
    for (const file of ['app.ts', 'app.contract', 'app.json']) {
      const record = sourceRecord(), original = readFileSync(resolve(dir, file), 'utf8');
      write('.exact/ios-prebuild.json', JSON.stringify(record));
      write(file, file === 'app.json' ? original.replace(/}\s*$/, ', "env": {"API_URL": {}} }') : original + '\n');
      write('.exact/ios-build.log', build);
      said.length = 0;
      await drive();
      assert.ok(said[0].startsWith("This failure is from a background build started before the app's sources last changed; the current sources are checked now.\n"), said.join('\n'));
      assert.ok(said[0].includes(build.trim()) && said[0].includes('failed (exit 1)'), said[0]);
      assert.equal(readFileSync(resolve(dir, '.exact/ios-prebuild-failed.log'), 'utf8'), build);
      assert.ok(said.some(line => line.startsWith('building for iOS:')), 'the current build still runs');
      assert.ok(!existsSync(resolve(dir, '.exact/ios-prebuild.json')));
      write(file, original);
    }
    said.length = 0;
    // One that ended before it could say (its shell gone) is dropped, unsaid.
    write('.exact/ios-prebuild.json', JSON.stringify({ args: ['--ios'], started: '08:02:54', pid: 2 ** 22 + 1 }));
    await drive();
    assert.ok(!said.some(line => line.includes('background')) && !existsSync(resolve(dir, '.exact/ios-prebuild.json')));
    // A second started while a build holds the claim starts nothing, and leaves the first's log and record.
    const lock = appleArtifacts(app).lock;
    mkdirSync(dirname(lock), { recursive: true });
    writeFileSync(lock, JSON.stringify({ pid: process.pid, app: app.id, source: dir, started: new Date().toISOString() }) + '\n');
    write('.exact/ios-prebuild.json', '{"id":"first"}');
    assert.equal(await prebuild(app, ['--ios', 'prebuilt-log-apple', '--prebuild'], { say: line => said.push(line) }), 1);
    assert.match(said.at(-1), /already running, so none was started/);
    assert.equal(readFileSync(resolve(dir, '.exact/ios-prebuild.json'), 'utf8'), '{"id":"first"}');
    assert.equal(readFileSync(resolve(dir, '.exact/ios-build.log'), 'utf8'), 'rebuilt');
    rmSync(lock);
    // So does one started before the first's build has claimed it: that record names a live shell.
    write('.exact/ios-prebuild.json', JSON.stringify({ id: 'first', pid: process.pid }));
    assert.equal(await prebuild(app, ['--ios', 'prebuilt-log-apple', '--prebuild'], { say: line => said.push(line) }), 1);
    assert.equal(JSON.parse(readFileSync(resolve(dir, '.exact/ios-prebuild.json'), 'utf8')).id, 'first');
    assert.equal(readFileSync(resolve(dir, '.exact/ios-build.log'), 'utf8'), 'rebuilt');
  } finally {
    if (previous === undefined) delete process.env.EXACT_APP_DIR; else process.env.EXACT_APP_DIR = previous;
    rmSync(dir, { recursive: true, force: true });
  }
});

test('a drive says the web-only symbol warnings in one line and lists each in its file', async () => {
  const { warningLines } = await import('./app-cli.mjs');
  const dir = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-warning-lines-')));
  try {
    const header = 'app.contract:9:3 warning [lower-header-unplaced] this `button` in this route\'s `header` is not shown on iOS';
    const first = 'app.contract:4:5 warning [lower-symbol-platform] symbol `sf/a` draws only on Apple, and the app ships web (app.json `host.web`), where it is blank. Declare an icon token';
    const later = 'app.contract:5:5 warning [lower-symbol-platform] symbol `sf/b` is blank on web too: write `symbol:add`, the role, which draws on every host';
    const native = 'app.contract:6:5 warning [lower-symbol-platform] symbol `sf/c` is blank on linux and web too: write `symbol:add`, the role, which draws on every host';
    const said = warningLines(dir, 'ios', [header, native, first, later]);
    assert.deepEqual(said.slice(0, 2), [header, native]);
    assert.match(said[2], /^app\.contract:4:5 warning \[lower-symbol-platform\] 2 SF Symbol uses draw blank on the web, here first: .*\.exact\/warnings-ios\.txt lists each, and how\)$/);
    assert.match(said[3], /^compiler: 4 warnings; /);
    assert.equal(readFileSync(resolve(dir, '.exact', 'warnings-ios.txt'), 'utf8'), [header, native, first, later].map((w) => `${w}\n`).join(''));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('a drive counts a warning that is information apart (tamber-unreachable, LLP 1121 D9)', async () => {
  const { warningLines } = await import('./app-cli.mjs');
  const dir = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-warning-notes-')));
  try {
    const note = 'app.contract:3:5 warning [tamber-unreachable] accent-color on macOS switches: NSSwitch draws the system accent (the first at line 4; LLP 1121 D9)';
    const header = 'app.contract:9:3 warning [lower-header-unplaced] this `button` in this route\'s `header` is not shown on iOS';
    assert.match(warningLines(dir, 'web', [note]).at(-1), /^compiler: 1 note \(not counted\); /);
    assert.match(warningLines(dir, 'web', [header, note]).at(-1), /^compiler: 1 warning and 1 note \(not counted\), 1 new \(above\); /);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

// What a failed JS-target web build says (host/web/build.mjs): a diagnostic whose marker line is bare keeps the
// paragraphs below it, as wasm-bindgen's schema mismatch says them; Cargo's progress and Bun's trace stay out.
test('a failed web build keeps the explanation under a bare error marker', async () => {
  const { webBuildFailure } = await import('./app-cli.mjs');
  const said = [
    '   Compiling demo-gpu v0.1.0 (/tmp/demo/gpu)',
    '    Finished `web` profile [optimized] target(s) in 1.20s',
    'error: ',
    '',
    'it looks like the Rust project used to create this Wasm file was linked against',
    'version of wasm-bindgen that uses a different bindgen format than this binary:',
    '',
    '  rust Wasm file schema version: 0.2.130',
    '     this binary schema version: 0.2.127',
    '',
    'Currently the bindgen format is unstable enough that these two schema versions',
    'must exactly match.',
    '',
    'demo: wasm-bindgen demo-gpu failed',
    '',
  ].join('\n');
  const child = spawnSync(process.execPath, ['-e', `process.stderr.write(${JSON.stringify(said)}); process.exit(1)`], { encoding: 'utf8' });
  assert.equal(child.status, 1);
  assert.deepEqual(webBuildFailure(child.stderr), [said.split('\n').slice(2, -1).join('\n')]);
  // A diagnostic that says its message on its line keeps that line alone: a rustc error's excerpt and Bun's trace stay out.
  const rustc = 'error[E0308]: mismatched types\n --> gpu/src/lib.rs:3:5\n  |\n3 |     1u8\n  |     ^^^ expected `u32`\n';
  const trace = 'error: no plan\n      at main (/x/build.mjs:1:1)\n\nBun v1.3.0 (macOS arm64)\n';
  assert.deepEqual(webBuildFailure(rustc + trace), ['error[E0308]: mismatched types\nerror: no plan']);
  // The capture's size caps say which packages (js/bake/src/typescript.mjs `stagePackages`).
  const capped = 'package declarations exceed 64 MiB: heavy 69.0 MiB (from page/main.ts). The type check stages …';
  assert.deepEqual(webBuildFailure(`${capped}\nerror: app: app.ts failed the type check every host's build runs\n`), [`${capped}\nerror: app: app.ts failed the type check every host's build runs`]);
  for (const refusal of [
    '@scratch/module@1.0.0 requires peer @scratch/peer@^1.4.0, but the app has @scratch/peer@2.0.0; install a compatible version',
    'package declarations: two copies of pkg (/app/node_modules/pkg, /module/node_modules/pkg); install one',
  ]) {
    const stderr = `${refusal}\nerror: app: app.ts failed the type check every host's build runs`;
    assert.deepEqual(webBuildFailure(stderr), [stderr]);
  }
});

// An Apple build's bake says its warnings on every build: the SF Symbols only the web draws blank one line where the
// first was (euxy's 18, every Release build), the rest each.
test('an Apple bake says the web-only symbol warnings in one line', async () => {
  const { bakeWarnings } = await import('./linked-plan.mjs');
  const alias = 'app.contract:2:5 warning [lower-symbol-alias] symbol `sf/arrow.up.arrow.down` is the role `sort`: write `symbol:sort`';
  const first = 'app.contract:4:5 warning [lower-symbol-platform] symbol `sf/a` draws only on Apple, and the app ships web (app.json `host.web`), where it is blank. Declare an icon token';
  const later = 'app.contract:5:5 warning [lower-symbol-platform] symbol `sf/b` is blank on web too: write `symbol:add`, the role, which draws on every host';
  assert.deepEqual(bakeWarnings('euxy', [alias, first, later]), [
    `warning: euxy: ${alias}`,
    'warning: euxy: app.contract:4:5 warning [lower-symbol-platform] 2 SF Symbol uses draw blank on the web, here first: an icon token or a role draws on every host (`bun exact.mjs contract build --json` says each, and how)',
  ]);
  assert.deepEqual(bakeWarnings('euxy', [alias]), [`warning: euxy: ${alias}`]);
});

// LLP 1156.000 D6: a long build says it is still working, with the log's last step, and names who holds a lock Cargo
// waits on, instead of one line and minutes of silence.
test('a long build says how long it has run and its last step, then a lock it waits on', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-logged-')), said = [];
  try {
    const script = 'console.log("   Compiling exact-kernel v0.1.0"); await Bun.sleep(400); console.error("    Blocking waiting for file lock on package cache"); await Bun.sleep(400);';
    const { code } = await logged(process.execPath, ['-e', script], resolve(dir, 'build.log'), { say: line => said.push(line), every: 100 });
    assert.equal(code, 0);
    // A tick may come before the child has said anything (a loaded machine starts Bun slowly); a later one quotes its step.
    assert.ok(said.some(line => /^ {2}still building \(\d+ s\): Compiling exact-kernel v0\.1\.0$/.test(line)), said.join('\n'));
    assert.ok(said.some(line => /Blocking waiting for file lock on package cache, (held by|cargo running|its holder)/.test(line)), said.join('\n'));
    assert.match(readFileSync(resolve(dir, 'build.log'), 'utf8'), /Compiling exact-kernel/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 30_000); // Naming the lock's holder runs `lsof`, which takes seconds on a Mac running many processes.

test("a package-cache lock's holder is named by the process that has the file open", () => {
  const calls = [];
  const run = (cmd, args) => { calls.push(cmd); return { stdout: cmd === 'lsof' ? '4242\n' : cmd === 'ps' ? '4242 /Users/me/.rustup/toolchains/1.97.0/bin/cargo\n' : '' }; };
  // Its pid and executable, never its arguments, which can carry a secret.
  assert.equal(lockHolder('Blocking waiting for file lock on package cache', run), 'held by: 4242 cargo');
  assert.deepEqual(calls, ['lsof', 'ps']);
  // A build directory's lock names no path: the cargo processes running are the candidates.
  assert.equal(lockHolder('Blocking waiting for file lock on build directory', () => ({ stdout: '' })), 'its holder is not found');
});

// The NDK's compilers and archiver, by the host the build runs on: Windows's are `.cmd` scripts and `.exe` tools.
test("the Android build finds the NDK's tools for a Mac, Linux and Windows host", async () => {
  const { androidToolchainEnv } = await import('./agent-android.mjs');
  const ndk = mkdtempSync(resolve(tmpdir(), 'exact-ndk-'));
  try {
    const hosts = { darwin: ['darwin-x86_64', '', ''], linux: ['linux-x86_64', '', ''], win32: ['windows-x86_64', '.cmd', '.exe'] };
    for (const [platform, [folder, script, exe]] of Object.entries(hosts)) {
      const bin = resolve(ndk, 'toolchains/llvm/prebuilt', folder, 'bin');
      mkdirSync(bin, { recursive: true });
      writeFileSync(resolve(bin, `aarch64-linux-android29-clang${script}`), '');
      const env = androidToolchainEnv({ ANDROID_NDK_HOME: ndk, HERMES_LEAN_SYS_DIR_aarch64_linux_android: 'hermes' }, platform);
      expect([env.CC_aarch64_linux_android, env.CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER, env.CXX_aarch64_linux_android, env.AR_aarch64_linux_android])
        .toEqual([`aarch64-linux-android29-clang${script}`, `aarch64-linux-android29-clang${script}`, `aarch64-linux-android29-clang++${script}`, `llvm-ar${exe}`].map((tool) => resolve(bin, tool)));
    }
    rmSync(resolve(ndk, 'toolchains/llvm/prebuilt/windows-x86_64'), { recursive: true });
    expect(() => androidToolchainEnv({ ANDROID_NDK_HOME: ndk }, 'win32')).toThrow(`the NDK at ${ndk} has no aarch64-linux-android29-clang.cmd`);
  } finally { rmSync(ndk, { recursive: true, force: true }); }
});

test.skipIf(process.platform === 'win32')('Gradle keeps its given environment with PATH javac and no JAVA_HOME or Studio', async () => {
  const { androidJavaEnv, runGradle } = await import('./agent-android.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-java-'));
  try {
    const bin = resolve(dir, 'bin'), studio = resolve(dir, 'absent-studio');
    mkdirSync(bin);
    for (const tool of ['java', 'javac']) writeFileSync(resolve(bin, tool), '#!/bin/sh\necho \'openjdk version "21.0.1"\' >&2\n', { mode: 0o755 });
    const env = { PATH: bin, GRADLE_OPTS: '-Xmx128m' }, calls = [];
    expect(Bun.which('javac', { PATH: env.PATH })).toBe(resolve(bin, 'javac'));
    for (const platform of ['darwin', 'linux']) {
      expect(androidJavaEnv(env, platform, studio)).toBe(env);
      expect(runGradle(['--version'], { env }, platform, { studio, run: (...args) => { calls.push(args); return { status: 0 }; } })).toBe(true);
      expect(calls.at(-1)[2].env).toBe(env);
      expect(calls.at(-1)[2].env).toEqual({ PATH: bin, GRADLE_OPTS: '-Xmx128m' });
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// The receipt fixture retains the pinned Android bundle's API and Intl flags; cargo records the build's environment.
test('the local Hermes builder targets Android 10 even when its caller requests a newer API', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-hermes-api-'));
  try {
    mkdirSync(resolve(dir, 'scripts'));
    writeFileSync(resolve(dir, 'scripts/build-hermes-vanilla-release.sh'), `#!/bin/sh
# aarch64-linux-android)
printf '%s' "$IBEX_ANDROID_API" > api
exit 17
`);
    const built = spawnSync(process.execPath, ['scripts/hermes-android.mjs', 'build'], {
      encoding: 'utf8', env: { ...process.env, IBEX_DIR: dir, IBEX_ANDROID_API: '30',
        EXACT_HERMES_ANDROID_CACHE: resolve(dir, 'cache') },
    });
    expect(built.status).toBe(17);
    expect(readFileSync(resolve(dir, 'api'), 'utf8')).toBe('29');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

function androidHermesFixture(dir, intl) {
  const bundle = resolve(dir, 'bundle');
  for (const path of ['lib', 'include', 'bin', 'share/icu']) mkdirSync(resolve(bundle, path), { recursive: true });
  const receipt = JSON.parse(readFileSync('vendor/ibex/crates/hermes-lean-sys/testdata/receipt-v2-valid.json', 'utf8'));
  receipt.target = 'aarch64-linux-android';
  receipt.build.flags = [`-DHERMES_ENABLE_INTL=${intl}`, '-DANDROID_PLATFORM=android-29'];
  const digest = path => `sha256-${createHash('sha256').update(readFileSync(resolve(bundle, path))).digest('hex')}`;
  const archives = ['hermesvm_a', 'hermesvmlean_a', 'jsi', 'boost_context', 'icui18n', 'icuuc', 'icudata', ...(intl ? ['icudata-en', 'icudata-full'] : [])]
    .map(name => `lib/lib${name}.a`).sort();
  for (const path of [...archives, 'bin/hermesc', 'include/sentinel.h']) writeFileSync(resolve(bundle, path), path);
  receipt.archives = archives.map(path => ({ path, digest: digest(path) }));
  receipt.engine.binaryDigest = digest(receipt.engine.binary);
  receipt.compiler.digest = digest('bin/hermesc');
  receipt.headers = [{ path: 'include/sentinel.h', digest: digest('include/sentinel.h') }];
  if (intl) {
    const filter = (name, source) => {
      const path = `share/icu/${name}.json`;
      writeFileSync(resolve(bundle, path), readFileSync(`vendor/ibex/scripts/${source}.json`));
      return { path, digest: digest(path) };
    };
    receipt.icu = {
      upstream: { artifact: 'unicode-org/icu', sourceCommit: '2d029329c82c7792b985024b2bdab5fc7278fbc8', sourceRef: 'release-74-2', sourceVersion: '74.2' },
      codeArchives: ['lib/libicui18n.a', 'lib/libicuuc.a'],
      data: {
        trimmed: { archive: 'lib/libicudata.a', filter: filter('filters-root-en', 'icu74-filter-root-en') },
        en: { archive: 'lib/libicudata-en.a', filter: filter('filters-en-intl', 'icu74-filter-en-intl') },
        full: { archive: 'lib/libicudata-full.a' },
      },
    };
  }
  return receipt;
}

// --check-dir pairs candidates with the pinned host compiler, never an override.
const hostHermesTarget = hermesTarget();
const hasHostHermes = hostHermesTarget && existsSync(hermesBundle(hostHermesTarget, { ...process.env, HERMES_LEAN_SYS_DIR: undefined }).hermesc);
const hermesBundleTest = test.skipIf(!hasHostHermes);
if (!hasHostHermes) console.warn('Skipping local Hermes bundle tests: pinned host compiler is absent; run exact setup.');

hermesBundleTest('the local Hermes builder requests and validates complete Intl and non-Intl bundles', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-hermes-intl-'));
  try {
    mkdirSync(resolve(dir, 'scripts'));
    writeFileSync(resolve(dir, 'scripts/build-hermes-vanilla-release.sh'), `#!/bin/sh
# aarch64-linux-android)
printf '%s' "$IBEX_ANDROID_INTL" > intl
tar -czf "$2" -C bundle .
`);
    for (const intl of [false, true]) {
      const receipt = androidHermesFixture(dir, intl);
      writeFileSync(resolve(dir, 'bundle/hermes-input-receipt.json'), JSON.stringify(receipt));
      const built = spawnSync(process.execPath, ['scripts/hermes-android.mjs', 'build', ...(intl ? ['--intl'] : [])], {
        encoding: 'utf8', env: { ...process.env, IBEX_DIR: dir, EXACT_HERMES_ANDROID_CACHE: resolve(dir, 'cache') },
      });
      expect(built.status, built.stderr).toBe(0);
      expect(readFileSync(resolve(dir, 'intl'), 'utf8')).toBe(String(intl));
      expect(JSON.parse(readFileSync(resolve(built.stdout.trim(), 'hermes-input-receipt.json'), 'utf8'))).toEqual(receipt);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 30_000);

for (const [name, mutate] of [
  ['disabled Intl', receipt => { receipt.build.flags = ['-DHERMES_ENABLE_INTL=false']; }],
  ['missing English metadata', receipt => { delete receipt.icu; }],
  ['empty English metadata', receipt => { receipt.icu.data.en = {}; }],
  ['missing English archive', (receipt, bundle) => rmSync(resolve(bundle, 'lib/libicudata-en.a'))],
  ['changed English archive', (receipt, bundle) => writeFileSync(resolve(bundle, 'lib/libicudata-en.a'), 'changed')],
  ['missing full archive', (receipt, bundle) => rmSync(resolve(bundle, 'lib/libicudata-full.a'))],
  ['wrong schema', receipt => { receipt.schema = 'invalid'; }],
  ['malformed JSON', () => '{'],
]) hermesBundleTest(`the local Hermes builder rejects ${name} without replacing the installed bundle`, () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-hermes-reject-'));
  try {
    mkdirSync(resolve(dir, 'scripts'));
    writeFileSync(resolve(dir, 'scripts/build-hermes-vanilla-release.sh'), `#!/bin/sh
# aarch64-linux-android)
tar -czf "$2" -C bundle .
`);
    const bundle = resolve(dir, 'bundle'), receiptPath = resolve(bundle, 'hermes-input-receipt.json');
    const previous = JSON.stringify(androidHermesFixture(dir, false));
    writeFileSync(receiptPath, previous);
    const env = { ...process.env, IBEX_DIR: dir, EXACT_HERMES_ANDROID_CACHE: resolve(dir, 'cache') };
    const run = args => spawnSync(process.execPath, ['scripts/hermes-android.mjs', ...args], { encoding: 'utf8', env });
    const installed = run(['build']);
    expect(installed.status, installed.stderr).toBe(0);
    const current = resolve(dir, 'cache/current'), before = realpathSync(current);
    const receipt = androidHermesFixture(dir, true);
    const json = mutate(receipt, bundle);
    writeFileSync(receiptPath, typeof json === 'string' ? json : JSON.stringify(receipt));
    const rejected = run(['build', '--intl']);
    expect(rejected.status, rejected.stderr).toBe(1);
    expect(realpathSync(current)).toBe(before);
    expect(readFileSync(resolve(current, 'hermes-input-receipt.json'), 'utf8')).toBe(previous);
    // Installing an archive directly must validate the same candidate too.
    if (name !== 'disabled Intl') {
      const archive = resolve(dir, 'candidate.tar.gz');
      expect(spawnSync('tar', ['-czf', archive, '-C', bundle, '.']).status).toBe(0);
      expect(run(['install', archive]).status).toBe(1);
      expect(realpathSync(current)).toBe(before);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 30_000);


for (const intl of [false, true]) hermesBundleTest(`the local Hermes installer authenticates every receipt file with Intl ${intl}`, () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-hermes-manifest-'));
  try {
    const bundle = resolve(dir, 'bundle'), receiptPath = resolve(bundle, 'hermes-input-receipt.json');
    const archive = resolve(dir, 'candidate.tar.gz'), current = resolve(dir, 'cache/current');
    const env = { ...process.env, EXACT_HERMES_ANDROID_CACHE: resolve(dir, 'cache') };
    const install = timeout => {
      expect(spawnSync('tar', ['-czf', archive, '-C', bundle, '.']).status).toBe(0);
      return spawnSync(process.execPath, ['scripts/hermes-android.mjs', 'install', archive], { encoding: 'utf8', env, timeout });
    };
    const writeReceipt = receipt => writeFileSync(receiptPath, JSON.stringify(receipt));
    const snapshot = () => readdirSync(current, { recursive: true, withFileTypes: true })
      .filter(entry => entry.isFile()).map(entry => {
        const path = resolve(entry.parentPath, entry.name);
        return [relative(current, path), createHash('sha256').update(readFileSync(path)).digest('hex')];
      }).sort();
    const complete = androidHermesFixture(dir, true);
    complete.build.flags[0] = `-DHERMES_ENABLE_INTL=${intl}`;
    writeReceipt(complete);
    const installed = install();
    expect(installed.status, installed.stderr).toBe(0);
    const before = realpathSync(current), contents = snapshot();
    const files = [...complete.archives, ...complete.headers, complete.compiler,
      complete.icu.data.trimmed.filter, complete.icu.data.en.filter].map(entry => entry.path ?? entry.binary);
    const failures = [];
    for (const [path, mutation] of [
      ...files.flatMap(path => ['missing', 'changed'].map(mutation => [path, mutation])),
      ...(process.platform === 'win32' ? [] : ['hermes-input-receipt.json', 'include/sentinel.h'].map(path => [path, 'fifo'])),
      ...['jsi', 'boost_context', 'icui18n', 'icuuc', 'icudata'].map(name => [`lib/lib${name}.a`, 'unlisted']),
    ]) {
      const receipt = androidHermesFixture(dir, true);
      receipt.build.flags[0] = `-DHERMES_ENABLE_INTL=${intl}`;
      if (mutation === 'missing') rmSync(resolve(bundle, path));
      else if (mutation === 'changed') writeFileSync(resolve(bundle, path), 'changed');
      else if (mutation === 'unlisted') {
        receipt.archives = receipt.archives.filter(entry => entry.path !== path);
        if (!intl) delete receipt.icu;
      }
      writeReceipt(receipt);
      if (mutation === 'fifo') {
        rmSync(resolve(bundle, path));
        expect(spawnSync('mkfifo', [resolve(bundle, path)]).status).toBe(0);
      }
      const rejected = install(mutation === 'fifo' ? 30000 : undefined);
      if (mutation === 'fifo') rmSync(resolve(bundle, path));
      try {
        expect(rejected.status, `${mutation} ${path}: ${rejected.stderr}`).toBe(1);
        expect(realpathSync(current), `${mutation} ${path}`).toBe(before);
        expect(snapshot(), `${mutation} ${path}`).toEqual(contents);
      } catch (error) { failures.push(`${mutation} ${path}: ${error.message}`); }
    }
    expect(failures).toEqual([]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 180_000);


for (const [form, dependencies, api, bundleApi = 30] of [
  ['API-29 Hermes', '[dependencies.exact-js]\npath = "../js"', 29, 29],
  ['inline', '[dependencies]\nexact-js = { path = "../js" }', 30],
  ['table', '[dependencies.exact-js]\npath = "../js"', 30],
  ['quoted key', '[dependencies]\n"exact-js" = { path = "../js" }', 30],
  ['target table', '[target.\'cfg(target_os = "android")\'.dependencies.exact-js]\npath = "../js"', 30],
  ['renamed', '[dependencies]\nengine = { package = "exact-js", path = "../js" }', 30],
  ['Linux-only target', '[target.\'cfg(target_os = "linux")\'.dependencies.exact-js]\npath = "../js"', 29],
  ['workspace alias', '[dependencies]\nengine = { workspace = true }', 30],
  ['transitive', '[dependencies]\nbridge = { path = "../bridge" }', 30],
  ['transitive build-only', '[build-dependencies]\nbridge = { path = "../bridge" }', 29],
  ['dev-only', '[dev-dependencies.exact-js]\npath = "../js"', 29],
  ['disabled optional', '[dependencies]\nexact-js = { path = "../js", optional = true }', 29],
  ['other member feature', '[dependencies]\nshared = { path = "../shared" }', 29],
  ['procedural macro', '[dependencies]\nmacros = { path = "../macros" }', 29],
  ['no runtime Hermes', '[build-dependencies.exact-js]\npath = "../js"', 29],
]) test.skipIf(process.platform === 'win32')(`the headless Android build passes API ${api} to cargo for ${form} dependencies`, async () => {
  const { patchLines } = await import('./app.mjs');
  const { hermesApi } = await import('./agent-android.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-android-link-'));
  try {
    const tools = resolve(dir, 'tools'), bundle = resolve(dir, 'bundle');
    const bin = resolve(dir, 'toolchains/llvm/prebuilt', process.platform === 'darwin' ? 'darwin-x86_64' : 'linux-x86_64', 'bin');
    for (const sub of [tools, bundle, bin, resolve(dir, 'linux')]) mkdirSync(sub, { recursive: true });
    for (const level of [29, 30]) writeFileSync(resolve(bin, `aarch64-linux-android${level}-clang`), '');
    expect(hermesApi(bundle)).toBeNull();
    writeFileSync(resolve(bundle, 'hermes-input-receipt.json'), JSON.stringify({ build: { flags: ['-DHERMES_ENABLE_INTL=false', `-DANDROID_PLATFORM=android-${bundleApi}`] } }));
    writeFileSync(resolve(dir, 'app.contract'), 'app "Fixture"\n');
    writeFileSync(resolve(dir, 'Cargo.toml'), `[workspace]\nresolver = "2"\nmembers = ["linux", "js", "bridge", "shared", "other", "macros"]\n[workspace.dependencies]\nengine = { package = "exact-js", path = "js" }\n[patch.crates-io]\n${patchLines(dir).join('\n')}\n`);
    writeFileSync(resolve(dir, 'rust-toolchain.toml'), `[toolchain]\nchannel = "${PINNED_RUST}"\n`);
    writeFileSync(resolve(dir, 'linux/Cargo.toml'), `[package]\nname = "${basename(dir)}-linux"\nversion = "0.1.0"\n${dependencies}\n`);
    for (const [name, deps] of [['linux', null], ['js', ''], ['bridge', '[dependencies]\nexact-js = { path = "../js" }'],
      ['shared', '[dependencies]\nexact-js = { path = "../js", optional = true }\n[features]\njs = ["dep:exact-js"]'],
      ['other', '[dependencies]\nshared = { path = "../shared", features = ["js"] }'],
      ['macros', '[lib]\nproc-macro = true\n[dependencies]\nexact-js = { path = "../js" }']]) {
      mkdirSync(resolve(dir, name, 'src'), { recursive: true });
      writeFileSync(resolve(dir, name, 'src/lib.rs'), '');
      if (deps !== null) writeFileSync(resolve(dir, name, 'Cargo.toml'), `[package]\nname = "${name === 'js' ? 'exact-js' : name}"\nversion = "0.1.0"\n${deps}\n`);
    }
    // Cargo resolves local fixture crates; the stand-in records the build without compiling them.
    const metadata = spawnSync('cargo', ['metadata', '--offline', '--format-version', '1', '--filter-platform', 'aarch64-linux-android'], { cwd: dir, encoding: 'utf8' });
    expect(metadata.status).toBe(0);
    writeFileSync(resolve(tools, 'rustup'), '#!/bin/sh\necho aarch64-linux-android\n', { mode: 0o755 });
    writeFileSync(resolve(tools, 'cargo'), `#!${process.execPath}
import { writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
if (process.argv[2] === 'tree') {
  writeFileSync(${JSON.stringify(resolve(dir, 'tree-call'))}, JSON.stringify(process.argv.slice(2)));
  const result = spawnSync(${JSON.stringify(Bun.which('cargo'))}, process.argv.slice(2), { encoding: 'utf8' });
  process.stdout.write(result.stdout); process.stderr.write(result.stderr); process.exit(result.status);
}
if (process.argv[2] === 'metadata') {
  writeFileSync(${JSON.stringify(resolve(dir, 'metadata-call'))}, JSON.stringify(process.argv.slice(2)));
  console.log(${JSON.stringify(metadata.stdout)});
}
else writeFileSync(${JSON.stringify(resolve(dir, 'cargo-call'))}, JSON.stringify({ args: process.argv.slice(2), env: process.env }));
`, { mode: 0o755 });
    const built = spawnSync(process.execPath, ['scripts/agent-android.mjs', 'build', dir], { encoding: 'utf8', env: {
      ...process.env, PATH: tools + delimiter + process.env.PATH, ANDROID_NDK_HOME: dir,
      CARGO_TARGET_DIR: resolve(dir, 'target'), HERMES_LEAN_SYS_DIR_aarch64_linux_android: bundle,
    } });
    expect(built.stderr).toBe('');
    expect(built.status).toBe(0);
    const call = JSON.parse(readFileSync(resolve(dir, 'cargo-call'), 'utf8'));
    expect(call.args[0]).toBe('build');
    expect(call.args.slice(call.args.indexOf('--target'), call.args.indexOf('--target') + 2)).toEqual(['--target', 'aarch64-linux-android']);
    expect(call.env.HERMES_LEAN_SYS_DIR_aarch64_linux_android).toBe(bundle);
    expect(call.env.CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER).toBe(resolve(bin, `aarch64-linux-android${api}-clang`));
    expect(call.env.CC_aarch64_linux_android).toBe(resolve(bin, 'aarch64-linux-android29-clang'));
    expect(JSON.parse(readFileSync(resolve(dir, 'tree-call'), 'utf8'))).toEqual([
      'tree', '-p', `${basename(dir)}-linux`, '--target', 'aarch64-linux-android', '-e', 'normal,no-proc-macro',
      '--prefix', 'none', '--format', '{p}', '--locked', '--offline',
    ]);
    if (form === 'Linux-only target') {
      const withoutHermes = spawnSync(process.execPath, ['scripts/agent-android.mjs', 'build', dir], { encoding: 'utf8', env: {
        ...process.env, PATH: tools + delimiter + process.env.PATH, ANDROID_NDK_HOME: dir, CARGO_TARGET_DIR: resolve(dir, 'target'),
        HERMES_LEAN_SYS_DIR_aarch64_linux_android: '', EXACT_HERMES_ANDROID_CACHE: resolve(dir, 'no-hermes'), CARGO_HOME: resolve(dir, 'cargo-home'),
      } });
      expect(withoutHermes.stderr).toBe('');
      expect(withoutHermes.status).toBe(0);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

for (const cold of [false, true]) test(`the Android dependency query ${cold ? 'fetches missing target crates once' : 'prepares a missing shells directory'}`, () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-android-query-')), shells = resolve(dir, '.shells'), calls = resolve(dir, 'calls');
  try {
    writeFileSync(resolve(dir, 'cargo'), `#!${process.execPath}
import { appendFileSync, existsSync, writeFileSync } from 'node:fs';
const args = process.argv.slice(2);
appendFileSync(${JSON.stringify(calls)}, JSON.stringify(args) + '\\n');
if (!existsSync(${JSON.stringify(shells)})) { console.error('shells missing before tree'); process.exit(1); }
if (args[0] === 'fetch') writeFileSync(${JSON.stringify(resolve(dir, 'fetched'))}, '');
else if (${cold} && !existsSync(${JSON.stringify(resolve(dir, 'fetched'))})) {
  console.error('attempting to make an HTTP request, but --offline was specified'); process.exit(1);
} else if (args[0] === 'tree') console.log('fixture-android v0.1.0');
else console.log(JSON.stringify({ packages: [{ id: 'app', name: 'fixture-android' }], resolve: { nodes: [{ id: 'app', deps: [] }] } }));
`, { mode: 0o755 });
    if (cold) mkdirSync(shells);
    expect(existsSync(shells)).toBe(cold);
    const child = spawnSync(process.execPath, ['--eval', `
      import { mkdirSync } from 'node:fs';
      import { androidUsesHermes } from ${JSON.stringify(resolve(import.meta.dir, 'agent-android.mjs'))};
      console.log(androidUsesHermes({ workspace: ${JSON.stringify(shells)}, crate: () => 'fixture-android',
        prepare: () => mkdirSync(${JSON.stringify(shells)}, { recursive: true }) }, 'android'));
    `], { encoding: 'utf8', env: { ...process.env, PATH: dir + delimiter + process.env.PATH } });
    expect(child.status).toBe(0);
    expect(child.stdout.trim()).toBe('false');
    expect(existsSync(shells)).toBe(true);
    const commands = readFileSync(calls, 'utf8').trim().split('\n').map(JSON.parse);
    expect(commands.map(args => args[0])).toEqual(cold ? ['tree', 'fetch', 'tree'] : ['tree']);
    if (cold) expect(commands[1]).toEqual(['fetch', '--locked', '--target', 'aarch64-linux-android']);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

for (const failure of ['cold', 'still cold', 'stale lock']) test(`the Android game query preserves its captured lock when ${failure}`, () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-android-game-query-')), calls = resolve(dir, 'calls');
  try {
    mkdirSync(resolve(dir, 'logic/src'), { recursive: true });
    writeFileSync(resolve(dir, 'logic/src/lib.rs'), `impl Game for Fixture { const ID: &'static str = "fixture"; }`);
    writeFileSync(resolve(dir, 'app.contract'), 'app "Fixture"\n');
    const lock = 'version = 4\n';
    writeFileSync(resolve(dir, 'Cargo.lock'), lock);
    writeFileSync(resolve(dir, 'cargo'), `#!${process.execPath}
import { appendFileSync, existsSync, writeFileSync } from 'node:fs';
const args = process.argv.slice(2);
appendFileSync(${JSON.stringify(calls)}, JSON.stringify(args) + '\\n');
if (args[0] === 'fetch') writeFileSync(${JSON.stringify(resolve(dir, 'fetched'))}, '');
else if (args[0] === 'metadata') {
  if (${failure === 'stale lock'}) { console.error('cannot update the lock file because --locked was passed'); process.exit(1); }
  if (${failure === 'still cold'} || !existsSync(${JSON.stringify(resolve(dir, 'fetched'))})) {
    console.error('failed to download jni-sys: attempting to make an HTTP request, but --offline was specified'); process.exit(1);
  }
  console.log(JSON.stringify({ packages: [], resolve: { nodes: [] } }));
} else if (args[0] === 'tree') console.log('fixture-android v0.1.0');
`, { mode: 0o755 });
    const child = spawnSync(process.execPath, ['--eval', `
      import { resolveApp } from ${JSON.stringify(resolve(import.meta.dir, 'app.mjs'))};
      import { androidUsesHermes } from ${JSON.stringify(resolve(import.meta.dir, 'agent-android.mjs'))};
      console.log(androidUsesHermes(resolveApp(${JSON.stringify(dir)}), 'android'));
    `], { encoding: 'utf8', env: { ...process.env, PATH: dir + delimiter + process.env.PATH } });
    expect(child.status).toBe(failure === 'cold' ? 0 : 1);
    if (failure === 'cold') expect(child.stdout.trim()).toBe('false');
    else expect(child.stderr).toContain(failure === 'stale lock' ? 'lock file' : '--offline was specified');
    expect(readFileSync(resolve(dir, 'Cargo.lock'), 'utf8')).toBe(lock);
    expect(readFileSync(resolve(dir, '.shells/Cargo.lock'), 'utf8')).toBe(lock);
    const commands = readFileSync(calls, 'utf8').trim().split('\n').map(JSON.parse);
    expect(commands.map(args => args[0])).toEqual(failure === 'stale lock' ? ['metadata']
      : failure === 'cold' ? ['metadata', 'fetch', 'metadata', 'tree'] : ['metadata', 'fetch', 'metadata']);
    for (const args of commands.filter(args => args[0] === 'metadata')) {
      expect(args).toEqual(['metadata', '--offline', '--locked', '--filter-platform', 'aarch64-linux-android', '--format-version', '1']);
    }
    if (failure !== 'stale lock') expect(commands[1]).toEqual(['fetch', '--locked', '--target', 'aarch64-linux-android']);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
