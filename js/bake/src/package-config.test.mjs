// A package's configuration (LLP 1133.021 §9 item 7): what each target's build reads and refuses, what Android's
// manifest and Apple's composed module carry, and what a binary's receipt records.
import { afterAll, expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { nativePackageConfig, packageConfig, packageConfigs, sourceString } from './package-config.mjs';
import { configMetaData } from '../../../scripts/android-manifest.mjs';
import { composedModule } from '../../../host/apple/compose.mjs';

const KEY = 'exact-sentinel-config-test-9f3k';
const app = mkdtempSync(resolve(tmpdir(), 'exact-package-config-'));
afterAll(() => rmSync(app, { recursive: true, force: true }));
mkdirSync(resolve(app, 'node_modules/@fixture'), { recursive: true });
symlinkSync(resolve(import.meta.dir, '../../../packages/fixtures/swatch'), resolve(app, 'node_modules/@fixture/swatch'), 'dir');
const extensions = { swatch: '@fixture/swatch' };
/** `packages` as the app's `app.json` entry for @fixture/swatch, and what `packageConfigs` throws or warns with it. */
const given = (entry, env = {}) => {
  writeFileSync(resolve(app, 'app.json'), JSON.stringify({ packages: { '@fixture/swatch': entry } }));
  for (const [k, v] of Object.entries(env)) if (v === undefined) delete process.env[k]; else process.env[k] = v;
};
const thrown = (fn) => { try { fn(); return ''; } catch (e) { return e.message; } };

test('an env key is read from its variable on every target, a plain key as given', () => {
  given(Object.fromEntries(['web', 'android', 'ios', 'macos'].map((t) => [t, { apiKey: { env: 'SWATCH_KEY' }, label: t }])), { SWATCH_KEY: KEY });
  for (const target of ['web', 'android', 'ios', 'macos']) expect(packageConfig(app, target, { extensions, release: true }).get('swatch')).toEqual({ apiKey: KEY, label: target });
  // Declared for no tvOS: nothing, and nothing required.
  expect(packageConfig(app, 'tvos', { extensions, release: true }).get('swatch')).toEqual({});
});

test("the value is read from the build's environment", () => {
  given({ ios: { apiKey: { env: 'SWATCH_KEY' } } }, { SWATCH_KEY: undefined });
  expect(packageConfig(app, 'ios', { extensions, release: true, env: { SWATCH_KEY: KEY } }).get('swatch')).toEqual({ apiKey: KEY });
});

test('a release-required key absent refuses a release and warns a development build, naming the variable, not a value', () => {
  given({ ios: { apiKey: { env: 'SWATCH_KEY' } } }, { SWATCH_KEY: undefined });
  expect(thrown(() => packageConfig(app, 'ios', { extensions, release: true }))).toMatch(/^swatch-key-missing: .*SWATCH_KEY is not$/);
  const warned = [];
  expect(packageConfig(app, 'ios', { extensions, warn: (m) => warned.push(m) }).get('swatch')).toEqual({});
  expect(warned).toEqual(["SWATCH_KEY is unset, so @fixture/swatch's apiKey is absent from this development build (a release build refuses it)"]);
  given({}, {});
  expect(thrown(() => packageConfig(app, 'macos', { extensions, release: true }))).toMatch(/^swatch-key-missing: /);
});

test('development overlays declared keys before resolving variables or enforcing release requirements', () => {
  const base = { '@fixture/swatch': { android: { label: 'base' } } };
  const overlay = { android: { apiKey: { env: 'TEST_KEY' } } };
  const manifest = { packages: base, development: { packages: { '@fixture/swatch': overlay } } };
  const save = () => writeFileSync(resolve(app, 'app.json'), JSON.stringify(manifest));
  const env = { TEST_KEY: KEY };
  const read = (development, release) => packageConfig(app, 'android', { extensions, development, release, env }).get('swatch');
  save();
  expect(read(true, true)).toEqual({ label: 'base', apiKey: KEY });
  const release = trust => packageConfig(app, 'android', { extensions, release: true, env: { ...env, EXACT_UPDATE_TRUST: trust } }).get('swatch');
  expect(release('development')).toEqual({ label: 'base', apiKey: KEY });
  expect(() => release('production')).toThrow('swatch-key-missing');
  expect(() => read(false, true)).toThrow('swatch-key-missing');
  base['@fixture/swatch'].android.apiKey = { env: 'PUBLIC_KEY' };
  overlay.android.label = 'test';
  env.PUBLIC_KEY = 'public';
  save();
  expect(read(true, false)).toEqual({ label: 'test', apiKey: KEY });
  Object.defineProperty(env, 'TEST_KEY', { get() { throw Error('production read the development variable'); } });
  expect(read(false, true)).toEqual({ label: 'base', apiKey: 'public' });
  expect(nativePackageConfig(manifest, 'android', env, false)).toEqual(nativePackageConfig({ packages: base }, 'android', env));
  const evidence = nativePackageConfig(manifest, 'android', { TEST_KEY: KEY }, true);
  expect(evidence['packages.@fixture/swatch'].apiKey.env).toBe('TEST_KEY');
  expect(JSON.stringify(evidence)).not.toContain(KEY);
  overlay.android.unknown = KEY;
  save();
  for (const development of [false, true]) expect(() => read(development, true)).toThrow('bake-config-unknown: app.json "development": {"packages":');
});

test('a value of another type than declared is refused, and the value is never said', () => {
  given({ android: { apiKey: KEY } }, { SWATCH_KEY: KEY });
  const said = thrown(() => packageConfig(app, 'android', { extensions }));
  expect(said).toMatch(/^bake-config-type: .*"apiKey".* needs \{"env": "<VARIABLE>"\}/);
  expect(said).not.toContain(KEY);
  given({ android: { apiKey: { env: 'SWATCH_KEY' }, label: 3 } });
  expect(thrown(() => packageConfig(app, 'android', { extensions }))).toMatch(/^bake-config-type: .*"label".* needs a string$/);
  given({ android: { apiKey: { env: 'SWATCH_KEY', default: 'x' } } });
  expect(thrown(() => packageConfig(app, 'android', { extensions }))).toMatch(/^bake-config-type: /);
  given({ android: { apiKey: { env: '' } } });
  expect(thrown(() => packageConfig(app, 'android', { extensions, release: true }))).toMatch(/^bake-config-type: /);
});

test('a key the interface does not declare for the target is refused, with the nearest name', () => {
  given({ web: { apiKy: { env: 'SWATCH_KEY' } } });
  expect(thrown(() => packageConfig(app, 'web', { extensions }))).toMatch(/^bake-config-unknown: .*declares no "apiKy" for web; did you mean "apiKey"\?$/);
  given({ web: { colour: 'red' } });
  expect(thrown(() => packageConfig(app, 'web', { extensions }))).toMatch(/\(it declares "apiKey", "label"\)$/);
  given({ web: { lable: 'x' } });
  expect(thrown(() => packageConfig(app, 'web', { extensions }))).toMatch(/did you mean "label"\?$/);
  given({ tvos: { apiKey: { env: 'SWATCH_KEY' } } });
  expect(thrown(() => packageConfig(app, 'tvos', { extensions }))).toMatch(/declares no configuration for tvos$/);
});

test('an entry for a package not installed is a warning; an unused one is not read', () => {
  const packages = { '@fixture/swatch': { web: { nope: 1 } } };
  const development = { packages: { '@fixture/swach': { web: { apiKey: 1 } } } };
  writeFileSync(resolve(app, 'app.json'), JSON.stringify({ packages, development }));
  for (const trust of [true, false]) {
    const warned = [];
    expect([...packageConfig(app, 'web', { extensions: {}, development: trust, warn: (m) => warned.push(m) })]).toEqual([]);
    expect(warned).toEqual(['app.json development.packages names @fixture/swach, which is not installed']);
  }
});

test("an Android key fills a vendor's <meta-data> only by a name on the list, once", () => {
  given({ android: { apiKey: { env: 'SWATCH_KEY' } } }, { SWATCH_KEY: 'k&"1' });
  const configs = packageConfigs(app, 'android', { extensions });
  expect(configMetaData(configs)).toEqual([]);
  const maps = (name, metadata) => ({ name, extension: name.split('/').at(-1), declared: { apiKey: { type: 'env', metadata } }, values: { apiKey: 'k&"1' } });
  expect(configMetaData([maps('@exact/maps', 'com.google.android.geo.API_KEY')])).toEqual(['        <meta-data android:name="com.google.android.geo.API_KEY" android:value="k&amp;&quot;1" />']);
  expect(thrown(() => configMetaData([maps('@x/maps', 'com.example.KEY')]))).toMatch(/fills <meta-data> "com.example.KEY", which is not on the list/);
  expect(thrown(() => configMetaData([maps('@exact/maps', 'com.google.android.geo.API_KEY'), maps('@x/maps', 'com.google.android.geo.API_KEY')]))).toMatch(/@x\/maps and @exact\/maps both fill/);
});

test('generated Kotlin and Swift carry any value as a literal of the same string', () => {
  const text = JSON.stringify({ apiKey: 'a"b\\c$d{e}\u0001 \\(f)' });
  expect(sourceString(text, 'kotlin')).toBe('"{\\"apiKey\\":\\"a\\\\\\"b\\\\\\\\c\\$d{e}\\\\u0001\\u2028\\\\\\\\(f)\\"}"');
  expect(sourceString(text, 'swift')).toBe('"{\\"apiKey\\":\\"a\\\\\\"b\\\\\\\\c$d{e}\\\\u0001\\u{2028}\\\\\\\\(f)\\"}"');
  const p = { extension: 'swatch', module: 'ExactPkg_swatch', className: 'SwatchModule', views: [] }, q = { ...p, extension: 'bell', module: 'ExactPkg_bell' };
  const swift = composedModule([p, q], false, new Map([['swatch', { apiKey: KEY, n: 2 }], ['bell', {}]]));
  expect(swift).toContain(`let exactPackageConfig: [String: String] = ["swatch": "{\\"apiKey\\":\\"${KEY}\\",\\"n\\":2}"]`);
  expect(composedModule([p], false)).toContain('let exactPackageConfig: [String: String] = [:]');
});

test("a binary's receipt records an env value by its digest, so a rotated key is a new binary", () => {
  const manifest = { packages: { '@exact/maps': { android: { apiKey: { env: 'MAPS_KEY' } }, web: { tiles: 't' } } } };
  const recorded = nativePackageConfig(manifest, 'android', { MAPS_KEY: KEY });
  expect(JSON.stringify(recorded)).not.toContain(KEY);
  expect(recorded['packages.@exact/maps'].apiKey.env).toBe('MAPS_KEY');
  expect(nativePackageConfig(manifest, 'android', { MAPS_KEY: `${KEY}2` })).not.toEqual(recorded);
  expect(nativePackageConfig(manifest, 'android', {})).not.toEqual(recorded);
  expect(nativePackageConfig(manifest, 'ios', { MAPS_KEY: KEY })).toEqual({});
  // A key pasted where a variable belongs is refused by the build, and recorded by its digest only.
  const pasted = nativePackageConfig({ packages: { '@exact/maps': { android: { apiKey: KEY } } } }, 'android');
  expect(JSON.stringify(pasted)).not.toContain(KEY);
  expect(nativePackageConfig({ packages: { '@exact/maps': { linux: { apiKey: KEY } } } }, 'linux')).toEqual({});
});
