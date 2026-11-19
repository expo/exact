import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { widgetBaseRefusal, widgetLayoutRefusal } from './repack.mjs';
import { widgetExtensionInfo, widgetsDigest } from './widgets.mjs';

// widgety: a bare app with one systemSmall widget.
const widgety = (manifest = {}) => ({ id: 'dev.exact.widgety', dir: '/nonexistent', displayName: 'Widgety', manifest: { widgets: [{ kind: 'counter', families: ['systemSmall'] }], ...manifest } });
const all = ['widgets', 'activities', 'intents', 'alarms'];

test('the widget digest moves with each input of the kept extension binary and its metadata, and nothing else', () => {
  const digest = widgetsDigest(widgety(), all);
  // What no extension input reads: the name, theme, the extension kinds' order.
  expect(widgetsDigest(widgety({ name: 'Renamed', theme: { accent: 'red' } }), all)).toBe(digest);
  expect(widgetsDigest(widgety(), [...all].reverse())).toBe(digest);
  const entry = { kind: 'counter', families: ['systemSmall'] };
  const changed = {
    kind: widgety({ widgets: [{ ...entry, kind: 'count' }] }),
    family: widgety({ widgets: [{ ...entry, families: ['systemSmall', 'systemMedium'] }] }),
    title: widgety({ widgets: [{ ...entry, title: 'Count' }] }),
    description: widgety({ widgets: [{ ...entry, description: 'Counts.' }] }),
    'a second widget': widgety({ widgets: [entry, { kind: 'other', families: ['systemSmall'] }] }),
    'a configuration intent': widgety({ widgets: [{ ...entry, configuration: { title: 'Pick', parameters: [{ name: 'p', title: 'P', values: [{ value: 'a', title: 'A' }] }] } }] }),
    'a configuration value': widgety({ widgets: [{ ...entry, configuration: { title: 'Pick', parameters: [{ name: 'p', title: 'P', values: [{ value: 'b', title: 'A' }] }] } }] }),
    'a Live Activity': widgety({ live_activities: [{ kind: 'timer' }] }),
    'the minimum OS': widgety({ host: { ios: { minimumOS: '18.0' } } }),
  };
  const digests = Object.entries(changed).map(([what, app]) => [what, widgetsDigest(app, all)]);
  for (const [what, d] of digests) expect([what, d]).not.toEqual([what, digest]);
  expect(new Set(digests.map(([, d]) => d)).size).toBe(digests.length);
  // The packaged extension kinds, and the SDK's widget Swift sources (a test's reader: one file's bytes differ).
  expect(widgetsDigest(widgety(), ['widgets'])).not.toBe(digest);
  expect(widgetsDigest(widgety(), all, { read: (file) => file.endsWith('Timeline.swift') ? 'edited' : readFileSync(file) })).not.toBe(digest);
  expect(widgetsDigest(widgety(), all, { read: (file) => readFileSync(file) })).toBe(digest);
});

test('a widget app repacks only on its own build with the same digest', () => {
  const upload = { app: 'dev.exact.widgety', digest: 'd1' };
  expect(widgetBaseRefusal(upload, { app: 'dev.exact.widgety', digest: 'd1', alarm: false })).toBeNull();
  expect(widgetBaseRefusal(upload, { app: 'dev.exact.template', digest: 'd1', alarm: false })).toContain('its own previous build');
  expect(widgetBaseRefusal(upload, { app: 'dev.exact.widgety', digest: undefined, alarm: false })).toContain('no widget digest');
  expect(widgetBaseRefusal(upload, { app: 'dev.exact.widgety', digest: 'd0', alarm: false })).toContain('declarations changed');
  expect(widgetBaseRefusal(upload, { app: 'dev.exact.widgety', digest: 'd1', alarm: true })).toContain('alarm');
});

test('a base and an upload that disagree about having widgets refuse instead of failing', () => {
  expect(widgetLayoutRefusal(false, { plugIns: false })).toBeNull();
  expect(widgetLayoutRefusal(false, { plugIns: true, extension: true, core: true })).toContain('PlugIns');
  expect(widgetLayoutRefusal(true, { plugIns: false, extension: false, core: false })).toContain('built without widgets');
  expect(widgetLayoutRefusal(true, { plugIns: true, extension: true, core: false })).toContain('built without widgets');
  expect(widgetLayoutRefusal(true, { plugIns: true, extension: true, core: true })).toBeNull();
});

test('the extension Info.plist is the app Info.plist\'s generation, versions and groups', () => {
  const appInfo = { CFBundleVersion: '1', CFBundleShortVersionString: '1.0', MinimumOSVersion: '17.0', UIDeviceFamily: [1], CFBundleSupportedPlatforms: ['iPhoneOS'], ExactKeychainAccessGroup: 'P.dev.exact.widgety', ExactWidgetGeneration: 'g1' };
  const info = widgetExtensionInfo(widgety(), appInfo);
  expect(info).toContain('<key>ExactWidgetGeneration</key><string>g1</string>');
  expect(info).toContain('<key>CFBundleIdentifier</key><string>dev.exact.widgety.widgets</string>');
  expect(widgetExtensionInfo(widgety(), { ...appInfo, ExactWidgetGeneration: 'g2' })).not.toBe(info);
});

test('repack-capabilities.json declares the widget repack, which a caller reads before asking', () => {
  const declared = JSON.parse(readFileSync(new URL('./repack-capabilities.json', import.meta.url), 'utf8'));
  expect(declared.widgets).toBe(1);
  // The declaration stands only while repack.mjs has no blanket refusal of such apps.
  expect(readFileSync(new URL('./repack.mjs', import.meta.url), 'utf8')).not.toContain("if (hasEntries(app)) refuse(");
});
