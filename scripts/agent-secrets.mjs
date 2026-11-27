import { existsSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { createRequire } from 'node:module';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { appSettings } from './app-imports.mjs';
import { injectBackendOrigin, staticFacts } from '../js/bake/src/typescript.mjs';
import { appleArtifacts } from '../host/apple/build.mjs';
import { webDist as defaultWebDist } from './app.mjs';

const SECRET_NAME = /^[a-z0-9._-]{1,64}$/;

/** Parse raw grant lines into plain vs presence-locked `secret.keep` names (LLP 1018.000 D2). */
export function parsePlainSecretGrants(text) {
  const plain = new Set(), presence = new Set();
  for (const raw of String(text ?? '').split('\n')) {
    const line = raw.trim();
    if (!line || line.startsWith('#')) continue;
    let words = line.split(/\s+/);
    if (words[0] === 'development') words = words.slice(1);
    if (words[0] !== 'secret.keep' || words.length < 2) continue;
    if (words.length === 2) plain.add(words[1]);
    else presence.add(words[1]);
  }
  return { plain, presence };
}

/** Collect authoritative plain & presence `secret.keep` grants from app.ts and build receipts (no Rust source regex). */
export function collectAppSecretGrants(selected, { launched, chosen, webDist, host } = {}) {
  const plain = new Set(), presence = new Set();
  let known = false;
  const addText = (text) => {
    const parsed = parsePlainSecretGrants(text);
    for (const n of parsed.plain) plain.add(n);
    for (const n of parsed.presence) presence.add(n);
    known = true;
  };
  if (selected?.dir && existsSync(resolve(selected.dir, 'app.ts'))) {
    const parse = createRequire(import.meta.url)('rolldown/utils').parseSync;
    const settings = appSettings(selected.dir, launched);
    const source = settings.modules.get('app.ts');
    const options = { source, load: settings.load };
    const code = injectBackendOrigin(settings.code, chosen, parse, launched, options);
    addText(staticFacts(code, parse, options).grants ?? '');
  }
  const dist = webDist ? resolve(webDist) : selected ? defaultWebDist(selected) : null;
  if (dist) {
    const adm = resolve(dist, '.gen', 'admission-data.js');
    if (existsSync(adm)) {
      const text = readFileSync(adm, 'utf8');
      const m = text.match(/export const plainSecretGrants=(\[[^\n]*\]);/);
      if (m) {
        for (const n of JSON.parse(m[1])) plain.add(n);
        known = true;
      }
      for (const match of text.matchAll(/"((?:development )?secret\.keep [^"]+)"/g)) addText(match[1]);
    }
  }
  if (selected) {
    const receiptPaths = [
      resolve(appleArtifacts(selected).embed, 'compat.json'),
      resolve(appleArtifacts(selected).embed, 'receipt.json'),
      resolve(selected.dir, 'target', 'bake', 'compat.json'),
      resolve(selected.dir, 'target', 'bake', 'receipt.json'),
    ];
    for (const p of receiptPaths) {
      if (!existsSync(p)) continue;
      try {
        const doc = JSON.parse(readFileSync(p, 'utf8'));
        const ceiling = doc?.inputs?.grantCeiling ?? doc?.compat?.inputs?.grantCeiling;
        if (typeof ceiling === 'string') addText(ceiling);
      } catch {}
    }
  }
  let nonDevWeb = false;
  if (host === 'web' && dist && existsSync(resolve(dist, 'index.html'))) {
    const genRt = resolve(dist, '.gen', 'rt.js');
    if (existsSync(genRt)) {
      const rtText = readFileSync(genRt, 'utf8');
      nonDevWeb = /export const DEVELOPMENT_STORE\s*=\s*null\b/.test(rtText);
    } else if (!existsSync(resolve(dist, 'glue.js'))) {
      // Production web-js bundle has no .gen/rt.js and no DEVELOPMENT_STORE seed hook.
      const indexHtml = readFileSync(resolve(dist, 'index.html'), 'utf8');
      nonDevWeb = !indexHtml.includes('__exactSeedStored') && !existsSync(resolve(dist, '.gen'));
    }
  }
  for (const p of presence) plain.delete(p);
  return { known, plain, presence, nonDevWeb };
}

/** Validate `secret` and `signed-in` steps without ever echoing secret values (LLP 1018.000 D2/D4). */
export function validateSecretSteps({ secrets, sessions, grants, host }) {
  if (!secrets.length && !sessions.length) return;
  if (host === 'web' && grants.nonDevWeb) {
    const st = secrets[0] ?? sessions[0];
    throw new Error(`line ${st.line}: ${st.op} requires a development web build (production web builds do not run test seed hooks)`);
  }
  for (const st of [...secrets, ...sessions]) {
    const name = st.op === 'secret' ? st.name : st.key;
    if (!SECRET_NAME.test(name) || name.startsWith('exact.presence.')) {
      throw new Error(`line ${st.line}: ${st.op} "${name}" is not a valid secret.keep name`);
    }
    if (grants.presence.has(name)) {
      throw new Error(`line ${st.line}: ${st.op} "${name}" is presence-locked and cannot be seeded`);
    }
    if (grants.known && !grants.plain.has(name)) {
      throw new Error(`line ${st.line}: ${st.op} requires a plain secret.keep grant for ${name}`);
    }
  }
}

/** Write a seed JSON string to a 0600 temp file, pass it to `stage`, and always remove the local file. */
export function withSeedTempFile(payload, stage) {
  const dir = mkdtempSync(join(tmpdir(), 'exact-seed-'));
  const file = join(dir, 'seed.json');
  writeFileSync(file, payload, { mode: 0o600 });
  try { return stage(file); } finally { rmSync(dir, { recursive: true, force: true }); }
}

/** Clean up a per-test scratch store on hosts whose store lives outside `storeBase` (LLP 1018.000 D3). */
export function cleanRemoteStore({ host, appId, store, env = process.env, udid = null, serial = null, apk = false }) {
  if (!store || !appId || /[\\/]|\.\./.test(store)) return;
  if (host === 'ios' && udid) {
    const c = spawnSync('xcrun', ['simctl', 'get_app_container', udid, appId, 'data'], { encoding: 'utf8', timeout: 5000 });
    const root = c.status === 0 ? c.stdout.trim() : '';
    if (root) rmSync(resolve(root, 'Library/Caches/exact', appId, 'agent', store), { recursive: true, force: true });
    return;
  }
  if (host === 'android' && !apk && serial) {
    const tool = env.ADB || (env.ANDROID_HOME ? resolve(env.ANDROID_HOME, 'platform-tools/adb') : 'adb');
    const target = `/data/local/tmp/exact/${appId}/home/.cache/exact/${appId}/agent/${store}`;
    spawnSync(tool, ['-s', serial, 'shell', `rm -rf '${target.replace(/'/g, `'\\''`)}'`], { timeout: 5000 });
  }
}
