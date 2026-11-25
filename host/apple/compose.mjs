import { createHash } from 'node:crypto';
import { isDeepStrictEqual } from 'node:util';
import { copyFileSync, cpSync, existsSync, lstatSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, realpathSync, renameSync, statSync, symlinkSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { basename, dirname, isAbsolute, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { within } from '../../scripts/paths.mjs';
import { directDependencies, swiftPackageFiles, swiftPackageInventories, swiftPackageOutputs } from '../../scripts/package-libraries.mjs';
import { spawnSync, swiftOptimization } from './toolchain.mjs';
import { evictArms, probeDigest } from './arms.mjs';
import { swiftPackagesProblems } from '../../scripts/app.mjs';
import { appleImplementations, moduleInterfaces } from '../../js/bake/src/typescript.mjs';
import { packageConfig, sourceString } from '../../js/bake/src/package-config.mjs';
import { machOType } from './assets.mjs';
import { capturePrivacyManifest } from './privacy.mjs';
// The Swift that composes an app's module with its packages' implementations (LLP 1133.001 D5), which
// host/apple/build.mjs writes beside the module's sources and never commits.

/** The module side's fixed Swift, in `root`: the table, its view entries, the hatch bindings, extensions and service query,
 * and ExactCore's walk to an app file through no link, which an extension's `file` takes too. */
export const moduleSeam = (resolve, root) => ['ExactNativeModule.swift', 'ExactNativeViews.swift', 'ExactNativeTabs.swift', 'ExactForma.swift', 'ExactExtensions.swift', 'ExactServices.swift', 'ExactPackageLibrary.swift']
  .map(f => resolve(root, 'host/apple/modules', f)).concat(resolve(root, 'host/apple/Sources/ExactCore/AppFileWalk.swift'));

/** Wrapped module libraries find their dependencies in sibling frameworks, including when dlopened by a host with no run path. */
export function moduleLibraryLinkArgs({ wrapped = false, module = null } = {}) {
  const name = module ?? 'ExactModules';
  const install = wrapped ? `@rpath/${name}.framework/${name}` : `@rpath/${module ? `lib${module}` : 'libexact_modules'}.dylib`;
  return ['-Xlinker', '-rpath', '-Xlinker', wrapped ? '@loader_path/..' : '@loader_path',
    '-Xlinker', '-install_name', '-Xlinker', install];
}

export const rosterProbeMain = `import Foundation
@main enum ExactRosterProbe {
  static func main() {
    let path = URL(fileURLWithPath: CommandLine.arguments[0]).deletingLastPathComponent()
        .appendingPathComponent("libexact_modules.dylib").path
    guard let library = dlopen(path, RTLD_NOW | RTLD_LOCAL),
          let entry = dlsym(library, "exact_native_abi") else { fatalError(String(cString: dlerror())) }
    let table = unsafeBitCast(entry, to: (@convention(c) () -> UnsafeRawPointer).self)()
    print(String(cString: table.load(fromByteOffset: 8, as: UnsafePointer<CChar>.self)))
  }
}
`;

/** Keep the runnable probe and all its dependencies together after the build's temporary files go.
 * Qualification covers the whole directory, including packages restored from their compilation cache. */
export function rosterProbe({ library, launcher, built, packagesIn, packages, placePackages, frameworks, swiftProducts = () => [] }) {
  const placed = async () => {
    const ready = await library();
    if (ready !== true) return ready;
    if (launcher) {
      const ready = await launcher();
      if (ready !== true) return ready;
    }
    placePackages(packagesIn);
    const name = `${basename(built)}-artifact`, dir = dirname(library.cached);
    const making = resolve(dir, `${name}.${process.pid}.tmp`);
    rmSync(making, { recursive: true, force: true });
    mkdirSync(making, { recursive: true });
    try {
      copyFileSync(launcher ? `${built}.dylib` : built, resolve(making, 'libexact_modules.dylib'));
      if (launcher) copyFileSync(built, resolve(making, 'ExactRosterProbe'));
      for (const p of packages) copyFileSync(resolve(packagesIn, `${p.module}.dylib`), resolve(making, `lib${p.module}.dylib`));
      for (const product of swiftProducts()) {
        for (const lib of product.libraries) copyFileSync(lib.built, resolve(making, lib.load));
        for (const fw of product.frameworks) cpSync(fw.placed, resolve(making, 'Frameworks', basename(fw.placed)), { recursive: true, dereference: true });
        for (const resource of product.resources) cpSync(resource, resolve(making, basename(resource)), { recursive: true });
      }
      for (const fw of frameworks) cpSync(fw.placed, resolve(making, 'Frameworks', basename(fw.placed)), { recursive: true, dereference: true });
      placed.cached = resolve(dir, `${probeDigest(making).slice(0, 16)}-${name}`);
      if (!existsSync(placed.cached)) {
        try { renameSync(making, placed.cached); }
        catch (error) { if (!existsSync(placed.cached)) throw error; }
      }
      const now = new Date();
      utimesSync(placed.cached, now, now);
      placed.artifact = resolve(placed.cached, launcher ? 'ExactRosterProbe' : 'libexact_modules.dylib');
      evictArms(dir, name);
      return true;
    } finally { rmSync(making, { recursive: true, force: true }); }
  };
  return placed;
}

/** Link declared system frameworks even when only an Objective-C category uses them. Frameworks introduced
 * above the app's minimum OS are weak-linked; the package checks `#available` before using them (LLP 1133.016 D15). */
export function systemFrameworks(packages, os, minimum) {
  const above = (version) => { const [a, b] = [version, minimum ?? '0'].map(v => String(v).split('.').map(Number)); for (let i = 0; i < 3; i++) if ((a[i] ?? 0) !== (b[i] ?? 0)) return (a[i] ?? 0) > (b[i] ?? 0); return false; };
  const names = new Map();
  for (const p of packages) for (const [name, at] of Object.entries(p.frameworks ?? {})) {
    if (at?.[os]) names.set(name, names.get(name) || above(at[os]));
  }
  return [...names].sort(([a], [b]) => a.localeCompare(b)).flatMap(([name, weak]) => weak
    ? ['-Xlinker', '-weak_framework', '-Xlinker', name] : ['-framework', name]);
}

/** Each package exports a retained implementation through the shared seam (LLP 1067.000.001 D1). */
export function packageAdapter(p) {
  return `import ExactPackage
@_cdecl("exact_package_create")
public func exactPackageCreate(_ raw: UnsafeMutableRawPointer) -> UnsafeMutableRawPointer {
    let context = Unmanaged<ExactPackageContext>.fromOpaque(raw).takeUnretainedValue()
    let impl = ${p.className}(context: context)
    return Unmanaged.passRetained(ExactPackageImplementation(impl${p.authenticator ? ', authenticator: impl' : ''})).toOpaque()
}
`;
}

/** Each used package's configuration for an Apple `target` (`ios`, `macos` or `tvos`), by extension, refused as
 * package-config.mjs says; `release` is a production or distributed build. */
export const appleConfig = (app, target, extensions, release) =>
  packageConfig(app.dir, target, { extensions, release, development: !release, warn: (m) => console.warn(`${app.name}: ${m}`) });

/** The initial library contains metadata and factories, without importing a package's implementation, and each
 * package's configuration (`config`, [appleConfig]'s) as JSON, which `ExactPackageContext.config` reads. */
export function composedModule(packages, ownModule, config = new Map()) {
  const authenticator = packages.find(p => p.authenticator);
  const lines = ['// Written by host/apple/build.mjs; never edited.'];
  lines.push(`let exactAuthenticator: String? = ${authenticator ? JSON.stringify(authenticator.extension) : 'nil'}`);
  lines.push(`let exactStartAtLoad: [String] = [${packages.filter(p => p.start).map(p => JSON.stringify(p.extension)).join(', ')}]`);
  const members = packages.flatMap(p => (p.credentialMembers ?? []).map(name => `${JSON.stringify(name)}: ${JSON.stringify(p.extension)}`));
  lines.push(`let exactCredentialMembers: [String: String] = [${members.join(', ') || ':'}]`);
  lines.push(`let exactExtensions: [String: (ExactModule, String) -> ExactExtensionAdapter] = [${packages.map(p => `${JSON.stringify(p.extension)}: { ExactPackageLibrary.adapter($0, ${JSON.stringify(p.extension)}, module: ${JSON.stringify(p.module)}, session: $1) }`).join(', ') || ':'}]`);
  lines.push(`let exactPackageModules: [String: String] = [${packages.map(p => `${JSON.stringify(p.extension)}: ${JSON.stringify(p.module)}`).join(', ') || ':'}]`);
  const configured = packages.filter(p => Object.keys(config.get(p.extension) ?? {}).length);
  lines.push(`let exactPackageConfig: [String: String] = [${configured.map(p => `${JSON.stringify(p.extension)}: ${sourceString(JSON.stringify(config.get(p.extension)), 'swift')}`).join(', ') || ':'}]`);
  const views = packages.flatMap(p => (p.views ?? []).map(tag => `${JSON.stringify(tag)}: ExactNativeFactory(trackProps: ${JSON.stringify(p.trackProps?.[tag] ?? [])}, package: { try ExactExtensions.view($0, ${JSON.stringify(p.extension)}, ${JSON.stringify(tag)}, id: $1, props: $2, events: $3) })`));
  lines.push(`let exactPackageViews: [String: ExactNativeFactory] = [${views.join(', ') || ':'}]`);
  if (!ownModule) lines.push('final class ExactComposedModule: ExactModule {}', 'let exactModule: ExactModule.Type = ExactComposedModule.self');
  return lines.join('\n') + '\n';
}

/** Compile each package against the seam's link stub, then link its independent library.
 * The final app library supplies the seam; the stub permits app Swift to import a package
 * without a build-time link cycle (LLP 1067.000.001 D1, D5). */
export function packageCompiler({ packages, packageDir, packageTargets, packageCopies, seam, packagesRoot, swiftc, production, distribution, ios, tv, minimum, app, extensions, swiftInputs }) {
  const destination = tv ? 'tvos' : ios ? 'ios' : 'macos';
  const cache = { snapshots: new Map(), manifests: new Map(), destination };
  const graph = app ? appSwiftPackageGraph(app, packages, extensions, cache) : swiftPackageGraph(app, packages, cache);
  if (swiftInputs) {
    const captured = swiftInputs.filter(i => i.kind === 'swift-package').map(i => i.identity);
    const current = swiftPackageInventories(app, packages, destination, cache.snapshots).map(i => i.identity);
    const changed = [...captured.filter(row => !current.some(i => isDeepStrictEqual(i, row))), ...current.filter(row => !captured.some(i => isDeepStrictEqual(i, row)))];
    if (changed.length) throw new Error(`${changed[0].interface}/${changed[0].directory} changed after its bake. Run the build again.`);
  }
  if (graph.length) assertSwiftLock(app, graph);
  const placed = new Set();
  const productsByDirectory = new Map(), resources = [], frameworks = [];
  const run = args => {
    const result = spawnSync('xcrun', args, { encoding: 'utf8' });
    if (result.status !== 0) throw new Error(`host/apple: package compilation failed:\n${result.stderr ?? result.error ?? ''}`);
  };
  const flags = dir => ['--sdk', packageTargets.get(dir)[0], 'swiftc',
    '-module-cache-path', resolve(packagesRoot, 'modules-module-cache'),
    '-parse-as-library', swiftOptimization(), '-swift-version', '5', ...packageTargets.get(dir)[1]];
  const objectArgs = (name, sources, dir, linkName = name) => [...flags(dir), '-wmo', '-module-name', name,
    ...(linkName ? ['-module-link-name', linkName] : []),
    '-emit-module', '-emit-module-path', resolve(dir, `${name}.swiftmodule`),
    '-I', dir, '-c', ...sources, '-o', resolve(dir, `${name}.o`)];
  const kept = (name, args, sources, imports, dir, compile) => {
    const key = createHash('sha256').update(JSON.stringify([name, args.map(a => a.replaceAll(dir, '<dir>')), sources.map(f => readFileSync(f, 'utf8')), imports, swiftc])).digest('hex').slice(0, 20);
    const entry = resolve(packagesRoot, 'packages-kept', `${name}-${key}`);
    const outputs = d => readdirSync(d).filter(f => f.startsWith(`${name}.`));
    if (existsSync(entry)) {
      for (const f of outputs(entry)) copyFileSync(resolve(entry, f), resolve(dir, f));
      return key;
    }
    compile();
    const making = `${entry}.${process.pid}.tmp`;
    rmSync(making, { recursive: true, force: true });
    mkdirSync(making, { recursive: true });
    for (const f of outputs(dir)) copyFileSync(resolve(dir, f), resolve(making, f));
    try { renameSync(making, entry); }
    catch { rmSync(making, { recursive: true, force: true }); }
    return key;
  };
  const libraries = packages.map(p => ({ module: p.module, load: `lib${p.module}.dylib`, framework: p.module,
    built: resolve(packageDir, `${p.module}.dylib`), placed: resolve(packageDir, `${p.module}.framework`),
    privacyManifest: p.privacyManifest }));
  const place = dir => {
    if (placed.has(dir)) return;
    mkdirSync(dir, { recursive: true });
    const wrapped = dir === packageDir && ios && production;
    const os = packageTargets.get(dir)[1].some(arg => /-apple-(ios|tvos)/.test(arg)) ? 'ios' : 'macos';
    const platform = dir === packageDir ? destination : packageTargets.get(dir)[1].some(arg => arg.endsWith('-macabi')) ? 'maccatalyst' : os;
    const traitChecks = [];
    const declarations = swiftPackageGraph({ dir: app?.dir, manifest: { host: { [os]: app?.manifest.host?.[os] } } }, packages, { ...cache, traitChecks }, platform);
    verifySwiftCopies(cache.snapshots);
    const product = declarations.length ? buildSwiftDependencies({ app, graph, declarations, traitChecks,
      compile: flags(dir).slice(3), sdkName: packageTargets.get(dir)[0], directory: dir, wrapped }) : null;
    if (product) {
      verifySwiftCopies(cache.snapshots);
      for (const library of product.libraries) library.privacyManifestPaths = packages
        .filter(p => p.swiftPackages?.length).map(p => p.privacyManifest);
      productsByDirectory.set(dir, [product]);
      if (dir === packageDir) {
        libraries.push(...product.libraries);
        frameworks.push(...product.frameworks);
        resources.push(...product.resources);
      }
    }
    if (!packages.length) { placed.add(dir); return; }
    const coreName = wrapped ? '@rpath/ExactModules.framework/ExactModules' : '@rpath/libexact_modules.dylib';
    const buildCondition = production || distribution ? [] : ['-D', 'EXACT_DEVELOPMENT'];
    const seamObject = [...objectArgs('ExactPackage', [seam], dir, null), ...buildCondition];
    const stub = [...flags(dir), ...buildCondition, '-module-name', 'ExactPackage', '-emit-module',
      '-emit-module-path', resolve(dir, 'ExactPackage.swiftmodule'),
      '-emit-tbd', '-emit-tbd-path', resolve(dir, 'ExactPackage.tbd'),
      '-Xfrontend', '-tbd-install_name', '-Xfrontend', coreName, seam];
    const seamKey = kept('ExactPackage', [...seamObject, ...stub], [seam], [], dir, () => {
      run(seamObject);
      run(stub);
    });
    copyFileSync(resolve(dir, 'ExactPackage.tbd'), resolve(dir, 'libexact_modules.tbd'));
    packages.forEach((p, i) => {
      const adapter = resolve(dir, `adapter-${p.module}.swift`);
      writeFileSync(adapter, packageAdapter(p));
      const sources = [...packageCopies[i], adapter];
      const sdkImports = p.swiftPackages?.length ? ['-D', 'SWIFT_PACKAGE', ...product.imports] : [];
      const object = [...objectArgs(p.module, sources, dir), ...sdkImports, ...buildCondition];
      const os = dir === packageDir ? (ios ? tv ? 'tvos' : 'ios' : 'macos')
        : packageTargets.get(dir)[1].some(arg => arg.includes('-apple-ios')) ? 'ios' : 'macos';
      const floor = minimum && typeof minimum === 'object' ? minimum[os] : minimum;
      const link = [...flags(dir), '-emit-library', resolve(dir, `${p.module}.o`),
        '-L', dir, '-lexact_modules', ...systemFrameworks([p], os, floor),
        ...moduleLibraryLinkArgs({ wrapped, module: p.module }), ...(p.swiftPackages?.length ? product.link : []), '-o', resolve(dir, `${p.module}.dylib`)];
      kept(p.module, [...object, ...link], sources, [seamKey, product?.digest], dir, () => {
        run(object);
        run(link);
      });
      copyFileSync(resolve(dir, `${p.module}.dylib`), resolve(dir, `lib${p.module}.dylib`));
    });
    placed.add(dir);
  };
  const imports = dir => (productsByDirectory.get(dir) ?? []).flatMap(p => p.imports);
  const link = dir => (productsByDirectory.get(dir) ?? []).flatMap(p => p.link);
  return { graph, place, libraries, frameworks, resources, imports, link, products: dir => productsByDirectory.get(dir) ?? [] };
}

const swiftPathIdentity = path => basename(path).replace(/\.git$/i, '').toLowerCase();
const swiftIdentity = url => swiftPathIdentity(new URL(url).pathname);
const swiftURL = url => url.replace(/\/+$/, '').replace(/\.git$/i, '');

function swiftPackageSnapshot(app, owner, snapshots) {
  if (snapshots.has(owner)) return snapshots.get(owner);
  const generated = swiftPackageOutputs(app, owner), omitted = [];
  const digest = swiftPackageFiles(app.dir, owner, undefined, omitted, generated);
  const cache = resolve(swiftRoot(app), 'sources');
  mkdirSync(cache, { recursive: true });
  // A changed copy can still have a reader. Publish a fresh generation beside it.
  let path;
  for (let generation = 0; ; generation++) {
    const parent = resolve(cache, generation ? `${digest}-${generation}` : digest);
    mkdirSync(parent, { recursive: true });
    path = resolve(parent, basename(realpathSync(owner)));
    if (!lstatSync(path, { throwIfNoEntry: false })) {
      const making = mkdtempSync(resolve(cache, 'copy-'));
      try {
        if (swiftPackageFiles(app.dir, owner, making, [], generated) !== digest) throw new Error(`${owner} changed while copying the Swift package. Run the build again.`);
        try { renameSync(making, path); break; }
        catch (error) { if (!lstatSync(path, { throwIfNoEntry: false })) throw error; }
      } finally { rmSync(making, { recursive: true, force: true }); }
    }
    try {
      if (lstatSync(path).isDirectory() && swiftPackageFiles(null, path, undefined, omitted) === digest) break;
    } catch { /* A damaged copy is skipped, including one with a broken link. */ }
  }
  const snapshot = { path: realpathSync(path), digest, omitted, generated };
  snapshots.set(owner, snapshot);
  return snapshot;
}

function verifySwiftCopies(snapshots) {
  for (const [owner, { path, digest, omitted }] of snapshots) {
    if (swiftPackageFiles(null, path, undefined, omitted) !== digest) throw new Error(`${owner}: the cached Swift package changed during the build. Run the build again.`);
  }
}

// SwiftPM resolves a dependency's relative paths from its declared directory. dump-package
// runs it as the root and canonicalizes that directory. Keep the declared path for both.
function dumpShippedPackage(app, root, directory) {
  const scratch = resolve(swiftRoot(app), 'inspect');
  const dump = path => JSON.parse(swiftRun(['package', '--package-path', path, '--scratch-path', scratch, 'dump-package'], path));
  if (directory === realpathSync(directory)) return dump(directory);
  mkdirSync(scratch, { recursive: true });
  const view = mkdtempSync(resolve(scratch, 'manifest-'));
  try {
    let source = root, dest = view;
    for (const part of [...relative(root, directory).split('/'), null]) {
      for (const name of readdirSync(source)) {
        if (name === part) mkdirSync(resolve(dest, name));
        else symlinkSync(resolve(source, name), resolve(dest, name));
      }
      if (!part) break;
      source = resolve(source, part);
      dest = resolve(dest, part);
    }
    const manifest = dump(dest);
    for (const dep of manifest.dependencies) {
      const local = dep.fileSystem?.[0];
      if (local && within(view, local.path)) {
        local.path = resolve(root, relative(view, local.path));
        local.identity = swiftPathIdentity(local.path);
      }
    }
    return manifest;
  } finally { rmSync(view, { recursive: true, force: true }); }
}

function shippedSwiftPackage(app, impl, value, { snapshots, manifests }, claim, platform, checks) {
  const owner = impl.swiftPackageRoot, source = impl.swiftPackagesSource;
  const snapshot = swiftPackageSnapshot(app, owner, snapshots);
  const original = path => resolve(owner, relative(snapshot.path, path));
  const manifest = resolve(snapshot.path, relative(owner, source));
  if (!within(snapshot.path, manifest) || !within(snapshot.path, realpathSync(manifest))) throw new Error(`${source} escapes the package directory ${owner}`);
  if (!isDeepStrictEqual(JSON.parse(readFileSync(manifest)).swiftPackages, impl.swiftPackages)) throw new Error(`${source} changed while capturing Swift packages. Run the build again.`);
  const inspected = new Map();
  function inspect(directory, identity = swiftPathIdentity(directory)) {
    if (!within(snapshot.path, directory) || existsSync(directory) && !within(snapshot.path, realpathSync(directory))) throw new Error(`${source}: ${directory} escapes the shipped package`);
    const physical = existsSync(directory) ? realpathSync(directory) : directory;
    for (const name of snapshot.omitted) {
      const link = resolve(snapshot.path, name);
      // A workspace install beside Package.swift can be unrelated. It stays out
      // of the copy. A Swift dependency that actually names it is refused here.
      const workspaceInstall = /(?:^|\/)node_modules\/(?:@[^/]+\/)?[^/]+$/.test(name);
      if (within(link, directory) || (!workspaceInstall && within(physical, link))) throw new Error(`${resolve(owner, name)} escapes the package directory ${owner} or has no target`);
    }
    const file = resolve(directory, 'Package.swift');
    if (!existsSync(file) || !statSync(file).isFile()) throw new Error(`${original(directory)} has no Package.swift`);
    claim(identity, resolve(realpathSync(owner), relative(snapshot.path, physical)), source);
    if (inspected.has(directory)) return inspected.get(directory);
    if (!manifests.has(directory)) {
      try {
        manifests.set(directory, dumpShippedPackage(app, snapshot.path, directory));
      } catch (error) { throw new Error(`${original(file)}: ${error.message}`); }
    }
    const description = manifests.get(directory), locals = new Map();
    const pkg = { description, locals, file: original(file), identity };
    inspected.set(directory, pkg);
    for (const dependency of description.dependencies) {
      const local = dependency.fileSystem?.[0];
      if (local) {
        locals.set((local.nameForTargetDependencyResolutionOnly ?? local.identity).toLowerCase(), { traits: local.traits, load: () => inspect(local.path, local.identity) });
        continue;
      }
      const dep = dependency.sourceControl?.[0], url = dep?.location.remote?.[0]?.urlString;
      const version = dep?.requirement.exact?.[0];
      if (!url || !version) throw new Error(`${original(file)}: ${url ?? JSON.stringify(dependency)} must be a remote URL at an exact version or a package inside ${owner}`);
      if (!impl.swiftPackages.some(p => p.url && swiftURL(p.url) === swiftURL(url) && p.exactVersion === version)) {
        throw new Error(`${original(file)}: list ${url} at exactVersion ${version} in ${source}`);
      }
    }
    return pkg;
  }
  const directory = resolve(dirname(manifest), value.path), pkg = inspect(directory);
  for (const name of value.products) if (!pkg.description.products.some(p => p.name === name)) throw new Error(`${pkg.file} does not declare product ${name} requested by ${source}`);
  if (platform) checks.push({ pkg, names: value.products, platform });
  return { ...value, path: directory, digest: snapshot.digest };
}

// SwiftPM evaluates the manifest on the build machine. Its platform conditions
// select edges and settings on the destination, independently of that machine.
function checkSwiftSettings(checks, resolved) {
  const seen = new Map(), host = process.platform === 'darwin' ? 'macos' : process.platform;
  let usesTraits = false;
  function use(pkg, names, destination, requested) {
    const declared = pkg.description.traits ?? [];
    usesTraits ||= declared.length > 0;
    const traits = new Set(resolved ? resolved.get(pkg.identity) : requested);
    if (!resolved) for (const name of traits) {
      for (const implied of declared.find(t => t.name === name)?.enabledTraits ?? []) traits.add(implied);
    }
    const traitKey = JSON.stringify([...traits].sort());
    for (const name of names) visit(pkg.description.products.find(p => p.name === name), destination);
    function visit(item, os) {
      if (!item) return;
      if (item.type === 'macro' || item.type === 'plugin') os = host;
      const key = os + traitKey;
      if (!seen.has(item)) seen.set(item, new Set());
      if (seen.get(item).has(key)) return;
      seen.get(item).add(key);
      const applies = condition => (!condition?.platformNames?.length || condition.platformNames.includes(os))
        && (!condition?.traits?.length || condition.traits.some(t => traits.has(t)));
      if (item.settings?.some(s => Object.hasOwn(s.kind, 'unsafeFlags') && applies(s.condition))) throw new Error(`${pkg.file}: ${item.name} uses unsafeFlags`);
      const target = name => pkg.description.targets.find(t => t.name === name);
      for (const name of item.targets ?? []) visit(target(name), os);
      function product(name, owner, platform) {
        const dependency = pkg.locals.get((owner ?? name).toLowerCase());
        // Before tools 5.2 SwiftPM looked up an unqualified product in every dependency.
        const legacy = Bun.semver.order(pkg.description.toolsVersion._version, '5.2.0') < 0;
        const candidates = dependency ? [dependency] : !owner && legacy ? pkg.locals.values() : [];
        for (const dep of candidates) use(dep.load(), [name], platform,
          (dep.traits ?? [{ name: 'default' }]).filter(t => applies(t.condition)).map(t => t.name));
      }
      for (const dependency of item.dependencies ?? []) {
        const args = dependency.target ?? dependency.byName ?? dependency.product;
        if (!applies(args.at(-1))) continue;
        if (dependency.product) product(args[0], args[1], os);
        else if (target(args[0])) visit(target(args[0]), os);
        else product(args[0], undefined, os);
      }
      for (const { plugin } of item.pluginUsages ?? []) {
        if (plugin[1]) product(plugin[0], plugin[1], host);
        else visit(target(plugin[0]), host);
      }
    }
  }
  for (const { pkg, names, platform } of checks) use(pkg, names, platform, ['default']);
  return usesTraits;
}

// Different consumers can enable different traits on the same package. SwiftPM
// reports their union for the generated root after resolving the locked graph.
function checkResolvedSwiftTraits(checks, dir, scratch) {
  if (!checks.length) return;
  const args = ['package', '--disable-automatic-resolution', ...(scratch ? ['--scratch-path', scratch] : []), 'show-dependencies', '--format', 'json'];
  const traits = new Map();
  function collect(pkg) {
    traits.set(pkg.identity, pkg.traits ?? []);
    for (const child of pkg.dependencies) collect(child);
  }
  collect(JSON.parse(swiftRun(args, dir)));
  checkSwiftSettings(checks, traits);
}

/** One graph for both Apple destinations and the app's own Swift (LLP 1133.001 D9). */
export function swiftPackageGraph(app, bound = [], cache = { snapshots: new Map(), manifests: new Map() }, building) {
  const declarations = ['ios', 'macos'].map(os => ({ source: `${resolve(app?.dir ?? '.', 'app.json')} host.${os}.swiftPackages`, values: app?.manifest?.host?.[os]?.swiftPackages }));
  for (const impl of bound) declarations.push({ source: `${impl.swiftPackagesSource ?? impl.extension} swiftPackages`, values: impl.swiftPackages, impl });
  const graph = new Map(), origins = new Map(), checks = [];
  function claim(identity, location, source) {
    const previous = origins.get(identity);
    if (previous && previous.location !== location) throw new Error(`Swift package conflict: ${previous.source} requests ${previous.location}; ${source} requests ${location}. Use one source of ${identity}.`);
    origins.set(identity, { location, source });
  }
  for (const { source, values, impl } of declarations) {
    const problems = swiftPackagesProblems(values, source, !!impl?.swiftPackageRoot);
    if (problems.length) throw new Error(problems.join('\n'));
    const target = impl?.swiftPackageTarget ?? cache.destination ?? 'macos';
    const platform = building ?? (!cache.destination || cache.destination === target ? target : null);
    for (const declared of values ?? []) {
      const value = declared.path ? shippedSwiftPackage(app, impl, declared, cache, claim, platform, checks) : declared;
      const identity = value.path ? swiftPathIdentity(value.path) : swiftIdentity(value.url);
      if (value.url) claim(identity, `${swiftURL(value.url)} at ${value.exactVersion}`, source);
      graph.set(identity, { ...value, identity, source, products: [...new Set([...(graph.get(identity)?.products ?? []), ...value.products])].sort() });
    }
  }
  if (checkSwiftSettings(checks)) cache.traitChecks?.push(...checks);
  return [...graph.values()].sort((a, b) => a.identity.localeCompare(b.identity));
}

/** Bind only interfaces in the compiled plan, including views reached through Contract libraries. */
export function appSwiftPackageGraph(app, bound = [], extensions, cache = { snapshots: new Map(), manifests: new Map() }) {
  if (extensions && !swiftPackageGraph(app, bound, cache).length) return [];
  extensions ??= swiftLockExtensions(app);
  return swiftPackageGraph(app, [...bound, ...['ios', 'macos', 'tvos'].flatMap(os => appleImplementations(app.dir, os, extensions))], cache);
}

function swiftLockExtensions(app) {
  if (!app.manifest?.app?.id) return {};
  const dependencies = new Set(directDependencies(app.dir).map(p => p.name));
  if (!moduleInterfaces(app.dir).some(i => dependencies.has(i.name))) return {};
  const root = fileURLToPath(new URL('../..', import.meta.url));
  const target = resolve(process.env.CARGO_TARGET_DIR ?? resolve(root, 'target'));
  const dir = resolve(swiftRoot(app), 'plan');
  mkdirSync(dir, { recursive: true });
  const run = (command, args) => {
    const result = spawnSync(command, args, { cwd: root, encoding: 'utf8', maxBuffer: 1 << 26 });
    if (result.status !== 0) throw new Error(`Swift package discovery failed:\n${result.stderr ?? result.error?.message ?? ''}\n${result.stdout ?? ''}`);
    return result.stdout;
  };
  run('cargo', ['build', '-q', '-p', 'contract', '-p', 'exact-bake', '--bins']);
  const plan = resolve(dir, 'app.plan'), compat = resolve(dir, 'compat.json');
  run(resolve(target, 'debug/contract'), ['build', resolve(app.dir, 'app.contract'), '-o', plan]);
  writeFileSync(compat, '{"inputs":{}}');
  return JSON.parse(run(resolve(target, 'debug/exact-bake'), ['graph', plan, compat, 'aarch64-apple-darwin', app.dir])).extensions;
}
const lockAdvice = app => `${resolve(app.dir, 'Package.resolved')}: run bun exact.mjs lock in ${app.dir}, then review and commit Package.resolved`;
export function assertSwiftLock(app, graph) {
  const remote = graph.filter(p => p.url);
  if (!remote.length) return null;
  let lock;
  try { lock = JSON.parse(readFileSync(resolve(app.dir, 'Package.resolved'), 'utf8')); }
  catch { throw new Error(`Swift packages need a lock; ${lockAdvice(app)}`); }
  if (![2, 3].includes(lock.version) || !Array.isArray(lock.pins)) throw new Error(`Invalid Swift package lock; ${lockAdvice(app)}`);
  for (const p of remote) {
    const pin = lock.pins.find(pin => pin.identity === p.identity);
    if (!pin || pin.state?.version !== p.exactVersion || swiftURL(pin.location ?? '') !== swiftURL(p.url) || !/^[0-9a-f]{40}$/.test(pin.state?.revision ?? '')) {
      throw new Error(`${p.source} requests ${p.url} at ${p.exactVersion}, which differs from the lock; ${lockAdvice(app)}`);
    }
  }
  return readFileSync(resolve(app.dir, 'Package.resolved'));
}
const products = declarations => declarations.flatMap(p => p.products.map(name => `.product(name: ${JSON.stringify(name)}, package: ${sourceString(p.identity, 'swift')})`));
function swiftManifest(graph, name, declarations, compile = [], link = []) {
  const triple = compile[compile.indexOf('-target') + 1] ?? '';
  const os = /-apple-(macosx?|ios|tvos)([\d.]+)/.exec(triple);
  const floors = { macos: '14.0', ios: '17.0', tvos: '17.0' };
  if (os) floors[os[1].startsWith('macos') ? 'macos' : os[1]] = os[2].includes('.') ? os[2] : `${os[2]}.0`;
  return `// swift-tools-version:5.9
import PackageDescription
let package = Package(name: "ExactSwiftProducts", platforms: [.macOS("${floors.macos}"), .iOS("${floors.ios}"), .tvOS("${floors.tvos}")],
  products: [.library(name: ${JSON.stringify(name)}, type: .dynamic, targets: [${JSON.stringify(name)}])],
  dependencies: [${graph.map(p => p.path ? `.package(path: ${sourceString(p.path, 'swift')})` : `.package(url: ${JSON.stringify(p.url)}, exact: ${JSON.stringify(p.exactVersion)})`).join(', ')}],
  targets: [.target(name: ${JSON.stringify(name)}, dependencies: [${products(declarations).join(', ')}], path: "Sources",
    swiftSettings: [.unsafeFlags(${JSON.stringify(compile)})], linkerSettings: [.unsafeFlags(${JSON.stringify(link)})])])
`;
}
const swiftRoot = app => resolve(app.target ?? resolve(app.dir, '.exact'), 'swift-packages', createHash('sha256').update(JSON.stringify([app.dir, app.id ?? app.manifest?.app?.id])).digest('hex').slice(0, 16));
function swiftRun(args, cwd) {
  const env = { ...process.env }; delete env.SDKROOT;
  const result = spawnSync('swift', args, { cwd, env, encoding: 'utf8', maxBuffer: 1 << 26 });
  if (result.status !== 0) throw new Error(`Swift package ${args[0]} failed:\n${result.stdout ?? ''}${result.stderr ?? result.error?.message ?? ''}`);
  return result.stdout;
}
export function lockSwiftPackages(app) {
  const cache = { snapshots: new Map(), manifests: new Map(), traitChecks: [] };
  const graph = appSwiftPackageGraph(app, [], undefined, cache);
  if (!graph.length) return null;
  const dir = resolve(swiftRoot(app), 'resolve');
  mkdirSync(resolve(dir, 'Sources'), { recursive: true });
  writeFileSync(resolve(dir, 'Sources/Resolve.swift'), 'public enum ExactResolve {}\n');
  writeFileSync(resolve(dir, 'Package.swift'), swiftManifest(graph, 'ExactResolve', graph));
  const lock = resolve(app.dir, 'Package.resolved'), generated = resolve(dir, 'Package.resolved');
  const remote = graph.some(p => p.url);
  if (remote && existsSync(lock)) copyFileSync(lock, generated);
  if (!remote) rmSync(generated, { force: true });
  verifySwiftCopies(cache.snapshots);
  swiftRun(['package', 'resolve'], dir);
  checkResolvedSwiftTraits(cache.traitChecks, dir);
  verifySwiftCopies(cache.snapshots);
  if (!remote) {
    if (existsSync(generated)) throw new Error(`SwiftPM resolved a remote dependency of a local-only graph in ${generated}`);
    if (!existsSync(lock)) return null;
    rmSync(lock);
    return `${lock}: removed (only local Swift packages)`;
  }
  const next = readFileSync(generated), was = existsSync(lock) && readFileSync(lock).equals(next);
  const temporary = `${lock}.${process.pid}.tmp`;
  writeFileSync(temporary, next); renameSync(temporary, lock);
  assertSwiftLock(app, graph);
  return `${lock}: ${was ? 'current; no changes' : 'resolved'}`;
}

export function swiftResourceOverlay(bin) {
  const roots = [], directory = resolve(bin, 'resource-accessors');
  for (const target of readdirSync(bin).filter(name => name.endsWith('.build'))) {
    for (const name of ['resource_bundle_accessor.swift', 'resource_bundle_accessor.m']) {
      const source = resolve(bin, target, 'DerivedSources', name);
      if (!existsSync(source)) continue;
      // SwiftPM's command-line layout puts bundles beside its products. Signed Mac apps use Contents/Resources.
      const contents = readFileSync(source, 'utf8')
        .replaceAll('Bundle.main.bundleURL', '(Bundle.main.resourceURL ?? Bundle.main.bundleURL)')
        .replaceAll('[NSBundle mainBundle] bundleURL', '[NSBundle mainBundle] resourceURL');
      const copy = resolve(directory, target, name);
      mkdirSync(resolve(directory, target), { recursive: true });
      if (!existsSync(copy) || readFileSync(copy, 'utf8') !== contents) writeFileSync(copy, contents);
      roots.push({ type: 'file', name: source, 'external-contents': copy });
    }
  }
  if (!roots.length) return [];
  const overlay = resolve(directory, 'overlay.json'), contents = JSON.stringify({ version: 0, roots });
  if (!existsSync(overlay) || readFileSync(overlay, 'utf8') !== contents) writeFileSync(overlay, contents);
  return ['-Xswiftc', '-vfsoverlay', '-Xswiftc', overlay, '-Xcc', '-ivfsoverlay', '-Xcc', overlay];
}

export function swiftPrivacyManifests(roots) {
  const paths = roots.flatMap(root => readdirSync(root, { recursive: true })
    .filter(file => basename(file) === 'PrivacyInfo.xcprivacy').map(file => resolve(root, file)));
  return [...new Set(paths)].sort().map(capturePrivacyManifest);
}

/** All consumers link one dynamic SwiftPM product, so an SDK's static state has one owner.
 * SwiftPM keeps its checksum verification and builds only the app's resolved versions. */
function buildSwiftDependencies({ app, graph, declarations, traitChecks, compile, sdkName, directory, wrapped }) {
  const name = 'ExactSwiftDependencies', link = [...moduleLibraryLinkArgs({ wrapped, module: name }),
    '-Xlinker', '-headerpad_max_install_names'];
  const output = resolve(directory, `lib${name}.dylib`);
  const locked = assertSwiftLock(app, graph);
  // Invocation-private module-cache paths do not identify the app's dependency build.
  const options = args => args.map(arg => isAbsolute(arg) ? '<path>' : arg);
  const key = createHash('sha256').update(JSON.stringify([graph, declarations, name, options(compile), options(link), sdkName])).digest('hex').slice(0, 20);
  const dir = resolve(swiftRoot(app), key), sourceDir = resolve(dir, 'Sources');
  mkdirSync(sourceDir, { recursive: true });
  const source = resolve(sourceDir, 'Dependencies.swift'), content = 'public enum ExactSwiftDependencies {}\n';
  if (!existsSync(source) || readFileSync(source, 'utf8') !== content) writeFileSync(source, content);
  writeFileSync(resolve(dir, 'Package.swift'), swiftManifest(graph, name, declarations, compile, link));
  const lock = resolve(dir, 'Package.resolved');
  if (locked) writeFileSync(lock, locked);
  const at = compile.indexOf('-target'), sdkAt = compile.indexOf('-sdk');
  // The shared product supplies every consumer with the same SDK artifacts and Clang module maps.
  const scratch = resolve(swiftRoot(app), 'build', key);
  const args = ['build', '--build-system', 'native', '--scratch-path', scratch, '--disable-build-manifest-caching', '--disable-automatic-resolution', '--resolver-fingerprint-checking', 'strict', '-c', 'release',
    ...(at >= 0 ? ['--triple', compile[at + 1]] : []), ...(sdkAt >= 0 ? ['--sdk', compile[sdkAt + 1]] : []), '--product', name];
  const bin = swiftRun([...args, '--show-bin-path'], dir).trim(), installAt = link.indexOf('-install_name');
  try {
    checkResolvedSwiftTraits(traitChecks, dir, scratch);
    swiftRun([...args, '--print-manifest-job-graph'], dir);
    const plan = JSON.parse(readFileSync(resolve(bin, 'description.json'), 'utf8'));
    const inputs = new Set(Object.values(plan.swiftCommands).find(c => c.moduleName === name)?.inputs?.map(input => input.name));
    const staticLibraries = Object.values(plan.copyCommands ?? {}).filter(copy => copy.outputs.some(output => inputs.has(output.name)))
      .flatMap(copy => copy.inputs.map(input => input.name)).flatMap(path => path.endsWith('.framework')
        ? [resolve(path, basename(path, '.framework'))] : path.endsWith('.a') ? [path] : [])
      .filter(path => machOType(path) !== 6);
    // Force-load every member so consumers can link symbols the shared product leaves unreferenced.
    // Pass -force_load to clang so it precedes SwiftPM's -framework of its copy.
    // ld64.lld reports duplicate symbols when a later -force_load loads members already taken from the copy.
    if (staticLibraries.length) writeFileSync(resolve(dir, 'Package.swift'), swiftManifest(graph, name, declarations, compile,
      [...link, ...[...new Set(staticLibraries)].flatMap(path => ['-Xclang-linker', '-force_load', '-Xclang-linker', path])]));
    swiftRun([...args, ...swiftResourceOverlay(bin)], dir);
  }
  catch (error) { if (!locked) throw error; throw new Error(`${error.message}\n${lockAdvice(app)}`); }
  if (locked ? !existsSync(lock) || !readFileSync(lock).equals(locked) : existsSync(lock)) throw new Error(`SwiftPM changed the build's lock; ${lockAdvice(app)}`);
  if (installAt >= 0) swiftRunTool(['install_name_tool', '-id', link[installAt + 2], resolve(bin, `lib${name}.dylib`)]);
  copyFileSync(resolve(bin, `lib${name}.dylib`), output);
  const imports = ['-I', bin, '-I', resolve(bin, 'Modules'), '-F', bin];
  // The native build plan carries the Clang module maps and headers that consumers of this Swift module need.
  const description = JSON.parse(readFileSync(resolve(bin, 'description.json'), 'utf8'));
  const command = Object.values(description.swiftCommands).find(c => c.moduleName === name);
  if (!command) throw new Error(`SwiftPM supplied no compiler context for ${name}`);
  for (let i = 0; i < command.otherArguments.length; i++) {
    const arg = command.otherArguments[i];
    if (['-I', '-F', '-Fsystem', '-Xcc'].includes(arg)) imports.push(arg, command.otherArguments[++i]);
  }
  const libraries = [{ built: output, load: `lib${name}.dylib`, framework: name, placed: resolve(directory, `${name}.framework`), swiftPackage: true }], frameworks = [], resources = [];
  const artifactDir = resolve(dirname(output), `${basename(output)}-dependencies`);
  mkdirSync(artifactDir, { recursive: true });
  for (const entry of readdirSync(bin)) {
    const from = resolve(bin, entry);
    if (entry.endsWith('.dylib') && entry !== `lib${name}.dylib`) {
      const framework = entry.replace(/^lib/, '').replace(/\.dylib$/, '');
      const built = resolve(artifactDir, entry); copyFileSync(from, built);
      libraries.push({ built, load: entry, framework, placed: resolve(artifactDir, `${framework}.framework`), swiftPackage: true });
    } else if (entry.endsWith('.framework') && machOType(resolve(from, basename(entry, '.framework'))) === 6) {
      frameworks.push({ placed: from, link: basename(entry, '.framework'), swiftPackage: true });
    } else if (/\.(bundle|resources)$/.test(entry)) resources.push(from);
  }
  if (existsSync(resolve(bin, 'PackageFrameworks'))) {
    imports.push('-F', resolve(bin, 'PackageFrameworks'));
    for (const entry of readdirSync(resolve(bin, 'PackageFrameworks'))) {
      const placed = resolve(bin, 'PackageFrameworks', entry), name = basename(entry, '.framework');
      if (entry.endsWith('.framework') && machOType(resolve(placed, name)) === 6) frameworks.push({ placed, link: name, swiftPackage: true });
    }
  }
  const privacyRoots = [bin];
  for (let i = 0; i < imports.length; i++) {
    if (['-F', '-Fsystem'].includes(imports[i]) && existsSync(imports[i + 1])) privacyRoots.push(imports[++i]);
  }
  const privacyManifests = swiftPrivacyManifests([...new Set(privacyRoots)]);
  for (const library of libraries) library.privacyManifests = privacyManifests;
  const paths = swiftRunTool(['otool', '-l', output]);
  for (const path of ['@loader_path/../Frameworks', '@loader_path/Frameworks']) {
    if (!paths.includes(`path ${path} (offset`)) swiftRunTool(['install_name_tool', '-add_rpath', path, output]);
  }
  if (wrapped) {
    for (const lib of libraries) swiftRunTool(['install_name_tool', '-id', `@rpath/${lib.framework}.framework/${lib.framework}`, lib.built]);
    for (const file of libraries.map(lib => lib.built)) for (const lib of libraries) {
      swiftRunTool(['install_name_tool', '-change', `@rpath/${lib.load}`, `@rpath/${lib.framework}.framework/${lib.framework}`, file]);
    }
  }
  const hash = createHash('sha256').update(locked ?? '');
  for (const lib of libraries) hash.update(lib.load).update(readFileSync(lib.built));
  for (const fw of frameworks) hash.update(fw.link).update(readFileSync(resolve(fw.placed, fw.link)));
  const digest = hash.digest('hex');
  const consumerLink = [...libraries.flatMap(lib => ['-L', dirname(lib.built), `-l${lib.load.slice(3, -6)}`]),
    ...frameworks.flatMap(fw => ['-F', dirname(fw.placed), '-framework', fw.link])];
  if (frameworks.length && sdkName === 'macosx') consumerLink.push('-Xlinker', '-rpath', '-Xlinker', '@loader_path/../Frameworks');
  return { bin, imports, link: consumerLink, libraries, frameworks, resources, digest };
}
function swiftRunTool(args) {
  const result = spawnSync('xcrun', args, { encoding: 'utf8' });
  if (result.status !== 0) throw new Error(`Swift package link failed: ${result.stderr ?? result.error?.message}`);
  return result.stdout;
}
