// Native test runs reuse their startup check and simulator lookups (expo/exact#1400).
import { spyOn, test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { appleArtifacts } from '../host/apple/build.mjs';
import { buildFirst, nativeBuild } from './agent-build.mjs';
import { foldTaken, rotateSimulator } from './duo/hinge.mjs';
import { resolveApp } from './app.mjs';

const root = resolve(import.meta.dir, '..');
function fixture() {
  const dir = realpathSync(mkdtempSync(resolve(tmpdir(), 'exact-launch-once-')));
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'launch-once', app: { id: 'com.exact.launch-once', name: 'Launch Once' } }));
  writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    text "hello"\n');
  return dir;
}
function run(script, dir) {
  const env = { ...process.env, EXACT_APP_DIR: dir, DEVELOPER_DIR: '/stub/Xcode' };
  for (const key of ['CARGO_TARGET_DIR', 'EXACT_MAC_BIN', 'EXACT_BUILD_CHECKED', 'EXACT_BUILD_CHECK_OUT', 'EXACT_APPLE_OUTPUT', 'EXACT_BAKE_OUTPUT', 'EXACT_BACKEND_ORIGIN', 'EXACT_SIM', 'EXACT_VARIANT', 'EXACT_VARIANT_APP', 'EXACT_PLAN', 'SIMCTL_CHILD_EXACT_PLAN', 'EXACT_DEV_PLAN', 'SIMCTL_CHILD_EXACT_DEV_PLAN']) delete env[key];
  const r = spawnSync(process.execPath, ['-e', script], { cwd: root, env, encoding: 'utf8', timeout: 20000 });
  assert.equal(r.status, 0, r.stdout + r.stderr + (r.error ?? ''));
}

function withPlanBuild(script) {
  const dir = fixture();
  try { run(`
    import assert from 'node:assert/strict';
    import { mock } from 'bun:test';
    import { createHash } from 'node:crypto';
    import { cpSync, existsSync, mkdirSync, readFileSync, renameSync, rmSync, statSync, symlinkSync, utimesSync, writeFileSync } from 'node:fs';
    import { dirname, resolve } from 'node:path';
    import * as builds from './host/apple/build.mjs';
    import * as devices from './host/apple/devices.mjs';
    import { buildFirst, nativeBuild, planOverride } from './scripts/agent-build.mjs';
    import { resolveApp } from './scripts/app.mjs';
    import { linkedBakeDir } from './scripts/linked-plan.mjs';
    const dir = process.env.EXACT_APP_DIR, app = resolveApp(dir);
    const check = process.env.EXACT_BUILD_CHECK_OUT = resolve(dir, '.exact/checked');
    mkdirSync(dirname(check), { recursive: true });
    process.env.EXACT_BAKE_OUTPUT = resolve(app.target, 'bake');
    process.env.EXACT_UPDATE_TRUST = 'development';
    const source = resolve(dir, 'source.swift'), contract = resolve(dir, 'app.contract');
    const hash = bytes => createHash('sha256').update(bytes).digest('hex');
    const moduleFor = (plan, bytecode = 'bytecode') => JSON.stringify({ appId: app.id, abi: 1, bytecodeVersion: 96,
      plan: { sha256: hash(plan) }, module: { sha256: hash(bytecode) } });
    const write = (path, bytes) => { mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, bytes); };
    const appTs = code => {
      write(resolve(dir, 'app.ts'), code);
      write(resolve(dir, '.exact/typescript-inputs.json'), JSON.stringify({ modules: [['app.ts', resolve(dir, 'app.ts')]] }));
    };
    const unexpected = () => assert.fail('the drive reached the native build or carrier');
    mock.module('./host/apple/devices.mjs', () => ({ ...devices, simulator: unexpected }));
    mock.module('./host/apple/build.mjs', () => ({ ...builds, assertAppleIdentity: unexpected }));
    const setup = (host, { fresh = false, backendOrigin, selectedApp = app } = {}) => {
      const paths = builds.appleArtifacts(app, { destination: host === 'ios' ? 'ios-simulator' : 'macos' });
      const linked = linkedBakeDir(process.env, host, paths.target);
      write(source, 'before');
      write(resolve(linked, 'app.plan'), 'original plan');
      write(resolve(linked, 'app.module.json'), moduleFor('original plan'));
      if (host === 'macos') write(builds.appleRunBinary(app), 'executable');
      const time = new Date(Date.now() + 60_000);
      const receipt = { target: paths.target, started: time.toISOString(), build: { trust: 'development', backendOrigin,
        compat: { inputs: { platform: host }, embedded: { plan: { sha256: hash('original plan') } } },
        binary: { inputs: [{ path: source, name: 'source.swift', sha256: hash('before') }], missing: [], directories: [] } } };
      const receiptPath = resolve(paths.bundle, host === 'ios' ? 'receipt.json' : 'Contents/Resources/receipt.json');
      write(receiptPath, JSON.stringify(receipt)); utimesSync(receiptPath, time, time);
      if (!fresh) utimesSync(contract, new Date(time.getTime() + 10_000), new Date(time.getTime() + 10_000));
      const bake = async (app, into) => {
        write(resolve(into, 'app.plan'), 'new plan');
        write(resolve(into, 'app.module.json'), moduleFor('new plan'));
        write(resolve(into, 'app.hbc'), 'bytecode');
        return { code: 0 };
      };
      return (callback, options) => buildFirst(host, selectedApp, { wait() {}, say() {}, build: unexpected, ...options,
        bake: async (...args) => { const result = await bake(...args); callback?.(); return result; } });
    };
    ${script}
  `, dir); } finally { rmSync(dir, { recursive: true, force: true }); }
}

test('a native edit during a plan bake prevents the startup check', () => withPlanBuild(`
  const drive = setup('ios');
  await assert.rejects(drive(() => write(source, 'changed during the bake')), /ios build is stale: .*source\\.swift/);
  assert.ok(!existsSync(check));
`));

test.each(['--plan', 'EXACT_PLAN', 'SIMCTL_CHILD_EXACT_PLAN'])('an explicit %s rebuilds stale plan sources', flag => withPlanBuild(`
  const flag = ${JSON.stringify(flag)}, host = flag === 'EXACT_PLAN' ? 'macos' : 'ios';
  const plan = resolve(dir, '.exact/fixture.plan');
  write(plan, 'fixture');
  if (flag !== '--plan') process.env[flag] = plan;
  let rebuilds = 0, bakes = 0;
  const drive = setup(host);
  assert.equal(await drive(() => bakes++, {
    ...(flag === '--plan' ? { plan } : {}),
    build() { rebuilds++; utimesSync(contract, 1, 1); return { code: 0, output: '' }; },
  }), 0);
  assert.equal(rebuilds, 1);
  assert.equal(bakes, 0);
  const checked = readFileSync(check, 'utf8');
  const state = nativeBuild(host, app, { checked });
  const { checkedLaunchPlan } = await import('./scripts/agent-build.mjs');
  assert.equal(readFileSync(checkedLaunchPlan(host, state, app, plan), 'utf8'), 'fixture');
`));

test.each(['ios', 'macos'])('%s refuses a missing or different requested fixture', host => withPlanBuild(`
  const host = ${JSON.stringify(host)}, plan = resolve(dir, '.exact/fixture.plan');
  write(plan, 'fixture');
  assert.equal(await setup(host, { fresh: true })(undefined, { plan }), 0);
  process.env.EXACT_BUILD_CHECKED = readFileSync(check, 'utf8');
  const open = host === 'ios' ? (await import('./scripts/agent-ios.mjs')).openIOS
    : (await import('./scripts/agent-stdio.mjs')).openStdio;
  for (const options of [{}, { env: { EXACT_PLAN: '' } }, { plan: plan + '.other' }, { plan: plan + '.other', env: { EXACT_PLAN: '' } }]) {
    await assert.rejects(open({ host, app: dir, ...options }), error =>
      error.stale === true && error.message.includes('the run checked a different plan'));
  }
  await assert.rejects(open({ host, app: dir, plan, env: { EXACT_PLAN: '' } }), /the drive reached the native build or carrier/);
`));

test('iOS refuses a replacement bundle with the old sidecar still present', () => withPlanBuild(`
  const paths = builds.appleArtifacts(app, { destination: 'ios-simulator' });
  const bundle = resolve(paths.bundle, 'receipt.json'), sidecar = resolve(paths.products, 'receipt.json');
  const drive = setup('ios', { fresh: true });
  const receipt = JSON.parse(readFileSync(bundle));
  write(sidecar, JSON.stringify(receipt)); utimesSync(sidecar, new Date(receipt.started), new Date(receipt.started));
  assert.equal(await drive(), 0);
  process.env.EXACT_BUILD_CHECKED = readFileSync(check, 'utf8');
  write(bundle, JSON.stringify({ ...receipt, compatibilityId: 'replacement' }));
  const { openIOS } = await import('./scripts/agent-ios.mjs');
  await assert.rejects(openIOS({ app: dir }), error => error.stale === true && error.message.includes(bundle));
`));

test.each(['ios', 'macos'])('%s reports a deleted receipt as a stale bundle', host => withPlanBuild(`
  const host = ${JSON.stringify(host)};
  assert.equal(await setup(host, { fresh: true })(), 0);
  process.env.EXACT_BUILD_CHECKED = readFileSync(check, 'utf8');
  const { receipt, built } = nativeBuild(host, app, { checked: process.env.EXACT_BUILD_CHECKED });
  const open = host === 'ios' ? (await import('./scripts/agent-ios.mjs')).openIOS
    : (await import('./scripts/agent-stdio.mjs')).openStdio;
  write(receipt, '{"build":');
  await assert.rejects(open({ host, app: dir }), error => error.stale === true && error.message.includes('rerun exact test or agent'));
  rmSync(receipt);
  await assert.rejects(open({ host, app: dir }), error => error.stale === true && error.message.includes(built));
`));

test.each([['macos', 'EXACT_DEV_PLAN'], ['ios', 'SIMCTL_CHILD_EXACT_DEV_PLAN'], ['ios', 'SIMCTL_CHILD_EXACT_MODULES']])(
  '%s refuses %s before certifying or launching', (host, key) => withPlanBuild(`
    const host = ${JSON.stringify(host)}, key = ${JSON.stringify(key)};
    const drive = setup(host, { fresh: true });
    const refuses = error => error.stale === true && error.message.includes(key);
    process.env[key] = resolve(dir, 'other');
    await assert.rejects(drive(), refuses);
    assert.ok(!existsSync(check));
    delete process.env[key];
    assert.equal(await drive(), 0);
    process.env.EXACT_BUILD_CHECKED = readFileSync(check, 'utf8');
    process.env[key] = resolve(dir, 'other');
    const open = host === 'ios' ? (await import('./scripts/agent-ios.mjs')).openIOS
      : (await import('./scripts/agent-stdio.mjs')).openStdio;
    await assert.rejects(open({ host, app: dir }), refuses);
  `));

test('a serverless checked build needs no recorded TypeScript server inputs', () => withPlanBuild(`
  write(resolve(dir, 'app.ts'), 'export const hello = "world";');
  assert.equal(await setup('macos', { fresh: true })(), 0);
  const run = JSON.parse(readFileSync(check));
  assert.equal(run.requestBase, null);
  assert.equal(run.server, null);
`));

test.each(['ios', 'macos'])('%s refuses an explicit plan that displaces the checked override', host => withPlanBuild(`
  const host = ${JSON.stringify(host)};
  const open = host === 'ios' ? (await import('./scripts/agent-ios.mjs')).openIOS
    : (await import('./scripts/agent-stdio.mjs')).openStdio;
  const plan = resolve(dir, 'fixture.plan');
  write(plan, 'bare plan');
  utimesSync(plan, 1, 1);
  assert.equal(await setup(host)(), 0);
  process.env.EXACT_BUILD_CHECKED = readFileSync(check, 'utf8');
  for (const options of [{ plan }, { env: { EXACT_PLAN: plan } }, { plan, env: { EXACT_PLAN: '' } }]) {
    await assert.rejects(open({ host, app: dir, ...options }), error =>
      error.stale === true && error.message.includes('the checked sources require the baked plan override'));
  }
`));

test.each([['ios', 'moves'], ['ios', 'changes'], ['macos', 'moves'], ['macos', 'changes']])('%s checked launches keep the startup app when a file dependency %s', (host, edit) => withPlanBuild(`
  const host = ${JSON.stringify(host)}, edit = ${JSON.stringify(edit)};
  const processes = await import('node:child_process');
  const spawnSync = processes.spawnSync;
  let installs = 0;
  mock.module('node:child_process', () => ({ ...processes, spawnSync(command, args, options) {
    if (command === process.execPath && args[0] === 'install' && options.cwd === dir) {
      installs++;
      return { status: 1 };
    }
    return spawnSync(command, args, options);
  } }));
  const pkg = JSON.stringify({ name: 'local-dep', version: '1.0.0' });
  write(resolve(dir, 'package.json'), JSON.stringify({ dependencies: { 'local-dep': 'file:dep' } }));
  write(resolve(dir, 'dep/package.json'), pkg);
  write(resolve(dir, 'node_modules/local-dep/package.json'), pkg);
  resolveApp(dir);
  assert.equal(await setup(host, { fresh: true })(), 0);
  process.env.EXACT_BUILD_CHECKED = readFileSync(check, 'utf8');
  const open = host === 'ios' ? (await import('./scripts/agent-ios.mjs')).openIOS
    : (await import('./scripts/agent-stdio.mjs')).openStdio;
  const launch = () => assert.rejects(open({ host, app: dir }), /the drive reached the native build or carrier/);
  await launch();
  if (edit === 'moves') renameSync(resolve(dir, 'dep'), resolve(dir, 'moved'));
  else write(resolve(dir, 'dep/index.js'), 'export const added = true;');
  await launch();
  assert.equal(installs, 0);
  assert.throws(() => resolveApp(dir), edit === 'moves' ? /the build would read the copy/ : /bun install/);
  assert.equal(installs, edit === 'moves' ? 0 : 1);
`));

test.each([['ios', 'backend'], ['ios', 'app ID'], ['macos', 'backend'], ['macos', 'app ID']])('%s checked launches keep the startup %s', (host, edit) => withPlanBuild(`
  const host = ${JSON.stringify(host)}, edit = ${JSON.stringify(edit)}, backendOrigin = 'http://127.0.0.1:4400';
  const editOrigin = origin => appTs('export const origin = "' + origin + '";');
  editOrigin(backendOrigin);
  const drive = setup(host, { fresh: true, backendOrigin });
  assert.equal(await drive(), 0);
  process.env.EXACT_BUILD_CHECKED = readFileSync(check, 'utf8');
  const open = host === 'ios' ? (await import('./scripts/agent-ios.mjs')).openIOS
    : (await import('./scripts/agent-stdio.mjs')).openStdio;
  const launch = () => assert.rejects(open({ host, app: dir }), /the drive reached the native build or carrier/);
  await launch();
  if (edit === 'backend') {
    editOrigin('http://127.0.0.1:4401');
    await launch();
    await assert.rejects(drive(), /baked backend .*4400 differs from app.ts origin .*4401/);
  } else {
    write(resolve(dir, 'app.json'), JSON.stringify({ name: 'launch-once', app: { id: 'com.exact.next', name: 'Next' } }));
    await launch();
    assert.equal(nativeBuild(host, resolveApp(dir)).missing, true);
  }
`));

test.each(['plan', 'app.module.json', 'app.hbc', 'rust/replacement', 'map'])('a changed %s prevents certification of the copied generation', changed => withPlanBuild(`
  const original = resolve(dir, '.exact/fixture/app.plan'), output = dirname(original), map = original + '.map.json';
  const files = { plan: original, map, ...Object.fromEntries(['app.module.json', 'app.hbc', 'rust/replacement'].map(name => [name, resolve(output, name)])) };
  for (const [name, file] of Object.entries(files)) write(file, name);
  const fs = await import('node:fs'), copyFileSync = fs.copyFileSync;
  mock.module('node:fs', () => ({ ...fs, copyFileSync(from, into) {
    copyFileSync(from, into);
    if (from === map) write(files[${JSON.stringify(changed)}], 'new bytes');
  } }));
  await assert.rejects(setup('ios', { fresh: true })(undefined, { plan: original }), /plan.*changed while staging/);
  assert.ok(!existsSync(check));
`));

test('certification refuses a stable plan or bytecode from another module generation', () => withPlanBuild(`
  const plan = resolve(dir, '.exact/fixture/app.plan');
  const drive = setup('ios', { fresh: true });
  write(resolve(dirname(plan), 'app.module.json'), moduleFor('original'));
  write(plan, 'updated');
  write(resolve(dirname(plan), 'app.hbc'), 'bytecode');
  await assert.rejects(drive(undefined, { plan }), /module receipt does not match/);
  assert.ok(!existsSync(check));
  write(plan, 'original');
  write(resolve(dirname(plan), 'app.hbc'), 'other JS');
  await assert.rejects(drive(undefined, { plan }), /module receipt does not match/);
  assert.ok(!existsSync(check));
`));

test('a checked fixture requires all the companions it staged', () => withPlanBuild(`
  const plan = resolve(dir, '.exact/fixture/app.plan');
  write(plan, 'fixture plan');
  const files = { 'app.module.json': moduleFor('fixture plan'), 'app.hbc': 'bytecode',
    'rust/replacement': 'rust', 'app.plan.map.json': 'map' };
  for (const [name, contents] of Object.entries(files)) write(resolve(dirname(plan), name), contents);
  assert.equal(await setup('ios', { fresh: true })(undefined, { plan }), 0);
  const checked = process.env.EXACT_BUILD_CHECKED = readFileSync(check, 'utf8');
  const stage = dirname(JSON.parse(checked).plan);
  const { openIOS } = await import('./scripts/agent-ios.mjs');
  for (const names of [['app.module.json', 'app.hbc'], ['rust/replacement'], ['app.plan.map.json']]) {
    for (const name of names) rmSync(resolve(stage, name));
    await assert.rejects(openIOS({ app: dir, plan }), error => error.stale === true && error.message.includes(stage));
    for (const name of names) write(resolve(stage, name), files[name]);
  }
  write(resolve(stage, 'app.hbc'), 'changed!');
  await assert.rejects(openIOS({ app: dir, plan }), error => error.stale === true && error.message.includes(stage));
`));

test.each(['suffix', 'relative', 'symlink', 'env', 'share'])('a checked launch accepts the startup app through %s', selection => withPlanBuild(`
  const selection = ${JSON.stringify(selection)};
  const share = { action: '/', method: 'GET', params: { text: 'text' } };
  write(resolve(dir, 'app.json'), JSON.stringify({ ...app.manifest, share_target: share }));
  write(resolve(dir, 'package.json'), JSON.stringify({ dependencies: { 'local-dep': 'file:dep' } }));
  const pkg = JSON.stringify({ name: 'local-dep', version: '1.0.0' });
  write(resolve(dir, 'dep/package.json'), pkg);
  write(resolve(dir, 'node_modules/local-dep/package.json'), pkg);
  const checkedApp = resolveApp(selection === 'suffix' ? 'launch-once-ios' : dir);
  assert.equal(await setup('ios', { fresh: true })(), 0);
  assert.equal(await buildFirst('ios', checkedApp, { wait() {} }), 0);
  process.env.EXACT_BUILD_CHECKED = readFileSync(check, 'utf8');
  renameSync(resolve(dir, 'dep'), resolve(dir, 'moved'));
  write(resolve(dir, 'app.json'), JSON.stringify(app.manifest));
  const { open } = await import('./scripts/agent-session.mjs');
  if (selection === 'relative') process.chdir(dir);
  if (selection === 'symlink' || selection === 'env') symlinkSync(dir, resolve(dir, '.exact/alias'), 'dir');
  if (selection === 'env') process.env.EXACT_APP_DIR = resolve(dir, '.exact/alias');
  if (selection === 'share') {
    const ios = await import('./scripts/agent-ios.mjs');
    mock.module('./scripts/agent-ios.mjs', () => ({ ...ios, openIOS({ env }) {
      assert.deepEqual(JSON.parse(env.EXACT_AGENT_SHARE), { target: share, share: { text: 'hello' } });
      return { host: 'ios', async ask(req) {
        if (req.op === 'tree') return { nodes: [{ id: 1, props: { testId: 'root' } }] };
        assert.deepEqual(req, { op: 'share', id: 1, target: share, share: { text: 'warm' } });
        return { closed: true };
      } };
    } }));
    const s = await open({ app: dir, host: 'ios', url: 'share:?text=hello' });
    assert.equal((await s.share('root', 'share:?text=warm')).closed, true);
  } else {
    const selector = { suffix: 'launch-once-ios', relative: '.', symlink: resolve(dir, '.exact/alias') }[selection];
    await assert.rejects(open({ host: 'ios', app: selector }), /the drive reached the native build or carrier/);
  }
`));

test('a malformed checked app falls back to app resolution', () => withPlanBuild(`
  assert.equal(await setup('ios', { fresh: true })(), 0);
  process.env.EXACT_BUILD_CHECKED = JSON.stringify({ ...JSON.parse(readFileSync(check)), app: { dir, id: app.id } });
  const { checkedAppleRun } = await import('./scripts/agent-build.mjs');
  assert.equal(checkedAppleRun('ios'), null);
`));

test('an external check binds the resolved directory and variant', () => withPlanBuild(`
  assert.equal(await setup('ios', { fresh: true })(), 0);
  process.env.EXACT_BUILD_CHECKED = readFileSync(check, 'utf8');
  const { checkedAppleRun } = await import('./scripts/agent-build.mjs');
  const alias = resolve(dir, '.exact/alias');
  symlinkSync(dir, alias, 'dir');
  assert.equal(checkedAppleRun('ios', alias)?.app.dir, dir);
  process.env.EXACT_VARIANT = 'beta';
  assert.equal(checkedAppleRun('ios', alias), null);
  delete process.env.EXACT_VARIANT;
  delete process.env.EXACT_APP_DIR;
  assert.equal(checkedAppleRun('ios', app.name + '-ios'), null);
  process.env.EXACT_BUILD_CHECKED = JSON.stringify({ ...JSON.parse(readFileSync(check)),
    app: { ...JSON.parse(readFileSync(check)).app, dir: resolve('apps/mosaic'), name: 'mosaic', selector: 'mosaic', outside: false } });
  assert.equal(checkedAppleRun('ios', 'mosaic-ios')?.app.dir, resolve('apps/mosaic'));
`));

test.each(['prefixed request', 'later server'])('a checked Apple test keeps its startup %s', behavior => withPlanBuild(String.raw`
  const prefixed = ${JSON.stringify(behavior)} === 'prefixed request';
  const reserve = () => Bun.serve({ port: 0, hostname: '127.0.0.1', fetch: () => new Response() });
  const ports = [reserve(), reserve()], [port, other] = ports.map(server => server.port);
  for (const server of ports) server.stop(true);
  const origin = 'http://127.0.0.1:' + port, prefix = prefixed ? '/api' : '';
  const originAt = (port, prefix) => 'export const origin = "http://127.0.0.1:' + port + prefix + '";';
  appTs(originAt(port, prefix));
  const log = resolve(dir, '.exact/server.log');
  write(resolve(dir, 'app.json'), JSON.stringify({ ...app.manifest, server: { database: false } }));
  const helper = resolve(dir, 'server-import.js');
  write(helper, 'export const newline = "\\n";');
  write(resolve(dir, 'server.ts'), 'import { appendFileSync } from "node:fs";\nimport { newline } from "./server-import";\n'
    + 'appendFileSync(' + JSON.stringify(log) + ', process.env.PORT + newline);\n'
    + 'Bun.serve({ port: Number(process.env.PORT), hostname: "127.0.0.1", fetch(req) {'
    + 'const path = new URL(req.url).pathname; return new Response("", { status: path === "/items" || path === "/api/items"'
    + ' ? (path === ' + JSON.stringify(prefix + '/items') + ' ? 200 : 404) : 200 }); } });');
  const drive = setup('ios', { fresh: true, backendOrigin: origin, selectedApp: resolveApp(dir) });
  if (prefixed) {
    const fs = await import('node:fs'), read = fs.readFileSync;
    let settings = 0;
    mock.module('node:fs', () => ({ ...fs, readFileSync(path, ...args) {
      if (path === resolve(dir, '.exact/typescript-inputs.json') && new Error().stack.includes('builtSettings') && ++settings === 2) appTs(originAt(other, prefix));
      return read(path, ...args);
    } }));
  }
  assert.equal(await drive(), 0);
  process.env.EXACT_BUILD_CHECKED = readFileSync(check, 'utf8');
  const checked = JSON.parse(process.env.EXACT_BUILD_CHECKED);
  assert.equal(checked.requestBase, origin + prefix);
  assert.equal(checked.server.port, port);
  const tools = await import('./scripts/tools.mjs'), processes = await import('node:child_process'), spawnSync = processes.spawnSync;
  mock.module('./scripts/tools.mjs', () => ({ ...tools, contractTool: () => ({ cmd: 'contract', pre: [] }) }));
  mock.module('node:child_process', () => ({ ...processes, spawnSync(cmd, args, options) {
    if (cmd !== 'contract') return spawnSync(cmd, args, options);
    return { status: 0, stdout: JSON.stringify({ tests: [{ name: 'request', steps: [{ op: 'request', method: 'GET',
      path: '/items', line: 2 }] }], properties: [] }) };
  } }));
  const session = await import('./scripts/agent-session.mjs');
  mock.module('./scripts/agent-session.mjs', () => ({ ...session, open: async () => ({
    host: 'ios', now: 0, op: async () => ({}), clock: async () => ({}),
    state: async () => ({ mutationRequests: 0 }), logs: async () => ({ lines: [] }), close: async () => {},
  }) }));
  const { main } = await import('./scripts/agent.mjs');
  const run = file => main(['ios', '--app', dir, '--test', resolve(dir, file)]);
  assert.equal(await run('first.test.contract'), 0);
  if (!prefixed) {
    appTs(originAt(other, prefix));
    assert.equal(await run('later.test.contract'), 0);
    const shadow = resolve(dir, 'server-import.ts');
    write(shadow, 'export const newline = "\\t";');
    const stale = error => error.stale === true && error.message.includes('rerun exact test or agent');
    await assert.rejects(run('later.test.contract'), stale);
    rmSync(shadow);
    write(helper, 'export const newline = "\\t";');
    await assert.rejects(run('later.test.contract'), stale);
  }
  assert.deepEqual(readFileSync(log, 'utf8').trim().split('\n'), prefixed ? [String(port)] : [String(port), String(port)]);
`));

test.each([false, true])('a checked test keeps fetch origins and sign-in grants after edits (baked origin: %s)', literal => withPlanBuild(String.raw`
  const listener = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch: () => new Response() });
  const origin = 'http://127.0.0.1:' + listener.port;
  const literal = ${JSON.stringify(literal)};
  const base = literal ? origin + '/api' : origin;
  const quote = String.fromCharCode(96);
  appTs(literal ? 'export const origin = "' + base + '"; export const grants = ' + quote + 'net.fetch ' + base + '$' + '{""}\nsecret.keep oauth' + quote + ';'
    : 'export const API = "' + origin + '"; export const grants = "secret.keep oauth";');
  const url = resolve(dir, 'app.contract');
  write(url, 'component App\n  url "' + origin + '/items"');
  write(resolve(dir, 'app.json'), JSON.stringify({ ...app.manifest, server: { database: false } }));
  write(resolve(dir, 'server.ts'), 'throw new Error("the runner should use the fixture stub");');
  const drive = setup('ios', { fresh: true, backendOrigin: literal ? origin : undefined, selectedApp: resolveApp(dir) });
  assert.equal(await drive(), 0);
  process.env.EXACT_BUILD_CHECKED = readFileSync(check, 'utf8');
  write(url, 'component App\n  url "http://127.0.0.1:12/items"');
  appTs('export const grants = "";');
  const tools = await import('./scripts/tools.mjs'), processes = await import('node:child_process'), spawnSync = processes.spawnSync;
  mock.module('./scripts/tools.mjs', () => ({ ...tools, contractTool: () => ({ cmd: 'contract', pre: [] }) }));
  mock.module('node:child_process', () => ({ ...processes, spawnSync(cmd, args, options) {
    if (cmd !== 'contract') return spawnSync(cmd, args, options);
    return { status: 0, stdout: JSON.stringify({ tests: [{ name: 'checks', steps: [
      { op: 'signed-in', key: 'oauth', account: 'alice', line: 2 },
      { op: 'fail-fetch', prefix: '/items', line: 3 }
    ] }], properties: [] }) };
  } }));
  mock.module('./packages/oauth/test/fixture.ts', () => ({ async signedInSession(server, key, account) {
    assert.deepEqual([server, key, account], [base, 'oauth', 'alice']);
    return 'token';
  } }));
  const session = await import('./scripts/agent-session.mjs');
  let launches = 0;
  mock.module('./scripts/agent-session.mjs', () => ({ ...session, open: async opts => {
    launches++;
    assert.equal(opts.failFetch, origin + '/items');
    assert.deepEqual(JSON.parse(opts.env.EXACT_AGENT_STORED), [['#oauth.oauth', 'token']]);
    return { host: 'ios', now: 0, op: async () => ({}), clock: async () => ({}),
      state: async () => ({ mutationRequests: 0 }), logs: async () => ({ lines: [] }), close: async () => {} };
  } }));
  const { runTests } = await import('./scripts/agent-test.mjs');
  try {
    const result = await runTests({ host: 'ios', app: dir, file: resolve(dir, 'later.test.contract') });
    assert.equal(result.passed, 1, JSON.stringify(result.results));
    assert.equal(launches, 1);
  } finally { listener.stop(true); }
`));

test.each([['ios', 'fixture'], ['macos', 'fixture'], ['ios', 'override'], ['macos', 'override']])('%s checked %s keeps the original plan and companions', (host, kind) => withPlanBuild(`
  const host = ${JSON.stringify(host)}, fixture = ${JSON.stringify(kind)} === 'fixture';
  const { overrideDir } = await import('./scripts/agent-build.mjs');
  const original = fixture ? resolve(dir, 'fixture/specimen.plan') : resolve(overrideDir(host, app), 'app.plan');
  const output = dirname(original), map = original + '.map.json';
  const module = moduleFor(fixture ? 'fixture plan' : 'new plan');
  if (fixture) {
    write(original, 'fixture plan');
    write(resolve(output, 'app.module.json'), module);
    write(resolve(output, 'app.hbc'), 'bytecode');
    write(resolve(output, 'rust/replacement'), 'rust bytes');
    write(map, 'source map');
  }
  assert.equal(await setup(host, { fresh: fixture })(undefined, fixture ? { plan: original } : {}), 0);
  const checked = process.env.EXACT_BUILD_CHECKED = readFileSync(check, 'utf8');
  const selected = () => nativeBuild(host, app, { checked }).plan;
  const alias = resolve(dir, '.exact/alias');
  symlinkSync(dir, alias, 'dir');
  const { checkedAppleRun } = await import('./scripts/agent-build.mjs');
  const run = checkedAppleRun(host, alias);
  assert.equal(run.app.dir, dir);
  const installed = resolve(dir, '.exact/installed.app');
  let launches = 0;
  const received = plan => {
    launches++;
    assert.equal(readFileSync(plan, 'utf8'), fixture ? 'fixture plan' : 'new plan');
    assert.equal(readFileSync(resolve(dirname(plan), 'app.module.json'), 'utf8'), module);
    assert.equal(readFileSync(resolve(dirname(plan), 'app.hbc'), 'utf8'), 'bytecode');
    if (fixture) assert.equal(readFileSync(resolve(dirname(plan), 'rust/replacement'), 'utf8'), 'rust bytes');
    throw new Error('launch captured');
  };
  const processes = await import('node:child_process');
  mock.module('node:child_process', () => ({ ...processes, spawn(bin, args, { env }) {
    if (host === 'ios') {
      assert.equal(bin, 'xcrun');
      assert.deepEqual(args.slice(0, 2), ['simctl', 'launch']);
      received(env.SIMCTL_CHILD_EXACT_PLAN);
    } else {
      assert.equal(bin, builds.appleRunBinary(app));
      received(env.EXACT_PLAN);
    }
  } }));
  if (host === 'ios') {
    const launch = await import('./scripts/agent-launch.mjs'), wire = await import('./scripts/agent-wire.mjs');
    const touches = await import('./host/apple/touches.mjs');
    mock.module('./scripts/agent-launch.mjs', () => ({ ...launch, exclusiveIOS: (udid, id, open) => open() }));
    mock.module('./scripts/agent-wire.mjs', () => ({ ...wire, simulatorConsole: spawn => spawn() }));
    mock.module('./host/apple/touches.mjs', () => ({ ...touches, withAccessibilityRuntime: (udid, enabled, open) => open() }));
    cpSync(run.bundle, installed, { recursive: true });
    mock.module('./host/apple/devices.mjs', () => ({ ...devices, simulator: () => ({ udid: 'sim' }),
      withSoftwareKeyboard: (dev, open) => open(), showSimulator() {}, crashWatch: () => () => [],
      installedBuild: () => ({ installed }), keepInstalledRelease: () => ({ container: installed, install: false }),
    }));
  } else {
    const paths = builds.appleArtifacts(app);
    mkdirSync(paths.capture, { recursive: true });
    mock.module('./host/apple/build.mjs', () => ({ ...builds, assertAppleIdentity() {} }));
  }
  const open = host === 'ios' ? (await import('./scripts/agent-ios.mjs')).openIOS
    : (await import('./scripts/agent-stdio.mjs')).openStdio;
  const launch = options => assert.rejects(open({ host, app: alias, checked: run, ...options }), /launch captured/);
  await launch(fixture ? { plan: original } : {});
  rmSync(alias);
  symlinkSync(resolve('apps/mosaic'), alias, 'dir');
  const first = selected();
  assert.equal(readFileSync(first ?? original, 'utf8'), fixture ? 'fixture plan' : 'new plan');
  for (const file of [original, resolve(output, 'app.module.json'), resolve(output, 'app.hbc'), ...(fixture ? [map, resolve(output, 'rust/replacement')] : [])])
    write(file, 'replacement ' + readFileSync(file, 'utf8'));
  await launch(fixture ? { plan: original } : {});
  if (fixture) await launch({ env: { EXACT_PLAN: original } });
  assert.equal(launches, fixture ? 3 : 2);
  const second = selected();
  assert.equal(readFileSync(second ?? original, 'utf8'), fixture ? 'fixture plan' : 'new plan');
  assert.equal(readFileSync(resolve(dirname(second), 'app.module.json'), 'utf8'), module);
  assert.equal(readFileSync(resolve(dirname(second), 'app.hbc'), 'utf8'), 'bytecode');
  if (fixture) {
    assert.equal(readFileSync(second + '.map.json', 'utf8'), 'source map');
    assert.equal(readFileSync(resolve(dirname(second), 'rust/replacement'), 'utf8'), 'rust bytes');
  }
  const product = host === 'ios' ? resolve(installed, 'receipt.json') : run.built;
  utimesSync(product, 1, 1);
  const stamp = statSync(product);
  const bytes = readFileSync(product, 'utf8');
  write(product, bytes.replace(host === 'ios' ? 'development' : 'executable', host === 'ios' ? 'Development' : 'Executable'));
  utimesSync(product, stamp.atime, stamp.mtime);
  assert.deepEqual([statSync(product).size, statSync(product).mtimeMs], [stamp.size, stamp.mtimeMs]);
  await assert.rejects(open({ host, app: alias, checked: run, ...(fixture ? { plan: original } : {}) }),
    error => error.stale === true && error.message.includes('rerun exact test or agent'));
  assert.equal(launches, fixture ? 3 : 2);
`));

test.each(['app.module.json', 'app.hbc', 'app.plan'])('a checked override requires %s as a file', name => withPlanBuild(`
  assert.equal(await setup('ios')(), 0);
  const checked = readFileSync(check, 'utf8'), plan = JSON.parse(checked).plan;
  const state = () => nativeBuild('ios', app, { checked });
  assert.equal(state().checked, true);
  const name = ${JSON.stringify(name)}, file = resolve(dirname(plan), name);
  rmSync(file);
  if (name === 'app.plan') mkdirSync(file);
  const missing = state();
  assert.equal(missing.checked, true);
  assert.deepEqual(missing.changed, [plan]);
  assert.equal(planOverride('ios', app, missing), null);
`));

test('test and agent carry one native startup check to every test file on iOS and macOS', () => {
  const dir = fixture();
  for (const name of ['one', 'two']) writeFileSync(resolve(dir, `${name}.test.contract`), 'test "opens"\n');
  try { run(`
    import assert from 'node:assert/strict';
    import { mock } from 'bun:test';
    import * as processes from 'node:child_process';
    import * as shells from './scripts/app-shells.mjs';
    import * as builds from './scripts/web-build.mjs';
    import { writeFileSync } from 'node:fs';
    import { basename } from 'node:path';
    const spawn = processes.spawn;
    let checks = 0, launches = 0, iosFiles = 0;
    mock.module('./scripts/app-shells.mjs', () => ({ ...shells, platformOf: () => null }));
    mock.module('./scripts/web-build.mjs', () => ({ ...builds, privateNativeBuild: () => ({ env: {}, seed() {}, close() {} }) }));
    mock.module('node:child_process', () => ({ ...processes, spawn(command, args, options) {
      if (basename(args[0]) === 'agent-build.mjs') {
        checks++;
        assert.ok(options.env.EXACT_BUILD_CHECK_OUT);
        assert.deepEqual(args.slice(-2), ['--plan', 'fixture.plan']);
        writeFileSync(options.env.EXACT_BUILD_CHECK_OUT, 'checked-' + checks);
      } else if (basename(args[0]) === 'agent.mjs') {
        launches++;
        assert.equal(options.env.EXACT_BUILD_CHECKED, 'checked-' + checks);
        if (args.includes('ios') && args.includes('--test')) {
          assert.equal(options.env.EXACT_IOS_TEST_CHECKED, iosFiles++ === 0 ? '' : 'sim:warned');
          return spawn(command, ['-e', 'process.send({ checkedSimulatorAccessibility: "sim", status: "warned" })'], options);
        }
      } else assert.fail('unexpected subprocess: ' + args);
      return spawn(command, ['-e', ''], options);
    } }));
    const { run } = await import('./scripts/app-cli.mjs');
    const platforms = new Set([process.platform, 'win32']);
    for (const platform of platforms) for (const host of ['ios', 'macos']) for (const verb of ['test', 'agent']) {
      Object.defineProperty(process, 'platform', { value: platform });
      const before = [checks, launches];
      const windows = platform === 'win32';
      assert.equal(await run({ dir: process.env.EXACT_APP_DIR, name: 'launch-once', exact2: process.cwd(),
        args: [verb, host, '--plan', 'fixture.plan', ...(verb === 'agent' ? ['tree'] : [])] }), windows ? 2 : 0);
      assert.equal(checks - before[0], windows ? 0 : 1);
      assert.equal(launches - before[1], windows ? 0 : verb === 'test' ? 2 : 1);
    }
    assert.equal(iosFiles, platforms.size === 1 ? 0 : 2);
  `, dir); } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.skipIf(process.platform === 'win32')('test ios checks a warm seed once, keeps a checked bake after test failure, and refuses a reusable seed for another backend', () => {
  const dir = fixture();
  try { run(`
    import assert from 'node:assert/strict';
    import { mock } from 'bun:test';
    import { EventEmitter } from 'node:events';
    import * as processes from 'node:child_process';
    import * as shells from './scripts/app-shells.mjs';
    import * as launch from './scripts/agent-launch.mjs';
    import * as sweep from './scripts/sweep.mjs';
    import { createHash } from 'node:crypto';
    import { appleArtifacts } from './host/apple/build.mjs';
    import { resolveApp } from './scripts/app.mjs';
    import { linkedBakeDir } from './scripts/linked-plan.mjs';
    import { mkdirSync, readFileSync, utimesSync, writeFileSync } from 'node:fs';
    import { basename, resolve } from 'node:path';
    const dir = process.env.EXACT_APP_DIR;
    process.env.CARGO_TARGET_DIR = resolve(dir, 'target');
    process.env.EXACT_UPDATE_TRUST = 'development';
    const app = resolveApp(dir);
    const hash = bytes => createHash('sha256').update(bytes).digest('hex');
    const origin = 'http://127.0.0.1:4400', other = 'http://127.0.0.1:4401';
    const source = resolve(dir, 'source.swift'), contract = resolve(dir, 'app.contract');
    const module = JSON.stringify({ appId: app.id, abi: 1, bytecodeVersion: 96 });
    writeFileSync(resolve(dir, 'app.ts'), "export const origin = '" + origin + "';\\n");
    mkdirSync(resolve(dir, '.exact'), { recursive: true });
    writeFileSync(resolve(dir, '.exact/typescript-inputs.json'), JSON.stringify({ modules: [['app.ts', resolve(dir, 'app.ts')]] }));
    writeFileSync(source, 'original native source');
    let checks = 0, launches = 0, builds = 0, bakes = 0, testCode = 0;
    mock.module('./scripts/app-shells.mjs', () => ({ ...shells, platformOf: () => null }));
    mock.module('./scripts/sweep.mjs', () => ({ ...sweep, startSweep() {} }));
    const receiptChanges = launch.receiptChanges;
    mock.module('./scripts/agent-launch.mjs', () => ({ ...launch,
      receiptChanges(...args) { checks++; return receiptChanges(...args); },
    }));
    const { buildFirst, nativeBuild, planOverride, refuseBuild } = await import('./scripts/agent-build.mjs');
    const build = () => {
      builds++;
      const paths = appleArtifacts(app, { destination: 'ios-simulator' });
      const receipt = resolve(paths.bundle, 'receipt.json'), time = new Date(Date.now() + 60_000);
      const linked = linkedBakeDir(process.env, 'ios', paths.target);
      mkdirSync(linked, { recursive: true });
      writeFileSync(resolve(linked, 'app.plan'), 'original plan');
      writeFileSync(resolve(linked, 'app.module.json'), module);
      mkdirSync(paths.bundle, { recursive: true });
      writeFileSync(receipt, JSON.stringify({ target: paths.target, started: time.toISOString(),
        build: { trust: 'development', backendOrigin: process.env.EXACT_BACKEND_ORIGIN ?? origin,
          compat: { inputs: { platform: 'ios' }, embedded: { plan: { sha256: hash('original plan') } } },
          binary: { inputs: [{ path: source, name: 'source.swift', sha256: hash(readFileSync(source)) }], missing: [], directories: [] } } }));
      utimesSync(receipt, time, time);
      return { code: 0, output: '' };
    };
    const bake = (app, into) => {
      bakes++;
      mkdirSync(into, { recursive: true });
      writeFileSync(resolve(into, 'app.plan'), 'updated plan');
      writeFileSync(resolve(into, 'app.module.json'), JSON.stringify({ ...JSON.parse(module), plan: { sha256: hash('updated plan') }, module: { sha256: hash('bytecode') } }));
      writeFileSync(resolve(into, 'app.hbc'), 'bytecode');
      return { code: 0, output: '' };
    };
    mock.module('node:child_process', () => ({ ...processes, spawn(command, args, options) {
      const child = new EventEmitter();
      Promise.resolve().then(async () => {
        switch (basename(args[0])) {
          case 'agent-build.mjs': return buildFirst('ios', app, { say() {}, wait() {}, build, bake });
          case 'agent.mjs': {
            launches++;
            const state = nativeBuild('ios', app, { checked: options.env.EXACT_BUILD_CHECKED });
            assert.equal(state.checked, true);
            if (state.plan) assert.equal(planOverride('ios', app, state)?.plan, state.plan);
            else refuseBuild('ios', state, app);
            return testCode;
          }
          default: assert.fail('unexpected subprocess: ' + args);
        }
      }).then(code => child.emit('exit', code), error => { console.error(error); child.emit('exit', 1); });
      return child;
    } }));
    const { run } = await import('./scripts/app-cli.mjs');
    const drive = async (expected = 0, ...args) => {
      const listeners = process.listeners('exit');
      const code = await run({ dir, name: app.name, exact2: process.cwd(), args: ['test', 'ios', ...args] });
      for (const fn of process.listeners('exit').filter(fn => !listeners.includes(fn))) {
        process.off('exit', fn);
        fn(code);
      }
      assert.equal(code, expected);
    };
    await drive();
    assert.equal(builds, 1);
    checks = launches = 0;
    await drive();
    assert.deepEqual({ checks, launches, builds }, { checks: 1, launches: 1, builds: 1 });

    const cache = resolve(app.target, 'native-reuse', hash(app.dir).slice(0, 16), 'ios/development/current/artifact.json');
    const before = readFileSync(cache, 'utf8');
    await drive(1, '--backend', other);
    assert.equal(readFileSync(cache, 'utf8'), before);
    assert.equal(launches, 1);
    writeFileSync(contract, 'component App\\n  text "changed"\\n');
    const later = new Date(Date.now() + 120_000);
    utimesSync(contract, later, later);
    await drive(1, '--backend', other);
    assert.equal(readFileSync(cache, 'utf8'), before);
    assert.deepEqual({ launches, builds, bakes }, { launches: 1, builds: 1, bakes: 0 });

    delete process.env.EXACT_BACKEND_ORIGIN;
    testCode = 1;
    await drive(1);
    await drive(1);
    assert.deepEqual({ launches, builds, bakes }, { launches: 3, builds: 1, bakes: 1 });
    assert.notEqual(readFileSync(cache, 'utf8'), before);

    const override = readFileSync(cache, 'utf8');
    utimesSync(contract, 1, 1);
    writeFileSync(source, 'changed native source');
    await drive(1, '--backend', other);
    assert.notEqual(readFileSync(cache, 'utf8'), override);
    testCode = 0;
    await drive(0, '--backend', other);
    assert.deepEqual({ launches, builds, bakes }, { launches: 5, builds: 2, bakes: 1 });
  `, dir); } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('iOS launches check sources once and follow the simulator’s current certified copy', () => {
  const dir = fixture();
  try { run(`
    import assert from 'node:assert/strict';
    import { mock } from 'bun:test';
    import { EventEmitter } from 'node:events';
    import { PassThrough } from 'node:stream';
    import * as processes from 'node:child_process';
    import * as net from 'node:net';
    import * as devices from './host/apple/devices.mjs';
    import * as builds from './host/apple/build.mjs';
    import * as launch from './scripts/agent-launch.mjs';
    import * as wire from './scripts/agent-wire.mjs';
    import * as touches from './host/apple/touches.mjs';
    import * as cleanup from './scripts/sweep.mjs';
    import { cpSync, existsSync, mkdirSync, readFileSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
    import { dirname, resolve } from 'node:path';
    import { appleExecutableName, resolveApp } from './scripts/app.mjs';
    import * as tools from './scripts/tools.mjs';
    const dir = process.env.EXACT_APP_DIR, app = resolveApp(dir), records = resolve(dir, 'keyboard-records');
    const paths = builds.appleArtifacts(app, { destination: 'ios-simulator' });
    const receipt = resolve(paths.products, 'receipt.json');
    mkdirSync(paths.bundle, { recursive: true }); mkdirSync(dirname(receipt), { recursive: true });
    const time = new Date(Date.now() + 60000);
    writeFileSync(receipt, JSON.stringify({ target: paths.target, started: time.toISOString(), build: { trust: 'development' } }));
    utimesSync(receipt, time, time);
    const realSpawnSync = processes.spawnSync;
    const counts = { check: 0, list: 0, boot: 0, keyboard: {}, container: {} }, settings = { A: '1', B: '0' }, assets = [];
    const states = { A: 'Shutdown', B: 'Booted' };
    let reinstall = false, failRestore = false, generation = 1, now = Date.now();
    let failingBoot, transientBoot, failInstall = false, replacement = 'A', activeSocket, pending;
    Date.now = () => now;
    const container = udid => resolve(dir, udid + '-' + generation + '.app');
    for (const udid of ['A', 'B']) mkdirSync(container(udid));
    for (const bundle of [paths.bundle, container('A'), container('B')]) {
      mkdirSync(resolve(bundle, '_CodeSignature'));
      for (const part of [appleExecutableName(app, true), 'receipt.json', '_CodeSignature/CodeResources']) {
        const file = resolve(bundle, part);
        writeFileSync(file, part === 'receipt.json' ? readFileSync(receipt) : 'installed'); utimesSync(file, 1, 1);
      }
    }
    const result = stdout => ({ status: 0, stdout: stdout ?? '', stderr: '' });
    const simctl = args => {
      assert.equal(args[0], 'simctl');
      if (args[1] === 'list') {
        counts.list++;
        return result(JSON.stringify({ devices: { 'com.apple.CoreSimulator.SimRuntime.iOS-27-0': ['A', 'B'].map(udid => ({
          udid, name: failingBoot && udid === replacement ? 'replacement' : udid, state: states[udid], deviceTypeIdentifier: 'com.apple.CoreSimulator.SimDeviceType.iPhone-17-Pro',
        })) } }));
      }
      if (args[1] === failingBoot && args[2] === replacement) return { status: 1, stderr: 'device became unavailable' };
      if (args[1] === transientBoot) {
        transientBoot = null; return { status: 1, stderr: 'temporary boot failure' };
      }
      if (args[1] === 'boot') { counts.boot++; states[args[2]] = 'Booted'; return result(); }
      if (args[1] === 'shutdown') { states[args[2]] = 'Shutdown'; return result(); }
      if (args[1] === 'bootstatus') return result();
      if (args[1] === 'get_app_container') {
        counts.container[args[2]] = (counts.container[args[2]] ?? 0) + 1;
        return result(container(args[2]));
      }
      if (args[1] === 'spawn' && args[3] === 'defaults') {
        const udid = args[2], verb = args[4];
        const count = counts.keyboard[udid] ??= { read: 0, delete: 0, write: 0 };
        count[verb]++;
        if (verb === 'read') return result(settings[udid]);
        if (verb === 'delete') { settings[udid] = null; return result(); }
        if (verb === 'write') { if (failRestore) return { status: 1 }; settings[udid] = '1'; return result(); }
      }
      assert.fail('unexpected simctl: ' + args);
    };
    const file = resolve(dir, 'recovery.test.contract');
    const authored = ['first', 'shutdown', 'third'].map(name => ({ name, steps: [{ op: 'clock', arg: 'data', line: 2 }] }));
    mock.module('./scripts/tools.mjs', () => ({ ...tools, contractTool: () => ({ cmd: 'contract', pre: [] }) }));
    mock.module('./scripts/sweep.mjs', () => ({ ...cleanup, startSweep() {} }));
    mock.module('node:child_process', () => ({ ...processes,
      spawnSync(command, args, options) {
        if (command === 'git') return realSpawnSync(command, args, options);
        if (command === 'contract') {
          assert.deepEqual(args, ['test', file]);
          return result(JSON.stringify({ tests: authored, properties: [] }));
        }
        assert.equal(command, 'xcrun'); return simctl(args);
      },
      spawn(command, args) {
        assert.equal(command, 'xcrun');
        const child = new EventEmitter(); child.stdout = new PassThrough(); child.stderr = new PassThrough(); child.kill = () => {};
        const r = simctl(args);
        queueMicrotask(() => { child.stdout.write(r.stdout); child.emit('close', r.status); });
        return child;
      },
    }));
    mock.module('node:net', () => ({ ...net, connect() {
      const socket = activeSocket = new EventEmitter();
      socket.end = () => { if (!socket.closed) { socket.closed = true; socket.emit('close'); } };
      queueMicrotask(() => socket.emit('connect')); return socket;
    } }));
    const receiptChanges = launch.receiptChanges;
    mock.module('./scripts/agent-launch.mjs', () => ({ ...launch,
      receiptChanges(...args) { counts.check++; return receiptChanges(...args); },
      exclusiveIOS: (udid, id, open) => open(),
    }));
    const { withSoftwareKeyboard, keepInstalledRelease } = devices;
    mock.module('./host/apple/devices.mjs', () => ({ ...devices,
      withSoftwareKeyboard: (dev, open) => withSoftwareKeyboard(dev, open, { records }),
      showSimulator() {}, crashWatch: () => () => [],
      keepInstalledRelease: (udid, id, bundle, executable, options) => udid === 'B'
        ? keepInstalledRelease(udid, id, bundle, executable, { ...options, installRecords: resolve(dir, 'install-records') })
        : { install: reinstall },
    }));
    mock.module('./host/apple/build.mjs', () => ({ ...builds, install() {
      if (failInstall) throw new Error('simctl install failed');
      generation++; cpSync(paths.bundle, container(process.env.EXACT_SIM), { recursive: true }); reinstall = false;
    } }));
    mock.module('./host/apple/touches.mjs', () => ({ ...touches, withAccessibilityRuntime: (udid, enabled, open) => open() }));
    mock.module('./scripts/agent-wire.mjs', () => ({ ...wire,
      simulatorConsole: () => ({ done: states[process.env.EXACT_SIM] !== 'Booted', retry: () => false,
        exited: Promise.resolve(0), process: { kill() {} } }),
      jsonLines: () => {
        let failure;
        return { next: async () => ({ ready: true }), ask: async req => {
          if (failure) throw new Error(failure);
          if (req.op === 'held') return new Promise((_, reject) => { pending = reject; });
          return { mutationRequests: 0 };
        }, fail(why) { failure = why; pending?.(new Error(why)); } };
      },
    }));
    // The asset root is read at launch, before any fake wire traffic.
    const fsAssets = await import('./host/apple/assets.mjs');
    mock.module('./host/apple/assets.mjs', () => ({ ...fsAssets, simulatorAssetRoot(installed) { assets.push(installed); return installed; } }));
    const { buildFirst, nativeBuild } = await import('./scripts/agent-build.mjs');
    const { openIOS } = await import('./scripts/agent-ios.mjs');
    process.env.EXACT_BUILD_CHECK_OUT = resolve(dir, 'checked');
    assert.equal(await buildFirst('ios', app, { wait() {}, build() { assert.fail('no native build'); } }), 0);
    process.env.EXACT_BUILD_CHECKED = readFileSync(process.env.EXACT_BUILD_CHECK_OUT, 'utf8');
    process.env.EXACT_SIM = 'A';
    for (let i = 0; i < 2; i++) {
      const carrier = await openIOS({ app: dir });
      assert.equal(settings.A, i === 0 ? null : '1');
      assert.equal(existsSync(resolve(records, 'A.json')), i === 0);
      await carrier.close();
      assert.equal(settings.A, '1');
      assert.ok(!existsSync(resolve(records, 'A.json')));
      writeFileSync(resolve(dir, 'app.contract'), 'edited during run');
      utimesSync(resolve(dir, 'app.contract'), new Date(time.getTime() + 10000), new Date(time.getTime() + 10000));
      now += 6000; // Past the old five-second device-list cache, without waiting.
    }
    assert.deepEqual(counts, { check: 1, list: 1, boot: 1, keyboard: { A: { read: 2, delete: 1, write: 1 } }, container: { A: 2 } });
    assert.deepEqual(assets, [container('A'), container('A')]);
    // A second device has its own cache, including a keyboard read that needs no warning.
    process.env.EXACT_SIM = 'B';
    for (let i = 0; i < 2; i++) { const carrier = await openIOS({ app: dir }); await carrier.close(); }
    assert.deepEqual(counts, { check: 1, list: 2, boot: 1, keyboard: { A: { read: 2, delete: 1, write: 1 }, B: { read: 1, delete: 0, write: 0 } }, container: { A: 2, B: 2 } });
    assert.equal(settings.B, '0');
    const lastUsed = resolve(dir, '.exact/simulator');
    assert.equal(readFileSync(lastUsed, 'utf8').trim(), 'B');
    process.env.EXACT_SIM = 'A';
    const old = container('A');
    generation++; cpSync(paths.bundle, container('A'), { recursive: true });
    const carrier = await openIOS({ app: dir }); await carrier.close();
    assert.ok(existsSync(old));
    assert.notEqual(assets.at(-1), old);
    assert.equal(assets.at(-1), container('A'));
    assert.equal(counts.container.A, 3);
    assert.equal(readFileSync(lastUsed, 'utf8').trim(), 'A');
    assert.equal(counts.check, 1);
    rmSync(container('A'), { recursive: true });
    generation++; cpSync(paths.bundle, container('A'), { recursive: true });
    const replaced = await openIOS({ app: dir }); await replaced.close();
    assert.equal(assets.at(-1), container('A'));
    assert.equal(counts.container.A, 4);
    // The shutdown falls within the life of B's device list. A new list must see A shut down.
    const { runTests } = await import('./scripts/agent-test.mjs');
    const recovery = await runTests({ host: 'ios', app: dir, file, atResult(test) {
      if (test.name === 'first') states.A = 'Shutdown';
    } });
    assert.deepEqual(recovery.results.map(test => [test.name, test.failures.length]), [['first', 0], ['shutdown', 1], ['third', 0]], JSON.stringify(recovery.results));
    assert.match(recovery.results[1].failures[0], /simulator launch exited/);
    assert.equal(settings.A, '1');
    const nextLaunch = await openIOS({ app: dir }); await nextLaunch.close();
    assert.equal(counts.list, 4);
    assert.equal(counts.boot, 2);
    assert.equal(counts.container.A, 8);
    assert.deepEqual(counts.keyboard.A, { read: 4, delete: 2, write: 2 });
    assert.equal(counts.check, 1);
    assert.ok(nativeBuild('ios', app).changed.some(path => path.endsWith('app.contract')), 'the next run sees the edit');

    devices.forgetSimulator('A');
    failInstall = reinstall = true;
    rmSync(container('A'), { recursive: true });
    const completed = [];
    await assert.rejects(runTests({ host: 'ios', app: dir, file, atResult: test => completed.push(test.name) }), /simctl install failed/);
    assert.deepEqual(completed, []);
    failInstall = reinstall = false;
    cpSync(paths.bundle, container('A'), { recursive: true });

    authored.push({ ...authored[0], name: 'after boot' });
    for (const command of ['boot', 'bootstatus']) {
      const bootRecovery = await runTests({ host: 'ios', app: dir, file, atResult(test) {
        if (test.name === 'first') { states.A = 'Shutdown'; transientBoot = command; }
      } });
      assert.deepEqual(bootRecovery.results.map(test => test.failures.length), [0, 1, 1, 0], JSON.stringify(bootRecovery.results));
      assert.match(bootRecovery.results[2].failures[0], new RegExp('simctl ' + command + ': temporary boot failure'));
    }

    const disconnected = await openIOS({ app: dir });
    const interrupted = disconnected.ask({ op: 'held' });
    states.A = 'Shutdown';
    activeSocket.end();
    await assert.rejects(interrupted, /the app ended/);
    await disconnected.close();
    const afterDisconnect = await openIOS({ app: dir });
    await afterDisconnect.close();

    const dev = { udid: 'A', bootedHere: true }, record = resolve(records, 'A.json');
    const open = callback => withSoftwareKeyboard(dev, callback, { records });
    await assert.rejects(open(() => { throw new Error('open failed'); }), /open failed/);
    assert.equal(settings.A, '1'); assert.ok(!existsSync(record));
    const badClose = await open(() => ({ close() { throw new Error('close failed'); } }));
    await assert.rejects(badClose.close(), /close failed/);
    assert.equal(settings.A, '1'); assert.ok(!existsSync(record));
    // The same exit callback still restores an open session.
    const beforeExit = process.listeners('exit');
    const atExit = await open(() => ({ close() {} }));
    process.listeners('exit').filter(fn => !beforeExit.includes(fn)).forEach(fn => fn());
    assert.equal(settings.A, '1'); assert.ok(!existsSync(record));
    await atExit.close();
    // A failed write leaves a record until a later session can put the setting back.
    const failedRestore = await open(() => ({ close() {} }));
    failRestore = true; await failedRestore.close();
    assert.equal(settings.A, null); assert.ok(existsSync(record));
    const stillFailed = await open(() => ({ close() {} })); await stillFailed.close();
    assert.ok(existsSync(record));
    failRestore = false;
    const readCount = counts.keyboard.A.read;
    const recovered = await withSoftwareKeyboard({ udid: 'A' }, () => ({ close() {} }), { records }); await recovered.close();
    assert.equal(counts.keyboard.A.read, readCount + 1);
    assert.equal(settings.A, '1'); assert.ok(!existsSync(record));

    for (const command of ['boot', 'bootstatus']) {
      const options = { cached: true, app: command };
      failingBoot = command; replacement = 'A'; states.A = 'Shutdown'; states.B = 'Booted';
      devices.forgetSimulator('A');
      assert.throws(() => devices.simulator('replacement', options), new RegExp('simctl ' + command + ': device became unavailable'));
      replacement = 'B';
      assert.equal(devices.simulator('replacement', options).udid, 'B');
    }
  `, dir); } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.each(['same app', 'sample host', 'empty pick', 'already remembered'])('explicit simulator selection updates the cached default for %s', selection => {
  const dir = fixture();
  try { run(`
    import assert from 'node:assert/strict';
    import { mock } from 'bun:test';
    import * as processes from 'node:child_process';
    import { mkdirSync, readFileSync, writeFileSync } from 'node:fs';
    import { resolve } from 'node:path';
    const dir = process.env.EXACT_APP_DIR, used = resolve(dir, '.exact/simulator');
    mkdirSync(resolve(dir, '.exact')); writeFileSync(used, 'A');
    const devices = ['A', 'B'].map(udid => ({ udid, name: udid, state: 'Booted',
      deviceTypeIdentifier: 'com.apple.CoreSimulator.SimDeviceType.iPhone-18-Pro' }));
    mock.module('node:child_process', () => ({ ...processes, spawnSync(command, args) {
      assert.equal(command, 'xcrun');
      if (args[1] === 'get_app_container') return { status: 0 };
      assert.equal(args[1], 'list');
      return { status: 0, stdout: JSON.stringify({ devices: { 'SimRuntime.iOS-27-0': devices } }) };
    } }));
    const { chooseSimulator } = await import('./host/apple/devices.mjs');
    const selection = ${JSON.stringify(selection)};
    const options = { dir, app: 'com.exact.launch-once', cached: true };
    if (selection === 'empty pick') process.env.EXACT_SIM = '';
    const sample = { ...options, app: 'com.exact.launch-once.host', cached: false };
    assert.equal(chooseSimulator(undefined, options).udid, 'A');
    if (selection === 'already remembered') writeFileSync(used, 'B');
    assert.equal(chooseSimulator('B', selection === 'sample host' ? sample : options).udid, 'B');
    assert.equal(readFileSync(used, 'utf8').trim(), 'B');
    assert.equal(chooseSimulator(undefined, options).udid, 'B');
    assert.equal(chooseSimulator('A', options).udid, 'A');
    assert.equal(chooseSimulator(undefined, options).udid, 'A');
  `, dir); } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.each(['spawn error', 'timeout', 'nonzero exit', 'synchronous throw'])('a keyboard probe retries after a %s', failure => {
  const dir = fixture();
  try { run(`
    import assert from 'node:assert/strict';
    import { mock } from 'bun:test';
    import { EventEmitter } from 'node:events';
    import { PassThrough } from 'node:stream';
    import * as processes from 'node:child_process';
    let reads = 0, timeout;
    const failure = ${JSON.stringify(failure)}, schedule = globalThis.setTimeout;
    globalThis.setTimeout = (callback, ms, ...args) => {
      if (ms === 20000) timeout = callback;
      return schedule(callback, ms, ...args);
    };
    mock.module('node:child_process', () => ({ ...processes, spawn(command, args) {
      assert.equal(command, 'xcrun');
      assert.deepEqual(args.slice(0, 5), ['simctl', 'spawn', 'sim', 'defaults', 'read']);
      if (++reads === 1 && failure === 'synchronous throw') throw new Error('spawn threw');
      const child = new EventEmitter();
      child.stdout = new PassThrough();
      child.stderr = new PassThrough();
      child.kill = () => child.emit('close', null, 'SIGKILL');
      if (reads > 1) queueMicrotask(() => {
        child.stderr.write('The domain/default pair of (com.apple.keyboard.preferences, AutomaticMinimizationEnabled) does not exist');
        child.emit('close', 1);
      });
      else if (failure === 'spawn error') queueMicrotask(() => { child.emit('error', new Error('spawn failed')); child.emit('close', -1); });
      else if (failure === 'nonzero exit') queueMicrotask(() => { child.stderr.write('simctl failed'); child.emit('close', 1); });
      else queueMicrotask(() => timeout());
      return child;
    } }));
    const { withSoftwareKeyboard } = await import('./host/apple/devices.mjs');
    for (let i = 0; i < 3; i++) {
      const carrier = await withSoftwareKeyboard({ udid: 'sim' }, () => ({ close() {} }), { records: process.env.EXACT_APP_DIR });
      await carrier.close();
    }
    assert.equal(reads, 2);
  `, dir); } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('macOS freshness checks the full product receipt when the bundle carries a reduced receipt', async () => {
  const checkOut = process.env.EXACT_BUILD_CHECK_OUT, macBin = process.env.EXACT_MAC_BIN;
  const previous = process.env.EXACT_APP_DIR, target = process.env.CARGO_TARGET_DIR, dir = fixture();
  const write = (path, text) => writeFileSync(resolve(dir, path), text);
  delete process.env.CARGO_TARGET_DIR;
  delete process.env.EXACT_MAC_BIN;
  try {
    process.env.EXACT_APP_DIR = dir;
    const app = resolveApp(dir), source = resolve(dir, 'source.swift');
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
    process.env.EXACT_BUILD_CHECK_OUT = resolve(dir, '.exact/checked');
    mkdirSync(dirname(process.env.EXACT_BUILD_CHECK_OUT), { recursive: true });
    assert.equal(await buildFirst('macos', app, { wait() {} }), 0);
    const checked = readFileSync(process.env.EXACT_BUILD_CHECK_OUT, 'utf8');
    write('source.swift', 'after');
    // Newer than the receipt, so only a run's own check (not the file-time walk) can pass it.
    const later = new Date(time.getTime() + 10_000);
    utimesSync(source, later, later);
    assert.ok(nativeBuild('macos', app).changed.includes('source.swift'));
    for (let launch = 0; launch < 2; launch++) assert.deepEqual(nativeBuild('macos', app, { checked }).changed, []);
    const replaced = JSON.parse(readFileSync(bundled, 'utf8')); replaced.build.binary.sha256 = 'other';
    writeFileSync(bundled, JSON.stringify(replaced));
    assert.deepEqual(nativeBuild('macos', app, { checked }).changed, [bundled]);
    write('source.swift', 'before');
  } finally {
    if (macBin === undefined) delete process.env.EXACT_MAC_BIN; else process.env.EXACT_MAC_BIN = macBin;
    if (checkOut === undefined) delete process.env.EXACT_BUILD_CHECK_OUT; else process.env.EXACT_BUILD_CHECK_OUT = checkOut;
    if (previous === undefined) delete process.env.EXACT_APP_DIR; else process.env.EXACT_APP_DIR = previous;
    if (target === undefined) delete process.env.CARGO_TARGET_DIR; else process.env.CARGO_TARGET_DIR = target;
    rmSync(dir, { recursive: true, force: true });
  }
});

test('rotate issues the turn on the new display and confirms the scene after landing', async () => {
  const sample = { orientation: 'portrait', device: 3, inner: true, ready: true };
  const reads = [
    { ...sample, inner: false, orientation: 'landscapeLeft' },
    { ...sample, ready: false }, sample,
    { ...sample, orientation: 'landscapeLeft' }, sample, { ...sample, orientation: 'landscapeLeft' },
  ];
  let count = 0, turns = 0, lands = 0;
  const result = await rotateSimulator({
    duo: true, fold: 180,
    ask({ op }) { assert.equal(op, 'layout'); return Promise.resolve(reads[count++]); },
    issue(turn) {
      assert.equal(count, 3);
      assert.equal(turn.helper, 'portrait');
      turns++;
      throw new Error('setter timed out');
    },
    land(result) { lands++; return Promise.resolve(result); },
  }, 'landscapeLeft');
  assert.deepEqual(result, { orientation: 'landscapeLeft' });
  assert.deepEqual([count, turns, lands], [6, 1, 1]);
});

test('a lost Duo turn is reissued from the reported pose at most twice', async () => {
  const turns = [], messages = [];
  let now = 0;
  const clock = spyOn(performance, 'now').mockImplementation(() => now);
  const log = spyOn(console, 'error').mockImplementation(message => messages.push(message));
  try {
    await assert.rejects(rotateSimulator({
      duo: true, fold: 0,
      async ask() {
        if (turns.length) now += 3100;
        return { orientation: 'portrait', device: 1, ready: true, inner: false, allowed: ['landscape-secondary'] };
      },
      issue(turn) { turns.push(turn.device); },
      land() { assert.fail('the scene never turned'); },
    }, 'landscapeLeft'), /device portrait .*scene stayed portrait on cover display/);
    assert.deepEqual(turns, ['landscapeRight', 'portrait', 'landscapeRight', 'portrait', 'landscapeRight']);
    assert.equal(messages.length, 2);
    assert.match(messages[0], /reissue 1\/2/);
    assert.match(messages[1], /reissue 2\/2/);
  } finally {
    clock.mockRestore();
    log.mockRestore();
  }
});

test('a scene that turns at the deadline gets the same landing as any input', async () => {
  let issued = false, now = 0;
  const clock = spyOn(performance, 'now').mockImplementation(() => now);
  try {
    const result = await rotateSimulator({
      duo: false,
      async ask() {
        return { orientation: issued ? 'landscapeLeft' : 'portrait', device: 1 };
      },
      issue() { issued = true; now = 10000; return 'landscapeRight'; },
      async land(result) { await Bun.sleep(1050); return result; },
    }, 'landscapeLeft');
    assert.deepEqual(result, { orientation: 'landscapeLeft' });
  } finally { clock.mockRestore(); }
});

test('a partial fold can report the new hinge angle on the outgoing cover', async () => {
  const sample = { angle: 127.78, inner: false, ready: true, device: 4, orientation: 'landscapeLeft' };
  const samples = [sample, sample, sample, { ...sample, inner: true, ready: false }, { ...sample, inner: true, orientation: 'portrait' },
    { ...sample, inner: true }, { ...sample, inner: true }];
  let reads = 0;
  await rotateSimulator({
    duo: true, fold: 130, display: () => reads === 1 ? {} : { inner: true, angle: 127.8 },
    async ask() { return samples[reads++]; },
    issue(turn) { assert.equal(reads, 5); assert.equal(turn.device, 'portrait'); },
    async land(result) { return result; },
  }, 'landscapeLeft');
  assert.equal(reads, samples.length);
});

test('a fold lands once UIKit has told the app of it, moved it to the display and divided its window', async () => {
  const flat = { 'horizontal-viewport-segments': 1, 'vertical-viewport-segments': 1 }, book = { ...flat, 'horizontal-viewport-segments': 2 };
  // `samples`: what the host says each time, its own facts and then the app's segments; the first is from before the fold
  const fold = async (degrees, [before, ...samples], { display = () => assert.fail('SpringBoard was asked'), still, within } = {}) => {
    let reads = 0;
    const at = () => samples[Math.min(reads, samples.length) - 1];
    const taken = await foldTaken({ degrees, display, before, still, within, quiet: 50, pause: 1, ask: async ({ simulatorRotation }) => {
      if (simulatorRotation) reads++;
      return simulatorRotation ? at() : { env: at().env, viewport: at().viewport };
    } });
    return [taken, reads];
  };
  const wide = { w: 951, h: 669 }, narrow = { w: 466, h: 678 };
  const open = { angle: 180, inner: true, ready: true, env: flat, viewport: wide }, half = { ...open, angle: 127.78, env: book };
  const closed = { angle: 0, inner: false, ready: true, env: flat, viewport: narrow };
  assert.deepEqual(await fold(130, [open, open, { ...half, env: flat }, half]), [true, 3]); // the division comes after the hinge
  assert.deepEqual(await fold(0, [open, open, { ...open, angle: 0 }, { ...closed, ready: false }, { ...closed, viewport: wide }, closed]), [true, 5]); // and the display, and its size
  assert.deepEqual(await fold(180, [closed, closed, { ...closed, angle: 180 }, open]), [true, 3]);
  // Between two partial angles, only the new one is taken, unless the driver folded to this one; a first fold can find it there
  assert.deepEqual(await fold(90, [half, half, { ...half, angle: 88 }], { still: false }), [true, 2]);
  assert.equal((await fold(90, [half, half], { still: false, within: 100 }))[0], false);
  assert.deepEqual(await fold(130, [half, half], { still: true }), [true, 1]);
  assert.equal((await fold(130, [half, half], { display: () => ({}) }))[0], true);
  assert.equal((await fold(0, [open, { ...closed, angle: null }], { within: 100 }))[0], false);
  // From the cover, or nearly closed, SpringBoard says which display has the app
  assert.deepEqual(await fold(130, [closed, { ...closed, angle: 127.78 }, half], { display: () => ({ inner: true, fresh: true }) }), [true, 2]);
  assert.deepEqual(await fold(20, [open, { ...half, angle: 18 }, { ...closed, angle: 18 }], { display: () => ({ inner: false, fresh: true }) }), [true, 2]);
  // A first fold to another partial angle waits for SpringBoard's
  assert.equal((await fold(90, [half, half], { display: () => ({ angle: 88 }), within: 100 }))[0], false);
  // Its word from before this fold is not of it: the display the app is on is taken once it has been quiet
  const [taken, reads] = await fold(130, [closed, { ...closed, angle: 127.78 }, half], { display: () => ({ inner: false, fresh: false }) });
  assert.ok(taken && reads > 2);
  assert.equal((await fold(20, [closed, { ...closed, angle: 18 }], { display: () => ({ inner: false, fresh: false }) }))[0], true);
  assert.equal((await fold(130, [open, open], { within: 100 }))[0], false);
});

test('losing the verification read cannot undo a turn the scene reported', async () => {
  const replies = [{ orientation: 'portrait' }, { orientation: 'landscapeRight' }, { error: 'connection closed' }];
  const result = await rotateSimulator({
    duo: false, async ask() { return replies.shift(); }, issue() {},
    async land(result) { return { ...result, epoch: 3 }; },
  }, 'landscapeRight');
  assert.deepEqual(result, { orientation: 'landscapeRight', epoch: 3 });
});
