// The stage 12 gate's builds of apps/library-gate's plans and its drives of
// them on each host (scripts/stage12-gate.mjs).
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { drive as scaleDrive, terminalPlan } from './scale-check.mjs';
import { webDist } from './app.mjs';
import { jsTargetBuild } from '../host/web/serve.mjs';

const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
export const LIBRARY_GATE = join(ROOT, 'apps/library-gate');

/** The web's wasm build of the agent's app (`bun host/web/build.mjs
 * <app> --wasm`, into this dist: EXACT_WASM_DIST, or the gate's own). */
const wasmDist = () => resolve(process.env.EXACT_WASM_DIST ?? join(ROOT, 'target', 'stage12-gate', 'web-wasm-dist'));

/** scale-check's drive on `host`, the web's two targets told apart: each
 * from a dist built for it, refused when the dist is the other's. The
 * terminal runs `plan`'s source compiled for it (`terminalOf`). */
export function drivePlan(host, plan, png, steps = [], app = null, driver = scaleDrive) {
  if (host === 'web-js') {
    if (!jsTargetBuild(webDist())) throw new Error(`the web dist ${webDist()} is not a JS-target build (bun host/web/build.mjs <app>)`);
    return driver('web', plan, png, steps, app);
  }
  if (host === 'web-wasm') {
    // library-gate's own artifact (apps/library-gate/web, theme2 13:47): a
    // wasm links what its own plan uses, so another app's refuses a cell
    // using what that app doesn't (Unlinked).
    const dist = wasmDist(), build = `EXACT_APP_DIR=${LIBRARY_GATE} EXACT_WEB_DIST=${dist} bun host/web/build.mjs library-gate --wasm`;
    if (!existsSync(join(dist, '.exact-build.json')) || jsTargetBuild(dist)) throw new Error(`${dist} is not a wasm build (${build})`);
    if (JSON.parse(readFileSync(join(dist, '.exact-build.json'), 'utf8')).app?.id !== 'com.exact.library-gate') throw new Error(`${dist} is not library-gate's wasm build (${build})`);
    const was = { dist: process.env.EXACT_WEB_DIST, app: process.env.EXACT_APP_DIR };
    Object.assign(process.env, { EXACT_WEB_DIST: dist, EXACT_APP_DIR: LIBRARY_GATE });
    try { return driver('web', plan, png, steps, app); } finally {
      for (const [k, v] of [['EXACT_WEB_DIST', was.dist], ['EXACT_APP_DIR', was.app]]) if (v === undefined) delete process.env[k]; else process.env[k] = v;
    }
  }
  if (host === 'terminal') return driver(host, terminalOf(plan), png, steps, app);
  return driver(host, plan, png, steps, app);
}

/** Each plan `buildPlan` wrote: its source, and its terminal compile once
 * one is made. */
const BUILT = new Map();

/** `source` compiled to `plan` for the GUI hosts, its source kept for the
 * terminal's compile: { ok, out }, the compiler's output. */
export function buildPlan(source, plan) {
  BUILT.delete(plan);
  const r = spawnSync('cargo', ['run', '-q', '-p', 'contract', '--', 'build', source, '-o', plan], { cwd: ROOT, encoding: 'utf8', timeout: 1_800_000, maxBuffer: 1 << 26 });
  const ok = r.status === 0 && !r.error;
  if (ok) BUILT.set(plan, { source, terminal: null });
  return { ok, out: `${r.stdout ?? ''}${r.stderr ?? ''}`, error: r.error };
}

/** The terminal's plan for `plan`: its source compiled for the terminal
 * (`plan`'s name, `.terminal.plan`), since the terminal host runs only a
 * terminal plan (LLP 1101.003 D9); made once for each build of `plan`. */
function terminalOf(plan) {
  const built = BUILT.get(plan);
  if (!built) throw new Error(`${plan}: no source recorded for it, so it has no terminal compile`);
  built.terminal ??= terminalPlan(built.source, `${plan.replace(/\.plan$/, '')}.terminal.plan`);
  return built.terminal;
}
