// An Android app's notification resources (LLP 1133.004.000 D1, D3; `android-channels.mjs`): the channel list, its
// strings per table, its sounds, and the small icon's pixels.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { androidNotificationResources, googleServices, resourceText, valuesDirectory } from './android-channels.mjs';
import { readPng } from './app-icons.mjs';
import { encodePng } from './png.mjs';

const inDir = (body) => { const dir = mkdtempSync(resolve(tmpdir(), 'exact-android-channels-')); try { return body(dir); } finally { rmSync(dir, { recursive: true, force: true }); } };
const files = (res) => existsSync(res) ? readdirSync(res, { recursive: true }).filter((path) => statSync(resolve(res, path)).isFile()).sort() : [];
const pixel = (image, x, y) => [...image.data.subarray((y * image.width + x) * 4, (y * image.width + x) * 4 + 4)];

test('declared channels become a list, strings per table and raw sounds, with the default table as values', () => inDir((dir) => {
  mkdirSync(resolve(dir, 'assets'));
  writeFileSync(resolve(dir, 'assets/Alert.WAV'), 'RIFF');
  const res = resolve(dir, 'res');
  const channels = [
    { id: 'urgencies', name: { en: 'Urgencies', 'pt-BR': 'Urgências' }, description: { en: "Alerts 'now'", 'pt-BR': 'Alertas' }, importance: 'high', sound: 'assets/Alert.WAV' },
    { id: 'news', name: { en: '@news' }, description: null, importance: 'low', sound: null },
  ];
  assert.deepEqual(androidNotificationResources({}, dir, res, { channels, locales: ['en', 'pt-BR'], base: 'en' }), { icon: false });
  // With no monochrome icon, the small icon is the platform's glyph.
  assert.deepEqual(files(res), ['drawable/exact_notification.xml', 'raw/exact_channel_urgencies.wav', 'values-b+pt+BR/exact_channels.xml', 'values/exact_channels.xml', 'xml/exact_channels.xml']);
  assert.match(readFileSync(resolve(res, 'drawable/exact_notification.xml'), 'utf8'), /android:drawable="@android:drawable\/ic_popup_reminder"/);
  assert.equal(readFileSync(resolve(res, 'xml/exact_channels.xml'), 'utf8'), [
    '<channels>',
    '    <channel id="urgencies" importance="high" name="@string/exact_channel_urgencies_name" description="@string/exact_channel_urgencies_description" sound="@raw/exact_channel_urgencies" />',
    '    <channel id="news" importance="low" name="@string/exact_channel_news_name" />',
    '</channels>', ''].join('\n'));
  assert.equal(readFileSync(resolve(res, 'values/exact_channels.xml'), 'utf8'), [
    '<resources>',
    '    <string name="exact_channel_urgencies_name">Urgencies</string>',
    '    <string name="exact_channel_urgencies_description">Alerts \\\'now\\\'</string>',
    '    <string name="exact_channel_news_name">\\@news</string>',
    '</resources>', ''].join('\n'));
  // A table that lacks a text has the base table's.
  assert.match(readFileSync(resolve(res, 'values-b+pt+BR/exact_channels.xml'), 'utf8'), /exact_channel_news_name">\\@news</);

  // An app that grants no notifications has none of it, and what a previous build wrote is gone.
  assert.equal(androidNotificationResources({}, dir, res, { channels: undefined }), null);
  assert.deepEqual(files(res), []);
}));

test('the mapped live activities ride as a raw list the reader reads, and none writes no file (LLP 1126 D13)', () => inDir((dir) => {
  const res = resolve(dir, 'res');
  const activities = [{ kind: 'patrol', android: { channel: 'default', title: 'title', progress: 'done' } }];
  androidNotificationResources({}, dir, res, { channels: [], base: 'en', activities });
  assert.deepEqual(JSON.parse(readFileSync(resolve(res, 'raw/exact_activities.json'), 'utf8')), activities);
  androidNotificationResources({}, dir, res, { channels: [], base: 'en' });
  assert.ok(!existsSync(resolve(res, 'raw/exact_activities.json')));
}));

test('the small icon is the monochrome glyph trimmed into 22 of 24 dp per density, drawn white by its alpha', () => inDir((dir) => {
  // A 432 px glyph whose opaque part is a 100 × 50 bar off centre, half transparent at its right end.
  const [w, h] = [432, 432], data = Buffer.alloc(w * h * 4);
  for (let y = 300; y < 350; y++) for (let x = 20; x < 120; x++) data.set([10, 20, 30, x >= 110 ? 128 : 255], (y * w + x) * 4);
  mkdirSync(resolve(dir, 'assets'));
  writeFileSync(resolve(dir, 'assets/mono.png'), encodePng({ width: w, height: h, data }));
  const res = resolve(dir, 'res');
  const manifest = { icons: [{ src: 'assets/mono.png', sizes: '432x432', purpose: 'monochrome' }] };
  assert.deepEqual(androidNotificationResources(manifest, dir, res, { channels: [], base: 'en' }), { icon: true });
  assert.deepEqual(files(res), ['drawable-hdpi/exact_notification.png', 'drawable-mdpi/exact_notification.png', 'drawable-xhdpi/exact_notification.png',
    'drawable-xxhdpi/exact_notification.png', 'drawable-xxxhdpi/exact_notification.png', 'xml/exact_channels.xml']);
  const icon = readPng(resolve(res, 'drawable-xxxhdpi/exact_notification.png'));
  assert.deepEqual([icon.width, icon.height], [96, 96]);
  // 88 px wide (22 of 24 dp), 44 tall, centred: columns 4 to 91, rows 26 to 69.
  assert.deepEqual([pixel(icon, 3, 48)[3], pixel(icon, 4, 48), pixel(icon, 48, 25)[3], pixel(icon, 48, 26)], [0, [255, 255, 255, 255], 0, [255, 255, 255, 255]]);
  assert.ok(pixel(icon, 90, 48)[3] < 255 && pixel(icon, 90, 48)[3] > 0);
}));

test("google-services.json's client for the app becomes Firebase's string resources, the configured path first (LLP 1133.004.000 D6)", () => inDir((dir) => {
  const file = (clients, project = { project_number: '123', project_id: 'gate-1', storage_bucket: 'gate-1.appspot.com' }) => JSON.stringify({ project_info: project, client: clients });
  const client = (id, app = `1:123:android:${id}`) => ({ client_info: { mobilesdk_app_id: app, android_client_info: { package_name: id } }, api_key: [{ current_key: `key-${id}` }] });
  assert.equal(googleServices(dir, 'dev.exact.gate', undefined), null);
  writeFileSync(resolve(dir, 'google-services.json'), file([client('dev.exact.other'), client('dev.exact.gate')]));
  const values = googleServices(dir, 'dev.exact.gate', undefined);
  assert.deepEqual(values, { google_app_id: '1:123:android:dev.exact.gate', gcm_defaultSenderId: '123', google_api_key: 'key-dev.exact.gate', project_id: 'gate-1', google_storage_bucket: 'gate-1.appspot.com' });
  // No client for the id is none at all.
  assert.equal(googleServices(dir, 'dev.exact.absent', undefined), null);
  // The configured path is read first; a path that yields no client leaves the file beside app.json.
  mkdirSync(resolve(dir, 'ci'));
  writeFileSync(resolve(dir, 'ci/firebase.json'), file([client('dev.exact.gate', '1:9:android:ci')], { project_number: '9', project_id: 'ci' }));
  assert.deepEqual(googleServices(dir, 'dev.exact.gate', 'ci/firebase.json'), { google_app_id: '1:9:android:ci', gcm_defaultSenderId: '9', google_api_key: 'key-dev.exact.gate', project_id: 'ci' });
  assert.deepEqual(googleServices(dir, 'dev.exact.gate', 'ci/missing.json'), values);
  writeFileSync(resolve(dir, 'google-services.json'), '{');
  assert.equal(googleServices(dir, 'dev.exact.gate', 'ci/missing.json'), null);

  const res = resolve(dir, 'res');
  androidNotificationResources({}, dir, res, { channels: [], base: 'en', firebase: { google_app_id: '1:2:android:3', project_id: '@p' } });
  assert.equal(readFileSync(resolve(res, 'values/exact_google_services.xml'), 'utf8'), ['<resources>',
    '    <string name="google_app_id" translatable="false">1:2:android:3</string>',
    '    <string name="project_id" translatable="false">\\@p</string>', '</resources>', ''].join('\n'));
}));

test('a strings table goes to its resource directory, and its text escapes as the resource compiler reads it', () => {
  assert.deepEqual(['en', 'es', 'pt-BR', 'zh-Hant-TW'].map((t) => valuesDirectory(t, 'en')), ['values', 'values-es', 'values-b+pt+BR', 'values-b+zh+Hant+TW']);
  assert.equal(resourceText(`<a & "b"> \\ ?q`), '&lt;a &amp; \\"b\\"> \\\\ ?q');
  assert.equal(resourceText('?q'), '\\?q');
});
