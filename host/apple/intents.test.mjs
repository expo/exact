import { test, expect } from 'bun:test';
import { mkdtempSync, readFileSync, writeFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { intentsMetadata as metadataPass, intentsSource } from './intents.mjs';
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
    const r = spawnSync(command, args, { encoding: 'utf8', timeout: 90000 });
    if (r.status !== 0) throw new Error(`${command}: ${r.error?.message ?? ''}\n${r.stdout}\n${r.stderr}`);
    return r.stdout.trim();
  };
  try {
    const params = [['book', 'string'], ['page', 'number'], ['title', 'string'], ['enabled', 'bool'], ['fraction', 'number'],
      ['note', 'option<string>'], ['count', 'option<number>'], ['favorite', 'option<bool>'], ['rating', 'option<number>'], ['related', 'option<string>']];
    const fields = { book: { entity: 'book', description: 'The book you read.' }, page: { type: 'integer', description: 'The page you reached.' }, count: { type: 'integer' },
      related: { entity: 'book' }, title: { description: 'A “title”.' } };
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
    const triple = 'arm64-apple-ios17.0-simulator';
    const compile = ['-parse-as-library', '-wmo', '-O', '-swift-version', '5', '-module-name', 'ExactIntents', '-target', triple, '-sdk', sdk,
      '-emit-const-values-path', values, '-Xfrontend', '-const-gather-protocols-file', '-Xfrontend', resolve(dir, 'protocols.json'), '-c', file, '-o', object];
    run(swiftc, compile);
    writeFileSync(resolve(dir, 'sources.txt'), file + '\n'); writeFileSync(resolve(dir, 'values.txt'), values + '\n'); writeFileSync(resolve(dir, 'none.txt'), '');
    const version = /Build version (\S+)/.exec(run('xcodebuild', ['-version']))[1];
    const extract = ['--toolchain-dir', toolchain, '--module-name', 'ExactIntents', '--sdk-root', sdk,
      '--xcode-version', version, '--platform-family', 'iOS', '--deployment-target', '17.0', '--target-triple', triple, '--output', dir,
      '--binary-file', object, '--source-file-list', resolve(dir, 'sources.txt'), '--swift-const-vals-list', resolve(dir, 'values.txt'),
      '--metadata-file-list', resolve(dir, 'none.txt'), '--static-metadata-file-list', resolve(dir, 'none.txt'),
      '--compile-time-extraction', '--deployment-aware-processing', '--no-app-shortcuts-localization'];
    run(resolve(toolchain, 'usr/bin/appintentsmetadataprocessor'), extract);
    const constants = JSON.parse(readFileSync(values, 'utf8')), actual = intentsMetadata(constants, major);
    const expected = JSON.parse(readFileSync(resolve(dir, 'Metadata.appintents/extract.actionsdata'), 'utf8'));
    expect(canonical(actual)).toEqual(canonical(expected));
    expect(actual.actions.LogPages.parameters).toHaveLength(params.length);
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
