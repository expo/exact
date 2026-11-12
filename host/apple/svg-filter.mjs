// The SVG filter pictures' Metal kernels (@ref LLP 1055.000 D14), compiled at build time and kept.
import { copyFileSync, existsSync, mkdirSync, readFileSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { createHash } from 'node:crypto';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { spawnSync } from './toolchain.mjs';
import { metalRequired, metalToolchainInstructions } from './metal.mjs';
import { appleCargo } from '../../scripts/cargo-check.mjs';
const root = resolve(fileURLToPath(new URL('../..', import.meta.url)));
const read = (cmd, args) => spawnSync(...appleCargo(cmd, args, { cwd: root, encoding: 'utf8' }));
const run = (cmd, args) => {
  const r = read(cmd, args);
  if (r.status !== 0) throw new Error(`${cmd} failed (${r.status ?? r.error?.message})${r.stderr?.length ? ': ' + String(r.stderr).trim() : ''}`);
  return r;
};

/** `metal --version` for an SDK, kept: off a Mac the compiler runs under Wine (about a second a call, several per build),
 * so its answer is kept on disk by the compiler file's size and time, and in this process. */
const metalVersions = new Map();
export function metalVersion(sdkName) {
  if (metalVersions.has(sdkName)) return metalVersions.get(sdkName);
  const tool = process.env.EXACT_APPLE_METAL_TOOLS && resolve(process.env.EXACT_APPLE_METAL_TOOLS, 'bin/metal.exe');
  const stamp = tool && existsSync(tool) ? `${statSync(tool).size}-${statSync(tool).mtimeMs}` : null;
  const file = stamp && resolve(root, 'target', 'metal-kept', `version-${sdkName}.json`);
  if (file) try { const kept = JSON.parse(readFileSync(file, 'utf8')); if (kept.stamp === stamp) { metalVersions.set(sdkName, kept.result); return kept.result; } } catch {}
  const r = read('xcrun', ['-sdk', sdkName, 'metal', '--version']), result = { status: r.status, stdout: r.stdout };
  if (file && r.status === 0) try { mkdirSync(dirname(file), { recursive: true }); writeFileSync(file, JSON.stringify({ stamp, result })); } catch {}
  metalVersions.set(sdkName, result);
  return result;
}
/** `host/apple/metal/SvgFilter.metal` compiled for `sdkName` into `out`
 *  (`ExactSvgFilter.metallib`, @ref LLP 1055.000 D14): a filter picture's
 *  kernels, compiled here and not at run time, where the first launch after
 *  an install paid 113–264 ms inside the commit that shows it. The host
 *  tests need Xcode's Metal toolchain for it (`required`); an app built
 *  without it gets `null`, and its filter pictures draw with Core Image. */
export function svgFilterLibrary(sdkName, minimum, out, required = false) {
  const version = metalVersion(sdkName), install = metalToolchainInstructions(), missing = version.status !== 0;
  if (missing && (required || metalRequired())) throw new Error(`host/apple: this build requires the Metal toolchain for its SVG filter kernels${required ? '' : ' (EXACT_APPLE_METAL_REQUIRED=1)'}. ${install}`);
  if (missing) return console.warn(`host/apple: no Metal toolchain, so no ${svgFilterLibraryName}: SVG filter pictures draw with Core Image. ${install}`), null;
  const flag = { iphoneos: `-mios-version-min=${minimum}`, iphonesimulator: `-mios-simulator-version-min=${minimum}`, appletvos: `-mtvos-version-min=${minimum}`, appletvsimulator: `-mtvos-simulator-version-min=${minimum}`, macosx: `-mmacosx-version-min=${minimum}` }[sdkName];
  // Kept by what it is made of (the kernel source, the selected SDK and its version, the deployment flag and the
  // compiler), shared by every app.
  const source = resolve(root, 'host/apple/metal/SvgFilter.metal');
  const sdk = ['--show-sdk-path', '--show-sdk-version'].map(q => spawnSync('xcrun', ['--sdk', sdkName, q], { encoding: 'utf8' }).stdout?.trim());
  const key = createHash('sha256').update(JSON.stringify([readFileSync(source, 'utf8'), sdkName, sdk, flag, version.stdout])).digest('hex').slice(0, 20);
  const kept = resolve(root, 'target', 'metal-kept', `${key}-${svgFilterLibraryName}`);
  if (existsSync(kept)) { copyFileSync(kept, out); return out; }
  const air = out.replace(/\.metallib$/, '') + '.air';
  run('xcrun', ['-sdk', sdkName, 'metal', '-std=metal3.0', flag, '-c', source, '-o', air]);
  run('xcrun', ['-sdk', sdkName, 'metallib', air, '-o', out]);
  rmSync(air, { force: true });
  try { mkdirSync(dirname(kept), { recursive: true }); copyFileSync(out, `${kept}.${process.pid}.tmp`); renameSync(`${kept}.${process.pid}.tmp`, kept); } catch {}
  return out;
}
export const svgFilterLibraryName = 'ExactSvgFilter.metallib';
