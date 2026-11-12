// The store category, Game Mode and the increased memory limit: app.json's `categories`, `host.<platform>.gameMode`
// and `host.ios.increasedMemoryLimit`, as the Apple bundles' Info.plist and entitlements carry them.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { CATEGORIES, MEMORY_ENTITLEMENT, appleCategory, memoryEntitlements } from './plist.mjs';
import { entitlements, infoPlist, macInfoPlist } from './build.mjs';
import { profile } from './devices.mjs';
import { nativeManifestMetadata, readManifest, webAppManifest } from '../../scripts/app.mjs';

const schema = JSON.parse(readFileSync(resolve(import.meta.dir, '../../scripts/app.schema.json'), 'utf8'));
const app = (manifest) => ({ id: 'dev.exact.dice', displayName: 'Dice', manifest: { app: { id: 'dev.exact.dice' }, ...manifest } });

/** The plist `text` as an object, read by plutil as the bundle's reader would. */
function parse(text) {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-store-keys-'));
  try {
    writeFileSync(resolve(dir, 'Info.plist'), text);
    const out = spawnSync('plutil', ['-convert', 'json', '-o', '-', resolve(dir, 'Info.plist')], { encoding: 'utf8' });
    assert.equal(out.status, 0, out.stderr);
    return JSON.parse(out.stdout);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}
const pick = (dict) => Object.fromEntries(['LSApplicationCategoryType', 'LSSupportsGameMode', 'GCSupportsGameMode'].filter((k) => k in dict).map((k) => [k, dict[k]]));
const ios = (manifest, options) => pick(parse(infoPlist(app(manifest), false, options)));
const mac = (manifest) => pick(parse(macInfoPlist(app(manifest))));

test('the schema admits exactly the categories the build maps', () => {
  assert.deepEqual(schema.properties.categories.items.enum.toSorted(), [...CATEGORIES.keys()].toSorted());
  // Every W3C standardized category (the manifest wiki's "Categories") is admitted.
  for (const w3c of ['books', 'business', 'education', 'entertainment', 'finance', 'fitness', 'food', 'games', 'government', 'health', 'kids', 'lifestyle', 'magazines', 'medical', 'music', 'navigation', 'news', 'personalization', 'photo', 'politics', 'productivity', 'security', 'shopping', 'social', 'sports', 'travel', 'utilities', 'weather']) assert.ok(CATEGORIES.has(w3c), w3c);
});

test('Apple files the app under the first category it has, and a game the shell builds under games', () => {
  assert.equal(appleCategory({ categories: ['dice-games', 'games'] }), 'dice-games');
  assert.equal(appleCategory({ categories: ['books', 'photo'] }), 'photography');
  assert.equal(appleCategory({ categories: ['health'] }), 'healthcare-fitness');
  assert.equal(appleCategory({ categories: ['books'] }), null);
  assert.equal(appleCategory({}), null);
  assert.equal(appleCategory({ game: { crate: 'dice-logic', type: 'Dice' } }), 'games');
  assert.equal(appleCategory({ categories: ['puzzle-games'], game: { crate: 'dice-logic', type: 'Dice' } }), 'puzzle-games');
});

test('a game is filed under its genre and offered Game Mode on iOS and macOS', () => {
  const dice = { categories: ['dice-games'] };
  assert.deepEqual(ios(dice), { LSApplicationCategoryType: 'public.app-category.dice-games', LSSupportsGameMode: true, GCSupportsGameMode: true });
  assert.deepEqual(mac(dice), { LSApplicationCategoryType: 'public.app-category.dice-games', LSSupportsGameMode: true });
  // iOS 18.6 reads LSSupportsGameMode alone, so an app that needs it carries no GCSupportsGameMode.
  assert.deepEqual(ios({ ...dice, host: { ios: { minimumOS: '18.6' } } }), { LSApplicationCategoryType: 'public.app-category.dice-games', LSSupportsGameMode: true });
  assert.deepEqual(ios({ ...dice, host: { ios: { minimumOS: '26.0' } } }), { LSApplicationCategoryType: 'public.app-category.dice-games', LSSupportsGameMode: true });
  assert.deepEqual(ios({ ...dice, host: { ios: { minimumOS: '18.5' } } }).GCSupportsGameMode, true);
  // A game too light for Game Mode turns it off, per platform.
  const off = { ...dice, host: { ios: { gameMode: false }, macos: { gameMode: false } } };
  assert.deepEqual(ios(off), { LSApplicationCategoryType: 'public.app-category.dice-games', LSSupportsGameMode: false, GCSupportsGameMode: false });
  assert.deepEqual(mac(off), { LSApplicationCategoryType: 'public.app-category.dice-games', LSSupportsGameMode: false });
  const shell = { game: { crate: 'dice-logic', type: 'Dice' } };
  assert.deepEqual(mac(shell), { LSApplicationCategoryType: 'public.app-category.games', LSSupportsGameMode: true });
});

test('an app that is not a game has its category and no Game Mode key unless it asks', () => {
  assert.deepEqual(ios({ categories: ['productivity'] }), { LSApplicationCategoryType: 'public.app-category.productivity' });
  assert.deepEqual(mac({ categories: ['social'] }), { LSApplicationCategoryType: 'public.app-category.social-networking' });
  assert.deepEqual(ios({}), {});
  assert.deepEqual(mac({}), {});
  assert.deepEqual(mac({ categories: ['video'], host: { macos: { gameMode: true } } }), { LSApplicationCategoryType: 'public.app-category.video', LSSupportsGameMode: true });
  assert.deepEqual(ios({ host: { ios: { gameMode: true } } }), { LSSupportsGameMode: true, GCSupportsGameMode: true });
});

test('tvOS and an App Clip carry neither the category nor Game Mode', () => {
  const dice = { categories: ['dice-games'] };
  assert.deepEqual(ios(dice, { tv: true }), {});
  assert.deepEqual(ios(dice, { clip: true, executable: 'DiceClip', id: 'dev.exact.dice.Clip' }), {});
});

test('host.ios.increasedMemoryLimit signs the app, and nothing it embeds, with the entitlement', () => {
  const asked = app({ host: { ios: { increasedMemoryLimit: true } } });
  assert.equal(parse(entitlements(asked, null, true, null, { simulator: true }))[MEMORY_ENTITLEMENT], true);
  assert.equal(parse(entitlements(asked, 'TEAM', true, null, { prefix: 'TEAM' }))[MEMORY_ENTITLEMENT], true);
  for (const embedded of [{ widgetHost: true }, { widgetTV: true }, { widgetAppId: asked.id, id: `${asked.id}.widgets` }, { clipParent: asked.id, id: `${asked.id}.Clip` }]) {
    assert.equal(parse(entitlements({ ...asked, ...embedded }, null, true, null, { simulator: true }))[MEMORY_ENTITLEMENT], undefined, JSON.stringify(embedded));
  }
  for (const absent of [{}, { host: { ios: { increasedMemoryLimit: false } } }]) {
    assert.deepEqual(memoryEntitlements(app(absent)), []);
    assert.equal(parse(entitlements(app(absent), null, true, null, { simulator: true }))[MEMORY_ENTITLEMENT], undefined);
  }
});

test('a device build takes only a profile that grants the increased memory limit, and names the capability', () => {
  const asked = app({ host: { ios: { increasedMemoryLimit: true } } });
  const make = (name, granted) => ({ name, entitlements: granted, appId: `TEAM.${asked.id}`, platforms: ['iOS'],
    dev: true, devices: ['PHONE'], expires: new Date(Date.now() + 86400000), team: 'TEAM', prefix: 'TEAM' });
  const plain = make('Plain profile', []), granted = make('Memory profile', [MEMORY_ENTITLEMENT]);
  assert.equal(profile('PHONE', asked.id, memoryEntitlements(asked), { installed: [plain, granted], named: null }).name, granted.name);
  assert.throws(() => profile('PHONE', asked.id, memoryEntitlements(asked), { installed: [plain], named: null }),
    /memory-device-profile: com\.apple\.developer\.kernel\.increased-memory-limit required by dev\.exact\.dice \(app\.json's host\.ios\.increasedMemoryLimit\).*enable Increased Memory Limit for the App ID dev\.exact\.dice/);
  assert.throws(() => profile('PHONE', asked.id, memoryEntitlements(asked), { installed: [], named: plain }), /memory-device-profile:.*EXACT_PROFILE \(Plain profile\) does not allow it/);
});

test('app.json refuses a category not on the list and a non-boolean Game Mode or memory limit', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-categories-'));
  const read = (extra) => {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Dice', app: { id: 'dev.exact.dice', name: 'Dice' }, ...extra }));
    return readManifest(dir, 'dice');
  };
  try {
    const manifest = read({ categories: ['dice-games', 'games'], host: { ios: { gameMode: false, increasedMemoryLimit: true }, macos: { gameMode: true } } });
    assert.deepEqual(manifest.categories, ['dice-games', 'games']);
    assert.throws(() => read({ categories: ['games/dice'] }), /categories\[0\]: "games\/dice" is not one of/);
    assert.throws(() => read({ categories: ['public.app-category.games'] }), /categories\[0\]/);
    assert.throws(() => read({ categories: 'games' }), /categories: expected array/);
    assert.throws(() => read({ categories: [] }), /categories: fewer than 1 items/);
    assert.throws(() => read({ host: { ios: { gameMode: 'yes' } } }), /host\.ios\.gameMode: expected boolean/);
    assert.throws(() => read({ host: { ios: { increasedMemoryLimit: 1 } } }), /host\.ios\.increasedMemoryLimit: expected boolean/);
    assert.throws(() => read({ host: { macos: { increasedMemoryLimit: true } } }), /host\.macos\.increasedMemoryLimit: not a known key/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('the web manifest carries the categories, and they are part of the Apple binaries\' identity', () => {
  const manifest = { name: 'Dice', app: { id: 'dev.exact.dice' }, categories: ['dice-games', 'games'] };
  assert.deepEqual(webAppManifest(manifest, 'Dice').categories, ['dice-games', 'games']);
  assert.equal(webAppManifest({ name: 'Dice' }, 'Dice').categories, undefined);
  const bare = { name: 'Dice', app: { id: 'dev.exact.dice' } };
  for (const platform of ['ios', 'macos']) assert.notDeepEqual(nativeManifestMetadata(manifest, platform), nativeManifestMetadata(bare, platform));
  for (const platform of ['android', 'linux', 'windows']) assert.deepEqual(nativeManifestMetadata(manifest, platform), nativeManifestMetadata(bare, platform));
});
