// The conformance fixture (LLP 1133.001 D7), as every platform's suite makes
// it: an app `exact new` made, its files replaced by
// `fixture/`'s, depending on both packages by `file:` and installed.
import { cpSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { ensureShells } from '../../../scripts/app-shells.mjs';

export const here = import.meta.dir, pkg = resolve(here, '..'), root = resolve(pkg, '../..');
export const capture = resolve(root, 'packages/media-capture');

export const run = (cmd, args, cwd, env = process.env) => {
  const r = spawnSync(cmd, args, { cwd, env, encoding: 'utf8', stdio: ['ignore', 'pipe', 'pipe'] });
  if (r.status !== 0) throw new Error(`${cmd} ${args.join(' ')}: ${r.stdout}\n${r.stderr}`);
  return r.stdout;
};

/** The fixture app's directory and its bundle id. `recording` and
 * `mediaCapture` are the two dependencies (`file:` the packages here by
 * default); `dependencies` are added beside them. */
export function makeFixture({ recording = `file:${pkg}`, mediaCapture = `file:${capture}`, dependencies = {}, directory = tmpdir() } = {}) {
  const dir = mkdtempSync(join(directory, 'exact-recording-')), app = join(dir, 'recfixture');
  run('bun', [resolve(root, 'scripts/exact.mjs'), 'new', app, '--with', 'ios,mac'], root);
  for (const f of ['app.contract', 'app.ts', 'strings']) cpSync(join(here, 'fixture', f), join(app, f), { recursive: true });
  rmSync(join(app, 'app.test.contract'), { force: true });
  const id = JSON.parse(readFileSync(join(app, 'app.json'), 'utf8')).app.id;
  writeFileSync(join(app, 'app.ts'), readFileSync(join(app, 'app.ts'), 'utf8').replace('__APP_ID__', id));
  const manifest = JSON.parse(readFileSync(join(app, 'package.json'), 'utf8'));
  manifest.dependencies = { ...manifest.dependencies, '@exact/media-capture': mediaCapture, '@exact/recording': recording, ...dependencies };
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
    console.log(`${ok ? "PASS" : "FAIL"} ${where} ${name}${ok ? "" : `: ${detail}`}`);
  };
  return { check, failed: () => failures };
}
