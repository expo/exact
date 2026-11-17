// The token names a native module reads (LLP 1121 D12), written from the
// app's design before the module compiles: `contract forma gen swift` or
// `kotlin` into the app's `.exact/forma/` (never committed), so a view names
// `.swatchRing` or `FormaColor.SWATCH_RING`, a param of its own `component`
// (D15) among them. An app with no design has none.
import { spawnSync } from 'node:child_process';
import { existsSync, rmSync } from 'node:fs';
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
