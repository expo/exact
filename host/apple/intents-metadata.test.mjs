import { test, expect } from 'bun:test';
import { mkdtempSync, readFileSync, writeFileSync, rmSync, cpSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { dirname, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { intentsMetadata } from './intents-metadata.mjs';
import { widgetBundleSource } from './widgets.mjs';

// Swift's constant values for Caltrain's widget extension (cfe0d7aea3's consumer): one configured widget whose intent
// has one String AppEnum row with two choices. Compiled on Linux by host/apple/widgets.mjs; file and line fields dropped.
const widget = () => JSON.parse(readFileSync(new URL('./fixtures/widget-configuration.swiftconstvalues', import.meta.url), 'utf8'));
const intent = 'Configure_6e6578742d747261696e73', row = `${intent}_726f7773`;

test('a widget configuration intent and its AppEnum row come out under the widget module', () => {
  const out = intentsMetadata(widget(), 27, { module: 'ExactWidgets' });
  expect(Object.keys(out.actions)).toEqual([intent]);
  const action = out.actions[intent];
  expect(action.fullyQualifiedTypeName).toBe(`ExactWidgets.${intent}`);
  expect(action.mangledTypeName).toBe('12ExactWidgets32Configure_6e6578742d747261696e73V');
  expect(action.title.key).toBe('Departures');
  expect(action.systemProtocols).toContain('com.apple.link.systemProtocol.WidgetConfiguration');
  expect(action.isDiscoverable).toBe(false);
  expect(action.parameters).toHaveLength(1);
  const [p] = action.parameters;
  expect(p.name).toBe('parameter_rows');
  expect(p.isOptional).toBe(true);
  expect(p.title.key).toBe('Departures');
  expect(p.valueType).toEqual({ linkEnumeration: { wrapper: { identifier: row } } });
  expect(p.typeSpecificMetadata).toEqual(['LNValueTypeSpecificMetadataKeyDefaultValue', { string: { wrapper: '74776f' } }]);
  expect(action.outputFlags).toBe(8);
  expect(action.systemProtocolMetadataV2).toEqual(['com.apple.link.systemProtocol.WidgetConfiguration', { empty: {} }]);
  expect(out.enums).toHaveLength(1);
  const [e] = out.enums;
  expect(e.identifier).toBe(row);
  expect(e.displayTypeName.key).toBe('Departures');
  expect(e.cases).toEqual([
    { identifier: '6f6e65', displayRepresentation: { title: { alternatives: [], key: '1' } } },
    { identifier: '74776f', displayRepresentation: { title: { alternatives: [], key: '2' } } },
  ]);
  expect(e).not.toHaveProperty('rawValues');
  expect(e.isSystem).toBe(false);
  expect(out.autoShortcuts).toEqual([]);
});

test('the module name scopes every type, and the intents module keeps its default', () => {
  expect(() => intentsMetadata(widget(), 27)).toThrow('invalid or duplicate Swift type');
  expect(() => intentsMetadata(widget(), 27, { module: 'Exact Widgets' })).toThrow('invalid module name');
  expect(intentsMetadata([], 27).enums).toEqual([]);
});

test('widget shapes outside what widgets.mjs generates fail the build', () => {
  const types = widget(), e = types.find(t => t.kind === 'enum');
  const titles = e.properties.find(p => p.label === 'caseDisplayRepresentations');
  titles.value.pop();
  expect(() => intentsMetadata(types, 27, { module: 'ExactWidgets' })).toThrow('incomplete case titles');

  const intRaw = widget(); intRaw.find(t => t.kind === 'enum').associatedTypeAliases.find(a => a.typeAliasName === 'RawValue').substitutedTypeName = 'Swift.Int';
  expect(() => intentsMetadata(intRaw, 27, { module: 'ExactWidgets' })).toThrow('unsupported AppEnum');

  const badDefault = widget();
  for (const p of badDefault.find(t => t.kind === 'struct').properties) {
    const d = p.value?.arguments?.find(a => a.label === 'default');
    if (d) d.value.name = 'vffff';
  }
  expect(() => intentsMetadata(badDefault, 27, { module: 'ExactWidgets' })).toThrow('invalid default');

  const lostWrapper = widget(); const s = lostWrapper.find(t => t.kind === 'struct');
  s.properties = s.properties.filter(p => p.label !== '_parameter_rows');
  expect(() => intentsMetadata(lostWrapper, 27, { module: 'ExactWidgets' })).toThrow('incomplete parameters');
});

// The check against Xcode: Caltrain's configured widget, as widgets.mjs generates it, through Xcode's metadata pass,
// for iOS and watchOS (whose intent also carries recommendations). EXACT_REFERENCE_OUT=DIR keeps Xcode's extracts
// and Swift's constant values there, for a Linux session to compare against without a Mac.
const canonical = value => {
  if (Array.isArray(value)) return value.map(canonical);
  if (value == null || typeof value !== 'object') return value;
  const result = Object.fromEntries(Object.entries(value).filter(([k]) => k !== 'generator').map(([k, v]) => [k, canonical(v)]));
  if (result.resolvableInputTypes) result.resolvableInputTypes.sort((a, b) => JSON.stringify(a).localeCompare(JSON.stringify(b)));
  return result;
};
const xcode = process.platform === 'darwin' && spawnSync('xcodebuild', ['-version']).status === 0;
for (const platform of ['ios', 'watchos']) {
  test.skipIf(!xcode)(`widget configuration metadata matches Xcode's extractor (${platform})`, () => {
    const dir = mkdtempSync(resolve(tmpdir(), 'exact-widget-intents-')), keep = process.env.EXACT_REFERENCE_OUT;
    const run = (command, args) => {
      const r = spawnSync(command, args, { encoding: 'utf8', timeout: 90000 });
      if (r.status !== 0) throw new Error(`${command}: ${r.error?.message ?? ''}\n${r.stdout}\n${r.stderr}`);
      return r.stdout.trim();
    };
    try {
      const app = JSON.parse(readFileSync(new URL('../../apps/caltrain/app.json', import.meta.url), 'utf8'));
      const entries = app.widgets.filter(w => w.configuration).map(w => ({ ...w, families: platform === 'watchos' ? w.watchFamilies : w.families }));
      // The intents alone: the Widget structs need the host's views, which the metadata pass does not read.
      const generated = widgetBundleSource(entries, { platform });
      const file = resolve(dir, 'Intents.swift');
      writeFileSync(file, generated.slice(0, generated.indexOf('\nstruct Entry0')) + '\nprotocol ExactConfigurationIntent { var exactConfiguration: [String: String] { get } }\n');
      const sdkName = platform === 'watchos' ? 'watchsimulator' : 'iphonesimulator';
      const sdk = run('xcrun', ['--sdk', sdkName, '--show-sdk-path']), major = parseInt(run('xcrun', ['--sdk', sdkName, '--show-sdk-version']));
      const swiftc = run('xcrun', ['--find', 'swiftc']), toolchain = resolve(dirname(swiftc), '../..');
      const triple = platform === 'watchos' ? 'arm64-apple-watchos10.0-simulator' : 'arm64-apple-ios17.0-simulator';
      const object = resolve(dir, 'ExactWidgets.o'), values = resolve(dir, 'ExactWidgets.swiftconstvalues');
      writeFileSync(resolve(dir, 'protocols.json'), readFileSync(new URL('./fixtures/widget-protocols.json', import.meta.url)));
      run(swiftc, ['-parse-as-library', '-wmo', '-O', '-swift-version', '5', '-module-name', 'ExactWidgets', '-target', triple, '-sdk', sdk,
        '-emit-const-values-path', values, '-Xfrontend', '-const-gather-protocols-file', '-Xfrontend', resolve(dir, 'protocols.json'), '-c', file, '-o', object]);
      writeFileSync(resolve(dir, 'sources.txt'), file + '\n'); writeFileSync(resolve(dir, 'values.txt'), values + '\n'); writeFileSync(resolve(dir, 'none.txt'), '');
      const version = /Build version (\S+)/.exec(run('xcodebuild', ['-version']))[1];
      run(resolve(toolchain, 'usr/bin/appintentsmetadataprocessor'), ['--toolchain-dir', toolchain, '--module-name', 'ExactWidgets', '--sdk-root', sdk,
        '--xcode-version', version, '--platform-family', platform === 'watchos' ? 'watchOS' : 'iOS', '--deployment-target', platform === 'watchos' ? '10.0' : '17.0',
        '--target-triple', triple, '--output', dir, '--binary-file', object, '--source-file-list', resolve(dir, 'sources.txt'),
        '--swift-const-vals-list', resolve(dir, 'values.txt'), '--metadata-file-list', resolve(dir, 'none.txt'), '--static-metadata-file-list', resolve(dir, 'none.txt'),
        '--compile-time-extraction', '--deployment-aware-processing', '--no-app-shortcuts-localization']);
      const expected = JSON.parse(readFileSync(resolve(dir, 'Metadata.appintents/extract.actionsdata'), 'utf8'));
      if (keep) {
        cpSync(resolve(dir, 'Metadata.appintents'), resolve(keep, `${platform}-Metadata.appintents`), { recursive: true });
        cpSync(values, resolve(keep, `${platform}-ExactWidgets.swiftconstvalues`)); cpSync(file, resolve(keep, `${platform}-Intents.swift`));
        writeFileSync(resolve(keep, `${platform}-xcode.txt`), run('xcodebuild', ['-version']) + `\nSDK ${major}\n`);
      }
      const actual = intentsMetadata(JSON.parse(readFileSync(values, 'utf8')), major, { module: 'ExactWidgets' });
      expect(canonical(actual)).toEqual(canonical(expected));
    } finally { rmSync(dir, { recursive: true, force: true }); }
  }, 120000);
}
