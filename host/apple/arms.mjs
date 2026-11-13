// The Apple build's compiled arms, cached by key in a directory every app's build shares.
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, realpathSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { createHash } from 'node:crypto';
import { machOType } from './assets.mjs';
import { xcframeworkSlice } from '../../scripts/package-libraries.mjs';

/** An arm's cached copies (`<key>-<name>`) beyond the three last used for
 * `name`, removed: different arguments (a development build, a production
 * one) keep a copy each, as every app's build shares the directory. Only an
 * exact `<16 hex>-<name>` is this name's; `probe-<name>` is another arm's. A copy's
 * kept roster (`<copy>.roster`) goes with it.
 * A copy a crashed build left half-written (`.<pid>.tmp`) goes after a day. */
export function evictArms(dir, name) {
  // A copy another build removed meanwhile is the oldest
  const at = (file) => { try { return statSync(resolve(dir, file)).mtimeMs; } catch { return 0; } };
  const entries = readdirSync(dir);
  const copies = entries.filter((f) => /^[0-9a-f]{16}-/.test(f) && f.slice(17) === name).sort((a, b) => at(b) - at(a));
  for (const old of copies.slice(3)) for (const f of [old, `${old}.roster`]) rmSync(resolve(dir, f), { force: true });
  for (const tmp of entries.filter((f) => f.endsWith('.tmp'))) {
    if (Date.now() - at(tmp) > 86_400_000) rmSync(resolve(dir, tmp), { force: true });
  }
}

/** The roster a probe arm's copy printed when it last ran, kept beside it (`<copy>.roster`): the copy's key is every
 * input the probe compiles from, so the same copy prints the same roster, and a build reads it here instead of running
 * the probe again (on the iOS simulator route, a simulator booted for it: about 7 s a build). The cached result must
 * match the executable's bytes. EXACT_NATIVE_ROSTERS imports results from a simulator that ran those same bytes;
 * an unmatched probe still needs execution. Undefined when none is kept. */
const digest = copy => createHash('sha256').update(readFileSync(copy)).digest('hex');
const rosterText = text => {
  const value = typeof text === 'string' && JSON.parse(text);
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('host/apple: a qualified native roster must be a JSON object');
  return text;
};

export function keptRoster(copy, env = process.env) {
  if (!copy) return;
  let hash;
  try {
    hash = digest(copy);
    const record = JSON.parse(readFileSync(`${copy}.roster`, 'utf8'));
    if (record.version === 1 && record.probeSha256 === hash) return rosterText(record.roster);
  } catch {}
  if (!env.EXACT_NATIVE_ROSTERS) return;
  const receipt = JSON.parse(readFileSync(env.EXACT_NATIVE_ROSTERS, 'utf8'));
  if (receipt.version !== 1 || !Array.isArray(receipt.probes)) throw new Error('host/apple: invalid native roster qualification file');
  const matches = receipt.probes.filter(record => record.probeSha256 === (hash ??= digest(copy)));
  if (matches.length > 1) throw new Error('host/apple: duplicate native roster qualifications for one probe');
  if (!matches.length) return;
  const text = rosterText(matches[0].roster);
  keepRoster(copy, text);
  return text;
}
export function keepRoster(copy, text) {
  if (!copy) return;
  const making = `${copy}.roster.${process.pid}.tmp`;
  writeFileSync(making, JSON.stringify({ version: 1, probeSha256: digest(copy), roster: rosterText(text) }));
  renameSync(making, `${copy}.roster`);
}

/** The routes an iOS or tvOS build reads its module roster on (LLP 1024 D3, "First-frame tags"), in order: Mac Catalyst,
 * an iOS simulator, macOS; tvOS has macOS alone. A route lacks only an xcframework slice (`missingSlice(variant)`).
 * The Swift packages are sources, so each route compiles them for its own target into `packages`: the build's own
 * directory where the targets are the same (a simulator build's simulator route), else one beside it. */
export function rosterRoutes({ tv, device, packageDir, missingSlice }) {
  return [...(tv ? [] : [['Mac Catalyst', 'maccatalyst'], ['iOS Simulator', true]]), ['macOS', false]].map(([via, variant]) => ({
    via, variant, lacking: missingSlice(variant),
    packages: variant === true && !device ? packageDir : `${packageDir}-${variant === true ? 'simulator' : variant || 'macos'}`,
  }));
}

/** The app's own sources copied into `dir` as they are when the build begins: the
 * arms compile the copies, so an edit made while the build runs lands in the next
 * build instead of failing this one (swiftc: "input file … was modified during the
 * build"). `key` names a copy by its author's path, so the arm cache keys as before;
 * `authored` puts the author's paths back into a compiler's output. */
export function snapshotSources(files, dir) {
  mkdirSync(dir, { recursive: true });
  const real = realpathSync(dir), authored = new Map();
  const copies = files.map((file) => {
    const copy = resolve(dir, basename(file));
    copyFileSync(file, copy);
    authored.set(resolve(real, basename(file)), file).set(copy, file); // the longer /private/var… first
    return copy;
  });
  return {
    copies,
    key: (arg) => authored.get(arg) ?? arg,
    authored: (text) => [...authored].reduce((out, [copy, file]) => out.replaceAll(copy, file), text),
  };
}

/** The module's xcframework inputs for one destination. Only a Mach-O dylib is embedded;
 * a static framework or archive is consumed by the link. */
export function moduleFrameworks(frameworks, target) {
  const args = [], stamped = [], libraries = [];
  const walk = dir => readdirSync(dir, { withFileTypes: true }).flatMap(e => e.isDirectory() ? walk(resolve(dir, e.name)) : [resolve(dir, e.name)]);
  for (const fw of frameworks) {
    const { dir, lib } = xcframeworkSlice(fw, target), framework = lib.LibraryPath.endsWith('.framework');
    const link = basename(lib.LibraryPath, '.framework'), placed = resolve(dir, lib.LibraryPath);
    // A hand-composed static slice may hold several archives beside the one its plist names.
    const files = framework ? [resolve(placed, link)] : readdirSync(dir).filter(f => /^lib.*\.a$/.test(f)).map(f => resolve(dir, f));
    if (!files.length) throw new Error(`host/apple: ${basename(fw)}'s ${lib.LibraryIdentifier} slice holds no static library (lib*.a) or framework`);
    const headers = !framework && lib.HeadersPath && existsSync(resolve(dir, lib.HeadersPath)) ? resolve(dir, lib.HeadersPath) : null;
    args.push(...(framework ? ['-F', dir, '-framework', link] : [...(headers ? ['-I', headers] : []), '-L', dir, ...files.map(f => `-l${basename(f).slice(3, -2)}`)]));
    stamped.push(...files, ...(headers ? walk(headers) : []));
    if (framework && machOType(files[0]) === 6) libraries.push({ placed, link });
  }
  if (libraries.length) args.push('-Xlinker', '-rpath', '-Xlinker', target.platform === 'macos' ? '@loader_path/../Frameworks' : '@executable_path/Frameworks');
  // Roster probes run before a bundle exists; only those links may search the input slices.
  const probeArgs = libraries.flatMap(lib => ['-Xlinker', '-rpath', '-Xlinker', dirname(lib.placed)]);
  return { args, stamped, libraries, probeArgs };
}
