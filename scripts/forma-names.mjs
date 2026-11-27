// The token names a native module reads (LLP 1121 D12), written from the
// app's design before the module compiles: `contract forma gen swift` or
// `kotlin` into the app's `.exact/forma/` (never committed), so a view names
// `.swatchRing` or `FormaColor.SWATCH_RING`, a param of its own `component`
// (D15) among them. An app with no design has none.
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const root = resolve(fileURLToPath(new URL('..', import.meta.url)));

/** Write `app`'s names for `language` (`swift`, `kotlin`); throws when the
 * compiler refuses the design (two names that map alike, named). */
export function writeFormaNames(app, language) {
  // No design, no names: one left from a design since removed is not compiled.
  const file = resolve(app.dir, '.exact/forma', language === 'swift' ? 'forma.swift' : 'FormaTokens.kt');
  if (!app.manifest?.forma && !existsSync(resolve(app.dir, 'app.forma'))) return rmSync(file, { force: true });
  const r = spawnSync('cargo', ['run', '-q', '-p', 'contract', '--bin', 'contract', '--manifest-path', resolve(root, 'Cargo.toml'), '--', 'forma', 'gen', language, app.dir], { cwd: root, stdio: ['ignore', 'ignore', 'inherit'] });
  if (r.status !== 0) throw new Error(`contract forma gen ${language} ${app.dir} failed (${r.status ?? r.error?.message})`);
}

const digest = bytes => createHash('sha256').update(bytes).digest('hex');

const hasDesign = app => existsSync(resolve(app.dir, typeof app.manifest?.forma === 'string' ? app.manifest.forma : 'app.forma'));

/** Package-only composition is decided after Cargo. Use the CLI in that case. */
export function wantsSwiftNames(app, sources, embedOnly) {
  return !embedOnly && sources.length > 0 && hasDesign(app);
}

/** Read only the selected target's output, under the Cargo capture lock.
 * An OUT_DIR alone is insufficient since old files survive build script runs. */
export function captureSwiftNames(dir, output, plan) {
  const records = [...output.matchAll(/^cargo:metadata=exact-forma-swift=([a-f0-9]{64}):([a-f0-9]{64})$/gm)];
  if (records.length !== 1 || records[0][1] !== plan) return;
  const file = resolve(dir, 'forma.swift');
  if (!existsSync(file)) return;
  const names = readFileSync(file);
  if (digest(names) === records[0][2]) return { plan, names };
}

/** Compile the same Swift bytes in this build's private directory. Older,
 * linked and custom bakes without a matching projection go through the CLI. */
export function writeSwiftNames(app, captured, bakedPlan, scratch) {
  if (!captured || !hasDesign(app) || digest(bakedPlan) !== captured.plan) {
    rmSync(resolve(scratch, 'forma.swift'), { force: true });
    writeFormaNames(app, 'swift');
    return resolve(app.dir, '.exact/forma/forma.swift');
  }
  // Keep the documented generated file for editors. Compile a private copy
  // so another build publishing its names cannot change this compiler's input.
  for (const dir of [resolve(app.dir, '.exact/forma'), scratch]) {
    const file = resolve(dir, 'forma.swift');
    if (existsSync(file) && readFileSync(file).equals(captured.names)) continue;
    mkdirSync(dir, { recursive: true });
    const temp = resolve(dir, `.forma.swift.${process.pid}.tmp`);
    writeFileSync(temp, captured.names);
    renameSync(temp, file);
  }
  return resolve(scratch, 'forma.swift');
}
