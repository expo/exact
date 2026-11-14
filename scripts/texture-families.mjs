// Which of a game's baked texture files a native bundle holds (LLP 1046.003,
// "Compressed textures"). The baker writes each texture as `x.tex` (RGBA8),
// `x.bc.tex` and `x.astc.tex`, and a device reads one of them
// (`game/render/src/surface/textures.rs`, `family`). Where every device of a
// platform reads one compressed family, its bundle holds that family alone:
// Metal on every Mac reads BC; every iPhone, iPad and Apple TV GPU (Apple2 and
// later) and the iOS simulator's read ASTC, which an M-series iPad reads too
// though it also reads BC. `TextureFamily::BUNDLED` (`game/engine/src/asset.rs`)
// is the renderer's side of this table. Other platforms hold every family.
//
// The bake decides (`bake/src/receipt.rs`): a roster it thinned names the family
// it kept in the cohort's inputs (`textureFamily`), and the bundle, a deploy's
// stream and a push's update thin by that family alone. An installed binary
// baked before thinning names none, and its renderer, which falls back to the
// RGBA8 file, is sent every file.

import { existsSync, readdirSync, rmSync } from 'node:fs';
import { resolve } from 'node:path';

/** The one family each platform's bundle holds; `ios` covers tvOS. */
const BUNDLED = { macos: 'bc', ios: 'astc' };

/** The family a bundle for `platform` holds, or null where it holds every one. */
export const bundledFamily = platform => BUNDLED[platform] ?? null;

const TEXTURE = /^(assets\/.+?)(?:\.(bc|astc))?\.tex$/;

/** The names among `names` (bundle paths) that a bundle holding `held` (`bc`,
 *  `astc`, or null for every family) leaves out: a baked texture's payloads of
 *  another family, and its RGBA8 file where the held payload is there instead.
 *  A `.tex` the bake made no payloads for (one an author wrote) stays, as does
 *  a payload with no RGBA8 beside it. */
export function unbundledTextures(names, held) {
  const dropped = new Set();
  if (!held) return dropped;
  const all = new Set(names);
  for (const name of all) {
    const [, stem, family] = TEXTURE.exec(name) ?? [];
    if (!stem) continue;
    if (family ? family !== held && all.has(`${stem}.tex`) : all.has(`${stem}.${held}.tex`)) dropped.add(name);
  }
  return dropped;
}

/** The platform's bundle, or the web bake when no native bake ran, thinned
 * by the installed cohort's texture family. A cohort baked before thinning
 * names no family and receives the files held aside in `thinned` too. */
export function streamBundle(bundle, platform, inputs) {
  const own = bundle.platforms?.[platform] ?? bundle;
  const full = own.thinned?.length && !inputs?.textureFamily ? { ...own, assets: [...own.assets, ...own.thinned] } : own;
  const dropped = unbundledTextures(full.assets.map(a => a.name), inputs?.textureFamily);
  return dropped.size ? { ...full, assets: full.assets.filter(a => !dropped.has(a.name)) } : full;
}

/** Remove from `dir`, a bundle's copy of the app's `assets/`, what a bundle
 *  holding `held` leaves out (`unbundledTextures`). */
export function thinTextures(dir, held) {
  const root = resolve(dir, 'assets');
  if (!held || !existsSync(root)) return;
  const names = readdirSync(root, { recursive: true }).map(name => `assets/${name}`);
  for (const name of unbundledTextures(names, held)) rmSync(resolve(dir, name));
}
