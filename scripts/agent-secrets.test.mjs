import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { parsePlainSecretGrants, validateSecretSteps, withSeedTempFile } from './agent-secrets.mjs';
import { storedScript } from './agent-web.mjs';
import { storeBase } from './agent-test.mjs';
import { judgeRelaunch, relaunchStore } from './verify.mjs';
import { contractTool } from './tools.mjs';

test('parsePlainSecretGrants separates plain secret.keep from presence-locked and secret.hold', () => {
  const { plain, presence } = parsePlainSecretGrants([
    'secret.keep castle.session',
    'development secret.keep dev.token',
    'secret.keep vault.key user-presence',
    'secret.keep opt.key biometry-any optional',
    'secret.hold process.secret',
    'net.fetch https://api.example.com',
  ].join('\n'));
  expect([...plain].sort()).toEqual(['castle.session', 'dev.token']);
  expect([...presence].sort()).toEqual(['opt.key', 'vault.key']);
});

test('validateSecretSteps enforces plain grants, rejects presence-locked and non-dev web, and never leaks values', () => {
  const grants = { known: true, plain: new Set(['castle.session']), presence: new Set(['vault.key']), nonDevWeb: false };
  const secretValue = 'SUPER-SENSITIVE-VALUE-999';
  expect(() => validateSecretSteps({
    secrets: [{ op: 'secret', name: 'castle.session', value: secretValue, line: 3 }],
    sessions: [],
    grants,
    host: 'web',
  })).not.toThrow();

  let ungranted = '';
  try {
    validateSecretSteps({
      secrets: [{ op: 'secret', name: 'other.token', value: secretValue, line: 4 }],
      sessions: [],
      grants,
      host: 'macos',
    });
  } catch (e) { ungranted = e.message; }
  expect(ungranted).toContain('line 4: secret requires a plain secret.keep grant for other.token');
  expect(ungranted).not.toContain(secretValue);

  let locked = '';
  try {
    validateSecretSteps({
      secrets: [{ op: 'secret', name: 'vault.key', value: secretValue, line: 5 }],
      sessions: [],
      grants,
      host: 'ios',
    });
  } catch (e) { locked = e.message; }
  expect(locked).toContain('presence-locked');
  expect(locked).not.toContain(secretValue);

  expect(() => validateSecretSteps({
    secrets: [{ op: 'secret', name: 'castle.session', value: secretValue, line: 6 }],
    sessions: [],
    grants: { ...grants, nonDevWeb: true },
    host: 'web',
  })).toThrow(/development web build/);
});

test('storedScript seeds #secret.<name> and enforces plainSecrets allowlist without echoing values', () => {
  const script = storedScript({
    EXACT_AGENT_STORAGE_FRESH: '1',
    EXACT_AGENT_STORED: JSON.stringify([['#secret.api.token', 'tok-42'], ['theme', '"dark"']]),
  }, 'http://127.0.0.1:4000');
  const local = new Map(), session = new Map();
  const fn = new Function('window', 'location', 'localStorage', 'sessionStorage', 'globalThis', script);
  const g = {};
  const win = {};
  win.top = win;
  fn(win, { origin: 'http://127.0.0.1:4000' }, { setItem: (k, v) => local.set(k, v) }, { setItem: (k, v) => session.set(k, v) }, g);
  expect(typeof g.__exactSeedStored).toBe('function');
  g.__exactSeedStored('exact.state.', 'exact.secret.', ['api.token']);
  expect(local.get('exact.secret.api.token')).toBe('tok-42');
  expect(local.get('exact.state.theme')).toBe('"dark"');
  expect(g.__exactSeedStored).toBeUndefined();

  // Ungranted secret refuses without echoing the secret value.
  const g2 = {};
  fn(win, { origin: 'http://127.0.0.1:4000' }, { setItem: () => {} }, { setItem: () => {} }, g2);
  let err = '';
  try { g2.__exactSeedStored('exact.state.', 'exact.secret.', []); } catch (e) { err = e.message; }
  expect(err).toContain('api.token');
  expect(err).not.toContain('tok-42');
});

test('withSeedTempFile removes temporary seed file even when staging throws', () => {
  let seen = '';
  expect(() => withSeedTempFile('[["#secret.a","v"]]', (file) => { seen = file; throw new Error('boom'); })).toThrow('boom');
  expect(seen.length).toBeGreaterThan(0);
  expect(Bun.file(seen).size).toBe(0);
});

test('storeBase resolves Windows LOCALAPPDATA scratch directory', () => {
  expect(storeBase('dev.exact.demo', 'windows', { LOCALAPPDATA: '/win/AppData/Local' })).toBe('/win/AppData/Local/exact/dev.exact.demo/agent');
});

test('verify treats seeded tests as writing storage for relaunch selection and judging', () => {
  const fresh = { ops: 0, texts: new Set(['Sign in']), resources: {}, slots: {} };
  const seeded = { name: 'signed in', store: 'verify.t0', failures: [], end: { ops: 0, reload: false, seeded: true, resources: { user: 'Ada' }, slots: {} } };
  expect(relaunchStore([seeded], fresh)).toBe(seeded);
  const verdict = judgeRelaunch(seeded, fresh, { texts: new Set(['Sign in']), resources: {}, slots: {} });
  expect(verdict.problems.some((p) => p.includes('lost'))).toBe(true);
});

test('contract test parses secret launch step, inherits top-level secret, and rejects duplicate/signed-in collision', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-secret-test-'));
  try {
    writeFileSync(join(dir, 'app.contract'), 'component App\n  view\n    text "ok"\n');
    const okFile = join(dir, 'ok.test.contract');
    writeFileSync(okFile, [
      'secret "castle.session" "top-token"',
      'test "inherits top"',
      '  expect text "a" == "ok"',
      'test "overrides with signed-in"',
      '  signed-in "castle.session" {"sub":"1"}',
      '  expect text "a" == "ok"',
    ].join('\n') + '\n');
    const compiler = contractTool({ targetDir: process.env.CARGO_TARGET_DIR });
    const r1 = spawnSync(compiler.cmd, [...compiler.pre, 'test', okFile, '--app', dir], { env: compiler.env, encoding: 'utf8' });
    expect(r1.status).toBe(0);
    const parsed = JSON.parse(r1.stdout);
    expect(parsed.tests[0].steps[0]).toMatchObject({ op: 'secret', name: 'castle.session', value: 'top-token' });
    expect(parsed.tests[1].steps.filter((s) => s.op === 'secret' || s.op === 'signed-in')).toHaveLength(1);
    expect(parsed.tests[1].steps[0].op).toBe('signed-in');

    const dupFile = join(dir, 'dup.test.contract');
    writeFileSync(dupFile, [
      'test "conflict"',
      '  secret "castle.session" "a"',
      '  signed-in "castle.session" {"sub":"1"}',
    ].join('\n') + '\n');
    const r2 = spawnSync(compiler.cmd, [...compiler.pre, 'test', dupFile, '--app', dir], { env: compiler.env, encoding: 'utf8' });
    expect(r2.status).not.toBe(0);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
