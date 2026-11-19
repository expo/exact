import { createHash } from 'node:crypto';
import { copyFileSync, cpSync, existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { basename, dirname, isAbsolute, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { directDependencies } from '../../scripts/package-libraries.mjs';
import { spawnSync, swiftOptimization } from './toolchain.mjs';
import { evictArms, probeDigest } from './arms.mjs';
import { swiftPackagesProblems } from '../../scripts/app.mjs';
import { appleImplementations, moduleInterfaces } from '../../js/bake/src/typescript.mjs';
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

/** The initial library contains metadata and factories, without importing a package's implementation. */
export function composedModule(packages, ownModule) {
  const authenticator = packages.find(p => p.authenticator);
  const lines = ['// Written by host/apple/build.mjs; never edited.'];
  lines.push(`let exactAuthenticator: String? = ${authenticator ? JSON.stringify(authenticator.extension) : 'nil'}`);
  lines.push(`let exactStartAtLoad: [String] = [${packages.filter(p => p.start).map(p => JSON.stringify(p.extension)).join(', ')}]`);
  const members = packages.flatMap(p => (p.credentialMembers ?? []).map(name => `${JSON.stringify(name)}: ${JSON.stringify(p.extension)}`));
  lines.push(`let exactCredentialMembers: [String: String] = [${members.join(', ') || ':'}]`);
  lines.push(`let exactExtensions: [String: (ExactModule, String) -> ExactExtensionAdapter] = [${packages.map(p => `${JSON.stringify(p.extension)}: { ExactPackageLibrary.adapter($0, ${JSON.stringify(p.extension)}, module: ${JSON.stringify(p.module)}, session: $1) }`).join(', ') || ':'}]`);
  lines.push(`let exactPackageModules: [String: String] = [${packages.map(p => `${JSON.stringify(p.extension)}: ${JSON.stringify(p.module)}`).join(', ') || ':'}]`);
  const views = packages.flatMap(p => (p.views ?? []).map(tag => `${JSON.stringify(tag)}: ExactNativeFactory(package: { try ExactExtensions.view($0, ${JSON.stringify(p.extension)}, ${JSON.stringify(tag)}, id: $1, props: $2, events: $3) })`));
  lines.push(`let exactPackageViews: [String: ExactNativeFactory] = [${views.join(', ') || ':'}]`);
  if (!ownModule) lines.push('final class ExactComposedModule: ExactModule {}', 'let exactModule: ExactModule.Type = ExactComposedModule.self');
  return lines.join('\n') + '\n';
}

/** Compile each package against the seam's link stub, then link its independent library.
 * The final app library supplies the seam; the stub permits app Swift to import a package
 * without a build-time link cycle (LLP 1067.000.001 D1, D5). */
export function packageCompiler({ packages, packageDir, packageTargets, packageCopies, seam, packagesRoot, swiftc, production, distribution, ios, tv, minimum, app, extensions }) {
  const graph = app ? appSwiftPackageGraph(app, packages, extensions) : swiftPackageGraph(app, packages);
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
    const declarations = swiftPackageGraph({ dir: app?.dir, manifest: { host: { [os]: app?.manifest.host?.[os] } } }, packages);
    const product = declarations.length ? buildSwiftDependencies({ app, graph, declarations,
      compile: flags(dir).slice(3), sdkName: packageTargets.get(dir)[0], directory: dir, wrapped }) : null;
    if (product) {
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
    const seamObject = objectArgs('ExactPackage', [seam], dir, null);
    const stub = [...flags(dir), '-module-name', 'ExactPackage', '-emit-module',
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
      const object = [...objectArgs(p.module, sources, dir), ...sdkImports, ...(!production && !distribution ? ['-D', 'EXACT_DEVELOPMENT'] : [])];
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

const swiftIdentity = url => new URL(url).pathname.replace(/\/+$/, '').split('/').at(-1).replace(/\.git$/i, '').toLowerCase();
const swiftURL = url => url.replace(/\/+$/, '').replace(/\.git$/i, '');

/** One graph for both Apple destinations and the app's own Swift (LLP 1133.001 D9, amended for #751). */
export function swiftPackageGraph(app, bound = []) {
  const declarations = ['ios', 'macos'].map(os => ({ source: `${resolve(app?.dir ?? '.', 'app.json')} host.${os}.swiftPackages`, values: app?.manifest?.host?.[os]?.swiftPackages }));
  for (const p of bound) declarations.push({ source: `${p.swiftPackagesSource ?? p.extension} swiftPackages`, values: p.swiftPackages });
  const graph = new Map();
  for (const { source, values } of declarations) {
    const problems = swiftPackagesProblems(values, source);
    if (problems.length) throw new Error(problems.join('\n'));
    for (const value of values ?? []) {
      const identity = swiftIdentity(value.url), previous = graph.get(identity);
      if (previous && (previous.exactVersion !== value.exactVersion || swiftURL(previous.url) !== swiftURL(value.url))) {
        throw new Error(`Swift package conflict: ${previous.source} requests ${previous.url} at ${previous.exactVersion}; ${source} requests ${value.url} at ${value.exactVersion}. Use one URL and version of ${identity}.`);
      }
      graph.set(identity, { ...value, identity, source, products: [...new Set([...(previous?.products ?? []), ...value.products])].sort() });
    }
  }
  return [...graph.values()].sort((a, b) => a.identity.localeCompare(b.identity));
}

/** Bind only interfaces in the compiled plan, including views reached through Contract libraries. */
export function appSwiftPackageGraph(app, bound = [], extensions) {
  if (extensions && !swiftPackageGraph(app, bound).length) return [];
  extensions ??= swiftLockExtensions(app);
  return swiftPackageGraph(app, [...bound, ...['ios', 'macos', 'tvos'].flatMap(os => appleImplementations(app.dir, os, extensions))]);
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
  let lock;
  try { lock = JSON.parse(readFileSync(resolve(app.dir, 'Package.resolved'), 'utf8')); }
  catch { throw new Error(`Swift packages need a lock; ${lockAdvice(app)}`); }
  if (![2, 3].includes(lock.version) || !Array.isArray(lock.pins)) throw new Error(`Invalid Swift package lock; ${lockAdvice(app)}`);
  for (const p of graph) {
    const pin = lock.pins.find(pin => pin.identity === p.identity);
    if (!pin || pin.state?.version !== p.exactVersion || swiftURL(pin.location ?? '') !== swiftURL(p.url) || !/^[0-9a-f]{40}$/.test(pin.state?.revision ?? '')) {
      throw new Error(`${p.source} requests ${p.url} at ${p.exactVersion}, which differs from the lock; ${lockAdvice(app)}`);
    }
  }
  return readFileSync(resolve(app.dir, 'Package.resolved'));
}
const products = declarations => declarations.flatMap(p => p.products.map(name => `.product(name: ${JSON.stringify(name)}, package: ${JSON.stringify(swiftIdentity(p.url))})`));
function swiftManifest(graph, name, declarations, compile = [], link = []) {
  const triple = compile[compile.indexOf('-target') + 1] ?? '';
  const os = /-apple-(macosx?|ios|tvos)([\d.]+)/.exec(triple);
  const floors = { macos: '14.0', ios: '17.0', tvos: '17.0' };
  if (os) floors[os[1].startsWith('macos') ? 'macos' : os[1]] = os[2].includes('.') ? os[2] : `${os[2]}.0`;
  return `// swift-tools-version:5.9
import PackageDescription
let package = Package(name: "ExactSwiftProducts", platforms: [.macOS("${floors.macos}"), .iOS("${floors.ios}"), .tvOS("${floors.tvos}")],
  products: [.library(name: ${JSON.stringify(name)}, type: .dynamic, targets: [${JSON.stringify(name)}])],
  dependencies: [${graph.map(p => `.package(url: ${JSON.stringify(p.url)}, exact: ${JSON.stringify(p.exactVersion)})`).join(', ')}],
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
  const graph = appSwiftPackageGraph(app);
  if (!graph.length) return null;
  const dir = resolve(swiftRoot(app), 'resolve');
  mkdirSync(resolve(dir, 'Sources'), { recursive: true });
  writeFileSync(resolve(dir, 'Sources/Resolve.swift'), 'public enum ExactResolve {}\n');
  writeFileSync(resolve(dir, 'Package.swift'), swiftManifest(graph, 'ExactResolve', graph));
  const lock = resolve(app.dir, 'Package.resolved'), generated = resolve(dir, 'Package.resolved');
  if (existsSync(lock)) copyFileSync(lock, generated);
  swiftRun(['package', 'resolve'], dir);
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
function buildSwiftDependencies({ app, graph, declarations, compile, sdkName, directory, wrapped }) {
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
  writeFileSync(resolve(dir, 'Package.resolved'), locked);
  const at = compile.indexOf('-target'), sdkAt = compile.indexOf('-sdk');
  // The shared product supplies every consumer with the same SDK artifacts and Clang module maps.
  const scratch = resolve(swiftRoot(app), 'build', key);
  const args = ['build', '--build-system', 'native', '--scratch-path', scratch, '--disable-build-manifest-caching', '--disable-automatic-resolution', '--resolver-fingerprint-checking', 'strict', '-c', 'release',
    ...(at >= 0 ? ['--triple', compile[at + 1]] : []), ...(sdkAt >= 0 ? ['--sdk', compile[sdkAt + 1]] : []), '--product', name];
  const bin = swiftRun([...args, '--show-bin-path'], dir).trim(), installAt = link.indexOf('-install_name');
  try {
    swiftRun([...args, '--print-manifest-job-graph'], dir);
    const plan = JSON.parse(readFileSync(resolve(bin, 'description.json'), 'utf8'));
    const inputs = new Set(Object.values(plan.swiftCommands).find(c => c.moduleName === name)?.inputs?.map(input => input.name));
    const staticLibraries = Object.values(plan.copyCommands ?? {}).filter(copy => copy.outputs.some(output => inputs.has(output.name)))
      .flatMap(copy => copy.inputs.map(input => input.name)).flatMap(path => path.endsWith('.framework')
        ? [resolve(path, basename(path, '.framework'))] : path.endsWith('.a') ? [path] : [])
      .filter(path => machOType(path) !== 6);
    // The empty shared target references no SDK symbols. Keep every archive member for its consumers.
    if (staticLibraries.length) writeFileSync(resolve(dir, 'Package.swift'), swiftManifest(graph, name, declarations, compile,
      [...link, ...[...new Set(staticLibraries)].flatMap(path => ['-Xlinker', '-force_load', '-Xlinker', path])]));
    swiftRun([...args, ...swiftResourceOverlay(bin)], dir);
  }
  catch (error) { throw new Error(`${error.message}\n${lockAdvice(app)}`); }
  if (!readFileSync(resolve(dir, 'Package.resolved')).equals(locked)) throw new Error(`SwiftPM changed the build's lock; ${lockAdvice(app)}`);
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
  const hash = createHash('sha256').update(locked);
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
