import { expect, mock, test } from 'bun:test';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, statSync, symlinkSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { captureWebProducts, cargoContractReads, changedContractReads, contractReadReceipt, loopWebDist, privateNativeBuild, privateWebBuild } from './web-build.mjs';
import { appleArtifacts } from '../host/apple/build.mjs';
import { buildFirst, failureSummary, nativeBuild, planOverride } from './agent-build.mjs';
import { webBuildFailure } from './app-cli.mjs';
import { linkedBakeDir } from './linked-plan.mjs';
import { resolveApp } from './app.mjs';
import { webChanges, webInputDigests } from './agent-launch.mjs';
import { replaceBuild } from '../host/web/serve.mjs';
let fallbackRoots = [];
mock.module('../host/web/build.mjs', () => ({ webSourceRoots: () => fallbackRoots }));

// A failed build's summary keeps the compiler's first error, which may be the cause of the rest (#705: the web
// build said only the last, which blamed another component's action of the same name).
test("a failed build's summary keeps the compiler's first error", () => {
  const error = (line, id, text) => [`/app/app.contract:${line}:3 [${id}] ${text}`, ` ${line} |   source ${line}`, '   |   ^^^^^^'].join('\n');
  const two = [error(6, 'syntax-method-call', '`.replace(…)` is a method call'), error(8, 'type-handler-action', '`follow` is an action of `Pill`')].join('\n');
  // The web build: every compiler error with its excerpt, in the order said.
  expect(webBuildFailure(`   Compiling app v0.1.0\n${two}\n`)).toEqual([two]);
  // A native build: from the first when the tail would leave it out.
  const many = Array.from({ length: 12 }, (_, i) => error(i + 1, 'type-unknown-name', `unknown name \`n${i}\``)).join('\n');
  expect(failureSummary(`app: the bake failed:\n${many}\n`, { host: 'macos', code: 1, log: 'x.log' })).toStartWith(error(1, 'type-unknown-name', 'unknown name `n0`'));
});

test('loops and concurrent checks publish into directories they alone own', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-web-owners-'));
  const app = { dir, target: resolve(dir, 'target') }, other = { ...app, dir: resolve(dir, 'other') };
  const checks = ['test', 'verify', 'agent', 'test'].map(kind => privateWebBuild(app, kind));
  try {
    const loops = [loopWebDist(app, 'js'), loopWebDist(app, 'wasm'), loopWebDist(other, 'js')];
    const outputs = [...loops, ...checks.map(check => check.dist)];
    expect(new Set(outputs).size).toBe(outputs.length);
    expect(new Set(checks.map(check => check.env.EXACT_BAKE_OUTPUT)).size).toBe(checks.length);
    for (const check of checks) expect(check.env.EXACT_WEB_TYPES).toBe('private');
    for (const [n, dist] of outputs.entries()) {
      const stage = resolve(dir, `stage-${n}`);
      mkdirSync(stage);
      writeFileSync(resolve(stage, 'index.html'), `server-${n}`);
      replaceBuild(stage, dist);
    }
    for (const [n, dist] of outputs.entries()) expect(readFileSync(resolve(dist, 'index.html'), 'utf8')).toBe(`server-${n}`);
    checks[0].close();
    expect(existsSync(checks[0].dist)).toBe(false);
    for (const dist of outputs.filter(dist => dist !== checks[0].dist)) expect(existsSync(resolve(dist, 'index.html'))).toBe(true);
  } finally { for (const check of checks) check.close(); rmSync(dir, { recursive: true, force: true }); }
});

test('Contract freshness compares compiled reads, including deletion and edits at an unchanged mtime', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-web-reads-'));
  const app = { dir, target: resolve(dir, 'target'), manifest: {} }, dist = resolve(app.target, 'dist');
  const entry = resolve(dir, 'app.contract'), used = resolve(dir, 'used.contract'), unused = resolve(dir, 'unused.contract');
  const text = 'component App\n  text "compiled"\n';
  const hash = value => createHash('sha256').update(value).digest('hex');
  try {
    mkdirSync(dist, { recursive: true });
    for (const path of [entry, used, unused]) writeFileSync(path, text);
    writeFileSync(resolve(dist, 'app.plan.map.json'), JSON.stringify({ sources: [entry, used].map(file => ({ file, sha256: hash(text) })) }));
    const reads = { contract: contractReadReceipt(dist) };
    const inputs = webInputDigests(app, true, []);
    writeFileSync(resolve(dist, '.exact-build.json'), JSON.stringify({ inputs, inputRoots: [], inputsAt: 0, reads }));
    expect(webChanges(dist, app).all).toEqual([]);
    writeFileSync(unused, 'changed');
    writeFileSync(resolve(dir, 'new.contract'), 'not imported');
    const output = resolve(dir, 'output.contract');
    writeFileSync(output, 'output');
    expect(webChanges(dist, app).all).toEqual([]);
    writeFileSync(resolve(dist, '.exact-build.json'), JSON.stringify({ inputs, inputRoots: [], inputsAt: 0 }));
    expect(webChanges(dist, app).app).toContain(output);
    writeFileSync(resolve(dist, '.exact-build.json'), JSON.stringify({ inputs, inputRoots: [], inputsAt: 0, reads }));
    utimesSync(used, 1, 1);
    expect(changedContractReads(reads.contract)).toEqual([]);
    // A save after compilation but before receipt publication must not be certified by a final reread.
    writeFileSync(used, 'changed during build');
    utimesSync(used, 1, 1);
    expect(contractReadReceipt(dist)[used]).toBe(hash(text));
    expect(webChanges(dist, app).app).toContain(used);
    rmSync(used);
    expect(changedContractReads(reads.contract)).toEqual([used]);
    writeFileSync(used, text);
    expect(webChanges(dist, app).all).toEqual([]);
    // Cargo can independently read Contract text with include_str!, outside the plan's graph.
    const included = resolve(dir, 'included.contract');
    writeFileSync(included, 'Rust input');
    const inputRoots = [{ file: included }];
    writeFileSync(resolve(dist, '.exact-build.json'), JSON.stringify({ inputs: webInputDigests(app, true, inputRoots), inputRoots, inputsAt: 0, reads }));
    writeFileSync(included, 'changed Rust input');
    expect(webChanges(dist, app).app).toContain(included);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.each(['asset edit', 'asset deletion', 'Cargo alias', 'loop fallback', 'loop deletion'])('%s stays a Contract input', kind => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-contract-inputs-'));
  const app = { dir, target: resolve(dir, 'target'), manifest: {} }, dist = resolve(app.target, 'dist');
  const included = resolve(dir, 'included.contract'), asset = resolve(dir, 'assets/example.contract');
  const alias = resolve(dir, 'alias');
  try {
    mkdirSync(dist, { recursive: true });
    mkdirSync(resolve(dir, 'assets'));
    symlinkSync(dir, alias);
    for (const path of [included, asset]) writeFileSync(path, 'read');
    const inputRoots = [{ file: realpathSync(included) }], reads = { contract: {} };
    const marker = resolve(dist, '.exact-build.json');
    writeFileSync(marker, JSON.stringify({ target: 'js', inputs: webInputDigests(app, true, inputRoots), inputRoots, inputsAt: 0, reads }));
    if (kind.startsWith('asset')) {
      if (kind === 'asset edit') writeFileSync(asset, 'edited asset');
      else rmSync(asset);
      expect(webChanges(dist, app).all).toContain(asset);
    } else if (kind === 'Cargo alias') {
      const linked = { ...app, dir: alias };
      writeFileSync(marker, JSON.stringify({ target: 'js', inputs: webInputDigests(linked, true, inputRoots), inputRoots, inputsAt: 0, reads }));
      writeFileSync(included, 'edited Cargo read');
      expect(webChanges(dist, linked).all).toContain(realpathSync(included));
    } else {
      fallbackRoots = inputRoots;
      const cargo = kind === 'loop deletion' ? cargoContractReads([{ file: included }]) : {};
      writeFileSync(marker, JSON.stringify({ target: 'js', inputsAt: 0, reads: { ...reads, cargo } }));
      if (kind === 'loop deletion') rmSync(included);
      else writeFileSync(included, 'edited Cargo read');
      expect(webChanges(dist, app).all.some(path => path === included || path === inputRoots[0].file)).toBe(true);
    }
  } finally { fallbackRoots = []; rmSync(dir, { recursive: true, force: true }); }
});

test.each(['app.wasm', 'app.hbc', 'app.js'])('packaging retains %s after another bake replaces it', name => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-web-products-'));
  const path = resolve(dir, name), sha256 = text => createHash('sha256').update(text).digest('hex');
  try {
    writeFileSync(path, 'first wasm');
    const receipt = text => ({ products: name.endsWith('.wasm') ? [{ path, sha256: sha256(text) }] : [], binary: { inputs: [{ path, name: `generated:app:web/${name}`, sha256: sha256(text) }] } });
    const first = captureWebProducts(receipt('first wasm'), resolve(dir, 'first'));
    writeFileSync(path, 'second wasm');
    const second = captureWebProducts(receipt('second wasm'), resolve(dir, 'second'));
    expect(readFileSync(first[path], 'utf8')).toBe('first wasm');
    expect(readFileSync(second[path], 'utf8')).toBe('second wasm');
    expect(() => captureWebProducts(receipt('first wasm'), resolve(dir, 'bad'))).toThrow('changed before capture');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('native verification owns its captures, products, bundle and receipt through launch', () => {
  const app = resolveApp('caltrain'), builds = ['test', 'verify', 'agent'].map(kind => privateNativeBuild(app, kind));
  const previous = process.env.EXACT_APPLE_OUTPUT;
  try {
    const paths = builds.map(build => {
      process.env.EXACT_APPLE_OUTPUT = build.env.EXACT_APPLE_OUTPUT;
      return appleArtifacts(app, { destination: 'ios-simulator' });
    });
    for (const field of ['capture', 'products', 'bundle', 'scratch', 'lock']) expect(new Set(paths.map(p => p[field])).size).toBe(builds.length);
    expect(new Set(paths.map(p => p.swift)).size).toBe(1);
    for (const [n, p] of paths.entries()) {
      mkdirSync(p.bundle, { recursive: true });
      writeFileSync(resolve(p.bundle, 'receipt.json'), `receipt-${n}`);
    }
    builds[0].close();
    expect(readFileSync(resolve(paths[1].bundle, 'receipt.json'), 'utf8')).toBe('receipt-1');
    expect(readFileSync(resolve(paths[2].bundle, 'receipt.json'), 'utf8')).toBe('receipt-2');
  } finally {
    if (previous === undefined) delete process.env.EXACT_APPLE_OUTPUT; else process.env.EXACT_APPLE_OUTPUT = previous;
    for (const build of builds) build.close();
  }
});

test.each(['ios', 'macos'])('repeated private %s drives reuse a validated build', async host => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-native-reuse-')), saved = { ...process.env };
  const sha256 = text => createHash('sha256').update(text).digest('hex');
  let builds = 0, bakes = 0, signed;
  const module = { appId: 'test.reuse', abi: 1, grants: '', surfaces: {}, bytecodeVersion: 96 };
  const outputs = [];
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'reuse', app: { id: 'test.reuse', name: 'Reuse' } }));
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  text "Ready"\n');
    const source = resolve(dir, 'native.swift'); writeFileSync(source, 'native source');
    const app = resolveApp(dir);
    const drive = async (plan = false) => {
      const owned = privateNativeBuild(app, 'agent'); outputs.push(owned);
      Object.assign(process.env, owned.env);
      await owned.seed?.(host);
      const code = await buildFirst(host, app, { say() {}, wait() {}, bake: async (app, into) => {
        bakes++; mkdirSync(into, { recursive: true });
        for (const [file, text] of [['app.plan', 'new plan'], ['app.hbc', 'bytecode'], ['app.module.json', JSON.stringify(module)]]) writeFileSync(resolve(into, file), text);
        return { code: 0, output: '' };
      }, build: async () => {
        builds++;
        const paths = appleArtifacts(app, { destination: host === 'ios' ? 'ios-simulator' : 'macos' });
        const bin = host === 'ios' ? resolve(paths.bundle, paths.executable) : resolve(paths.bundle, 'Contents/MacOS', paths.executable);
        mkdirSync(resolve(bin, '..'), { recursive: true }); writeFileSync(bin, 'executable');
        const generated = resolve(owned.env.EXACT_BAKE_OUTPUT, 'module.bin');
        mkdirSync(resolve(generated, '..'), { recursive: true }); writeFileSync(generated, 'captured input');
        const linked = linkedBakeDir(process.env, host, paths.target);
        mkdirSync(linked, { recursive: true });
        writeFileSync(resolve(linked, 'app.plan'), 'original plan'); writeFileSync(resolve(linked, 'app.module.json'), JSON.stringify(module));
        const receipt = { target: paths.target, started: new Date(Date.now() + 60_000).toISOString(), build: { trust: 'development',
          compat: { inputs: { platform: host, linked: ['router', 'io', 'markdown'], rustMode: 'off' }, embedded: { plan: { sha256: sha256('original plan') } } }, graph: { uses: [] },
          binary: { sha256: 'binary', inputs: [source, generated].map(path => ({ path, name: path, sha256: sha256(readFileSync(path)) })), missing: [], directories: [] } } };
        signed = JSON.stringify(receipt);
        for (const path of [resolve(paths.products, 'receipt.json'), host === 'ios' ? resolve(paths.bundle, 'receipt.json') : resolve(paths.bundle, 'Contents/Resources/receipt.json')]) {
          mkdirSync(resolve(path, '..'), { recursive: true }); writeFileSync(path, JSON.stringify(receipt));
        }
        return { code: 0, output: '' };
      } });
      expect(code).toBe(0);
      expect(nativeBuild(host, app).missing).not.toBe(true);
      const state = nativeBuild(host, app);
      if (plan) expect(planOverride(host, app, state)).not.toBeNull();
      else expect(state.changed).toEqual([]);
      const paths = appleArtifacts(app, { destination: host === 'ios' ? 'ios-simulator' : 'macos' });
      expect(readFileSync(host === 'ios' ? resolve(paths.bundle, 'receipt.json') : resolve(paths.bundle, 'Contents/Resources/receipt.json'), 'utf8')).toBe(signed);
      owned.close();
      expect(existsSync(owned.env.EXACT_APPLE_OUTPUT)).toBe(false);
    };
    const identity = createHash('sha256').update(resolve(app.dir)).digest('hex').slice(0, 16);
    const cache = resolve(app.target, 'native-reuse', identity, host, 'development/current');
    await drive();
    const firstArtifact = readFileSync(resolve(cache, 'artifact.json'), 'utf8');
    await drive();
    expect(readFileSync(resolve(cache, 'artifact.json'), 'utf8')).toBe(firstArtifact);
    expect(builds).toBe(1);
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  text "Updated"\n');
    const later = new Date(Date.now() + 120_000); utimesSync(resolve(dir, 'app.contract'), later, later);
    await drive(true); await drive(true);
    expect(builds).toBe(1); expect(bakes).toBe(1);
    utimesSync(resolve(dir, 'app.contract'), 1, 1);
    writeFileSync(source, 'changed native source');
    await drive();
    expect(builds).toBe(2);
    const artifact = JSON.parse(readFileSync(resolve(cache, 'artifact.json'), 'utf8'));
    const executable = artifact.files.find(([path, kind]) => kind === 'file' && path.includes('.app/') && !path.endsWith('.json'))[0];
    writeFileSync(resolve(cache, 'files', executable), 'corrupt cached executable');
    await drive();
    expect(builds).toBe(3);
  } finally {
    for (const owned of outputs) owned.close();
    for (const key of Object.keys(process.env)) if (!(key in saved)) delete process.env[key];
    Object.assign(process.env, saved); rmSync(dir, { recursive: true, force: true });
  }
}, 60_000);


test('Cargo claims use the platform locking implementation on Windows', () => {
  const source = `import {mock} from 'bun:test';
    let held = false;
    const locks = await import(${JSON.stringify(import.meta.resolve('../host/web/build-lock.mjs'))});
    mock.module(${JSON.stringify(import.meta.resolve('../host/web/build-lock.mjs'))}, () => ({...locks, takeLock(path, options) {
      if (!path.endsWith('.flock') || options.waitMs !== 0) throw new Error('wrong claim');
      held = true; return () => { held = false; };
    }}));
    const {claimBuildOutput} = await import(${JSON.stringify(import.meta.resolve('./app.mjs'))});
    const {mkdtempSync, rmSync} = await import('node:fs');
    const {tmpdir} = await import('node:os');
    const dir = mkdtempSync(tmpdir() + '/exact-windows-claim-');
    Object.defineProperty(process, 'platform', {value:'win32'});
    try { const release = claimBuildOutput({id:'windows', dir}, dir + '/claim');
      if (!held) throw new Error('platform lock was bypassed'); release();
      if (held) throw new Error('platform lock was not released');
    } finally { rmSync(dir, {recursive:true, force:true}); }`;
  const result = spawnSync(process.execPath, ['--eval', source], { encoding: 'utf8' });
  expect(result.status, result.stderr).toBe(0);
});

test.each(['contract', 'cargo'])('a JS loop keeps an unchanged %s read fresh after a save', kind => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-unchanged-contract-'));
  const app = { dir, target: resolve(dir, 'target'), manifest: {} }, dist = resolve(app.target, 'dist');
  const file = resolve(dir, 'included.contract');
  try {
    mkdirSync(dist, { recursive: true }); writeFileSync(file, 'same bytes');
    const reads = { contract: {}, [kind]: cargoContractReads([{ file }]) };
    writeFileSync(resolve(dist, '.exact-build.json'), JSON.stringify({ target: 'js', reads }));
    fallbackRoots = [{ file }];
    writeFileSync(file, 'same bytes');
    const future = new Date(Date.now() + 1000); utimesSync(file, future, future);
    expect(webChanges(dist, app).all).toEqual([]);
    rmSync(file);
    expect(webChanges(dist, app).all).toContain(file);
  } finally { fallbackRoots = []; rmSync(dir, { recursive: true, force: true }); }
});

test('Cargo includes changed at the build start or deleted remain uncertain reads', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-cargo-build-start-')), file = resolve(dir, 'included.contract');
  try {
    writeFileSync(file, 'compiled bytes');
    const began = Date.now() + 60_000, roots = [{ file }];
    expect(changedContractReads(cargoContractReads(roots, began))).toEqual([]);
    utimesSync(file, 1, 1);
    expect(cargoContractReads(roots, statSync(file).ctimeMs)).toEqual({ [file]: null });
    writeFileSync(file, 'later bytes'); utimesSync(file, began / 1000, began / 1000);
    expect(cargoContractReads(roots, began)).toEqual({ [file]: null });
    expect(changedContractReads(cargoContractReads(roots, began))).toEqual([file]);
    rmSync(file);
    expect(cargoContractReads(roots, began)).toEqual({ [file]: null });
    expect(changedContractReads(cargoContractReads(roots, began))).toEqual([file]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});


test('paired module capture includes bytecode named only by the embedded receipt', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-paired-products-'));
  const path = resolve(dir, 'app.module.json'), bytecode = resolve(dir, 'app.hbc');
  const sha256 = text => createHash('sha256').update(text).digest('hex');
  try {
    writeFileSync(bytecode, 'first bytecode');
    const text = JSON.stringify({ module: { file: 'app.hbc', sha256: sha256('first bytecode') } }); writeFileSync(path, text);
    const products = captureWebProducts({ products: [], binary: { inputs: [{ path, name: 'generated:app:web/app.module.json', sha256: sha256(text) }] } }, resolve(dir, 'capture'));
    writeFileSync(bytecode, 'second bytecode'); writeFileSync(path, '{}');
    expect(readFileSync(products[bytecode], 'utf8')).toBe('first bytecode');
    expect(readFileSync(products[path], 'utf8')).toBe(text);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('JS Rust producers generate privately and read grants from their captured module', async () => {
  const { cpSync } = await import('node:fs');
  const source = readFileSync(resolve(import.meta.dir, '../host/web-js/build.mjs'), 'utf8');
  const start = source.indexOf('if (rustLinked) {\n  mkdirSync');
  const body = source.slice(start, source.indexOf('// The granted secrets:', start));
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-js-rust-owner-')), out = resolve(dir, 'out'), gen = resolve(out, '.gen');
  try {
    mkdirSync(gen, { recursive: true }); writeFileSync(resolve(gen, 'app.bind.plan'), 'plan');
    const env = { resolve, mkdirSync, existsSync, dirname: path => resolve(path, '..'), cpSync, readFileSync, createHash,
      rustLinked: true, root: dir, sep: '/', moduleSourceDirs: () => [], devReload: true, devLogic: [], opt: () => null, out, gen, app: 'fixture', canvas2d: false, appDir: dir, ts: false,
      normalizeGrants: () => ({ entries: [] }),
      buildModule(app, at) {
        expect(at).toBe(resolve(gen, 'rust-module')); mkdirSync(at, { recursive: true });
        const path = resolve(at, 'app.module.wasm'); writeFileSync(path, 'captured'); return path;
      },
      async moduleGrants(path) { expect(path).toBe(resolve(out, 'rust/wasm/app.module.wasm')); return ''; },
    };
    await new Function('env', `return (async () => {const {${Object.keys(env).join(',')}} = env; let moduleStorage, rustGrants, rustGrantSet; ${body}})();`)(env);
    expect(readFileSync(resolve(out, 'rust/wasm/app.module.wasm'), 'utf8')).toBe('captured');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('a Rust module holds its Cargo product claim through optimization and capture', async () => {
  const { awaitBuildOutput } = await import('./app.mjs');
  const { lockHeld } = await import('../host/web/build-lock.mjs');
  const source = readFileSync(resolve(import.meta.dir, '../host/web-js/module.mjs'), 'utf8');
  const body = source.slice(source.indexOf('function module('), source.indexOf('\nexport function buildModule'));
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-js-rust-cargo-')), out = resolve(dir, 'private');
  const lock = resolve(dir, '.claims/fixture.lock.flock');
  try {
    const env = { TARGET: dir, resolve, dirname: path => resolve(path, '..'), mkdirSync, awaitBuildOutput, moduleDepInfo: new Map(),
      fresh: () => false, cargoWasm() { expect(lockHeld(lock)).toBe(true); return { status: 0 }; },
      replace: (path, write) => write(path), optimize(from, to) { expect(lockHeld(lock)).toBe(true); writeFileSync(to, 'captured module'); },
    };
    const build = new Function('env', `const {${Object.keys(env).join(',')}} = env; ${body}; return module;`)(env);
    expect(readFileSync(build(resolve(dir, 'Cargo.toml'), 'fixture', out, 'fixture'), 'utf8')).toBe('captured module');
    expect(lockHeld(lock)).toBe(false);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('Windows test, agent and verify allocate private outputs before preparing children', () => {
  const source = `import {mock} from 'bun:test';
    import {EventEmitter} from 'node:events';
    import * as children from 'node:child_process';
    import * as shells from ${JSON.stringify(import.meta.resolve('./app-shells.mjs'))};
    import {mkdtempSync, writeFileSync, rmSync} from 'node:fs';
    import {tmpdir} from 'node:os';
    const calls = [];
    mock.module(${JSON.stringify(import.meta.resolve('./app-shells.mjs'))}, () => ({...shells, platformOf: () => null}));
    mock.module('node:child_process', () => ({...children, spawn(command, args, options) {
      calls.push({args, env: options.env});
      const child = new EventEmitter(); child.kill = () => {}; child.unref = () => {};
      queueMicrotask(() => child.emit('exit', 0)); return child;
    }}));
    const {run} = await import(${JSON.stringify(import.meta.resolve('./app-cli.mjs'))});
    const dir = mkdtempSync(tmpdir() + '/exact-windows-checks-');
    writeFileSync(dir + '/app.json', JSON.stringify({name:'fixture', app:{id:'com.exact.fixture', name:'Fixture'}, host:{windows:{}}}));
    writeFileSync(dir + '/app.contract', 'component App\\n  text "hello"\\n');
    try {
      const outputs = [];
      for (const verb of ['test','agent','verify']) {
        const start = calls.length;
        const code = await run({dir, name:'fixture', exact2:${JSON.stringify(resolve(import.meta.dir, '..'))}, args:[verb,'windows', ...(verb === 'agent' ? ['tree'] : [])]});
        if (code) throw new Error('command failed: ' + code);
        const selected = calls.slice(start).filter(call => ['agent-build.mjs','agent.mjs','verify.mjs'].some(file => call.args[0]?.endsWith('/scripts/' + file)));
        if (!selected.length || selected.some(call => !call.env.EXACT_WINDOWS_OUTPUT || !call.env.EXACT_BAKE_OUTPUT)) throw new Error(verb + ' bypassed isolation');
        if (new Set(selected.map(call => call.env.EXACT_WINDOWS_OUTPUT)).size !== 1) throw new Error('preparation and drive disagree');
        outputs.push(selected[0].env.EXACT_WINDOWS_OUTPUT);
      }
      if (new Set(outputs).size !== 3) throw new Error('commands share output');
    } finally { rmSync(dir, {recursive:true, force:true}); }`;
  const result = spawnSync(process.execPath, ['--eval', source], { encoding: 'utf8' });
  expect(result.status, result.stderr).toBe(0);
});

test('Windows packaging and preparation use the selected executable after Cargo output changes', async () => {
  const { copyFileSync } = await import('node:fs');
  const { windowsBuild } = await import('./agent-build.mjs');
  const source = readFileSync(resolve(import.meta.dir, '../host/windows/build.mjs'), 'utf8');
  const start = source.indexOf('if (import.meta.main) {') + 'if (import.meta.main) {'.length;
  const body = source.slice(start, source.lastIndexOf('}'));
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-windows-products-'));
  const app = { dir, target: resolve(dir, 'target'), name: 'fixture', id: 'com.exact.fixture', displayName: 'fixture', manifest: {}, crate: () => 'fixture-windows' };
  const build = privateNativeBuild(app, 'agent'), before = process.env.EXACT_WINDOWS_OUTPUT;
  try {
    process.env.EXACT_WINDOWS_OUTPUT = build.env.EXACT_WINDOWS_OUTPUT;
    const path = resolve(dir, 'fixture-windows.exe'); writeFileSync(path, 'first executable');
    const env = { process: { platform: 'win32', argv: [], env: build.env }, guardFlags: () => [], resolveApp: () => app, prepareShaders() {},
      bakeTarget: () => 'x86_64-pc-windows-msvc', pickIcon: () => null, resolve, mkdirSync, developmentBuildEnv: () => build.env,
      copyFileSync, readFileSync, writeFileSync, createHash, windowsFile: () => 'fixture.exe', closeOpened() {}, syncStaticTree() {}, copyShaders() {},
      writePackagedReceipt() {}, unclaimedNote: () => null, console: { log() {} },
      buildBake(app, kind, target, options) {
        const receipt = { products: [{ path, sha256: createHash('sha256').update('first executable').digest('hex') }], compat: {} };
        options.capture(receipt); writeFileSync(path, 'later executable'); return receipt;
      },
    };
    await new Function('env', `return (async()=>{const {${Object.keys(env).join(',')}} = env; ${body}})();`)(env);
    const selected = windowsBuild(app).bin;
    expect(selected).toBe(resolve(build.env.EXACT_WINDOWS_OUTPUT, 'fixture.exe'));
    expect(readFileSync(selected, 'utf8')).toBe('first executable');
  } finally {
    if (before === undefined) delete process.env.EXACT_WINDOWS_OUTPUT; else process.env.EXACT_WINDOWS_OUTPUT = before;
    build.close(); rmSync(dir, { recursive: true, force: true });
  }
});

test('one-shot wasm packaging preserves the bake receipt Rust reloads consume', async () => {
  const { bakeOutput, readBuilds } = await import('./app.mjs');
  const source = readFileSync(resolve(import.meta.dir, '../host/web/build.mjs'), 'utf8');
  const start = source.indexOf('const buildEnv = developmentBuildEnv();');
  const body = source.slice(start, source.indexOf('// Proofs select', start));
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-persistent-bake-')), app = { id: 'com.exact.fixture', dir, target: resolve(dir, 'target') };
  const exits = [];
  try {
    const env = { app, resolve, mkdirSync, mkdtempSync, rmSync, developmentBuildEnv: () => ({}), bakeOutput,
      process: { env: {}, once: (event, fn) => exits.push(fn) } };
    const buildEnv = new Function('env', `const {${Object.keys(env).join(',')}} = env; ${body}; return buildEnv;`)(env);
    mkdirSync(buildEnv.EXACT_BAKE_OUTPUT, { recursive: true });
    writeFileSync(resolve(buildEnv.EXACT_BAKE_OUTPUT, 'web-wasm32-unknown-unknown.build.json'), JSON.stringify({ version: 1, compat: { inputs: { app: app.id } } }));
    for (const exit of exits) exit();
    expect(readBuilds(app, {}).map(build => build.compat.inputs.app)).toEqual([app.id]);
  } finally { for (const exit of exits) exit(); rmSync(dir, { recursive: true, force: true }); }
});
