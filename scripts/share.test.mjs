// `exact share`'s decisions: which link a share publishes to, and when it refuses or rebuilds nothing.
import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { branchOf, mismatch, numbers, parseLink, plan, sourceOf, withBuildNumber } from './share.mjs';

const url = 'https://appdrop.app/d/k3m9x2p7qa';
const record = { url, branch: 'main', source: 'a b', revision: 4 };

test('the first share, and --new-link, create a link', () => {
  expect(plan(null, { branch: 'main', source: 'a b' }).action).toBe('create');
  expect(plan(record, { branch: 'main', source: 'c d', newLink: true }).action).toBe('create');
});

test('a share from the link\'s branch updates it, or rebuilds nothing when the source is unchanged', () => {
  expect(plan(record, { branch: 'main', source: 'c d' })).toMatchObject({ action: 'update', url, id: 'k3m9x2p7qa' });
  expect(plan(record, { branch: 'main', source: 'a b' })).toMatchObject({ action: 'current', revision: 4 });
  // A dirty tree has no source: always rebuilt.
  expect(plan(record, { branch: 'main', source: null }).action).toBe('update');
});

test('a share from another branch refuses, unless --force publishes over it', () => {
  const refused = plan(record, { branch: 'feat/onboarding', source: 'a b' });
  expect(refused).toMatchObject({ action: 'refuse', url, from: 'main' });
  expect(mismatch(refused, 'feat/onboarding')).toBe(`This app's AppDrop link, ${url}, was last shared from main; this checkout is on feat/onboarding. `
    + "Pass --new-link to give feat/onboarding its own link, or --force to publish over main's.");
  // Forced, it publishes even when the source matches: the branch the link records changes.
  expect(plan(record, { branch: 'feat/onboarding', source: 'a b', force: true }).action).toBe('update');
});

test('a build takes the app\'s version and the next build number past the committed one and the link\'s', () => {
  expect(numbers({ version: '0.2.0', buildNumber: 15 }, 15)).toEqual({ version: '0.2.0', build: 16 });
  // A link published past the committed number, from another checkout, still rises
  expect(numbers({ version: '0.2.0', buildNumber: 9 }, 15).build).toBe(16);
  // An app that commits no numbers keeps the build's default version and the link's next revision
  expect(numbers({}, 3)).toEqual({ version: null, build: 4 });
  expect(numbers({}).build).toBe(1);
});

test('the share writes the build number back into app.json as written, and nothing else', () => {
  const manifest = '{\n  "name": "Gallery",\n  "version": "0.2.0",\n  "buildNumber": 15,\n  "icons": []\n}\n';
  expect(withBuildNumber(manifest, 16)).toBe(manifest.replace('15', '16'));
  expect(withBuildNumber('{ "name": "Gallery" }', 16)).toBe('{ "name": "Gallery" }');
});

test('only a share link is a link; its update key never is', () => {
  expect(parseLink(`${url}/`)).toEqual({ url, origin: 'https://appdrop.app', id: 'k3m9x2p7qa' });
  expect(parseLink(`${url}/manage#secret`)).toBeNull();
  expect(parseLink('http://appdrop.app/d/k3m9x2p7qa')).toBeNull();
});

test('the branch and source come from git', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-share-'));
  try {
    const git = (...args) => spawnSync('git', ['-C', dir, '-c', 'user.name=t', '-c', 'user.email=t@t', ...args], { encoding: 'utf8' });
    expect(branchOf(dir)).toBeNull();
    git('init', '-q', '-b', 'feat/x');
    writeFileSync(join(dir, 'app.json'), '{}');
    git('add', '.'); git('commit', '-qm', 'one');
    expect(branchOf(dir)).toBe('feat/x');
    const head = git('rev-parse', 'HEAD').stdout.trim();
    expect(sourceOf(dir, dir)).toBe(`${head} ${head}`);
    writeFileSync(join(dir, 'app.json'), '{"changed":true}');
    expect(sourceOf(dir, dir)).toBeNull();
    git('checkout', '-q', '--detach');
    expect(branchOf(dir)).toBe(head.slice(0, branchOf(dir).length));
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// A tester's build with an update store needs a verification key; without one the bake refused it minutes in.
test('a share of an app with an update store and no keys is refused before it builds, naming what to add', async () => {
  const { unsignedStore } = await import('./share.mjs');
  expect(unsignedStore({})).toContain('"deploy": { "store": { "ios": "0" } }');
  expect(unsignedStore({ deploy: { store: { ios: '0' } } })).toBeNull();
  expect(unsignedStore({ deploy: { signing: { keys: { k1: 'pub' } } } })).toBeNull();
  expect(unsignedStore({ deploy: { signing: { keys: {} } } })).not.toBeNull();
});
