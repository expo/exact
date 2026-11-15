import { createHash } from 'node:crypto';
import { copyFileSync, cpSync, existsSync, mkdirSync, readFileSync, readdirSync, renameSync, rmSync, utimesSync, writeFileSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { spawnSync } from './toolchain.mjs';
import { evictArms, probeDigest } from './arms.mjs';
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
export function rosterProbe({ library, launcher, built, packagesIn, packages, placePackages, frameworks }) {
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
export function packageCompiler({ packages, packageDir, packageTargets, packageCopies, seam, packagesRoot, swiftc, production, distribution, ios, tv, minimum }) {
  const placed = new Set();
  const run = args => {
    const result = spawnSync('xcrun', args, { encoding: 'utf8' });
    if (result.status !== 0) throw new Error(`host/apple: package compilation failed:\n${result.stderr ?? result.error ?? ''}`);
  };
  const flags = dir => ['--sdk', packageTargets.get(dir)[0], 'swiftc',
    '-module-cache-path', resolve(packagesRoot, 'modules-module-cache'),
    '-parse-as-library', '-O', '-swift-version', '5', ...packageTargets.get(dir)[1]];
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
    if (placed.has(dir) || !packages.length) return;
    mkdirSync(dir, { recursive: true });
    const wrapped = dir === packageDir && ios && production;
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
      const object = [...objectArgs(p.module, sources, dir), ...(!production && !distribution ? ['-D', 'EXACT_DEVELOPMENT'] : [])];
      const os = dir === packageDir ? (ios ? tv ? 'tvos' : 'ios' : 'macos')
        : packageTargets.get(dir)[1].some(arg => arg.includes('-apple-ios')) ? 'ios' : 'macos';
      const floor = minimum && typeof minimum === 'object' ? minimum[os] : minimum;
      const link = [...flags(dir), '-emit-library', resolve(dir, `${p.module}.o`),
        '-L', dir, '-lexact_modules', ...systemFrameworks([p], os, floor),
        ...moduleLibraryLinkArgs({ wrapped, module: p.module }), '-o', resolve(dir, `${p.module}.dylib`)];
      kept(p.module, [...object, ...link], sources, [seamKey], dir, () => {
        run(object);
        run(link);
      });
      copyFileSync(resolve(dir, `${p.module}.dylib`), resolve(dir, `lib${p.module}.dylib`));
    });
    placed.add(dir);
  };
  return { place, libraries };
}
