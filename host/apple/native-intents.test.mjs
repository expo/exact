import { test, expect } from 'bun:test';
import { cpSync, mkdtempSync, readFileSync, writeFileSync, rmSync, mkdirSync, existsSync, readdirSync, renameSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { intentsMetadata as emit } from './intents-metadata.mjs';
import { buildIntents, nativeIntentModule, intentsMetadata, mergeIntentsMetadata, launchIntentModule } from './intents.mjs';
import { lockSwiftPackages, moduleLibraryLinkArgs } from './compose.mjs';
import { resolveApp } from '../../scripts/app.mjs';
import { appleArtifacts } from './build.mjs';
import { useXcode } from './devices.mjs';
import { readPrivacyManifest } from './privacy.mjs';
import { plistFile } from './assets.mjs';

const fixture = name => readFileSync(new URL(`./fixtures/native-intents.${name}`, import.meta.url), 'utf8');
const constants = () => JSON.parse(fixture('swiftconstvalues'));
const expected = () => JSON.parse(fixture('actionsdata'));
const canonical = data => {
  if (Array.isArray(data)) return data.map(canonical);
  if (data == null || typeof data !== 'object') return data;
  const out = Object.fromEntries(Object.entries(data).filter(([key]) => key !== 'generator').map(([key, value]) => [key, canonical(value)]));
  if (out.resolvableInputTypes) out.resolvableInputTypes.sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
  return out;
};

test.skipIf(process.platform !== 'darwin')('app Swift products reach the roster probe and App Intents typecheck', () => {
  useXcode();
  const dir = resolve(import.meta.dir, '../../target/swift-app-build');
  mkdirSync(resolve(dir, 'modules/apple'), { recursive: true });
  const swiftPackages = [
    { url: 'https://github.com/apple/swift-collections.git', exactVersion: '1.1.4', products: ['DequeModule'] },
    { url: 'https://github.com/ReactiveX/RxSwift.git', exactVersion: '6.9.0', products: ['RxCocoa-Dynamic'] },
  ];
  const implementation = resolve(dir, 'node_modules/@fixture/reactive');
  mkdirSync(resolve(implementation, 'apple'), { recursive: true });
  writeFileSync(resolve(dir, 'package.json'), JSON.stringify({ dependencies: { '@fixture/reactive': '1.0.0' } }));
  writeFileSync(resolve(implementation, 'package.json'), JSON.stringify({ name: '@fixture/reactive', version: '1.0.0',
    exports: { '.': './exact.json', './apple': './apple/exact.json' } }));
  writeFileSync(resolve(implementation, 'exact.json'), JSON.stringify({ kind: 'interface', name: '@fixture/reactive', names: [], views: ['reactive-view'] }));
  writeFileSync(resolve(implementation, 'apple/exact.json'), JSON.stringify({ kind: 'implementation', implements: '@fixture/reactive@1.0.0',
    platform: 'apple', targets: ['macos', 'ios'], sources: { swift: ['Reactive.swift'] }, class: 'Reactive',
    swiftPackages }));
  const implementationPrivacy = {
    NSPrivacyAccessedAPITypes: [{ NSPrivacyAccessedAPIType: 'NSPrivacyAccessedAPICategoryUserDefaults', NSPrivacyAccessedAPITypeReasons: ['CA92.1'] }],
    NSPrivacyCollectedDataTypes: [{ NSPrivacyCollectedDataType: 'NSPrivacyCollectedDataTypeOtherDiagnosticData', NSPrivacyCollectedDataTypeLinked: false,
      NSPrivacyCollectedDataTypeTracking: false, NSPrivacyCollectedDataTypePurposes: ['NSPrivacyCollectedDataTypePurposeAppFunctionality'] }],
  };
  writeFileSync(resolve(implementation, 'apple/PrivacyInfo.xcprivacy'), plistFile(implementationPrivacy));
  writeFileSync(resolve(implementation, 'apple/Reactive.swift'), `import Foundation
import ExactPackage
import RxCocoa
import RxSwift
public final class Reactive: ExactPackage {
    public init(context: ExactPackageContext) {}
    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) { reply([:]) }
    public func call(_ name: String, _ args: [String: Any]) throws -> [String: Any] { [:] }
    public static var count: Int {
        UserDefaults.standard.set(3, forKey: "exact-reactive-fixture")
        return UserDefaults.standard.integer(forKey: "exact-reactive-fixture")
    }
    public static var configured: Bool {
        get { Hooks.recordCallStackOnError }
        set { Hooks.recordCallStackOnError = newValue }
    }
}
`);
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Swift Products', app: { id: 'com.exact.swiftproducts', name: 'Swift Products' },
    rust: false, modules: ['exact-probe'], host: { macos: { minimumOS: '15.0', swiftPackages }, ios: { minimumOS: '18.0', swiftPackages } },
    deploy: { store: { macos: '0', ios: '0' } } }));
  writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    reactive-view\n');
  writeFileSync(resolve(dir, 'app.ts'), 'export const grants = "";\nexport const answer = () => { throw new Error("No sources"); };\n');
  writeFileSync(resolve(dir, 'modules/apple/Own.swift'), `import AppIntents
import ExactPkg_reactive
#if SWIFT_PACKAGE
import DequeModule
import RxSwift
#endif
final class ProbeModule: ExactModule {
    override class var views: [String: ExactNativeFactory] {
        precondition(Reactive.count == 3)
#if SWIFT_PACKAGE
        precondition(Deque([1, 2, 3]).count == Reactive.count)
        Hooks.recordCallStackOnError = true
        precondition(Reactive.configured, "app configuration must reach the implementation")
        Reactive.configured = false
        precondition(!Hooks.recordCallStackOnError, "implementation configuration must reach app Swift")
#endif
        let resources = Bundle(url: Bundle.main.resourceURL!.appendingPathComponent("RxSwift_RxSwift.bundle"))
        precondition(resources?.url(forResource: "PrivacyInfo", withExtension: "xcprivacy") != nil, "SwiftPM resources missing from roster probe")
        return ["exact-probe": ExactNativeFactory(firstFrame: true) { _, _ in throw ExactNativeRefusal("probe only") }]
    }
}
let exactModule: ExactModule.Type = ProbeModule.self
struct ProductIntent: AppIntent {
    static let title: LocalizedStringResource = "Product intent"
    func perform() async throws -> some IntentResult { .result() }
}
`);
  const app = resolveApp(dir);
  lockSwiftPackages(app);
  const locked = readFileSync(resolve(dir, 'Package.resolved'));
  for (const [destination, selection, trust] of [
    ['macos', 'none', 'development'], ['macos', 'all', 'development'],
    ['ios-simulator', 'all', 'development'], ['ios-simulator', 'all', 'production'],
  ]) {
    const manifest = structuredClone(app.manifest);
    if (selection === 'none') for (const os of ['macos', 'ios']) delete manifest.host[os].swiftPackages;
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify(manifest));
    const result = spawnSync(process.execPath, [resolve(import.meta.dir, 'build.mjs'),
      ...(destination === 'macos' ? ['--bundle'] : ['--ios', '--no-install']), '--extensions', selection], {
      encoding: 'utf8', timeout: 600000, env: { ...process.env, EXACT_APP_DIR: dir, EXACT_IDENTITY: '-', EXACT_UPDATE_TRUST: trust, EXACT_UPDATE_GENESIS: '1' },
    });
    const log = `${result.stdout ?? ''}${result.stderr ?? ''}`;
    writeFileSync(resolve(dir, `${destination}-${selection}-${trust}.log`), log);
    expect(result.status, log).toBe(0);
    expect(log).not.toContain('cannot qualify');
    const paths = appleArtifacts(app, { destination, trust });
    if (trust === 'production') {
      const signature = spawnSync('codesign', ['--verify', '--deep', '--strict', paths.bundle], { encoding: 'utf8' });
      expect(signature.status, signature.stderr).toBe(0);
      for (const name of ['ExactModules', 'ExactPkg_reactive', 'ExactSwiftDependencies']) {
        const binary = resolve(paths.bundle, 'Frameworks', `${name}.framework`, name);
        const linked = spawnSync('xcrun', ['otool', '-L', binary], { encoding: 'utf8' });
        expect(linked.status, linked.stderr).toBe(0);
        expect(linked.stdout).toContain('@rpath/RxCocoa-Dynamic.framework/RxCocoa-Dynamic');
        expect(linked.stdout).not.toContain('@rpath/libRxCocoa-Dynamic.dylib');
      }
      for (const name of ['ExactSwiftDependencies', 'RxCocoa-Dynamic']) {
        const privacy = readPrivacyManifest(resolve(paths.bundle, 'Frameworks', `${name}.framework/PrivacyInfo.xcprivacy`));
        expect(privacy.NSPrivacyTracking).toBe(false);
        expect(privacy.NSPrivacyCollectedDataTypes).toEqual(implementationPrivacy.NSPrivacyCollectedDataTypes);
      }
      const privacy = readPrivacyManifest(resolve(paths.bundle, 'Frameworks/ExactPkg_reactive.framework/PrivacyInfo.xcprivacy'));
      expect(privacy.NSPrivacyAccessedAPITypes).toEqual(implementationPrivacy.NSPrivacyAccessedAPITypes);
    }
    const resources = destination === 'macos' ? resolve(paths.bundle, 'Contents/Resources') : paths.bundle;
    if (trust !== 'production') {
      const privacy = readPrivacyManifest(resolve(resources, 'PrivacyInfo.xcprivacy'));
      expect(privacy.NSPrivacyTracking).toBe(false);
      expect(privacy.NSPrivacyCollectedDataTypes).toEqual(implementationPrivacy.NSPrivacyCollectedDataTypes);
    }
    const plist = spawnSync('plutil', ['-convert', 'json', '-o', '-', resolve(paths.bundle, destination === 'macos' ? 'Contents/Info.plist' : 'Info.plist')], { encoding: 'utf8' });
    expect(plist.status, plist.stderr).toBe(0);
    expect(JSON.parse(plist.stdout).ExactFirstFrameModules).toEqual(['exact-probe']);
    const metadata = resolve(resources, 'Metadata.appintents/extract.actionsdata');
    if (selection === 'all') expect(JSON.parse(readFileSync(metadata, 'utf8')).actions.ProductIntent).toBeDefined();
    else expect(existsSync(metadata)).toBe(false);
    expect(readFileSync(resolve(dir, 'Package.resolved')).equals(locked)).toBe(true);
    if (destination === 'macos') {
      const packages = resolve(app.target, 'swift-packages');
      const accessor = resolve(packages, readdirSync(packages, { recursive: true }).find(file =>
        file.includes('/build/') && file.endsWith('apple-macosx/release/RxSwift.build/DerivedSources/resource_bundle_accessor.swift')));
      const buildPath = JSON.parse(readFileSync(accessor, 'utf8').match(/let buildPath = (".*")/)[1]);
      const description = JSON.parse(readFileSync(resolve(accessor, '../../../description.json'), 'utf8'));
      const args = Object.values(description.swiftCommands).find(command => command.moduleName === 'RxSwift').otherArguments;
      const overlays = args.flatMap((arg, i) => arg === '-vfsoverlay' ? [arg, args[i + 1]] : []);
      const relocated = mkdtempSync(resolve(dir, 'relocated-')), bundle = resolve(relocated, 'Products.app');
      try {
        cpSync(paths.bundle, bundle, { recursive: true });
        const binary = resolve(bundle, 'Contents/MacOS', paths.executable), main = resolve(relocated, 'main.swift');
        writeFileSync(main, `import Foundation
let resource = Bundle.module.url(forResource: "PrivacyInfo", withExtension: "xcprivacy")!
precondition(!((try! Data(contentsOf: resource)).isEmpty))
print(resource.path)
`);
        rmSync(binary);
        // The SDK's accessor is internal; compile it with its build settings to exercise Bundle.module.
        const compiled = spawnSync('xcrun', ['swiftc', ...overlays, accessor, main, '-o', binary], { encoding: 'utf8' });
        expect(compiled.status, compiled.stderr).toBe(0);
        const signed = spawnSync('codesign', ['--force', '--sign', '-', '--timestamp=none', bundle], { encoding: 'utf8' });
        expect(signed.status, signed.stderr).toBe(0);
        const verified = spawnSync('codesign', ['--verify', '--deep', '--strict', bundle], { encoding: 'utf8' });
        expect(verified.status, verified.stderr).toBe(0);
        renameSync(buildPath, `${buildPath}-unavailable`);
        try {
          const read = spawnSync(binary, [], { encoding: 'utf8' });
          expect(read.status, read.stderr).toBe(0);
          expect(read.stdout.trim()).toBe(resolve(bundle, 'Contents/Resources/RxSwift_RxSwift.bundle/PrivacyInfo.xcprivacy'));
        } finally { renameSync(`${buildPath}-unavailable`, buildPath); }
      } finally { rmSync(relocated, { recursive: true, force: true }); }
    }
  }
}, 900000);

test('native intent properties and availability match the Xcode fixture', () => {
  const output = emit(constants(), 27, { module: 'ExactAppModules' });
  expect(canonical(output)).toEqual(canonical(expected()));
  const screenshot = output.actions.NativeAction.parameters[0];
  expect(screenshot.valueType).toEqual({ intents: { wrapper: { typeIdentifier: 12 } } });
  expect(screenshot.typeSpecificMetadata).toEqual(['LNValueTypeMetadataKeyFileSupportedTypes', { array: { elements: [{ string: { wrapper: 'public.image' } }] } }]);
});

test('a native-only shortcuts provider keeps its identity, and a second provider is refused', () => {
  const native = expected();
  const merged = mergeIntentsMetadata(emit([], 27), [native]);
  expect(merged.autoShortcutProviderMangledName).toBe(native.autoShortcutProviderMangledName);
  expect(merged.autoShortcuts).toEqual(native.autoShortcuts);
  const second = { ...native, autoShortcutProviderMangledName: 'AnotherProvider', actions: {} };
  expect(() => mergeIntentsMetadata(structuredClone(native), [second])).toThrow('more than one shortcuts provider');
});

test('native intent behavior the portable emitter cannot express is refused', () => {
  for (const label of ['openAppWhenRun', 'isDiscoverable', 'supportedModes', 'parameterSummary']) {
    const types = constants(), action = types[0];
    action.properties = action.properties.filter(p => p.label !== label);
    action.properties.push({ label, isStatic: 'true', valueKind: 'Runtime' });
    expect(() => emit(types, 27, { module: 'ExactAppModules' })).toThrow(label);
  }
  for (const fallback of [{ valueKind: 'Runtime' }, { valueKind: 'RawLiteral', value: '9007199254740993' }]) {
    const types = constants(), count = types.find(t => t.typeName.endsWith('.ShortcutAction')).properties.find(p => p.label === 'count');
    Object.assign(count.value.arguments.find(a => a.label === 'default'), fallback);
    expect(() => emit(types, 27, { module: 'ExactAppModules' })).toThrow('default for count');
  }
  for (const attribute of [{ renamed: 'OtherAction' }, { isUnavailable: true }, { isDeprecated: true }]) {
    const types = constants(); Object.assign(types[0].availabilityAttributes[0], attribute);
    expect(() => emit(types, 27, { module: 'ExactAppModules' })).toThrow('availability');
  }
});

for (const platform of ['macos', 'ios']) {
test.skipIf(process.platform !== 'darwin' || (platform === 'ios' && !process.env.EXACT_SIM))(`native declarations match Xcode extraction and resolve in a fresh process before any view (${platform})`, () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-native-launch-'));
  const runApple = (command, args) => {
    const r = spawnSync(command, args, { encoding: 'utf8', timeout: 90000, env: { ...process.env, TMPDIR: dir } });
    if (r.status !== 0) throw new Error(`${command}: ${r.stderr}\n${r.stdout}`);
    return r.stdout.trim();
  };
  try {
    const file = resolve(dir, 'Native.swift'), binary = resolve(dir, platform === 'ios' ? 'Frameworks/libexact_modules.dylib' : 'libexact_modules.dylib');
    if (platform === 'ios') mkdirSync(resolve(dir, 'Frameworks'));
    writeFileSync(file, fixture('swift'));
    const sdkName = platform === 'ios' ? 'iphonesimulator' : 'macosx';
    const sdk = runApple('xcrun', ['--sdk', sdkName, '--show-sdk-path']);
    const triple = `${process.arch === 'arm64' ? 'arm64' : 'x86_64'}-apple-${platform === 'ios' ? 'ios18.0-simulator' : 'macos15.0'}`;
    const deployment = platform === 'ios' ? '18.0' : '15.0';
    const args = ['--sdk', sdkName, 'swiftc', '-parse-as-library', '-emit-library', '-module-name', 'ExactAppModules',
      '-sdk', sdk, '-target', triple, ...moduleLibraryLinkArgs(), file, '-o', binary];
    runApple('xcrun', args);
    const native = nativeIntentModule({ moduleName: 'ExactAppModules', binary, sources: [file], args,
      directory: resolve(dir, 'constants'), swiftc: 'test', runApple });
    const actual = JSON.parse(readFileSync(native.values, 'utf8'));
    const metadata = intentsMetadata({ ...native, dir, sdk, triple, platform, deployment, runApple });
    const extracted = JSON.parse(readFileSync(resolve(metadata, 'extract.actionsdata'), 'utf8'));
    expect(canonical(extracted)).toEqual(canonical(expected()));
    const built = buildIntents({ app: { manifest: {} }, compat: {}, nativeModules: [native], directory: resolve(dir, 'built'),
      sdkName, sdk, triple, platform, deployment, swiftc: 'test', runApple });
    const probe = resolve(dir, 'Probe.swift'), executable = resolve(dir, 'probe');
    writeFileSync(probe, `import AppIntents
@main struct Probe {
    static func main() async throws {
        guard let type = _typeByName("${extracted.actions.ShortcutAction.mangledTypeName}") as? any AppIntent.Type else {
            fatalError("native intent is not mapped at launch")
        }
        _ = try await type.init().perform()
        print("native intent performed without opening a view")
    }
}
`);
    runApple('xcrun', ['swiftc', '-parse-as-library', '-sdk', sdk, '-target', triple, probe, ...(built.launchLink ?? []), '-o', executable]);
    const ran = spawnSync(platform === 'ios' ? 'xcrun' : executable, platform === 'ios' ? ['simctl', 'spawn', process.env.EXACT_SIM, executable] : [], { encoding: 'utf8', env: { ...process.env, TMPDIR: dir } });
    expect(ran.stderr).toBe('');
    expect(ran.status).toBe(0);
    expect(ran.stdout.trim()).toBe('native intent performed without opening a view');
    expect(canonical(emit(actual, 27, { module: 'ExactAppModules' }))).toEqual(canonical(extracted));
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 120000);
}

// A full Apple build supplies the framework, C module maps, link and bundle assembly.
test.skipIf(process.platform !== 'darwin' || !process.env.EXACT_SIM)('a widget app extracts launch intents before linking, retains their types and honors extension selection', async () => {
  const { resolveApp } = await import('../../scripts/app.mjs');
  const { appleArtifacts } = await import('./build.mjs');
  const directory = resolve(import.meta.dir, '../../target/native-intents-build');
  const fixtureDirectory = resolve(import.meta.dir, 'fixtures/native-intents-app');
  mkdirSync(directory, { recursive: true });
  for (const name of readdirSync(fixtureDirectory, { recursive: true })) {
    const from = resolve(fixtureDirectory, name), to = resolve(directory, name);
    if (!name.endsWith('.swift') && !name.endsWith('.json') && !name.endsWith('.contract') && !name.endsWith('.ts')) continue;
    mkdirSync(resolve(to, '..'), { recursive: true });
    const bytes = readFileSync(from);
    if (!existsSync(to) || !bytes.equals(readFileSync(to))) writeFileSync(to, bytes);
  }
  const tools = resolve(directory, 'test-tools'), extractionLog = resolve(directory, 'extractions.log');
  mkdirSync(tools, { recursive: true });
  writeFileSync(resolve(tools, 'xcrun'), `#!/bin/sh
case " $* " in
  *" -emit-const-values-path "*) printf '%s\\n' "$*" >> "$EXACT_CONSTANT_EXTRACTIONS" ;;
esac
exec /usr/bin/xcrun "$@"
`, { mode: 0o755 });
  const run = (command, args, extra = {}) => {
    const result = spawnSync(command, args, { encoding: 'utf8', timeout: 600000,
      env: { ...process.env, EXACT_APP_DIR: directory, PATH: `${tools}:${process.env.PATH}`, EXACT_CONSTANT_EXTRACTIONS: extractionLog, ...extra } });
    if (result.status !== 0) throw new Error(`${command} ${args.join(' ')}: ${result.error?.message ?? ''}\n${result.stdout}\n${result.stderr}`);
    return result.stdout;
  };
  const app = resolveApp(directory);
  try {
    for (const production of [false, true]) for (const selection of ['all', 'none', 'widgets']) {
      const trust = production ? 'production' : 'development';
      const paths = appleArtifacts(app, { destination: 'ios-simulator', trust, optimize: production ? 'size' : 'speed' });
      const resources = paths.bundle, executable = resolve(resources, paths.executable);
      // Changing both declarations invalidates constants without discarding the populated cache.
      for (const relative of ['modules/apple/Intent.swift', 'modules/intentprobe/apple/launch/Intent.swift']) {
        const file = resolve(directory, relative);
        writeFileSync(file, readFileSync(resolve(fixtureDirectory, relative), 'utf8') + `\n// ${process.pid} ${trust} ${selection}\n`);
      }
      writeFileSync(extractionLog, '');
      const result = run(process.execPath, [resolve(import.meta.dir, 'build.mjs'), '--ios', '--sim', process.env.EXACT_SIM, '--no-install', '--extensions', selection,
        ...(production ? ['--optimize', 'size', '--host'] : [])], { EXACT_UPDATE_TRUST: trust, EXACT_UPDATE_GENESIS: '1' });
      writeFileSync(resolve(directory, `build-${trust}-${selection}.log`), result);
      const metadata = resolve(resources, 'Metadata.appintents/extract.actionsdata');
      if (selection !== 'all') {
        expect(existsSync(metadata)).toBe(false);
        expect(run('xcrun', ['otool', '-L', executable])).not.toContain('@rpath/libexact_modules.dylib');
        // Excluding intents must skip constant extraction too, including after another build populated its cache.
        expect(readFileSync(extractionLog, 'utf8')).toBe('');
        continue;
      }
      expect(readFileSync(extractionLog, 'utf8')).toContain('-module-name ExactAppModules');
      expect(readFileSync(extractionLog, 'utf8')).toContain('-module-name ExactLaunchParts');
      const actions = JSON.parse(readFileSync(metadata, 'utf8')).actions;
      expect(Object.keys(actions).sort()).toEqual([production ? 'ProductionIntent' : 'DevelopmentIntent', 'NativeLaunchIntent', 'NativeScreenshotIntent', 'SaveScreenshot'].sort());
      expect(existsSync(resolve(resources, 'Frameworks/ExactCore.framework'))).toBe(true);
      const pkg = import.meta.dir, scratch = resolve(paths.swift, 'widgets');
      const binDir = readFileSync(resolve(scratch, readdirSync(scratch).find(f => f.startsWith('bin-path-'))), 'utf8');
      const sdk = run('xcrun', ['--sdk', 'iphonesimulator', '--show-sdk-path']).trim(), commands = [];
      rmSync(resolve(directory, 'inspection'), { recursive: true, force: true });
      const launch = launchIntentModule({ pkg, binDir, sourceDirectory: resolve(pkg, '.launch-parts', app.id),
        conditions: ['SWIFT_PACKAGE', ...(!production ? ['DEBUG'] : [])], binary: executable, framework: resolve(resources, 'Frameworks/ExactCore.framework'), sdkName: 'iphonesimulator', sdk,
        triple: 'arm64-apple-ios18.0-simulator', directory: resolve(directory, 'inspection'), swiftc: 'test',
        runApple: (command, args) => { commands.push(args); return run(command, args); } });
      expect(launch.binary.endsWith('.o')).toBe(true);
      expect(commands.length).toBeGreaterThan(0);
      for (const args of commands) {
        expect(args.includes('DEBUG')).toBe(!production);
        expect(args).toContain('SWIFT_PACKAGE');
        expect(args).toContain('-F');
        expect(args).toContain(`-fmodule-map-file=${resolve(pkg, 'Sources/CExact/module.modulemap')}`);
      }
      const constants = JSON.parse(readFileSync(launch.values, 'utf8'));
      expect(constants.some(t => t.typeName === 'ExactLaunchParts.NativeLaunchIntent')).toBe(true);
      const extracted = buildIntents({ app, compat: {}, nativeModules: [launch], directory: resolve(directory, 'inspection-metadata'),
        sdkName: 'iphonesimulator', sdk, triple: 'arm64-apple-ios18.0-simulator', platform: 'ios', deployment: '18.0', swiftc: 'test',
        runApple: (command, args) => run(command, args) });
      expect(extracted.launchLink).toContain(`_exact_launch_intent_type_${actions.NativeLaunchIntent.mangledTypeName}`);
      run('xcrun', ['strip', '-x', executable]);
      run('codesign', ['--force', '--sign', '-', '--preserve-metadata=entitlements', resources]);
      run('xcrun', ['simctl', 'install', process.env.EXACT_SIM, resources]);
      for (const action of Object.values(actions)) {
        const output = run('xcrun', ['simctl', 'launch', '--console', '--terminate-running-process', process.env.EXACT_SIM, app.id],
          { SIMCTL_CHILD_EXACT_PROBE_INTENT: action.mangledTypeName });
        expect(output).toContain(`resolved intent before any view: ${action.mangledTypeName}`);
      }
    }
  } finally { rmSync(resolve(import.meta.dir, '.launch-parts', app.id), { recursive: true, force: true }); }
}, 900000);
