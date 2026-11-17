import { test, expect } from 'bun:test';
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { intentsMetadata as metadataPass, intentsSource, buildIntents, nativeIntentModule } from './intents.mjs';
import { intentsMetadata } from './intents-metadata.mjs';

// Input connections and type-specific metadata are sets and keyed pairs;
// Xcode's extractor emits their order differently between invocations.
function canonical(value) {
  if (Array.isArray(value)) return value.map(canonical);
  if (value == null || typeof value !== 'object') return value;
  const result = Object.fromEntries(Object.entries(value).filter(([k]) => k !== 'generator').map(([k, v]) => [k, canonical(v)]));
  if (result.resolvableInputTypes) result.resolvableInputTypes.sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
  if (result.typeSpecificMetadata) {
    const pairs = [];
    for (let i = 0; i < result.typeSpecificMetadata.length; i += 2) pairs.push(result.typeSpecificMetadata.slice(i, i + 2));
    result.typeSpecificMetadata = pairs.sort(([a], [b]) => a.localeCompare(b)).flat();
  }
  return result;
}

test.skipIf(process.platform !== 'darwin')('portable App Intents metadata matches the native extractor for every emitted parameter and face', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-intents-'));
  const run = (command, args) => {
    // `swiftc` leaves a `TemporaryDirectory.*` in TMPDIR each run: the children's TMPDIR is this test's directory, removed after.
    const r = spawnSync(command, args, { encoding: 'utf8', timeout: 90000, env: { ...process.env, TMPDIR: dir } });
    if (r.status !== 0) throw new Error(`${command}: ${r.error?.message ?? ''}\n${r.stdout}\n${r.stderr}`);
    return r.stdout.trim();
  };
  try {
    const params = [['book', 'string'], ['page', 'number'], ['title', 'string'], ['enabled', 'bool'], ['fraction', 'number'],
      ['aisle', 'enum<"produce","dairy">'], ['note', 'option<string>'], ['count', 'option<number>'], ['favorite', 'option<bool>'], ['rating', 'option<number>'], ['related', 'option<string>'], ['image', 'string'], ['blob', 'string']];
    const fields = { book: { entity: 'book', description: 'The book you read.' }, page: { type: 'integer', description: 'The page you reached.' }, count: { type: 'integer' },
      blob: { type: 'file', accept: ['application/octet-stream'] }, image: { type: 'file', accept: ['image/png', 'image/jpeg', 'image/heic'], description: 'The screenshot.' }, related: { entity: 'book' }, title: { description: 'A “title”.' } };
    const faces = [
      { signature: { name: 'logPages', description: 'Record “reading” progress.', parameters: params, consequence: 'writes' },
       face: { title: 'Log Pages', parameters: fields, apple: { phrases: ['Log ${book} in ${applicationName}'] } } },
      { signature: { name: 'publicAction', description: 'Public action.', parameters: [], consequence: 'writes' }, face: { apple: { authentication: 'always' } } },
      { signature: { name: 'deleteEntry', description: 'Delete an entry.', parameters: [], consequence: 'destroys' }, face: { apple: { authentication: 'local' } } },
    ];
    const file = resolve(dir, 'ExactIntents.swift'), object = resolve(dir, 'ExactIntents.o'), values = resolve(dir, 'ExactIntents.swiftconstvalues');
    writeFileSync(file, intentsSource(faces, { book: { title: 'A Book' } }));
    writeFileSync(resolve(dir, 'protocols.json'), JSON.stringify(['AppIntent', 'AppEntity', 'EntityStringQuery', 'AppShortcutsProvider']));
    const sdk = run('xcrun', ['--sdk', 'iphonesimulator', '--show-sdk-path']);
    const major = parseInt(run('xcrun', ['--sdk', 'iphonesimulator', '--show-sdk-version']));
    const swiftc = run('xcrun', ['--find', 'swiftc']), toolchain = resolve(dirname(swiftc), '../..');
    const triple = 'arm64-apple-ios18.0-simulator';
    const compile = ['-parse-as-library', '-wmo', '-O', '-swift-version', '5', '-module-name', 'ExactIntents', '-target', triple, '-sdk', sdk,
      '-emit-const-values-path', values, '-Xfrontend', '-const-gather-protocols-file', '-Xfrontend', resolve(dir, 'protocols.json'), '-c', file, '-o', object];
    run(swiftc, compile);
    writeFileSync(resolve(dir, 'sources.txt'), file + '\n'); writeFileSync(resolve(dir, 'values.txt'), values + '\n'); writeFileSync(resolve(dir, 'none.txt'), '');
    const version = /Build version (\S+)/.exec(run('xcodebuild', ['-version']))[1];
    const extract = ['--toolchain-dir', toolchain, '--module-name', 'ExactIntents', '--sdk-root', sdk,
      '--xcode-version', version, '--platform-family', 'iOS', '--deployment-target', '18.0', '--target-triple', triple, '--output', dir,
      '--binary-file', object, '--source-file-list', resolve(dir, 'sources.txt'), '--swift-const-vals-list', resolve(dir, 'values.txt'),
      '--metadata-file-list', resolve(dir, 'none.txt'), '--static-metadata-file-list', resolve(dir, 'none.txt'),
      '--compile-time-extraction', '--deployment-aware-processing', '--no-app-shortcuts-localization'];
    run(resolve(toolchain, 'usr/bin/appintentsmetadataprocessor'), extract);
    const constants = JSON.parse(readFileSync(values, 'utf8')), actual = intentsMetadata(constants, major);
    const expected = JSON.parse(readFileSync(resolve(dir, 'Metadata.appintents/extract.actionsdata'), 'utf8'));
    expect(canonical(actual)).toEqual(canonical(expected));
    expect(actual.actions.LogPages.parameters).toHaveLength(params.length);
    const image = actual.actions.LogPages.parameters.find(p => p.name === 'image');
    expect(image.typeSpecificMetadata).toEqual(['LNValueTypeMetadataKeyFileSupportedTypes', { array: { elements:
      ['public.png', 'public.jpeg', 'public.heic'].map(wrapper => ({ string: { wrapper } })) } }]);
    expect(actual.actions.LogPages.parameters.find(p => p.name === 'blob').typeSpecificMetadata).toEqual(['LNValueTypeMetadataKeyFileSupportedTypes', { array: { elements: [{ string: { wrapper: 'public.data' } }] } }]);
    const broken = structuredClone(constants), action = broken.find(t => t.typeName === 'ExactIntents.LogPages');
    action.properties.find(p => p.label === 'page').type = 'Swift.Array<Swift.Int>';
    expect(() => intentsMetadata(broken, major)).toThrow('unsupported parameter type');
    const incomplete = structuredClone(constants), missing = incomplete.find(t => t.typeName === 'ExactIntents.LogPages');
    missing.properties = missing.properties.filter(p => p.label !== 'book');
    expect(() => intentsMetadata(incomplete, major)).toThrow('incomplete parameters');
    expect(() => intentsMetadata(constants, 28)).toThrow('unqualified SDK');
    const withoutPhrases = structuredClone(faces); delete withoutPhrases[0].face.apple.phrases;
    writeFileSync(file, intentsSource(withoutPhrases, { book: { title: 'A Book' } }));
    run(swiftc, compile); run(resolve(toolchain, 'usr/bin/appintentsmetadataprocessor'), extract);
    const noShortcuts = intentsMetadata(JSON.parse(readFileSync(values, 'utf8')), major);
    expect(canonical(noShortcuts)).toEqual(canonical(JSON.parse(readFileSync(resolve(dir, 'Metadata.appintents/extract.actionsdata'), 'utf8'))));
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 120000);

// Linux writes the metadata with the portable emitter and runs no processor.
test.skipIf(process.platform === 'linux')('the metadata pass runs the processor and toolchain of the Xcode xcrun selects', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-intents-tool-'));
  const toolchain = '/Applications/Xcode-beta.app/Contents/Developer/Toolchains/XcodeDefault.xctoolchain';
  const ran = [];
  try {
    expect(() => metadataPass({ moduleName: 'ExactIntents', dir, sdk: '/sdk', triple: 'arm64-apple-ios17.0', platform: 'ios',
      deployment: '17.0', binary: resolve(dir, 'ExactIntents.o'), sources: [], values: resolve(dir, 'values'),
      runApple: (command, args) => ran.push([command, args[args.indexOf('--toolchain-dir') + 1]]),
      find: tool => ({ status: 0, stdout: `${toolchain}/usr/bin/${tool}\n` }) })).toThrow('wrote no');
    expect(ran).toEqual([[`${toolchain}/usr/bin/appintentsmetadataprocessor`, toolchain]]);
    expect(() => metadataPass({ dir, sources: [], runApple: () => {}, find: () => ({ status: 1, stdout: '', stderr: 'xcrun: error: unable to find utility' }) }))
      .toThrow('unable to find utility');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('file faces refuse types that Apple does not declare', () => {
  for (const mime of ['text/markdown', 'audio/ogg']) {
    expect(() => intentsSource([{ signature: { name: 'save', parameters: [['image', 'string']] },
      face: { parameters: { image: { type: 'file', accept: [mime] } } } }])).toThrow('declared');
  }
});

test.skipIf(process.platform !== 'darwin')('perform imports the file before posting JSON arguments through the door', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-perform-'));
  try {
    const file = resolve(dir, 'Perform.swift'), binary = resolve(dir, 'perform');
    writeFileSync(file, intentsSource([{ signature: { name: 'save', description: 'Save a PNG.', parameters: [['image', 'string']], consequence: 'writes' },
      face: { parameters: { image: { type: 'file', accept: ['image/png'] } } } }]) + `
@main struct Probe {
    @MainActor static func main() async throws {
        let target = URL(fileURLWithPath: CommandLine.arguments[1])
        let bytes = Data([137, 80, 78, 71])
        let landing = NotificationCenter.default.addObserver(forName: Notification.Name("exact.tools.pickedPath"), object: nil, queue: nil) { note in
            let body = note.object as! NSMutableDictionary
            body["taken"] = true
            (body["reply"] as! ([String: Any]) -> Void)(["path": "app:/tmp/picked/image.png", "file": target.path])
        }
        let door = NotificationCenter.default.addObserver(forName: Notification.Name("exact.tools.call"), object: nil, queue: nil) { note in
            let body = note.object as! NSMutableDictionary
            body["taken"] = true
            let args = body["args"] as! [String: Any]
            precondition(JSONSerialization.isValidJSONObject(args), "the door must receive JSON")
            precondition(args["image"] as? String == "app:/tmp/picked/image.png")
            precondition((try? Data(contentsOf: target)) == bytes)
            (body["reply"] as! ([String: Any]) -> Void)(["outcome": "ok"])
        }
        defer { NotificationCenter.default.removeObserver(landing); NotificationCenter.default.removeObserver(door) }
        let source = target.deletingLastPathComponent().appendingPathComponent("original.png")
        try FileManager.default.createDirectory(at: source.deletingLastPathComponent(), withIntermediateDirectories: true)
        try bytes.write(to: source)
        for file in [IntentFile(data: bytes, filename: "image.png", type: .png), IntentFile(fileURL: source, type: .png)] {
            try? FileManager.default.removeItem(at: target)
            let intent = Save()
            intent.image = file
            _ = try await intent.perform()
        }
        precondition((try? Data(contentsOf: source)) == bytes)
        for file in [IntentFile(data: Data(count: (64 << 20) + 1), filename: "large.png", type: .png),
                     IntentFile(data: bytes, filename: "wrong.txt", type: .plainText)] {
            try FileManager.default.removeItem(at: target)
            let intent = Save()
            intent.image = file
            do { _ = try await intent.perform(); preconditionFailure("invalid file reached the door") }
            catch { precondition(!FileManager.default.fileExists(atPath: target.path)) }
            try bytes.write(to: target)
        }
    }
}
`);
    const env = { ...process.env, TMPDIR: dir };
    const compiled = spawnSync('xcrun', ['swiftc', '-parse-as-library', file, '-o', binary], { encoding: 'utf8', env });
    expect(compiled.stderr).toBe(''); expect(compiled.status).toBe(0);
    const ran = spawnSync(binary, [resolve(dir, 'picked/image.png')], { encoding: 'utf8', env });
    expect(ran.status).toBe(0);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 90000);


test.skipIf(process.platform !== 'darwin').each(['scalar', 'files', 'helper'])('file imports preserve authored names: %s', mode => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-intent-names-'));
  const secondFile = mode === 'files';
  const faces = [{ signature: { name: 'save', description: 'Save files.', parameters: [['image', 'string'], ['imagePath', 'string']], consequence: 'writes' },
    face: { parameters: { image: { type: 'file', accept: ['image/png'] }, ...(secondFile ? { imagePath: { type: 'file', accept: ['image/png'] } } : {}) } } }];
  if (mode === 'helper') faces.push({ signature: { name: 'intentFiles', description: 'An authored name.', parameters: [], consequence: 'reads' }, face: {} });
  try {
    const file = resolve(dir, 'Names.swift'), binary = resolve(dir, 'names');
    writeFileSync(file, intentsSource(faces) + `
@main struct Probe {
    @MainActor static func main() async throws {
        let directory = URL(fileURLWithPath: CommandLine.arguments[1])
        let landing = NotificationCenter.default.addObserver(forName: Notification.Name("exact.tools.pickedPath"), object: nil, queue: nil) { note in
            let body = note.object as! NSMutableDictionary
            let name = body["name"] as! String
            body["taken"] = true
            (body["reply"] as! ([String: Any]) -> Void)(["path": "app:/tmp/picked/" + name, "file": directory.appendingPathComponent(name).path])
        }
        let door = NotificationCenter.default.addObserver(forName: Notification.Name("exact.tools.call"), object: nil, queue: nil) { note in
            let body = note.object as! NSMutableDictionary
            body["taken"] = true
            let args = body["args"] as! [String: Any]
            precondition(args["image"] as? String == "app:/tmp/picked/image.png")
            precondition(args["imagePath"] as? String == ${JSON.stringify(secondFile ? 'app:/tmp/picked/other.png' : 'authored-path')})
            (body["reply"] as! ([String: Any]) -> Void)(["outcome": "ok"])
        }
        defer { NotificationCenter.default.removeObserver(landing); NotificationCenter.default.removeObserver(door) }
        let intent = Save()
        intent.image = IntentFile(data: Data([137, 80, 78, 71]), filename: "image.png", type: .png)
        intent.imagePath = ${secondFile ? 'IntentFile(data: Data([137, 80, 78, 71]), filename: "other.png", type: .png)' : '"authored-path"'}
        _ = try await intent.perform()
    }
}
`);
    const env = { ...process.env, TMPDIR: dir };
    const compiled = spawnSync('xcrun', ['swiftc', '-parse-as-library', file, '-o', binary], { encoding: 'utf8', env });
    expect(compiled.stderr).toBe(''); expect(compiled.status).toBe(0);
    if (compiled.status === 0) {
      const ran = spawnSync(binary, [resolve(dir, 'picked')], { encoding: 'utf8', env });
      expect(ran.status).toBe(0);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 90000);

test.skipIf(process.platform !== 'darwin').each([false, true])('native module intents join bundle metadata, generated faces: %s', generated => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-native-intents-'));
  const runApple = (command, args) => {
    const r = spawnSync(command, args, { encoding: 'utf8', env: { ...process.env, TMPDIR: dir } });
    if (r.status !== 0) throw new Error(`${command}: ${r.stderr}\n${r.stdout}`);
    return r.stdout.trim();
  };
  try {
    const sdk = runApple('xcrun', ['--sdk', 'iphonesimulator', '--show-sdk-path']), triple = 'arm64-apple-ios18.0-simulator';
    const nativeModules = ['ExactAppModules'].map(moduleName => {
      const file = resolve(dir, moduleName + '.swift'), binary = resolve(dir, moduleName + '.dylib');
      writeFileSync(file, `import AppIntents
struct ${moduleName}Action: AppIntent {
    static let title: LocalizedStringResource = "${moduleName} action"
    func perform() async throws -> some IntentResult { .result() }
}
`);
      // A native package arrives as an object the link takes, as build.mjs passes ExactPackage.o.
      const pkg = resolve(dir, 'Package.c'), pkgObject = resolve(dir, 'ExactPackage.o');
      writeFileSync(pkg, 'int exact_package_probe(void) { return 1; }\n');
      runApple('xcrun', ['--sdk', 'iphonesimulator', 'clang', '-c', '-target', triple, '-isysroot', sdk, pkg, '-o', pkgObject]);
      const args = ['--sdk', 'iphonesimulator', 'swiftc', '-parse-as-library', '-emit-library', '-module-name', moduleName,
        '-target', triple, '-sdk', sdk, file, pkgObject, '-o', binary];
      runApple('xcrun', args);
      const input = { moduleName, binary, sources: [file], args, directory: resolve(dir, 'constants'), swiftc: 'test', runApple };
      const collected = nativeIntentModule(input);
      expect(nativeIntentModule({ ...input, runApple: () => { throw new Error('cached constants were compiled again'); } })).toEqual(collected);
      return collected;
    });
    const face = { signature: { name: 'generated', description: 'Generated action.', parameters: [], consequence: 'reads' }, face: { doors: ['apple'] } };
    const built = buildIntents({ app: { manifest: {} }, compat: { inputs: { faces: generated ? [face] : [] } }, nativeModules,
      directory: resolve(dir, 'built'), sdkName: 'iphonesimulator', sdk, triple, platform: 'ios', deployment: '18.0', swiftc: 'test', runApple });
    expect(built).not.toBeNull();
    const bundle = resolve(dir, 'App.app');
    built.place(bundle);
    const actions = JSON.parse(readFileSync(resolve(bundle, 'Metadata.appintents/extract.actionsdata'), 'utf8')).actions;
    expect(actions.ExactAppModulesAction.fullyQualifiedTypeName).toBe('ExactAppModules.ExactAppModulesAction');
    expect(Object.keys(actions)).toHaveLength(generated ? 2 : 1);
    if (generated) expect(actions.Generated.fullyQualifiedTypeName).toBe('ExactIntents.Generated');
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 90000);
