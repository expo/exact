import { expect, test } from 'bun:test';
import { bindingChanges, packagedKinds, repack, repackedBuild, versionsOf } from './repack.mjs';
import { frameworkInfo, plistFile } from './assets.mjs';
import { EXTENSION_KINDS } from './extensions.mjs';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { repackBase, reservedPlan } from './native-extras.mjs';

// A template base's compat.json (`exact new`, the placeholder icon) and an app's bake from it.
const compat = (inputs, rest = {}) => ({ id: 'c', target: 'aarch64-apple-ios', inputs: { platform: 'ios', executors: ['hermes'], trust: 'development', minimumOS: '17.0', ...inputs }, embedded: { trust: 'development', plan: 'p1', assets: 'a1' }, ...rest });
const template = compat({ app: 'dev.exact.template', icons: ['icon.png'], rustGrants: 'g1' });

test('an app with its own icons repacks onto a base drawn with others', () => {
  expect(bindingChanges(template, compat({ app: 'com.example.photos', icons: ['assets/icon.png', 'assets/icon-dark.png'], rustGrants: 'g2' }))).toEqual([]);
  expect(bindingChanges(template, compat({ app: 'com.example.photos' }))).toEqual([]);
  // The plan's and assets' digests are the bundle's own.
  expect(bindingChanges(template, compat({ app: 'dev.exact.template', icons: ['icon.png'] }, { embedded: { trust: 'development', plan: 'p2', assets: 'a2' } }))).toEqual([]);
});

test('a new version or build number repacks, and frameworks take the one they are given', () => {
  const stamped = (version, buildNumber) => compat({ app: 'dev.exact.template', icons: ['icon.png'] }, { embedded: { trust: 'development', plan: 'p1', assets: 'a1', version, buildNumber } });
  expect(bindingChanges(stamped('0.1.0', '1'), stamped('2.3.4', '57'))).toEqual([]);
  expect(bindingChanges(stamped('0.1.0', '1'), { ...stamped('0.1.0', '2'), embedded: { ...stamped('0.1.0', '2').embedded, trust: 'production' } })).toEqual(['embedded']);
  const app = { id: 'com.example.photos', manifest: {} };
  const info = plistFile(frameworkInfo('ExactWidgetReload', app, 'iPhoneOS', { CFBundleVersion: '57', CFBundleShortVersionString: '2.3.4' }));
  expect(versionsOf(info)).toEqual({ CFBundleVersion: '57', CFBundleShortVersionString: '2.3.4' });
  expect(versionsOf('<dict></dict>')).toEqual({ CFBundleVersion: null, CFBundleShortVersionString: null });
});

test('what the binary was built to provide still refuses a repack, by name', () => {
  expect(bindingChanges(template, compat({ app: 'com.example.photos', icons: ['other.png'], executors: ['hermes', 'wasmi'] }))).toEqual(['inputs.executors']);
  expect(bindingChanges(template, compat({ app: 'dev.exact.template', icons: ['icon.png'], minimumOS: '18.0', hatches: ['x'] }))).toEqual(['inputs.minimumOS', 'inputs.hatches']);
  expect(bindingChanges(template, compat({ app: 'dev.exact.template', icons: ['icon.png'] }, { embedded: { trust: 'production', plan: 'p1', assets: 'a1' } }))).toEqual(['embedded']);
});

test("a widget's plan edit moves embedded.plans and repacks; the packaged kinds are the full build's", () => {
  const withPlans = (counter) => compat({ app: 'dev.exact.template', icons: ['icon.png'] }, { embedded: { trust: 'development', plan: 'p2', assets: 'a2', plans: { counter } } });
  expect(bindingChanges(withPlans('fbc65f8d'), withPlans('367c3460'))).toEqual([]);
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-repack-kinds-'));
  try {
    expect([...packagedKinds([], { id: 'dev.exact.widgety', dir, manifest: {} })].sort()).toEqual(EXTENSION_KINDS.filter(k => k !== 'notifications').sort());
    expect([...packagedKinds(['--extensions', 'widgets'], { id: 'dev.exact.widgety', dir, manifest: {} })]).toEqual(['widgets']);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('a development archive is a repack base, and repack runs, only with EXACT_EXPERIMENTAL=1', async () => {
  const was = process.env.EXACT_EXPERIMENTAL;
  const archive = { ios: true, tv: false, device: true, ipa: 'a.ipa', developmentArchive: true };
  try {
    delete process.env.EXACT_EXPERIMENTAL;
    expect(repackBase(archive)).toBe(false);
    expect(reservedPlan('/plan', repackBase(archive))).toBe('/plan');
    await expect(repack(['--base', 'b.ipa', '--archive', 'o.ipa', 'app'])).rejects.toThrow('repack refused: repack is experimental: set EXACT_EXPERIMENTAL=1');
    process.env.EXACT_EXPERIMENTAL = '1';
    expect(repackBase(archive)).toBe(true);
    for (const other of [{ tv: true }, { device: false }, { ipa: null }, { developmentArchive: false }, { ios: false }]) expect(repackBase({ ...archive, ...other })).toBe(false);
    // A distributed build, or one that bakes production trust, is never a base.
    expect(repackBase({ ...archive, distribution: true })).toBe(false);
    expect(repackBase(archive, { EXACT_EXPERIMENTAL: '1', EXACT_UPDATE_TRUST: 'production' })).toBe(false);
    expect(repackBase(archive, { EXACT_EXPERIMENTAL: '1', EXACT_UPDATE_TRUST: 'development' })).toBe(true);
  } finally {
    if (was === undefined) delete process.env.EXACT_EXPERIMENTAL; else process.env.EXACT_EXPERIMENTAL = was;
  }
});

test("a repack's receipt names what its own bake read and selected, not its base's", () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-repack-receipt-'));
  try {
    const app = { dir: resolve(dir, 'app') }, baked = resolve(dir, 'baked');
    mkdirSync(app.dir); mkdirSync(baked);
    const sources = digest => ({ reads: [{ kind: 'file', path: resolve(app.dir, 'app.ts'), digest }] });
    const settings = value => ({ dir: app.dir, env: { API_URL: { value, source: 'default' } } });
    const native = { version: 1, trust: 'development', binary: { sha256: 'n' }, products: [{ path: 'libapp.a', sha256: 'p' }] };
    const base = { ...native, backendOrigin: 'https://a.example', settings: settings('a'), typescriptSources: sources('x'), compat: { id: 'base' } };
    writeFileSync(resolve(baked, 'app.module.json'), JSON.stringify({ backendOrigin: 'https://b.example', env: settings('b').env }));
    writeFileSync(resolve(baked, 'app.sources.json'), JSON.stringify(sources('y')));
    expect(repackedBuild(base, baked, app, { compat: { id: 'new' } })).toEqual({ ...native, backendOrigin: 'https://b.example', settings: settings('b'), typescriptSources: sources('y'), compat: { id: 'new' } });
    // A whole receipt's linked plan and what its bake read are this bake's.
    const inputs = paths => ({ paths, missing: [], watchedDirectories: [] });
    writeFileSync(resolve(baked, 'inputs.json'), JSON.stringify(inputs(['app.ts', 'lib.ts'])));
    expect(repackedBuild({ ...base, linkedPlan: '/base-bake', linkedInputs: inputs(['app.ts']) }, baked, app, { compat: { id: 'new' } }))
      .toMatchObject({ linkedPlan: baked, linkedInputs: inputs(['app.ts', 'lib.ts']) });
    // A bake with no TypeScript module keeps none of the base's.
    rmSync(resolve(baked, 'app.module.json')); rmSync(resolve(baked, 'app.sources.json'));
    expect(repackedBuild(base, baked, app, { compat: { id: 'new' } })).toEqual({ ...native, backendOrigin: null, compat: { id: 'new' } });
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// A full development archive, then a repack of it with another version and build number: the repacked IPA carries the
// new ones in the app's Info.plist, each framework's and the fitted compat.json, and it repacked rather than refused.
test.skipIf(process.platform !== 'darwin')('a repack stamps a new version and build number into the bundle', async () => {
  const { spawnSync } = await import('node:child_process');
  const { readdirSync, readFileSync, existsSync } = await import('node:fs');
  const { createApp } = await import('../../game/new.mjs');
  const { readLinkedSections } = await import('./sections.mjs');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(tmpdir(), 'exact-repack-versions-'));
  try {
    const app = resolve(dir, 'stamped');
    // An app that names iOS, with one widget (so its plan is in its own ExactCore, beside the widget frameworks), and
    // the native workspace its first iOS build generates.
    createApp(app, { with: ['ios'] });
    const manifest = JSON.parse(readFileSync(resolve(app, 'app.json'), 'utf8'));
    writeFileSync(resolve(app, 'app.json'), JSON.stringify({ ...manifest, widgets: [{ kind: 'note', title: 'Note', families: ['systemSmall'], component: 'NoteWidget', props: 'NoteWidgetProps', refresh: 'noteWidget', preview: { title: 'Note' } }] }, null, 2));
    writeFileSync(resolve(app, 'app.contract'), `${readFileSync(resolve(app, 'app.contract'), 'utf8')}\nshape NoteWidgetProps\n  title: string\n\ncomponent NoteWidget\n  props\n    title: string\n  view\n    text title\n`);
    writeFileSync(resolve(app, 'app.ts'), readFileSync(resolve(app, 'app.ts'), 'utf8').replace('const sources: Sources = {', 'const sources: Sources = {\n  noteWidget: ([date]) => [{ date, props: { title: "Note" } }],'));
    (await import('../../scripts/app-shells.mjs')).ensureShells(app);
    const env = (build, version) => ({ ...process.env, EXACT_EXPERIMENTAL: '1', EXACT_APP_DIR: app, EXACT_FEEDBACK: '0', EXACT_BUILD_NUMBER: build, EXACT_VERSION: version });
    const run = (script, args, build, version) => spawnSync(process.execPath, [resolve(root, script), ...args], { cwd: root, env: env(build, version), encoding: 'utf8', timeout: 1_800_000 });
    const base = resolve(dir, 'base.ipa'), out = resolve(dir, 'repacked.ipa');
    const built = run('host/apple/build.mjs', ['--device', '--archive', base, '--unsigned', '--development', '--no-install', app], '1', '0.1.0');
    expect(built.status, built.stderr.slice(-4000)).toBe(0);
    const repacked = run('host/apple/repack.mjs', ['--base', base, '--archive', out, app], '57', '2.3.4');
    expect(repacked.status, repacked.stderr.slice(-4000)).toBe(0);
    expect(repacked.stdout).toContain('repacked from the base');
    const unzipped = resolve(dir, 'out');
    expect(spawnSync('unzip', ['-q', out, '-d', unzipped]).status).toBe(0);
    const bundle = resolve(unzipped, 'Payload', readdirSync(resolve(unzipped, 'Payload'))[0]);
    const json = (plist) => JSON.parse(spawnSync('plutil', ['-convert', 'json', '-o', '-', plist], { encoding: 'utf8' }).stdout);
    const stamp = { CFBundleVersion: '57', CFBundleShortVersionString: '2.3.4' };
    expect(json(resolve(bundle, 'Info.plist'))).toMatchObject(stamp);
    const frameworks = readdirSync(resolve(bundle, 'Frameworks')).filter(entry => entry.endsWith('.framework'));
    expect(frameworks).toContain('ExactCore.framework');
    for (const framework of frameworks) expect([framework, json(resolve(bundle, 'Frameworks', framework, 'Info.plist'))]).toMatchObject([framework, stamp]);
    expect(json(resolve(bundle, 'PlugIns/ExactWidgets.appex/Info.plist'))).toMatchObject(stamp);
    const compat = JSON.parse(readLinkedSections(resolve(bundle, 'Frameworks/ExactCore.framework/ExactCore')).compat.toString('utf8'));
    expect(compat.embedded).toMatchObject({ version: '2.3.4', buildNumber: '57' });
    // Every signature still verifies over what was rewritten.
    for (const signed of [bundle, resolve(bundle, 'PlugIns/ExactWidgets.appex'), ...frameworks.map(framework => resolve(bundle, 'Frameworks', framework))]) expect(spawnSync('codesign', ['--verify', '--strict', signed]).status).toBe(0);
    expect(existsSync(resolve(bundle, 'receipt.json'))).toBe(true);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 3_600_000);
