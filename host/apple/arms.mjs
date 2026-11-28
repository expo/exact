// The Apple build's compiled arms, cached by key in a directory every app's build shares.
import { copyFileSync, existsSync, mkdirSync, readdirSync, readFileSync, realpathSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { basename, dirname, relative, resolve, sep } from 'node:path';
import { createHash } from 'node:crypto';
import { machOType } from './assets.mjs';
import { xcframeworkSlice } from '../../scripts/package-libraries.mjs';

/** An arm's cached copies (`<key>-<name>`) beyond the three last used for
 * `name`, removed: different arguments (a development build, a production
 * one) keep a copy each, as every app's build shares the directory. Only an
 * exact `<16 hex>-<name>` is this name's; `probe-<name>` is another arm's. A copy's
 * kept roster (`<copy>.roster`) goes with it.
 * What a crashed build left half-written (`.<key>-<name>.<pid>.tmp`, armOutput) goes after a day. */
export function evictArms(dir, name) {
  // A copy another build removed meanwhile is the oldest
  const at = (file) => { try { return statSync(resolve(dir, file)).mtimeMs; } catch { return 0; } };
  const entries = readdirSync(dir);
  const copies = entries.filter((f) => /^[0-9a-f]{16}-/.test(f) && f.slice(17) === name).sort((a, b) => at(b) - at(a));
  for (const old of copies.slice(3)) for (const f of [old, `${old}.roster`]) rmSync(resolve(dir, f), { recursive: true, force: true });
  for (const tmp of entries.filter((f) => f.endsWith('.tmp'))) {
    if (Date.now() - at(tmp) > 86_400_000) rmSync(resolve(dir, tmp), { recursive: true, force: true });
  }
}

/** An arm's compile arguments. ld64 and lld name a dylib by its `-o` path (LC_ID_DYLIB) unless given an install
 * name, and that path is a temporary one (armOutput): a dylib is named `@rpath/<name>`, which is also how the app
 * finds it beside the executable. An install name the arguments already give is kept, in any spelling the linker
 * takes (lld's MachO Options.td: `-install_name` and its aliases), passed as swiftc's `-Xlinker` or in a clang `-Wl,`
 * list. A release build renames each loose dylib to its framework's longer name (wrapFramework); the linker's default
 * pad after the load commands is 0x20 bytes, so the dylib keeps 0x100 unless the arguments set their own. */
export function armArgs(args, built) {
  const given = args.flatMap((a) => a.startsWith('-Wl,') ? a.slice(4).split(',') : [a]);
  if (!args.includes('-emit-library') || given.some((a) => ['-install_name', '-dylib_install_name', '-dylinker_install_name'].includes(a))) return args;
  const pad = given.some((a) => a === '-headerpad' || a === '-headerpad_max_install_names') ? [] : ['-Xlinker', '-headerpad', '-Xlinker', '0x100'];
  return [...args, '-Xlinker', '-install_name', '-Xlinker', `@rpath/${basename(built)}`, ...pad];
}

/** An arm's cached copy's key: its arguments (`built` as `<out>`), its sources' text, the compiler and `extra`. The
 * output's layout is part of it, since the linker writes the output's name into the bytes (armOutput): an arm whose
 * arguments armArgs leaves as they are (an executable, or a dylib whose arguments name it) is still compiled anew. */
export const armKey = (args, sources, swiftc, extra) =>
  createHash('sha256').update(JSON.stringify([args, sources, swiftc, extra, 'output-under-own-name'])).digest('hex').slice(0, 16);

// The Swift glue written into linkRoot by build.mjs and writeSwiftNames.
const generatedSwift = /^(?:forma|ExactRosterProbe|Exact(?:DataKeys|Composed)-[A-Za-z0-9._-]+)\.swift$/;

// Exact's package outputs can move. SDK paths, other search paths and
// arbitrary arguments that happen to contain the same prefix cannot.
function packageSearchPath(arg, flag, dir) {
  return ((flag === '-I' || flag === '-L') && arg === dir)
    || (flag === '-I' && dirname(arg) === dir && basename(arg).startsWith('objc-'))
    || (flag === '-L' && arg === resolve(dir, 'libExactSwiftDependencies.dylib-dependencies'));
}

/** Normalize Clang/Swift module-cache paths so relocating a checkout or scratch
 * directory never invalidates cached arms, packages, or native-intent scans. */
export function normalizeModuleCacheArgs(args) {
  return args.map((a, i) => {
    if (args[i - 1] === '-module-cache-path') return '<module-cache>';
    if (a.startsWith('-fmodules-cache-path=')) return '-fmodules-cache-path=<module-cache>';
    return a;
  });
}

/** Content hash of external library/framework files with slice-relative labels. */
export function contentStamp(files) {
  return files.map(entry => {
    const [label, file] = Array.isArray(entry) ? entry : [basename(entry), entry];
    return [label, createHash('sha256').update(readFileSync(file)).digest('hex')];
  });
}

/** Whether a Swift source exposes its physical file path (#file/#filePath without a continuous #sourceLocation). */
export function isPathSensitive(source) {
  if (!/#file(?:Path)?\b/.test(source)) return false;
  return !source.startsWith('#sourceLocation(') || /#sourceLocation\s*\(\s*\)/.test(source);
}

const SOUND_OBJECT = /^sound_(?:mixer-c|render-c|engine-m)-[0-9a-f]{16}\.o$/;
const withinPrefix = (path, base) => Boolean(base) && (path === base || path.startsWith(base + sep));
const normalizeModuleMapFlag = (arg, { swiftRootDir, appDir, root, dir, packageDir, packageDirs }) => {
  if (!arg.startsWith('-fmodule-map-file=')) return null;
  const path = arg.slice('-fmodule-map-file='.length);
  if (withinPrefix(path, swiftRootDir)) return `-fmodule-map-file=<swift-pkg>${path.slice(swiftRootDir.length)}`;
  if (withinPrefix(path, dir)) return `-fmodule-map-file=<dir>${path.slice(dir.length)}`;
  if (packageDirs?.some(d => withinPrefix(path, d))) return `-fmodule-map-file=<packages>${path.slice(packageDir.length)}`;
  if (withinPrefix(path, appDir)) return `-fmodule-map-file=<app-source>/${relative(appDir, path)}`;
  if (withinPrefix(path, root)) return `-fmodule-map-file=<exact-root>/${relative(root, path)}`;
  return null;
};

/** Normalize an arm's argument list for cache keying (used by armSourceKey and nativeIntentModule). */
export function normalizeArmArgs(args, files, { built, snapshot, packageDir, packageDirs, linkRoot, root, appDir, swiftRootDir }) {
  const generated = new Set(files.filter(f => generatedSwift.test(basename(f)) && resolve(linkRoot, basename(f)) === f));
  const sources = files.map(f => readFileSync(f, 'utf8'));
  // An explicit source path in a compiled Swift file without a continuous #sourceLocation may survive in the output.
  const compiled = new Set(args);
  const pathSensitive = files.some((f, i) => compiled.has(f) && isPathSensitive(sources[i]));
  const fileSet = new Set(files);
  const keyArgs = normalizeModuleCacheArgs(args).map((a, i) => {
    const flag = args[i - 1];
    if (a === built) return '<out>';
    const moduleMap = normalizeModuleMapFlag(a, { swiftRootDir, appDir, root, packageDir, packageDirs });
    if (moduleMap) return moduleMap;
    const authored = snapshot ? snapshot.key(a) : a;
    if (authored !== a) {
      if (pathSensitive) return [authored, a];
      return withinPrefix(authored, appDir) ? ['app-source', relative(appDir, authored)] : authored;
    }
    if (!pathSensitive && generated.has(a)) return ['generated-swift', basename(a)];
    const matchedDir = packageDirs.find(dir => packageSearchPath(a, flag, dir) || a === resolve(dir, 'ExactPackage.o'));
    if (matchedDir) return '<packages>' + (matchedDir.startsWith(packageDir) ? a.slice(packageDir.length) : a.slice(matchedDir.length));
    if (withinPrefix(a, swiftRootDir)) return '<swift-pkg>' + a.slice(swiftRootDir.length);
    if (!pathSensitive && SOUND_OBJECT.test(basename(a))) return ['sound-object', basename(a)];
    if (withinPrefix(a, appDir) && (flag === '-F' || flag === '-L' || (!pathSensitive && (fileSet.has(a) || flag === '-I' || flag === '-import-objc-header'))))
      return ['app-source', relative(appDir, a)];
    if (withinPrefix(a, root) && (flag === '-F' || flag === '-L' || (!pathSensitive && (fileSet.has(a) || flag === '-I' || flag === '-import-objc-header'))))
      return ['exact-root', relative(root, a)];
    return a;
  });
  return { keyArgs, sources };
}

/** A private build restores the previous arms under a fresh linkRoot. Only
 * Exact's generated Swift files in that root lose their temporary directory
 * in the key. Their names and contents still count, and the tagged key
 * cannot collide with an authored path of the same name. */
export function armSourceKey(args, files, options) {
  const { keyArgs, sources } = normalizeArmArgs(args, files, options);
  return armKey(keyArgs, sources, options.swiftc, options.extra);
}

/** A roster probe reads package copies in the main build's directory but
 * writes its objects elsewhere. Name each copied source as the main build
 * does. Its relative path, #sourceLocation and full text remain in the key. */
export function packageKeyArgs(args, dir, packageDir, copies, name, { root, appDir, swiftRootDir, pathSensitive = false } = {}) {
  const products = new Set(['o', 'swiftmodule', 'tbd', 'dylib'].map(ext => resolve(dir, `${name}.${ext}`)));
  products.add(resolve(dir, `adapter-${name}.swift`));
  const baseModule = name.replace(/_objc_\d+$/, '');
  const objcDir = resolve(dir, `objc-${baseModule}`);
  const objcObject = new RegExp(`^${name.replace(/[.*+?^${}()|[\]\\]/g, '\\$&')}_objc_\\d+\\.o$`);
  return normalizeModuleCacheArgs(args).map((a, i) => {
    const flag = args[i - 1];
    const moduleMap = normalizeModuleMapFlag(a, { swiftRootDir, appDir, root, dir, packageDir });
    if (moduleMap) return moduleMap;
    if (!pathSensitive && copies.has(a) && a.startsWith(packageDir + sep)) return '<dir>' + a.slice(packageDir.length);
    if (products.has(a) || packageSearchPath(a, flag, dir)) return '<dir>' + a.slice(dir.length);
    if ((flag === '-I' || flag === '-c' || flag === '-MF') && (a === objcDir || a.startsWith(objcDir + sep))) return '<dir>' + a.slice(dir.length);
    if (dirname(a) === dir && objcObject.test(basename(a))) return `<dir>/${basename(a)}`;
    if (withinPrefix(a, swiftRootDir)) return '<swift-pkg>' + a.slice(swiftRootDir.length);
    if (!pathSensitive && withinPrefix(a, appDir) && (flag === '-I' || flag === '-c' || flag === '-import-objc-header' || a.endsWith('.swift')))
      return ['app-source', relative(appDir, a)];
    if (!pathSensitive && withinPrefix(a, root) && (flag === '-I' || flag === '-c' || flag === '-import-objc-header' || a.endsWith('.swift')))
      return ['exact-root', relative(root, a)];
    return a;
  });
}

/** Whether any authored Swift file mentions AppIntents or a gathered intent protocol. */
export function hasIntentSignal(files, protocols) {
  const pattern = new RegExp(`\\b(?:AppIntents|${protocols.join('|')})\\b`);
  return files.some(f => pattern.test(readFileSync(f, 'utf8')));
}

/** Where an arm compiles before it is renamed to its cached copy: under its own name, in a directory of this compile's
 * own (`<dir>/.<key>-<name>.<pid>.tmp/`, emptied first). lld hashes the output's file name into LC_UUID, so a name
 * with the process id in it made every build's dylib differ; the key and name keep two arms
 * compiling at once in one build apart, as they name its cached copy. */
export function armOutput(dir, key, built) {
  const own = resolve(dir, `.${key}-${basename(built)}.${process.pid}.tmp`);
  rmSync(own, { recursive: true, force: true });
  mkdirSync(own, { recursive: true });
  return resolve(own, basename(built));
}

/** The roster a probe arm's copy printed when it last ran, kept beside it (`<copy>.roster`): the copy's key is every
 * input the probe compiles from, so the same copy prints the same roster, and a build reads it here instead of running
 * the probe again (on the iOS simulator route, a simulator booted for it: about 7 s a build). The cached result must
 * match every file in the probe artifact. EXACT_NATIVE_ROSTERS imports results from a simulator that ran those same bytes;
 * an unmatched probe still needs execution. Undefined when none is kept. */
export function probeDigest(copy) {
  const hash = file => createHash('sha256').update(readFileSync(file)).digest('hex');
  if (!statSync(copy).isDirectory()) return hash(copy);
  const walk = dir => readdirSync(dir).sort().flatMap(name => {
    const file = resolve(dir, name);
    return statSync(file).isDirectory() ? walk(file) : [[relative(copy, file), hash(file)]];
  });
  return createHash('sha256').update(JSON.stringify(walk(copy))).digest('hex');
}
const rosterText = text => {
  const value = typeof text === 'string' && JSON.parse(text);
  if (!value || typeof value !== 'object' || Array.isArray(value)) throw new Error('host/apple: a qualified native roster must be a JSON object');
  return text;
};

export function keptRoster(copy, env = process.env) {
  if (!copy) return;
  let hash;
  try {
    hash = probeDigest(copy);
    const record = JSON.parse(readFileSync(`${copy}.roster`, 'utf8'));
    if (record.version === 1 && record.probeSha256 === hash) return rosterText(record.roster);
  } catch {}
  if (!env.EXACT_NATIVE_ROSTERS) return;
  const receipt = JSON.parse(readFileSync(env.EXACT_NATIVE_ROSTERS, 'utf8'));
  if (receipt.version !== 1 || !Array.isArray(receipt.probes)) throw new Error('host/apple: invalid native roster qualification file');
  const matches = receipt.probes.filter(record => record.probeSha256 === (hash ??= probeDigest(copy)));
  if (matches.length > 1) throw new Error('host/apple: duplicate native roster qualifications for one probe');
  if (!matches.length) return;
  const text = rosterText(matches[0].roster);
  keepRoster(copy, text);
  return text;
}
export function keepRoster(copy, text) {
  if (!copy) return;
  const making = `${copy}.roster.${process.pid}.tmp`;
  writeFileSync(making, JSON.stringify({ version: 1, probeSha256: probeDigest(copy), roster: rosterText(text) }));
  renameSync(making, `${copy}.roster`);
}

/** The routes an iOS or tvOS build reads its module roster on (LLP 1024 D3, "First-frame tags"), in order: Mac Catalyst,
 * an iOS simulator, macOS; tvOS has macOS alone. A route lacks only an xcframework slice (`missingSlice(variant)`).
 * The Swift packages are sources, so each route compiles them for its own target into `packages`: the build's own
 * directory where the targets are the same (a simulator build's simulator route), else one beside it. */
export function rosterRoutes({ tv, device, packageDir, missingSlice, platform = process.platform }) {
  // On Linux only the iOS Simulator probe can be qualified via EXACT_NATIVE_ROSTERS (build.mjs writes native-probes.json
  // for variant === true alone); Mac Catalyst and macOS cannot run and would only compile dead probes.
  if (!tv && platform === 'linux') {
    return [{ via: 'iOS Simulator', variant: true, lacking: missingSlice(true), packages: !device ? packageDir : `${packageDir}-simulator` }];
  }
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
  const walk = d => readdirSync(d, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))
    .flatMap(e => e.isDirectory() ? walk(resolve(d, e.name)) : [resolve(d, e.name)]);
  for (const fw of frameworks) {
    const { dir, lib } = xcframeworkSlice(fw, target), framework = lib.LibraryPath.endsWith('.framework');
    const link = basename(lib.LibraryPath, '.framework'), placed = resolve(dir, lib.LibraryPath);
    // A hand-composed static slice may hold several archives beside the one its plist names.
    const files = framework ? [resolve(placed, link)] : readdirSync(dir).filter(f => /^lib.*\.a$/.test(f)).sort().map(f => resolve(dir, f));
    if (!files.length) throw new Error(`host/apple: ${basename(fw)}'s ${lib.LibraryIdentifier} slice holds no static library (lib*.a) or framework`);
    const headers = !framework && lib.HeadersPath && existsSync(resolve(dir, lib.HeadersPath)) ? resolve(dir, lib.HeadersPath) : null;
    const fwHeaders = framework ? ['Headers', 'Modules'].map(s => resolve(placed, s)).filter(existsSync).flatMap(walk) : [];
    args.push(...(framework ? ['-F', dir, '-framework', link] : [...(headers ? ['-I', headers] : []), '-L', dir, ...files.map(f => `-l${basename(f).slice(3, -2)}`)]));
    for (const f of [...files, ...fwHeaders, ...(headers ? walk(headers) : [])]) stamped.push([relative(dir, f), f]);
    if (framework && machOType(files[0]) === 6) libraries.push({ placed, link });
  }
  if (libraries.length) args.push('-Xlinker', '-rpath', '-Xlinker', target.platform === 'macos' ? '@loader_path/../Frameworks' : '@executable_path/Frameworks');
  return { args, stamped, stamp: contentStamp(stamped), libraries };
}
