import { expect, test } from 'bun:test';
import { bindingChanges, repack } from './repack.mjs';
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

test('what the binary was built to provide still refuses a repack, by name', () => {
  expect(bindingChanges(template, compat({ app: 'com.example.photos', icons: ['other.png'], executors: ['hermes', 'wasmi'] }))).toEqual(['inputs.executors']);
  expect(bindingChanges(template, compat({ app: 'dev.exact.template', icons: ['icon.png'], minimumOS: '18.0', hatches: ['x'] }))).toEqual(['inputs.minimumOS', 'inputs.hatches']);
  expect(bindingChanges(template, compat({ app: 'dev.exact.template', icons: ['icon.png'] }, { embedded: { trust: 'production', plan: 'p1', assets: 'a1' } }))).toEqual(['embedded']);
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
  } finally {
    if (was === undefined) delete process.env.EXACT_EXPERIMENTAL; else process.env.EXACT_EXPERIMENTAL = was;
  }
});
