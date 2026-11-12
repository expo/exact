// LLP 1126: generated configuration identities and the metadata WidgetKit reads.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { tmpdir } from 'node:os';
import { widgetBundleSource, widgetCompilation } from '../host/apple/widgets.mjs';
import { intentsMetadata } from '../host/apple/intents.mjs';

test('a configured widget generates an AppIntentConfiguration whose intent names its chosen values', () => {
  const configuration = { title: 'Piggybank "plan"', parameters: [{ name: 'categories', type: 'enum', title: 'Categories', default: 'automatic',
    values: [{ value: 'automatic', title: 'Automatic' }, { value: 'one', title: '1' }, { value: 'seven', title: '7' }] }] };
  const source = widgetBundleSource([{ kind: 'add', families: ['systemSmall'] }, { kind: 'plan-card', families: ['systemSmall'], configuration }, { kind: 'plan_card', families: ['systemSmall'], configuration }]);
  const kind = 'Configure_706c616e2d63617264', parameter = `${kind}_63617465676f72696573`;
  assert.ok(source.includes(`intent: ${kind}.self`));
  assert.ok(source.includes('intent: Configure_706c616e5f63617264.self'));
  assert.match(source, /StaticConfiguration\(kind: kind, provider: ExactTimelineProvider\(kind: kind\)\)/);
  assert.ok(source.includes('case v6f6e65 = "6f6e65"'));
  assert.ok(source.includes(`var \`parameter_categories\`: ${parameter}?`));
  assert.ok(source.includes('(`parameter_categories` ?? .v6175746f6d61746963).exactValue'));
  const reordered = widgetBundleSource([{ kind: 'plan_card', families: ['systemSmall'], configuration }, { kind: 'plan-card', families: ['systemSmall'], configuration }]);
  for (const name of [kind, 'Configure_706c616e5f63617264']) {
    assert.ok(reordered.includes(`intent: ${name}.self`));
  }
  const other = { name: 'detail', type: 'enum', title: 'Detail', values: [{ value: 'brief', title: 'Brief' }] };
  const declarations = parameters => widgetBundleSource([{ kind: 'plan-card', families: ['systemSmall'], configuration: { ...configuration, parameters } }]);
  const before = declarations([other, ...configuration.parameters]);
  const after = declarations([...configuration.parameters, other]);
  const enumNames = text => [...text.matchAll(/enum (\w+): String, AppEnum/g)].map(m => m[1]).sort();
  assert.deepEqual(enumNames(before), enumNames(after));
  assert.deepEqual(enumNames(after), [parameter, `${kind}_64657461696c`].sort());
  assert.match(source, /static var title: LocalizedStringResource = "Piggybank \\"plan\\""/, 'labels are escaped Swift literals');
  assert.match(source, /IntentDescription\("Piggybank \\"plan\\""\)/, 'the description defaults to the title');
});

for (const [platform, sdkName, triple, alarm] of [
  ['ios', 'iphonesimulator', 'arm64-apple-ios17.0-simulator', 'ExactAlarms'],
  ['macos', 'macosx', 'arm64-apple-macos14.0', null],
  ['watchos', 'watchsimulator', 'arm64-apple-watchos10.0-simulator', null],
]) {
  const sdk = process.platform === 'darwin' && spawnSync('xcrun', ['--sdk', sdkName, '--show-sdk-path'], { encoding: 'utf8' });
  test.skipIf(!sdk || sdk.status !== 0)(`${platform}: generated widget choices compile and produce intent metadata`, () => {
    const dir = mkdtempSync(resolve(tmpdir(), 'exact-widget-intent-'));
    try {
      writeFileSync(resolve(dir, 'Stand-ins.swift'), `import SwiftUI
import WidgetKit
struct ExactWidgetEntry: TimelineEntry { let date: Date; var configuration: Data? }
struct ExactWidgetView: View { let entry: ExactWidgetEntry; var body: some View { Text(entry.date, style: .time) } }
struct ExactTimelineProvider: TimelineProvider {
    let kind: String
    func placeholder(in context: Context) -> ExactWidgetEntry { ExactWidgetEntry(date: Date()) }
    func getSnapshot(in context: Context, completion: @escaping (ExactWidgetEntry) -> Void) { completion(placeholder(in: context)) }
    func getTimeline(in context: Context, completion: @escaping (Timeline<ExactWidgetEntry>) -> Void) { completion(Timeline(entries: [placeholder(in: context)], policy: .never)) }
}
extension EnvironmentValues { var widgetDisplayGamut: String { get { "srgb" } set {} } }
`);
      const runApple = (cmd, args) => {
        const result = spawnSync(cmd, args, { encoding: 'utf8' });
        assert.equal(result.status, 0, result.stderr || result.stdout);
        return result;
      };
      const configuration = { title: 'Piggybank "plan"', parameters: [
        { name: 'categories', type: 'enum', title: 'Categories', default: 'two', values: [{ value: 'one', title: '1' }, { value: 'two', title: '2' }] },
        { name: 'detail', type: 'enum', title: 'Detail', values: [{ value: '\u00e9', title: 'Composed' }, { value: 'e\u0301', title: 'Decomposed' }] },
      ] };
      const families = platform === 'watchos' ? ['accessoryRectangular'] : ['systemSmall'];
      const entries = [{ kind: 'add', families }, ...['plan-card', 'plan_card'].map(kind => ({ kind, families, configuration }))];
      const file = resolve(dir, 'WidgetBundle.swift'), values = resolve(dir, 'values.swiftconstvalues');
      writeFileSync(file, widgetBundleSource(entries, { platform, alarmMember: alarm ? '; if #available(iOS 26.0, *) { ExactAlarmCountdown() }' : '' }));
      const compilation = widgetCompilation({ entries, platform, activities: false, alarm, source: file, values, directory: dir });
      assert.equal(compilation.args.filter(a => a === '-module-name').length, 1);
      assert.equal(compilation.moduleName, alarm ?? 'ExactWidgets');
      const sources = compilation.sources.filter(p => !/\/(Timeline|WidgetTimelineWaiters)\.swift$/.test(p));
      sources.push(resolve(dir, 'Stand-ins.swift'));
      const sdkPath = sdk.stdout.trim(), binary = resolve(dir, 'Widget.o');
      runApple('xcrun', ['--sdk', sdkName, 'swiftc', '-parse-as-library', '-O', '-target', triple, '-sdk', sdkPath,
        ...compilation.args, '-emit-object', ...sources, '-o', binary]);
      const metadata = intentsMetadata({ moduleName: compilation.moduleName, dir, sdk: sdkPath, triple, platform,
        deployment: platform === 'ios' ? '17.0' : platform === 'macos' ? '14.0' : '10.0', binary, sources, values, runApple });
      const actions = JSON.parse(readFileSync(resolve(metadata, 'extract.actionsdata'), 'utf8'));
      assert.ok(JSON.stringify(actions).includes('Configure_706c616e2d63617264'), `${platform}: configuration metadata missing`);
      for (const parameter of ['categories', 'detail']) assert.ok(JSON.stringify(actions).includes(parameter), `${platform}: parameter metadata missing`);
      if (platform === 'macos') {
        writeFileSync(file, readFileSync(file, 'utf8').replace('@main struct ExactWidgets', 'struct ExactWidgets'));
        const checks = resolve(dir, 'RoundTrip.swift');
        const intent = 'Configure_706c616e2d63617264', enumeration = `${intent}_64657461696c`;
        writeFileSync(checks, `@main struct RoundTrip {
    static func main() {
        let intent = ${intent}()
        ${configuration.parameters[1].values.map(({ value }) => {
          const bytes = Buffer.from(value, 'utf8');
          return `intent.parameter_detail = ${enumeration}(rawValue: ${enumeration}.v${bytes.toString('hex')}.rawValue)!
        precondition(Array(intent.exactConfiguration["detail"]!.utf8) == [${[...bytes].join(', ')}])`;
        }).join('\n        ')}
    }
}`);
        const executable = resolve(dir, 'RoundTrip');
        runApple('xcrun', ['--sdk', sdkName, 'swiftc', '-parse-as-library', '-target', triple, '-sdk', sdkPath,
          ...sources, checks, '-o', executable]);
        runApple(executable, []);
      }
    } finally { rmSync(dir, { recursive: true, force: true }); }
  }, 120_000);
}

test('parameter names cannot shadow generated intent members', () => {
  const parameters = ['title', 'description', 'exactConfiguration', 'exactRecommendations', 'self'].map(name => ({
    name, type: 'enum', title: name, values: [{ value: 'one', title: '1' }],
  }));
  const source = widgetBundleSource([{ kind: 'card', families: ['systemSmall'], configuration: { title: 'Card', parameters } }]);
  for (const { name } of parameters) {
    assert.ok(source.includes(`var \`parameter_${name}\`:`));
    assert.ok(source.includes(`"${name}": (\`parameter_${name}\` ?? .v6f6e65).exactValue`));
  }
});

test('watch recommendations cover the product of choices with the default first', () => {
  const parameters = [
    { name: 'rows', type: 'enum', title: 'Rows', default: 'two', values: [{ value: 'one', title: '1' }, { value: 'two', title: '2' }] },
    { name: 'detail', type: 'enum', title: 'Detail', values: [{ value: 'brief', title: 'Brief' }, { value: 'full', title: 'Full' }] },
  ];
  const source = widgetBundleSource([{ kind: 'card', families: ['accessoryRectangular'], configuration: { title: 'Card', parameters } }], { platform: 'watchos' });
  const choices = [...source.matchAll(/intent\.`parameter_rows` = \.(\w+); intent\.`parameter_detail` = \.(\w+)/g)].map(m => m.slice(1));
  assert.deepEqual(choices, [['v74776f', 'v6272696566'], ['v74776f', 'v66756c6c'], ['v6f6e65', 'v6272696566'], ['v6f6e65', 'v66756c6c']]);
  for (const label of ['Rows: 2, Detail: Brief', 'Rows: 2, Detail: Full', 'Rows: 1, Detail: Brief', 'Rows: 1, Detail: Full']) assert.ok(source.includes(JSON.stringify(label)));
  parameters[0].values.forEach(v => { v.title = 'Same'; });
  const duplicateLabels = widgetBundleSource([{ kind: 'card', families: ['accessoryRectangular'], configuration: { title: 'Card', parameters } }], { platform: 'watchos' });
  assert.ok(duplicateLabels.includes('Rows: Same (one), Detail: Brief'));
  assert.ok(duplicateLabels.includes('Rows: Same (two), Detail: Brief'));
  parameters[0].values.push({ value: 'three', title: 'Same (one)' });
  const descriptions = () => {
    const source = widgetBundleSource([{ kind: 'card', families: ['accessoryRectangular'], configuration: { title: 'Card', parameters } }], { platform: 'watchos' });
    return [...source.matchAll(/description: ("(?:[^"\\]|\\.)*")\) \}\(\)/g)].map(m => JSON.parse(m[1]).normalize('NFC'));
  };
  const labels = descriptions();
  assert.equal(labels.length, 6);
  assert.equal(new Set(labels).size, labels.length, 'complete recommendation descriptions distinguish every choice');
  parameters[0].values.reverse();
  assert.deepEqual(descriptions().sort(), [...labels].sort(), 'disambiguation is stable across option reordering');
  parameters[0].values = [{ value: 'one', title: '\u00e9' }, { value: 'two', title: 'e\u0301' }];
  assert.equal(new Set(descriptions()).size, 4, 'Swift-equivalent titles still distinguish choices');
});
