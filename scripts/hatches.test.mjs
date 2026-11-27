// Access hatches in the build scripts (LLP 1075.003.000.001 §4.3, §5): which
// words a platform's module handles, and the delivery check over them.
// `bun test ./scripts/hatches.test.mjs`.
import { test } from 'bun:test';
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { writeDataKeys } from '../host/apple/data-keys.mjs';
import assert from 'node:assert/strict';
import { classifyArtifacts, hatchWords } from './app.mjs';

test('a platform handles every word of a list, and of an object the words that name it', () => {
  assert.deepEqual(hatchWords({ hatches: ['dot', 'avatar'] }, 'linux'), ['avatar', 'dot']);
  const manifest = { hatches: { dot: ['web', 'ios'], avatar: ['ios', 'macos'], nowhere: [] } };
  assert.deepEqual(hatchWords(manifest, 'ios'), ['avatar', 'dot']);
  assert.deepEqual(hatchWords(manifest, 'web'), ['dot']);
  assert.deepEqual(hatchWords(manifest, 'linux'), []);
  assert.deepEqual(hatchWords({}, 'ios'), []);
});

// LLP 1075.003.000.001 §4.3: a plan's hatch words are a requirement per
// platform, against what the installed cohort's module was built to handle.
test('a bundle whose plan needs a hatch word the installed module lacks is not published to that stream', () => {
  const plan = (hatches) => ({ name: 'app.plan', requires: hatches ? { hatches } : {} });
  const candidate = (hatches) => ({ binary: { sha256: 'same' }, graph: { artifacts: [plan(hatches)], sources: {} }, compat: { inputs: {} } });
  const cohort = (hatches) => ({ binary: 'same', sources: {}, compat: { id: 'c', inputs: { store: { L: 'A' }, ...(hatches !== undefined ? { hatches } : {}) } } });
  // An Apple cohort built without `avatar`: no bundle, and the word is named.
  const refused = classifyArtifacts(candidate(['avatar', 'dot']), cohort(['dot']));
  assert.equal(refused.bundle, false);
  assert.deepEqual(refused.missing, ["app.plan: hatches.avatar (handled by this platform's module, not by the installed one)"]);
  // A cohort from before the capability existed handles nothing.
  assert.equal(classifyArtifacts(candidate(['dot']), cohort(undefined)).bundle, false);
  assert.equal(classifyArtifacts(candidate(['dot']), cohort(null)).bundle, false);
  // The same plan for a platform that does not handle the word asks nothing of it: its Linux cohort takes the bundle.
  assert.equal(classifyArtifacts(candidate(null), cohort(null)).bundle, true);
  // A cohort that handles more than the plan marks is fine.
  assert.equal(classifyArtifacts(candidate(['dot']), cohort(['avatar', 'dot'])).bundle, true);
});

// LLP 1156.006.000 D25: what the classifier calls a proof that a binary cannot run an update (`needsBinary`), which
// `push` may leave a stream on, apart from what it could not check, which refuses the push.
test('a requirement the cohort\'s receipt does not meet is a proof; evidence the classifier lacks is not', () => {
  const candidate = (requires, extra = {}) => ({ binary: { sha256: 'same' }, graph: { artifacts: [{ name: 'app.plan', requires }], sources: {} }, compat: { inputs: {} }, ...extra });
  const cohort = (inputs = {}, L = 'A') => ({ binary: 'same', sources: {}, compat: { id: 'c', inputs: { store: { L }, ...inputs } } });
  const proof = classifyArtifacts(candidate({ hatches: ['avatar'], appleUses: ['picker'], typescriptRuntime: 5 }), cohort({ hatches: [], linked: ['io'], typescriptRuntime: 4 }));
  assert.equal(proof.missing.length, 3);
  assert.deepEqual(proof.needsBinary, proof.missing);
  // Grants the cohort never recorded, a key it does not carry, a receipt that is absent, a binary with no carrier.
  const unknown = classifyArtifacts(candidate({ grantCeiling: 'net.fetch https://a.example' }), cohort({ grantCeiling: null }));
  assert.deepEqual([unknown.missing, unknown.needsBinary], [['app.plan: grantCeiling (unknown baked grants)'], []]);
  const outside = classifyArtifacts(candidate({ grantCeiling: 'net.fetch https://a.example' }), cohort({ grantCeiling: 'net.fetch https://b.example' }));
  assert.deepEqual(outside.needsBinary, ['app.plan: grantCeiling (net.fetch https://a.example)']);
  const unsigned = classifyArtifacts(candidate({ hatches: ['avatar'] }, { trust: 'production' }), cohort({ hatches: [], keys: {} }), { id: 'k', public: 'p' });
  assert.deepEqual(unsigned.needsBinary, ["app.plan: hatches.avatar (handled by this platform's module, not by the installed one)"]);
  assert.equal(unsigned.missing.length, 2);
  assert.deepEqual(classifyArtifacts(candidate({ hatches: ['avatar'] }), { binary: 'same' }).needsBinary ?? [], []);
  assert.deepEqual(classifyArtifacts(candidate({ hatches: ['avatar'] }), cohort({ hatches: [] }, '0')).needsBinary, []);
});

test('data and hatch declarations accept digits and hyphens after their first letter', async () => {
  const { readManifest } = await import('./app.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'manifest-data-names-'));
  const words = ['v-2', 'v--2', 'v-', 'v--'];
  try {
    for (const hatches of [words, Object.fromEntries(words.map(w => [w, ['ios', 'android']]))]) {
      writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Data', app: { id: 'dev.exact.data', name: 'Data' }, data: words, hatches }));
      assert.deepEqual(readManifest(dir, 'data').data, words);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.skipIf(process.platform !== 'darwin')('typed Swift data and hatch keys keep every admitted word distinct', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'data-keys-'));
  const words = ['v2', 'v-2', 'v--2', 'v-', 'v--'];
  try {
    const file = resolve(dir, 'Keys.swift');
    writeDataKeys({ manifest: { data: words, hatches: words } }, file, 'ios');
    const text = readFileSync(file, 'utf8');
    for (const [word, key] of [['v2', 'v2'], ['v-2', 'v_2'], ['v--2', 'v__2'], ['v-', 'v_'], ['v--', 'v__']]) {
      assert.ok(text.includes('`' + key + '` = ExactDataKey("' + word + '")'), text);
      assert.ok(text.includes('`' + key + '` = ExactHatchKey("' + word + '")'), text);
    }
    const result = spawnSync('swiftc', ['-typecheck', file], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stdout + result.stderr);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 30000);

test('hatch stubs preserve digits and trailing or repeated hyphens on every host', async () => {
  const { hatch } = await import('./exact.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'hatch-word-stubs-')), was = process.env.EXACT_APP_DIR;
  try {
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    text "word"\n');
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Words', app: { id: 'dev.exact.words', name: 'Words' }, host: { web: {}, ios: {}, macos: {}, linux: {}, android: {} } }));
    process.env.EXACT_APP_DIR = dir;
    for (const [word, key] of [['v-2', 'v_2'], ['v--2', 'v__2'], ['v-', 'v_'], ['v--', 'v__']]) {
      assert.deepEqual(hatch([word], () => {}).todo, []);
      const text = path => readFileSync(resolve(dir, path), 'utf8');
      assert.ok(text('modules/apple/Hatches.swift').includes('element.hatch == .' + key));
      assert.ok(text('modules/android/Hatches.kt').includes('private fun ' + key + '('));
      assert.ok(text('modules/web/index.js').includes('import * as ' + key + 'Hatch'));
      assert.ok(text('modules/linux/hatches.rs').includes('#[allow(non_snake_case)]\nfn ' + word.replaceAll('-', '_') + '_hatch<'));
    }
  } finally {
    if (was === undefined) delete process.env.EXACT_APP_DIR; else process.env.EXACT_APP_DIR = was;
    rmSync(dir, { recursive: true, force: true });
  }
});
