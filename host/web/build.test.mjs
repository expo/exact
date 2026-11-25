import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, symlinkSync, utimesSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, relative, resolve } from 'node:path';
import { webChanges, webInputDigests } from '../../scripts/agent-launch.mjs';
import { versionFile, webSourceRoots } from './build.mjs';
import { appSources } from './serve.mjs';
import { WEB_TOOLCHAIN } from '../../scripts/app.mjs';

const root = resolve(import.meta.dir, '../..');
const write = (dir, file, text) => {
  const path = resolve(dir, file);
  mkdirSync(dirname(path), { recursive: true }); writeFileSync(path, text);
  return path;
};

test('a declarative web roster needs the page ABI and checks the current artifact', async () => {
  const { webModuleTags } = await import('../web-js/module.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'web-module-roster-'));
  const tags = () => webModuleTags(dir, '/', ['map-view'], true);
  try {
    const declarations = "export const abi = 1, roster = { 'map-view': {} };";
    write(dir, 'modules/index.js', declarations);
    expect(await tags()).toEqual([]);
    write(dir, 'modules/index.js', declarations + 'export function create() {} export function setProps() {} export function destroy() {}');
    expect(await tags()).toEqual(['map-view']);
    write(dir, 'modules/table.js', readFileSync(resolve(dir, 'modules/index.js'), 'utf8'));
    write(dir, 'modules/index.js', "export * from './table.js';");
    expect(await tags()).toEqual(['map-view']);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.skipIf(process.platform !== 'darwin')('a DOM-dependent roster reads the staged page and its assets in system WebKit', async () => {
  const { webModuleTags } = await import('../web-js/module.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'web-page-roster-'));
  try {
    write(dir, 'index.html', '<meta name="table" content="./assets/table.json">');
    write(dir, 'assets/table.json', '{"map-view":{}}');
    write(dir, 'modules/index.js', `
      const table = document.querySelector('meta[name=table]').content;
      export const abi = 1, roster = await (await fetch(table)).json();
      export function create() {} export function setProps() {} export function destroy() {}
    `);
    expect(await webModuleTags(dir, '/mounted/', ['map-view'], true)).toEqual(['map-view']);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.each([false, true].flatMap(game => [null, WEB_TOOLCHAIN].map(toolchain => [game, toolchain])))('web receipts include external Cargo inputs (generated game: %s, toolchain: %s)', (game, toolchain) => {
  const dir = mkdtempSync(resolve(root, 'target/web-external-'));
  try {
    const shell = game ? 'app/.shells/web' : 'app/web', up = game ? '../../../' : '../../';
    write(dir, 'Cargo.toml', `[workspace]\nmembers=["${shell}"${game ? ',"app/.shells/gpu"' : ''}]\nresolver="2"\n`);
    write(dir, `${shell}/Cargo.toml`, '[package]\nname="fixture-web"\nversion="0.1.0"\nedition="2021"\n');
    // The app's own text: a picture, log or note beside app.json is no input unless the compiler read it.
    write(dir, `${shell}/src/lib.rs`, `pub const CONTENT: &str = include_str!("../${up}shared/content.contract");\npub const NOTE: &str = include_str!("../${up}app/content.txt");\n`);
    if (game) {
      write(dir, 'app/.shells/gpu/Cargo.toml', '[package]\nname="fixture-gpu"\nversion="0.1.0"\nedition="2021"\n');
      write(dir, 'app/.shells/gpu/src/lib.rs', '');
      write(dir, 'app/logic/src/lib.rs', '');
    }
    write(dir, `${game ? 'app/.shells/gpu' : shell}/build.rs`, `#[path="${up}shared/helper.rs"] mod helper;\nfn main() { helper::watch(); }\n`);
    const paths = [write(dir, 'shared/content.contract', 'original'),
      write(dir, 'shared/helper.rs', `pub fn watch() { println!("cargo:rerun-if-changed=${up}shared/config.json"); }\n`),
      write(dir, 'shared/config.json', '{}'), write(dir, 'app/content.txt', 'original')];
    const built = spawnSync('cargo', [...toolchain ? [`+${toolchain}`] : [], 'build', '--offline', '--target-dir', resolve(dir, 'target')], { cwd: dir, encoding: 'utf8' });
    expect(built.status, built.stderr).toBe(0);
    const metadata = JSON.parse(spawnSync('cargo', ['metadata', '--offline', '--format-version', '1'], { cwd: dir, encoding: 'utf8' }).stdout);
    const app = { dir: resolve(dir, 'app'), target: resolve(dir, 'target'), workspace: dir, manifest: game ? { game: {} } : {},
      crate: kind => `fixture-${kind}`, prepare: () => metadata };
    const inputRoots = webSourceRoots(app, false), inputs = webInputDigests(app, false, inputRoots);
    const dist = resolve(dir, 'dist');
    write(dist, '.exact-build.json', JSON.stringify({ inputs, inputRoots, inputsAt: Date.now() }));
    for (const path of paths) {
      const shown = game ? path : relative(root, path), before = readFileSync(path);
      expect(inputs[shown], shown).toBeString();
      expect(webChanges(dist, app).all).toEqual([]);
      const side = path.startsWith(app.dir + '/') ? 'app' : 'shared';
      writeFileSync(path, 'changed');
      expect(webChanges(dist, app)[side]).toContain(shown);
      rmSync(path);
      expect(webChanges(dist, app)[side]).toContain(shown);
      writeFileSync(path, before);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60_000); // Each case builds its fixture with Cargo, cold.

test('a relocated warm proof refreshes roots and digests together', async () => {
  const dir = mkdtempSync(resolve(root, 'target/web-relocated-'));
  const original = resolve(dir, 'original'), snapshot = resolve(dir, 'snapshot'), dist = resolve(snapshot, 'dist');
  try {
    const manifest = { game: {} };
    write(original, 'logic/src/lib.rs', 'source');
    write(original, '.shells/.cargo/config.toml', '[build]\n');
    const app = { dir: original, manifest };
    const inputRoots = [...webSourceRoots(app, false), { file: resolve(original, '.shells/.cargo/config') }];
    write(original, 'dist/.exact-build.json', JSON.stringify({ inputs: webInputDigests(app, false, inputRoots), inputRoots }));
    cpSync(original, snapshot, { recursive: true });
    // Exercise the same relocation step the warm-build branch of the proof runs.
    const source = readFileSync(resolve(root, 'game/proof.mjs'), 'utf8');
    const from = source.indexOf("        const marker = resolve(dist, '.exact-build.json');");
    const to = source.indexOf('\n        return;', from);
    const { publicFileCards } = await import('./serve.mjs');
    const { closeFilesystemReader } = await import('../../scripts/filesystem.mjs');
    const bindings = { resolve, dist, readFileSync, writeFileSync, webInputDigests, webSourceRoots, appInfo: { dir: snapshot, manifest }, publicFileCards, closeFilesystemReader };
    await new (async function () {}).constructor(...Object.keys(bindings), source.slice(from, to))(...Object.values(bindings));
    rmSync(original, { recursive: true });
    expect(webChanges(dist, bindings.appInfo).all).toEqual([]);
    write(snapshot, 'logic/src/lib.rs', 'changed');
    expect(webChanges(dist, bindings.appInfo).app).toContain(resolve(snapshot, 'logic/src/lib.rs'));
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// Whether a leaf is built and fresh is the test's to say, not the checkout's: a markdown leaf another command
// built would otherwise excuse the toolchain this asks for.
test.each(['rust', 'markdown', 'markdown-taken', 'motion', 'editor', 'flow', 'plain', 'gpu', 'gpu-ready', 'gpu-broken'])('JS prerequisites follow the %s artifacts', mode => {
  const dir = mkdtempSync(resolve(root, 'target/web-prerequisites-'));
  try {
    write(dir, 'app.json', JSON.stringify({ app: { id: 'com.exact.fixture', name: 'Fixture' }, name: 'fixture', rust: mode === 'rust' ? { module: { package: 'fixture-data' } } : false }));
    write(dir, 'app.contract', 'component App\n  text "ready"\n');
    if (mode.startsWith('gpu')) write(dir, 'gpu/src/lib.rs', '');
    const script = `
      import { mock } from 'bun:test';
      import { mkdirSync, writeFileSync } from 'node:fs';
      import { resolve } from 'node:path';
      import * as processes from 'node:child_process';
      const appPath = ${JSON.stringify(resolve(root, 'scripts/app.mjs'))};
      const actual = await import(appPath);
      mock.module(appPath, () => ({ ...actual,
        resolveApp: () => ({ hasGpu: true, manifest: {}, crate: kind => 'fixture-' + kind, target: ${JSON.stringify(resolve(dir, 'target'))} }),
        webToolchainEnv() { throw new Error('missing test rust-src'); } }));
      mock.module('node:child_process', () => ({ ...processes, spawnSync(command, args, options) {
        if (command === 'wasm-bindgen') return ${JSON.stringify(mode)} === 'gpu-ready' ? { status: 0 }
          : { status: ${mode === 'gpu-broken' ? 1 : 'null'}, error: { code: 'ENOENT' } };
        if (args.includes('js')) {
          const gen = args[args.indexOf('-o') + 1]; mkdirSync(gen, { recursive: true });
          writeFileSync(resolve(gen, 'app.js'), 'export const sources={};export const wait=0;');
          if (${JSON.stringify(mode)} !== 'plain' && ${JSON.stringify(mode)} !== 'rust') writeFileSync(resolve(gen, ${JSON.stringify(mode.replace('-taken', '') + '.flag')}), '');
          return { status: 0 };
        }
        return processes.spawnSync(command, args, options);
      } }));
      const modulePath = ${JSON.stringify(resolve(root, 'host/web-js/module.mjs'))};
      const modules = await import(modulePath);
      mock.module(modulePath, () => ({ ...modules, webCompiler() { return { cmd: 'fixture-compiler', pre: [], done() {} }; },
        leafTaken: () => ${JSON.stringify(mode.endsWith('-taken'))} }));
      process.argv = [process.execPath, ${JSON.stringify(resolve(root, 'host/web-js/build.mjs'))}, 'fixture', '--prerequisites', '--out', ${JSON.stringify(resolve(dir, 'out'))}];
      await import(${JSON.stringify(resolve(root, 'host/web-js/build.mjs'))});
    `;
    const checked = spawnSync(process.execPath, ['-e', script], { encoding: 'utf8', timeout: 30000, env: { ...process.env, EXACT_APP_DIR: dir } });
    if (['plain', 'gpu-ready', 'markdown-taken'].includes(mode)) expect(checked.status, checked.stderr).toBe(0);
    else { expect(checked.status).not.toBe(0); expect(checked.stderr).toContain(mode.startsWith('gpu') ? 'wasm-bindgen' : 'missing test rust-src'); }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.each(['ready', 'missing', 'broken'])('a clean game prepares its workspace before checking prerequisites (%s GPU tools)', tools => {
  const dir = mkdtempSync(resolve(root, 'target/web-game-order-'));
  const script = `
    import { mock } from 'bun:test';
    import * as processes from 'node:child_process';
    import { existsSync, mkdirSync, writeFileSync } from 'node:fs';
    import { resolve } from 'node:path';
    import assert from 'node:assert/strict';
    const appPath = ${JSON.stringify(resolve(root, 'scripts/app.mjs'))};
    const actual = await import(appPath), run = processes.spawnSync, dir = ${JSON.stringify(dir)}, shells = resolve(dir, '.shells'), calls = [];
    assert.ok(!existsSync(shells));
    mock.module(appPath, () => ({ ...actual, resolveApp: () => ({ name: 'fixture', dir, target: resolve(dir, 'target'),
      manifest: { game: {} }, hasGpu: true, workspace: shells, crate: kind => 'fixture-' + kind,
      prepare() {
        calls.push('prepare'); mkdirSync(shells);
        return { workspace_root: shells,
          packages: ['web', 'gpu'].map(id => ({ id, name: 'fixture-' + id, manifest_path: resolve(shells, id, 'Cargo.toml') })),
          resolve: { nodes: ['web', 'gpu'].map(id => ({ id, deps: [] })) } };
      },
    }), webToolchainEnv(env) { assert.ok(existsSync(shells)); calls.push('toolchain'); return env; } }));
    mock.module('node:child_process', () => ({ ...processes, spawnSync(command, args, options) {
      if (command === 'wasm-bindgen') {
        assert.ok(existsSync(shells)); calls.push('wasm-bindgen');
        return { status: ${tools === 'ready' ? 0 : tools === 'broken' ? 1 : 'null'}, error: ${tools === 'missing' ? "{code:'ENOENT'}" : 'null'} };
      }
      if (command === 'cargo') {
        assert.ok(existsSync(shells), 'Cargo ran before the game workspace existed');
        assert.equal(args[0], 'metadata'); calls.push('metadata');
      }
      return run(command, args, options);
    } }));
    process.argv = [process.execPath, ${JSON.stringify(resolve(root, 'host/web/build.mjs'))}, 'fixture', '--check'];
    await import(${JSON.stringify(resolve(root, 'host/web/build.mjs'))});
    assert.deepEqual(calls, ['prepare', 'metadata', 'toolchain', 'wasm-bindgen']);
    assert.ok(!existsSync(resolve(dir, 'target/web-dist')));
    writeFileSync(resolve(dir, 'checked'), 'ready');
  `;
  try {
    const checked = spawnSync(process.execPath, ['-e', script], { encoding: 'utf8', timeout: 30_000 });
    if (tools === 'ready') {
      expect(checked.status, checked.stderr).toBe(0);
      expect(checked.stdout).toMatch(/web build prerequisites ready/);
      expect(existsSync(resolve(dir, 'checked'))).toBe(true);
    } else {
      expect(checked.status).not.toBe(0);
      expect(checked.stderr).toMatch(/wasm-bindgen unavailable/);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('web freshness follows render and GPU dependencies outside the app', () => {
  const dir = mkdtempSync(resolve(root, 'target/web-closure-')), appDir = resolve(dir, 'app');
  const write = (path, text) => { mkdirSync(dirname(resolve(dir, path)), { recursive: true }); writeFileSync(resolve(dir, path), text); };
  const crates = ['app/web', 'app/linux', 'app/gpu', 'app/gpu-addon', 'render-input', 'gpu-input', 'addon-input'];
  try {
    write('Cargo.toml', `[workspace]\nresolver="2"\nmembers=${JSON.stringify(crates)}\n`);
    for (const [i, crate] of crates.entries()) {
      const name = i < 4 ? `fixture-${['web', 'linux', 'gpu', 'gpu-addon'][i]}` : crate;
      const dependency = [null, 'render-input', 'gpu-input', 'addon-input'][i];
      write(`${crate}/Cargo.toml`, `[package]\nname="${name}"\nversion="0.1.0"\nedition="2021"\n` +
        (dependency ? `[dependencies]\n${dependency}={path="../../${dependency}"}\n` : ''));
      write(`${crate}/src/lib.rs`, 'pub fn source() {}\n');
    }
    write('app/linux/src/bin/fixture-render.rs', 'fn main() {}\n');
    const metadata = spawnSync('cargo', ['metadata', '--offline', '--format-version', '1'], { cwd: dir, encoding: 'utf8' });
    expect(metadata.status).toBe(0);
    const app = { dir: appDir, workspace: dir, manifest: { gpu: { modules: { addon: ['addon'] } } }, crate: kind => `fixture-${kind}` };
    const dist = resolve(dir, 'dist'); mkdirSync(dist);
    for (const js of [true, false]) {
      const inputRoots = webSourceRoots(app, js), inputs = webInputDigests(app, js, inputRoots);
      write('dist/.exact-build.json', JSON.stringify({ target: js ? 'js' : 'wasm', inputs, inputRoots, inputsAt: Date.now() }));
      const drive = { ...app, get workspace() { throw new Error('a drive queried Cargo'); } };
      expect(inputs['host/web-js/src/main.rs']).toBeString();
      expect(webChanges(dist, drive).all).toEqual([]);
      for (const crate of ['render-input', 'gpu-input', 'addon-input']) {
        const path = resolve(dir, crate, 'src/lib.rs');
        const shown = path.slice(root.length + 1);
        expect(inputs[shown]).toBeString();
        writeFileSync(path, 'pub fn changed() {}\n');
        expect(webChanges(dist, drive).shared).toContain(shown);
        writeFileSync(path, 'pub fn source() {}\n');
        expect(webChanges(dist, drive).all).toEqual([]);
        rmSync(path);
        expect(webChanges(dist, drive).shared).toContain(shown);
        writeFileSync(path, 'pub fn source() {}\n');
      }
      const entry = resolve(appDir, 'linux/src/bin/fixture-render.rs');
      writeFileSync(entry, 'fn main() { panic!(); }\n');
      expect(webChanges(dist, drive).app).toContain(entry.slice(root.length + 1));
      writeFileSync(entry, 'fn main() {}\n');
      write('dist/.exact-build.json', JSON.stringify({ target: js ? 'js' : 'wasm', inputs, inputsAt: Date.now() }));
      expect(webChanges(dist, drive).all).toEqual([resolve(dir, 'dist/.exact-build.json').slice(root.length + 1)]);
      write('render-input/src/lib.rs', 'pub fn changed() {}\n');
      expect(webChanges(dist, drive).all).toEqual([resolve(dir, 'dist/.exact-build.json').slice(root.length + 1)]);
      write('render-input/src/lib.rs', 'pub fn source() {}\n');
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('a web marker without digests checks backend selection by time', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'web-marker-time-')), dist = resolve(dir, 'dist');
  const marker = resolve(dist, '.exact-build.json'), selection = resolve(dir, '.exact/backend-origin');
  // After every shared source was last written: those are checked by time too.
  const now = Date.now(), app = { dir, manifest: {} }, before = new Date(now + 30_000), built = new Date(now + 60_000), after = new Date(now + 90_000);
  try {
    mkdirSync(dist); mkdirSync(resolve(dir, '.exact'));
    writeFileSync(marker, JSON.stringify({ target: 'js' })); utimesSync(marker, built, built);
    expect(webChanges(dist, app).all).toEqual([]);
    writeFileSync(selection, 'http://127.0.0.1:4412'); utimesSync(selection, before, before);
    expect(webChanges(dist, app).all).toEqual([]);
    utimesSync(selection, after, after);
    expect(webChanges(dist, app).app).toEqual([selection]);
    rmSync(selection);
    expect(webChanges(dist, app).all).toEqual([]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('a web marker without roots checks the shared runtime by time, as the dev loop writes it', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'web-dev-marker-')), dist = resolve(dir, 'dist'), marker = resolve(dist, '.exact-build.json');
  try {
    mkdirSync(dist);
    writeFileSync(marker, JSON.stringify({ exactBuild: 1, target: 'js' }));
    const later = new Date(Date.now() + 60_000); utimesSync(marker, later, later);
    expect(webChanges(dist, { dir, manifest: {} }).all).toEqual([]);
    // Built before the runtime's sources were last written: host/web-js/rt.js changed since.
    utimesSync(marker, new Date(10_000), new Date(10_000));
    expect(webChanges(dist, { dir, manifest: {} }).shared).toContain('host/web-js/rt.js');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// expo/exact#415: what a build writes under the app is no source of it, wherever Cargo's target directory is, so a
// build's receipt and `serve` list the same sources before and after it.
test('the sources a receipt lists leave out the build output, dependencies and Cargo\'s target directory', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'web-sources-')), out = resolve(dir, 'dist'), was = process.env.CARGO_TARGET_DIR;
  try {
    write(dir, 'app.contract', 'component App\n  view\n    text "a"\n');
    write(dir, 'assets/a.txt', 'a');
    process.env.CARGO_TARGET_DIR = resolve(dir, 'build');
    const before = appSources(dir, out);
    for (const file of ['build/web-js-dev-stage/0123/app.js', 'target/debug/x', 'node_modules/p/index.js', 'dist/index.html', 'dist.gen/x/app.js', 'app.contract.d.ts', '.exact/x'])
      write(dir, file, 'x');
    expect(appSources(dir, out).digest).toBe(before.digest);
    // The app named through a link, and the output by its real path (as `web-build --out` names it): the same files.
    const link = `${dir}-link`;
    symlinkSync(dir, link);
    try { expect(appSources(link, resolve(realpathSync(dir), 'dist')).digest).toBe(before.digest); } finally { rmSync(link, { force: true }); }
    write(dir, 'assets/b.txt', 'b');
    const added = appSources(dir, out).digest;
    rmSync(resolve(dir, 'assets/a.txt'));
    expect([added === before.digest, appSources(dir, out).digest === added]).toEqual([false, false]);
  } finally {
    if (was === undefined) delete process.env.CARGO_TARGET_DIR; else process.env.CARGO_TARGET_DIR = was;
    rmSync(dir, { recursive: true, force: true });
  }
});

test("a wasm build's app.version.json carries the bake's version and whether its trust is production", () => {
  expect(JSON.parse(versionFile({ version: '2.3.4', buildNumber: '56', trust: 'production', seq: 0 }))).toEqual({ version: '2.3.4', buildNumber: '56', production: true });
  expect(JSON.parse(versionFile({ version: '', buildNumber: '', trust: 'development' }))).toEqual({ version: '', buildNumber: '', production: false });
  expect(versionFile({ version: '1', buildNumber: '1', trust: 'development' }).endsWith('}\n')).toBe(true);
});

// expo/exact#812: a dev-only bin in a dependency crate (and another build's dep-info for it)
// is not an input of the web build.
test('web freshness ignores dev-only bins and foreign dep-info in GPU dependencies', () => {
  const dir = mkdtempSync(resolve(root, 'target/web-devbin-')), appDir = resolve(dir, 'app');
  const put = (path, text) => { mkdirSync(dirname(resolve(dir, path)), { recursive: true }); writeFileSync(resolve(dir, path), text); };
  try {
    put('Cargo.toml', '[workspace]\nresolver="2"\nmembers=["app/web","app/gpu","physics"]\n');
    put('app/web/Cargo.toml', '[package]\nname="devbin-web"\nversion="0.1.0"\nedition="2021"\n');
    put('app/web/src/lib.rs', 'pub fn web() {}\n');
    put('app/gpu/Cargo.toml', '[package]\nname="devbin-gpu"\nversion="0.1.0"\nedition="2021"\n[dependencies]\nphysics={path="../../physics"}\n[[bin]]\nname="gallery-trace"\npath="src/bin/gallery-trace.rs"\n');
    put('app/gpu/src/lib.rs', 'pub fn gpu() { physics::step(); }\n');
    put('app/gpu/src/bin/gallery-trace.rs', 'fn main() {}\n');
    put('physics/Cargo.toml', '[package]\nname="physics"\nversion="0.1.0"\nedition="2021"\n[features]\ntrace=[]\n[[bin]]\nname="trace"\npath="src/bin/trace.rs"\nrequired-features=["trace"]\n');
    put('physics/src/lib.rs', 'pub fn step() {}\n');
    put('physics/src/bin/trace.rs', 'fn main() {}\n');
    put('physics/src/bin/extra.rs', 'pub fn extra() {}\n');
    // Simulate a prior native build of `trace` leaving dep-info in target/debug.
    put('target/debug/trace.d', `target/debug/trace: ${resolve(dir, 'physics/src/bin/trace.rs')} ${resolve(dir, 'physics/src/bin/extra.rs')}\n`);
    const meta = spawnSync('cargo', ['metadata', '--offline', '--format-version', '1'], { cwd: dir, encoding: 'utf8' });
    expect(meta.status).toBe(0);
    const app = { dir: appDir, workspace: dir, target: resolve(dir, 'target'), manifest: {}, crate: kind => `devbin-${kind}` };
    const dist = resolve(dir, 'dist'); mkdirSync(dist);
    const inputRoots = webSourceRoots(app, false), inputs = webInputDigests(app, false, inputRoots);
    put('dist/.exact-build.json', JSON.stringify({ target: 'wasm', inputs, inputRoots, inputsAt: Date.now() }));
    const traceShown = resolve(dir, 'physics/src/bin/trace.rs').slice(root.length + 1);
    const galleryShown = resolve(dir, 'app/gpu/src/bin/gallery-trace.rs').slice(root.length + 1);
    const libShown = resolve(dir, 'physics/src/lib.rs').slice(root.length + 1);
    expect(inputs[traceShown]).toBeUndefined();
    expect(inputs[galleryShown]).toBeUndefined();
    expect(inputs[libShown]).toBeString();
    const drive = { ...app, get workspace() { throw new Error('a drive queried Cargo'); } };
    writeFileSync(resolve(dir, 'physics/src/bin/trace.rs'), 'fn main() { println!("edit"); }\n');
    writeFileSync(resolve(dir, 'app/gpu/src/bin/gallery-trace.rs'), 'fn main() { println!("edit"); }\n');
    expect(webChanges(dist, drive).all).toEqual([]);
    writeFileSync(resolve(dir, 'physics/src/lib.rs'), 'pub fn step() { let _ = 1; }\n');
    expect(webChanges(dist, drive).shared).toContain(libShown);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
