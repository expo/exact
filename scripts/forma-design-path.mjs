// The app's design file, as the compiler finds it (LLP 1121 D2), read as the
// wire reads it: `app.json` only as a regular file of at most 1 MiB, so a
// FIFO or a giant there never stalls the server (stage 10b's review).

import { existsSync, readFileSync, statSync } from 'node:fs';
import { extname, resolve } from 'node:path';

/** `app.json`, read only as a regular file of at most 1 MiB (a
 * FIFO or a giant there never stalls the server); null without one. */
function manifest(appDir) {
  try { const m = resolve(appDir, 'app.json'), st = statSync(m); return st.isFile() && st.size <= 1 << 20 ? JSON.parse(readFileSync(m, 'utf8')) : null; } catch { return null; }
}

export function manifestForma(appDir) {
  return manifest(appDir)?.forma ?? null;
}

/** Refuse a former name before a build can silently omit the design. */
export function checkDesignName(appDir) {
  if (existsSync(resolve(appDir, 'app.tamber'))) throw new Error('rename app.tamber to app.forma');
  if (Object.hasOwn(manifest(appDir) ?? {}, 'tamber')) throw new Error("rename app.json's tamber key to forma");
}

/** The app's design, as the compiler finds it: the file `app.json`'s
 * `"forma"` names, else `app.forma`; null when there is none. */
export function designPath(appDir) {
  checkDesignName(appDir);
  const named = manifestForma(appDir);
  const path = resolve(appDir, typeof named === 'string' ? named : 'app.forma');
  if (!existsSync(path)) return null;
  if (extname(path) === '.tamber') throw new Error(`rename ${path} to ${path.slice(0, -'.tamber'.length)}.forma`);
  return path;
}
