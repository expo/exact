// The Android manifest fragment's allow-list (LLP 1133.030 D9): what a package may add, and each refusal by name.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { cpSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { fragmentPermissions, manifestFragment } from './android-manifest.mjs';
import { androidImplementations, appleImplementations, packageConfig, webImplementations } from '../js/bake/src/typescript.mjs';

test('package implementation filters use the supplied identities and ignore unused source text', () => {
  const app = mkdtempSync(resolve(tmpdir(), 'exact-library-views-'));
  try {
    mkdirSync(resolve(app, 'node_modules/@fixture'), { recursive: true });
    symlinkSync(resolve(import.meta.dir, '../packages/fixtures/swatch'), resolve(app, 'node_modules/@fixture/swatch'), 'dir');
    writeFileSync(resolve(app, 'app.contract'), 'component App\n  view\n    text "plain"\n');
    for (const target of ['macos', 'ios']) assert.deepEqual(appleImplementations(app, target, { swatch: '@fixture/swatch' }).map(b => b.extension), ['swatch']);
    assert.deepEqual(androidImplementations(app, { swatch: '@fixture/swatch' }).map(b => b.extension), ['swatch']);
    writeFileSync(resolve(app, 'app.contract'), '// "@fixture/swatch"\ncomponent Unused\n  view\n    swatch-box tint="red"\ncomponent App\n  view\n    text "plain"\n');
    assert.deepEqual(appleImplementations(app, 'macos', {}), []);
    assert.deepEqual(androidImplementations(app, {}), []);
    mkdirSync(resolve(app, 'node_modules/@exact'), { recursive: true });
    symlinkSync(resolve(import.meta.dir, '../packages/maps'), resolve(app, 'node_modules/@exact/maps'), 'dir');
    writeFileSync(resolve(app, 'app.contract'), '// exact-map\ncomponent App\n  view\n    text "plain"\n');
    assert.deepEqual([...packageConfig(app, 'web', { extensions: {} })], []);
    assert.throws(() => packageConfig(app, 'web', { extensions: { maps: '@exact/maps' } }), /maps-tiles-missing/);
  } finally { rmSync(app, { recursive: true, force: true }); }
});

test('only the selected interface supplies an implementation and configuration when basenames match', () => {
  const app = mkdtempSync(resolve(tmpdir(), 'exact-package-scopes-'));
  try {
    for (const scope of ['a', 'b']) {
      const dir = resolve(app, `node_modules/@${scope}/swatch`);
      cpSync(resolve(import.meta.dir, '../packages/fixtures/swatch'), dir, { recursive: true });
      for (const file of ['package.json', 'exact.json', 'apple/exact.json', 'android/exact.json', 'web/exact.json']) {
        const path = resolve(dir, file);
        writeFileSync(path, readFileSync(path, 'utf8').replaceAll('@fixture/swatch', `@${scope}/swatch`).replaceAll('swatch-box', `${scope}-box`));
      }
      const path = resolve(dir, 'exact.json'), json = JSON.parse(readFileSync(path, 'utf8'));
      json.config = { web: { key: { type: 'string', required: true } } };
      writeFileSync(path, JSON.stringify(json));
    }
    writeFileSync(resolve(app, 'app.json'), JSON.stringify({ packages: { '@a/swatch': { web: { key: 'selected' } } } }));
    writeFileSync(resolve(app, 'app.contract'), 'component App\n  view\n    a-box tint="red"\n');
    const extensions = { swatch: '@a/swatch' };
    assert.equal(appleImplementations(app, 'macos', extensions).length, 1);
    assert.equal(androidImplementations(app, extensions).length, 1);
    assert.ok(appleImplementations(app, 'macos', extensions)[0].sources.every(p => p.includes('/@a/swatch/')));
    assert.ok(androidImplementations(app, extensions)[0].dirs.every(p => p.includes('/@a/swatch/')));
    assert.ok(webImplementations(app, extensions).get('swatch').includes('/@a/swatch/'));
    assert.deepEqual([...packageConfig(app, 'web', { extensions })], [['swatch', { key: 'selected' }]]);
  } finally { rmSync(app, { recursive: true, force: true }); }
});

const impl = (manifest) => [{ extension: 'alarms', className: 'exact.alarms.AlarmsModule', manifest }];

test("alarms' fragment is allowed, unexported, and nothing beyond what it names", () => {
  const json = JSON.parse(readFileSync(resolve(import.meta.dir, '../packages/alarms/android/exact.json'), 'utf8'));
  const xml = manifestFragment(impl(json.manifest));
  assert.equal((xml.match(/android:exported="false"/g) ?? []).length, 3);
  assert.doesNotMatch(xml, /exported="true"|<provider|<uses-permission|android:process|<activity/);
  assert.match(xml, /<service android:name="exact\.alarms\.AlarmService" android:exported="false" android:directBootAware="true" android:foregroundServiceType="systemExempted" \/>/);
  assert.match(xml, /<action android:name="android\.intent\.action\.LOCKED_BOOT_COMPLETED" \/>/);
  // The alarm's own receiver has no filter: its intent is explicit.
  assert.match(xml, /<receiver android:name="exact\.alarms\.AlarmReceiver" android:exported="false" android:directBootAware="true" \/>/);
});

test('an implementation with no fragment adds nothing', () => {
  assert.equal(manifestFragment(impl(undefined)), '');
  assert.equal(manifestFragment([]), '');
});

test('speech action queries and package queries share one element and deduplicate across implementations', () => {
  const queries = [
    { action: 'android.speech.RecognitionService' },
    { package: 'com.google.android.apps.healthdata' },
    { action: 'android.intent.action.TTS_SERVICE' },
  ];
  const xml = manifestFragment([...impl({ queries: [...queries, ...queries] }), ...impl({ queries })]);
  assert.equal(xml, [
    '    <queries>',
    '        <intent><action android:name="android.speech.RecognitionService" /></intent>',
    '        <package android:name="com.google.android.apps.healthdata" />',
    '        <intent><action android:name="android.intent.action.TTS_SERVICE" /></intent>',
    '    </queries>',
  ].join('\n'));
});

test('an unlisted query action is refused with the package and action named', () => {
  assert.throws(() => manifestFragment([{
    extension: 'speech-recognition', className: 'exact.speechrecognition.SpeechRecognitionModule',
    manifest: { queries: [{ action: 'android.intent.action.MAIN' }] },
  }]), (e) => e.message.includes("speech-recognition's Android implementation") &&
    e.message.includes('android.intent.action.MAIN') && e.message.includes('not on the list'));
});

test('a query names exactly one package or action', () => {
  for (const query of [{}, null, { package: 'com.google.android.apps.healthdata', action: 'android.intent.action.TTS_SERVICE' }]) {
    assert.throws(() => manifestFragment(impl({ queries: [query] })),
      (e) => e.message.includes("alarms's Android implementation") && e.message.includes('exactly one of package or action'));
  }
});

test('androidImplementations carries the bound speech packages through manifest fragments', () => {
  const app = mkdtempSync(resolve(tmpdir(), 'exact-speech-manifest-'));
  const packages = ['speech-recognition', 'speech-synthesis'];
  try {
    mkdirSync(resolve(app, 'node_modules/@exact'), { recursive: true });
    for (const name of packages) symlinkSync(resolve(import.meta.dir, '../packages', name), resolve(app, 'node_modules/@exact', name), 'dir');
    writeFileSync(resolve(app, 'app.ts'), 'import { startSpeechRecognition } from "@exact/speech-recognition";\nimport { speak } from "@exact/speech-synthesis";\n');
    const bound = androidImplementations(app, Object.fromEntries(packages.map(p => [p, `@exact/${p}`]))).sort((a, b) => a.extension.localeCompare(b.extension));
    assert.deepEqual(bound.map((b) => b.extension), packages);
    assert.deepEqual(bound.map((b) => b.manifest), [
      { queries: [{ action: 'android.speech.RecognitionService' }] },
      { queries: [{ action: 'android.intent.action.TTS_SERVICE' }] },
    ]);
    assert.ok(bound.every((b) => !Object.hasOwn(b, 'queries')));
    assert.equal(manifestFragment(bound), [
      '    <queries>',
      '        <intent><action android:name="android.speech.RecognitionService" /></intent>',
      '        <intent><action android:name="android.intent.action.TTS_SERVICE" /></intent>',
      '    </queries>',
    ].join('\n'));
    writeFileSync(resolve(app, 'app.ts'), 'export const unused = true;\n');
    assert.deepEqual(androidImplementations(app, {}), []);
  } finally { rmSync(app, { recursive: true, force: true }); }
});

test('the allow-list refuses, naming what', () => {
  const refuses = (manifest, what) => assert.throws(() => manifestFragment(impl(manifest)), (e) => e.message.includes(what) && e.message.includes("alarms's Android implementation"));
  refuses({ providers: [] }, 'names "providers"');
  refuses({ activities: [{ name: 'exact.alarms.A', actions: ['android.intent.action.MAIN'] }] }, 'android.intent.action.MAIN');
  refuses({ activities: [{ name: 'exact.alarms.A' }] }, 'no actions');
  refuses({ activities: [{ name: 'exact.alarms.A', actions: ['androidx.health.ACTION_SHOW_PERMISSIONS_RATIONALE'], exported: false }] }, '"exported"');
  refuses({ queries: [{ package: 'com.android.chrome' }] }, 'com.android.chrome');
  refuses({ aliases: [{ name: 'exact.alarms.B', targetActivity: 'exact.alarms.A', action: 'android.intent.action.VIEW_PERMISSION_USAGE', category: 'android.intent.category.HEALTH_PERMISSIONS' }] }, 'not an activity it declares');
  refuses({ receivers: [{ name: 'exact.alarms.R', actions: ['android.intent.action.PACKAGE_ADDED'] }] }, 'PACKAGE_ADDED');
  refuses({ receivers: [{ name: 'exact.alarms.R', exported: true }] }, '"exported"');
  refuses({ receivers: [{ name: 'exact.alarms.R', permission: 'x' }] }, '"permission"');
  refuses({ services: [{ name: 'exact.alarms.S', foregroundServiceType: 'location' }] }, '"location"');
  refuses({ services: [{ name: 'exact.alarms.S', process: ':ring' }] }, '"process"');
  refuses({ services: [{ name: 'dev.exact.canvas.Other' }] }, 'not a class in exact.alarms');
  refuses({ services: [{ name: 'exact.alarms.S"/><provider' }] }, 'not a class in exact.alarms');
  refuses({ services: [{ name: 'exact.alarms.S' }, { name: 'exact.alarms.S' }] }, 'twice');
});

test("health's fragment: Health Connect queried, the rationale and the usage alias exported for the system alone", () => {
  const json = JSON.parse(readFileSync(resolve(import.meta.dir, '../packages/health/android/exact.json'), 'utf8'));
  const health = [{ extension: 'health', className: 'exact.health.HealthModule', manifest: json.manifest }];
  const xml = manifestFragment(health, { privacyPolicy: 'https://example.com/privacy?a=1&b=2' });
  assert.match(xml, /^ {4}<queries>\n {8}<package android:name="com\.google\.android\.apps\.healthdata" \/>\n {4}<\/queries>\n {4}<application>/);
  assert.match(xml, /<meta-data android:name="exact\.privacy_policy" android:value="https:\/\/example\.com\/privacy\?a=1&amp;b=2" \/>/);
  assert.match(xml, /<activity android:name="exact\.health\.PrivacyPolicyActivity" android:exported="true" android:theme="@android:style\/Theme\.NoDisplay">/);
  assert.match(xml, /<activity-alias android:name="exact\.health\.ViewPermissionUsageActivity" android:exported="true" android:targetActivity="exact\.health\.PrivacyPolicyActivity" android:permission="android\.permission\.START_VIEW_PERMISSION_USAGE">/);
  assert.match(xml, /<category android:name="android\.intent\.category\.HEALTH_PERMISSIONS" \/>/);
  assert.doesNotMatch(xml, /<provider|<uses-permission|android:process/);
  // No activity, no policy carried.
  assert.doesNotMatch(manifestFragment([{ ...health[0], manifest: { queries: json.manifest.queries } }], { privacyPolicy: 'https://example.com/privacy' }), /meta-data|<application/);
});

test("device's normal permission is declared, and a runtime one refused", () => {
  const json = JSON.parse(readFileSync(resolve(import.meta.dir, '../packages/device/android/exact.json'), 'utf8'));
  const bound = [{ extension: 'device', className: 'exact.device.DeviceModule', manifest: json.manifest }];
  assert.deepEqual(fragmentPermissions(bound), ['android.permission.ACCESS_NETWORK_STATE']);
  assert.equal(manifestFragment(bound), '', 'a permission adds no component');
  assert.deepEqual(fragmentPermissions(impl(undefined)), []);
  assert.throws(() => fragmentPermissions(impl({ permissions: ['android.permission.ACCESS_FINE_LOCATION'] })), (e) => e.message.includes('ACCESS_FINE_LOCATION') && e.message.includes('normal permissions only'));
});

test('a query with a data scheme: SENDTO mailto: and smsto: only (LLP 1133.024 D22 item 6)', () => {
  const xml = manifestFragment(impl({ queries: [{ action: 'android.intent.action.SENDTO', scheme: 'mailto' }, { action: 'android.intent.action.SENDTO', scheme: 'smsto' }] }));
  assert.match(xml, /<intent><action android:name="android\.intent\.action\.SENDTO" \/><data android:scheme="mailto" \/><\/intent>/);
  assert.match(xml, /<data android:scheme="smsto" \/>/);
  assert.throws(() => manifestFragment(impl({ queries: [{ action: 'android.intent.action.SENDTO', scheme: 'https' }] })), /not on the list/);
  assert.throws(() => manifestFragment(impl({ queries: [{ action: 'android.intent.action.VIEW', scheme: 'mailto' }] })), /not on the list/);
});

test("payments' callbacks service is exported only to Play services' binder, with Google Pay's switch", () => {
  const json = JSON.parse(readFileSync(resolve(import.meta.dir, '../packages/payments/android/exact.json'), 'utf8'));
  const payments = (manifest) => manifestFragment([{ extension: 'payments', className: 'exact.payments.PaymentsModule', manifest }]);
  const xml = payments(json.manifest);
  assert.match(xml, /<service android:name="exact\.payments\.PaymentCallbacks" android:exported="true" android:permission="com\.google\.android\.gms\.permission\.BIND_PAYMENTS_CALLBACK_SERVICE">/);
  assert.match(xml, /<action android:name="com\.google\.android\.gms\.wallet\.callback\.PAYMENT_DATA_CALLBACKS" \/>/);
  assert.match(xml, /<meta-data android:name="com\.google\.android\.gms\.wallet\.api\.enabled" android:value="true" \/>/);
  const refuses = (manifest, what) => assert.throws(() => payments(manifest), (e) => e.message.includes(what));
  refuses({ services: [{ name: 'exact.payments.S', permission: 'android.permission.BIND_JOB_SERVICE', actions: ['x'] }] }, 'BIND_JOB_SERVICE');
  refuses({ services: [{ name: 'exact.payments.S', permission: 'com.google.android.gms.permission.BIND_PAYMENTS_CALLBACK_SERVICE', actions: ['android.intent.action.MAIN'] }] }, 'does not bind');
  refuses({ metadata: [{ name: 'com.google.android.geo.API_KEY', value: 'k' }] }, 'API_KEY');
});

test('a meta-data is only a listed name with its value, both strings', () => {
  const pay = 'com.google.android.gms.wallet.api.enabled';
  const refuses = (metadata, what) => assert.throws(() => manifestFragment(impl({ metadata })), (e) => e.message.includes(what) && e.message.includes("alarms's Android implementation"));
  refuses([{ name: 'review.unknown' }], 'without a string name and value');
  refuses([{ name: 'review.unknown', value: 'true' }], 'review.unknown');
  refuses([{ value: 'true' }], 'without a string name and value');
  refuses([{ name: pay }], 'without a string name and value');
  refuses([{ name: pay, value: true }], 'without a string name and value');
  refuses([{ name: pay, value: 'false' }], 'not on the list');
  refuses([{ name: 'a"/><provider android:name="x', value: 'true' }], 'not on the list');
  refuses([{ name: '__proto__', value: 'true' }], 'not on the list');
  refuses([null], 'without a string name and value');
  refuses([7], 'without a string name and value');
  assert.match(manifestFragment(impl({ metadata: [{ name: pay, value: 'true' }] })), /<meta-data android:name="com\.google\.android\.gms\.wallet\.api\.enabled" android:value="true" \/>/);
});

test('app and package Maven coordinates share validation and refuse conflicting versions by owner', async () => {
  const { mavenCoordinates } = await import('./agent-android.mjs');
  const dir = resolve(tmpdir(), 'maps');
  const app = maven => ({ name: 'maps', dir, manifest: { host: { android: { maven } } } });
  const bound = [{ extension: 'camera', mavenSource: '/packages/camera/android/exact.json', maven: ['g:camera:1', 'g:shared:2'] }];
  assert.deepEqual(mavenCoordinates(app(undefined), []), []);
  assert.deepEqual(mavenCoordinates(app(['g:own:1']), []), ['g:own:1']);
  assert.deepEqual(mavenCoordinates(app(['g:own:1', 'g:shared:2', 'g:own:1']), bound), ['g:camera:1', 'g:own:1', 'g:shared:2']);
  assert.deepEqual(mavenCoordinates({ dir, manifest: {} }, bound), ['g:camera:1', 'g:shared:2']);
  assert.throws(() => mavenCoordinates(app(['g:shared:3']), bound), error => {
    for (const text of ['g:shared:2', 'g:shared:3', resolve(dir, 'app.json'), '/packages/camera/android/exact.json']) assert.ok(error.message.includes(text), error.message);
    return true;
  });
  assert.throws(() => mavenCoordinates(app(['g:own:1', 'g:own:2']), []), /g:own:1.*g:own:2/);
  assert.throws(() => mavenCoordinates(app([]), [...bound, { extension: 'other', mavenSource: '/packages/other/android/exact.json', maven: ['g:shared:3'] }]), /camera.*other/);
  for (const maven of [null, 'g:a:1', [12], ['g:a'], ['g:a:+'], ['g:a:1.+'], ['g:a:[1,2)'], ['g:a:latest.release'],
    ['g:a:LATEST'], ['g:a:RELEASE'], ['g:a:1-SNAPSHOT'], ['g:a:1:classifier'], ['g:a:1@aar'], ['https://repo/g:a:1'], ['g:a:1\n']]) {
    assert.throws(() => mavenCoordinates(app(maven), []), /app.json.*maven/, JSON.stringify(maven));
    assert.throws(() => mavenCoordinates(app([]), [{ ...bound[0], maven }]), /camera.*maven/, JSON.stringify(maven));
  }
});

test('the Android package reader preserves Maven declarations and names their source', async () => {
  const { mavenCoordinates } = await import('./agent-android.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-package-maven-'));
  try {
    const pkg = resolve(dir, 'node_modules/@fixture/swatch');
    cpSync(resolve(import.meta.dir, '../packages/fixtures/swatch'), pkg, { recursive: true });
    const path = resolve(pkg, 'android/exact.json'), declaration = JSON.parse(readFileSync(path, 'utf8'));
    const app = { dir, manifest: { host: { android: { maven: ['g:shared:2'] } } } };
    for (const maven of [['g:shared:2'], ['g:shared:3'], 'g:shared:2', null]) {
      writeFileSync(path, JSON.stringify({ ...declaration, maven }));
      const bound = androidImplementations(dir, { swatch: '@fixture/swatch' });
      assert.equal(bound[0].mavenSource, path);
      if (Array.isArray(maven) && maven[0] === 'g:shared:2') assert.deepEqual(mavenCoordinates(app, bound), maven);
      else assert.throws(() => mavenCoordinates(app, bound), error => error.message.includes(path));
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
