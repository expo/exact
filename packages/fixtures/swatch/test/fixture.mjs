// The fixture app, as each suite makes it: an app `exact new` made outside
// the repo, its view replaced by `fixture/app.contract`, depending on this
// package by `file:` and installed (LLP 1133.001 D8).
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { ensureShells } from '../../../../scripts/app-shells.mjs';

export const here = import.meta.dir, pkg = resolve(here, '..'), root = resolve(pkg, '../../..');

export const run = (cmd, args, cwd, env = process.env) => {
  const r = spawnSync(cmd, args, { cwd, env, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  if (r.status !== 0) throw new Error(`${cmd} ${args.join(' ')}: ${r.stdout}\n${r.stderr}`);
  return r.stdout;
};

/** The key the suites give the fixture's configuration, with the characters a generated Kotlin or Swift literal must
 * escape, and its part no build output, log, state or receipt may hold ([LEAK], plain even where JSON escapes the
 * rest); the fixture reports the key as `apiKey=<length> chars ending <last four>`. */
export const LEAK = `sentinel-${process.pid}-k9Qz`, KEY = `x$"\\${LEAK}`;
export const reportedConfig = (target) => `label=swatch ${target} apiKey=${KEY.length} chars ending ${KEY.slice(-4)} development=true`;
process.env.SWATCH_KEY = KEY;

/** The fixture app's directory and its bundle id. */
export function makeFixture() {
  const dir = mkdtempSync(join(tmpdir(), 'exact-swatch-')), app = join(dir, 'swatchfixture');
  run('bun', [resolve(root, 'scripts/exact.mjs'), 'new', app, '--with', 'ios,mac,android'], root);
  for (const f of ['app.contract', 'app.ts']) cpSync(join(here, 'fixture', f), join(app, f));
  rmSync(join(app, 'app.test.contract'), { force: true });
  // Its configuration on every target: the key from SWATCH_KEY, which the suites set to [KEY].
  const appJson = JSON.parse(readFileSync(join(app, 'app.json'), 'utf8')), id = appJson.app.id;
  appJson.packages = { '@fixture/swatch': Object.fromEntries(['web', 'android', 'ios', 'macos'].map((t) => [t, { apiKey: { env: 'SWATCH_KEY' } }])) };
  appJson.development = { packages: { '@fixture/swatch': Object.fromEntries(['web', 'android', 'ios', 'macos'].map(t => [t, { label: `swatch ${t}` }])) } };
  writeFileSync(join(app, 'app.json'), JSON.stringify(appJson, null, 2));
  writeFileSync(join(app, 'app.ts'), readFileSync(join(app, 'app.ts'), 'utf8').replace('__APP_ID__', id));
  const manifest = JSON.parse(readFileSync(join(app, 'package.json'), 'utf8'));
  manifest.dependencies = { ...manifest.dependencies, '@fixture/swatch': `file:${pkg}` };
  writeFileSync(join(app, 'package.json'), JSON.stringify(manifest, null, 2));
  run('bun', ['install'], app);
  // The native workspace written before the build starts: one the build wrote
  // is newer than the build, and the driver refuses the build as stale.
  ensureShells(app);
  return { app, id };
}

/** A suite's PASS or FAIL line; `failed()` counts the FAILs. */
export function checker() {
  let failures = 0;
  const check = (where, name, ok, detail) => {
    if (!ok) failures++;
    console.log(`${ok ? 'PASS' : 'FAIL'} ${where} ${name}${ok ? '' : `: ${detail}`}`);
  };
  return { check, failed: () => failures };
}
