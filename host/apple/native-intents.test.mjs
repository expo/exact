import { test, expect } from 'bun:test';
import { mkdtempSync, readFileSync, writeFileSync, rmSync, mkdirSync, existsSync, readdirSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { intentsMetadata as emit } from './intents-metadata.mjs';
import { buildIntents, nativeIntentModule, intentsMetadata, mergeIntentsMetadata, launchIntentModule } from './intents.mjs';
import { moduleLibraryLinkArgs } from './compose.mjs';

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
