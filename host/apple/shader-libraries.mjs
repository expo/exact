// A GPU module's Metal libraries, compiled at build time (EXACT-PATCHES.md 16 in
// vendor/wgpu-hal). wgpu writes each pipeline's MSL at run time, and Metal compiles
// it on every launch its shader cache misses: on a fresh install, most of a game's
// first-frame pipeline time. A development run records each source it compiled under
// `<app>/.exact/metal/` (the bundle's `ExactMetalSources`, buildinfo.mjs); a build
// compiles each recorded source for its SDK into `<key>.metallib`, which the module
// loads in place of the source when the key (the source and its options) matches.
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, renameSync, rmSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from './toolchain.mjs';
import { metalVersion } from './svg-filter.mjs';
import { appleCargo } from '../../scripts/cargo-check.mjs';
const root = resolve(fileURLToPath(new URL('../..', import.meta.url)));
const read = (cmd, args) => spawnSync(...appleCargo(cmd, args, { cwd: root, encoding: 'utf8' }));

/** Where development runs of `app` record the MSL they compiled. */
export const recordedSources = (app) => resolve(app.dir, '.exact', 'metal');

/** `metal`'s language flag for a recorded MSL version on `sdkName`: one standard
 * from MSL 3, a platform's own before it. */
export function languageFlag(msl, sdkName) {
  const [major] = msl.split('.').map(Number);
  if (major >= 3) return `-std=metal${msl}`;
  return `-std=${sdkName === 'macosx' ? 'macos' : 'ios'}-metal${msl}`;
}

/** Each recorded source of `app` compiled for `sdkName` into `out` as
 * `<key>.metallib`; the number written. Kept by what it is made of (the source,
 * its options, the SDK and the compiler) in `target/metal-kept`, shared by every
 * app. A source the compiler refuses (an MSL version newer than the SDK) is left
 * to the run-time compile; without Xcode's Metal toolchain nothing is written. */
export function shaderLibraries(app, sdkName, out) {
  const dir = recordedSources(app);
  if (!existsSync(dir)) return 0;
  const version = metalVersion(sdkName);
  if (version.status !== 0) {
    console.warn(`host/apple: no Metal toolchain, so ${app.name}'s GPU shaders compile at run time`);
    return 0;
  }
  const sdk = ['--show-sdk-path', '--show-sdk-version'].map(q => spawnSync('xcrun', ['--sdk', sdkName, q], { encoding: 'utf8' }).stdout?.trim());
  let written = 0, refused = 0;
  for (const file of readdirSync(dir).filter(f => f.endsWith('.metal')).sort()) {
    const key = file.slice(0, -'.metal'.length);
    if (!/^[0-9a-f]{16}$/.test(key) || !existsSync(resolve(dir, `${key}.json`))) continue;
    const source = readFileSync(resolve(dir, file), 'utf8'), options = JSON.parse(readFileSync(resolve(dir, `${key}.json`), 'utf8'));
    const flags = [languageFlag(options.msl, sdkName), ...(options.preserveInvariance ? ['-fpreserve-invariance'] : [])];
    const kept = resolve(root, 'target', 'metal-kept', 'shaders', createHash('sha256').update(JSON.stringify([source, flags, sdkName, sdk, version.stdout])).digest('hex').slice(0, 24) + '.metallib');
    if (!existsSync(kept)) {
      mkdirSync(dirname(kept), { recursive: true });
      const air = `${kept}.${process.pid}.air`, lib = `${kept}.${process.pid}.tmp`;
      const compiled = read('xcrun', ['-sdk', sdkName, 'metal', ...flags, '-c', resolve(dir, file), '-o', air]);
      const linked = compiled.status === 0 && read('xcrun', ['-sdk', sdkName, 'metallib', air, '-o', lib]).status === 0;
      rmSync(air, { force: true });
      if (!linked) { rmSync(lib, { force: true }); refused++; continue; }
      renameSync(lib, kept);
    }
    mkdirSync(out, { recursive: true });
    copyFileSync(kept, resolve(out, `${key}.metallib`));
    written++;
  }
  if (refused) console.warn(`host/apple: ${refused} recorded GPU shader${refused === 1 ? '' : 's'} did not compile for ${sdkName}; they compile at run time`);
  return written;
}

/** The bundle tree `capture/exact-metal` for a build of `app` whose GPU artifacts
 * (the primary `gpu/` module and every named one, build.mjs `gpuArtifacts`) are
 * `artifacts`: replaced by the libraries `shaderLibraries` compiles, or removed
 * when the app has no GPU module. The number written. */
export function bundleShaderLibraries(app, sdkName, capture, artifacts) {
  const out = resolve(capture, 'exact-metal');
  rmSync(out, { recursive: true, force: true });
  return artifacts.length ? shaderLibraries(app, sdkName, out) : 0;
}
