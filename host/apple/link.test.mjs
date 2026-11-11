import { test } from 'bun:test';
import assert from 'node:assert/strict';
import {existsSync, mkdtempSync, mkdirSync, readFileSync, writeFileSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join} from 'node:path';
import { spawnSync } from 'node:child_process';
import { assertActivityRefusals } from './activities.mjs';
import { resolveApp } from '../../scripts/app.mjs';
import { appleComposition, assertCapabilityFrameworks, assertLinkedSdk, iosSimulatorTarget, loadedDylibs } from './link.mjs';
import { systemFrameworks } from './compose.mjs';

test('an x64 Linux host can select the matching ARM simulator Rust and Swift targets', () => {
  assert.deepEqual(iosSimulatorTarget('arm64'), { target: 'aarch64-apple-ios-sim', triple: 'arm64-apple-ios17.0-simulator' });
  for (const arch of ['x64', 'x86_64']) assert.deepEqual(iosSimulatorTarget(arch), { target: 'x86_64-apple-ios', triple: 'x86_64-apple-ios17.0-simulator' });
  for (const arch of ['ia32', '__proto__', 'toString']) assert.throws(() => iosSimulatorTarget(arch), /unsupported iOS simulator architecture/);
  const environment = { ...process.env }; delete environment.EXACT_IOS_SIM_ARCH;
  const source = `Object.defineProperty(process,'platform',{value:'linux'}); const {iosSimulatorTarget}=await import('./host/apple/link.mjs'); console.log(JSON.stringify(iosSimulatorTarget()));`;
  const result = spawnSync(process.execPath, ['-e', source], { cwd: join(import.meta.dir, '../..'), encoding: 'utf8', env: environment });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(JSON.parse(result.stdout).target, 'aarch64-apple-ios-sim');
});

test('the complete builder uses the selected simulator CPU with an app minimum OS, including native modules', () => {
  for (const [arch, target, cpu] of [['arm64', 'aarch64-apple-ios-sim', 'arm64'], ['x86_64', 'x86_64-apple-ios', 'x86_64']]) {
    const script = `import {iosTarget,iosTripleFor,moduleTripleFor} from './host/apple/build.mjs';
      const app={manifest:{host:{ios:{minimumOS:'18.2'}}}};
      console.log(JSON.stringify([iosTarget,iosTripleFor(app,false),moduleTripleFor(app,true),iosTripleFor(app,true)]));`;
    const r = spawnSync(process.execPath, ['-e', script], { cwd: join(import.meta.dir, '../..'), encoding: 'utf8', env: { ...process.env, EXACT_IOS_SIM_ARCH: arch } });
    assert.equal(r.status, 0, r.stderr);
    assert.deepEqual(JSON.parse(r.stdout), [target, `${cpu}-apple-ios18.2-simulator`, `${cpu}-apple-ios18.2-simulator`, 'arm64-apple-ios18.2']);
  }
});

test('package frameworks retain category-only dependencies and weak-link APIs newer than the deployment target', () => {
  const packages = [
    { frameworks: { PhotosUI: { ios: '14.0' } } },
    { frameworks: { FoundationModels: { ios: '26.0', macos: '26.0' } } },
    { frameworks: { PhotosUI: { ios: '14.0' } } },
  ];
  assert.deepEqual(systemFrameworks(packages, 'ios', '17.0'), [
    '-Xlinker', '-weak_framework', '-Xlinker', 'FoundationModels', '-framework', 'PhotosUI',
  ]);
  assert.deepEqual(systemFrameworks(packages, 'ios', '26.0'), ['-framework', 'FoundationModels', '-framework', 'PhotosUI']);
  assert.deepEqual(systemFrameworks(packages, 'macos', '14.0'), ['-Xlinker', '-weak_framework', '-Xlinker', 'FoundationModels']);
});

test('production links what the plan uses, the manifest names and the grants open; development all but the photo picker and sign-in sheet, which it links as production would (LLP 1047.001.000 E, F)', () => {
  const graph = { uses: ['markdown', 'picker'] };
  assert.deepEqual(appleComposition(graph, true), { link: 'markdown,picker', products: ['ExactMarkdown'] });
  assert.deepEqual(appleComposition({ uses: [] }, true), { link: 'none', products: [] });
  assert.deepEqual(appleComposition(graph, true, [], true).products, ['ExactMarkdown', 'ExactAuthSession']);
  assert.deepEqual(appleComposition(graph, false).products, ['ExactGroupedLists', 'ExactMarkdown', 'ExactSurfaces', 'ExactDrag']);
  assert.deepEqual(appleComposition({ uses: ['picker', 'media_picker'] }, false, [], true).products,
    ['ExactGroupedLists', 'ExactMarkdown', 'ExactSurfaces', 'ExactDrag', 'ExactMediaPicker', 'ExactAuthSession']);
  assert.deepEqual(appleComposition(graph, false, ['auth_session']).products.at(-1), 'ExactAuthSession');
  assert.deepEqual(appleComposition({ uses: ['numerals'] }, false).products.at(-1), 'ExactNumerals');
  assert.ok(!appleComposition(graph, false).products.includes('ExactNumerals'));
  assert.equal(appleComposition(undefined, false).link, 'all');
});

test('the load commands are read from the file, each slice of a universal one; a binary with no capability framework passes the check', () => {
  assert.ok(loadedDylibs('/bin/ls').includes('/usr/lib/libSystem.B.dylib'));
  assert.doesNotThrow(() => assertCapabilityFrameworks('/bin/ls', []));
});

test('SDK checks read thin, universal and fat64 executables on any build host, including every slice', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-sdk-'));
  const thin = (major, minor, patch = 0) => {
    const b = Buffer.alloc(56);
    b.writeUInt32LE(0xfeedfacf, 0);
    b.writeUInt32LE(1, 16); b.writeUInt32LE(24, 20);
    b.writeUInt32LE(0x32, 32); b.writeUInt32LE(24, 36);
    b.writeUInt32LE(7, 40); b.writeUInt32LE((major << 16) | (minor << 8) | patch, 48);
    return b;
  };
  const fat = (slices, wide) => {
    const width = wide ? 32 : 20, header = Buffer.alloc(8 + slices.length * width);
    header.writeUInt32BE(wide ? 0xcafebabf : 0xcafebabe, 0); header.writeUInt32BE(slices.length, 4);
    let offset = header.length;
    slices.forEach((slice, i) => {
      const at = 8 + i * width;
      if (wide) { header.writeBigUInt64BE(BigInt(offset), at + 8); header.writeBigUInt64BE(BigInt(slice.length), at + 16); }
      else { header.writeUInt32BE(offset, at + 8); header.writeUInt32BE(slice.length, at + 12); }
      offset += slice.length;
    });
    return Buffer.concat([header, ...slices]);
  };
  const path = join(dir, 'App (Exact)');
  const check = (bytes, passes) => {
    writeFileSync(path, bytes);
    if (passes) assert.doesNotThrow(() => assertLinkedSdk(path, '26.4'));
    else assert.throws(() => assertLinkedSdk(path, '26.4'), /records SDK .*not 26\.4/);
  };
  try {
    check(thin(26, 4, 1), true);
    check(thin(26, 5), false);
    for (const wide of [false, true]) {
      check(fat([thin(26, 4), thin(26, 4, 2)], wide), true);
      check(fat([thin(26, 4), thin(26, 5)], wide), false);
      const truncatedSlice = fat([thin(26, 4), thin(26, 4)], wide).subarray(0, -1);
      writeFileSync(path, truncatedSlice);
      assert.throws(() => assertLinkedSdk(path, '26.4'), /invalid 64-bit Mach-O/);
    }
    const missing = thin(26, 4); missing.writeUInt32LE(0x1b, 32); check(missing, false);
    const oversized = thin(26, 4); oversized.writeUInt32LE(64, 36);
    writeFileSync(path, oversized);
    assert.throws(() => assertLinkedSdk(path, '26.4'), /invalid 64-bit Mach-O/);
    for (const bytes of [Buffer.alloc(0), Buffer.alloc(4), thin(26, 4).subarray(0, 40)]) {
      writeFileSync(path, bytes);
      assert.throws(() => assertLinkedSdk(path, '26.4'), /invalid 64-bit Mach-O/);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});


test('ordinary app executables refuse WidgetKit, ActivityKit and SwiftUI, including versioned Mac frameworks (LLP 1126.001 WP16)', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-activity-links-'));
  try {
    for (const framework of ['WidgetKit', 'ActivityKit', 'SwiftUI']) for (const version of ['', 'Versions/A/']) {
      const name = Buffer.from(`/System/Library/Frameworks/${framework}.framework/${version}${framework}\0`);
      const size = (24 + name.length + 7) & ~7, bytes = Buffer.alloc(32 + size);
      bytes.writeUInt32LE(0xfeedfacf, 0); bytes.writeUInt32LE(1, 16); bytes.writeUInt32LE(size, 20);
      bytes.writeUInt32LE(0xc, 32); bytes.writeUInt32LE(size, 36); bytes.writeUInt32LE(24, 40); name.copy(bytes, 56);
      const executable = join(dir, 'App'); writeFileSync(executable, bytes);
      assert.throws(() => assertCapabilityFrameworks(executable, []), new RegExp(framework));
      assert.doesNotThrow(() => assertCapabilityFrameworks(executable, framework === 'SwiftUI' ? [] : ['ExactSnapshot'], framework === 'SwiftUI' ? ['SwiftUI'] : []));
    }
  } finally { rmSync(dir, {recursive:true,force:true}); }
});

test("the ActivityKit arm compiles the checkout's sources when the build runs from an app's own directory", () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-activity-arm-'));
  try {
    // A child process, so the working directory is the app's and not this process's.
    const script = `const { packageActivityArm } = await import(${JSON.stringify(new URL('./activities.mjs', import.meta.url).href)});
try { packageActivityArm({ shared: { base: process.cwd() }, device: false, triple: 'arm64-apple-ios17.0-simulator', sdk: '/sdk', runApple: (cmd, args) => { console.log(JSON.stringify(args)); throw new Error('stop'); } }); } catch {}`;
    const r = spawnSync(process.execPath, ['-e', script], { cwd: dir, encoding: 'utf8' });
    const sources = JSON.parse(r.stdout || '[]').filter(arg => arg.endsWith('.swift'));
    assert.equal(sources.length, 2, r.stderr);
    for (const source of sources) assert.ok(existsSync(source), source);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('activity refusals allow native APNs without activities and still scan every activity bundle binary', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-activity-scope-'));
  const plain = { manifest: {} }, empty = { manifest: { live_activities: [] } };
  const activity = resolveApp('caltrain');
  const granted = 'net.fetch https://x/\ndevice.notifications purpose.notifications\ndevice.push';
  const runApple = (cmd, args) => {
    const result = spawnSync(cmd, args, { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return result;
  };
  const bytes = readFileSync('/bin/ls');
  const directories = [dir, join(dir, 'PlugIns', 'Widget.appex'), join(dir, 'Frameworks', 'Arm.framework')];
  try {
    for (const directory of directories) {
      mkdirSync(directory, { recursive: true });
      writeFileSync(join(directory, 'Info.plist'), JSON.stringify({ CFBundleExecutable: 'Binary' }));
      writeFileSync(join(directory, 'Binary'), bytes);
    }
    const binaries = [...directories.map(directory => join(directory, 'Binary')), join(dir, 'Frameworks', 'Loose.dylib')];
    writeFileSync(binaries.at(-1), bytes);
    assert.equal(assertActivityRefusals(dir, runApple, activity), binaries.length);
    for (const binary of binaries) {
      for (const symbol of ['registerForRemoteNotifications', 'pushToStartTokenUpdates']) {
        writeFileSync(binary, Buffer.concat([bytes, Buffer.from(`\0${symbol}\0`)]));
        if (symbol === 'registerForRemoteNotifications') for (const app of [plain, empty]) {
          assert.equal(assertActivityRefusals(dir, runApple, app), 0);
        }
        assert.throws(() => assertActivityRefusals(dir, runApple, activity), new RegExp(symbol));
        // With `device.push`, the app registers beside its activities; push-to-start stays refused (LLP 1133.004 §9 P8).
        if (symbol === 'registerForRemoteNotifications') assert.equal(assertActivityRefusals(dir, runApple, activity, granted), binaries.length);
        else assert.throws(() => assertActivityRefusals(dir, runApple, activity, granted), new RegExp(symbol));
      }
      writeFileSync(binary, bytes);
    }
    for (const directory of directories) {
      const plist = join(directory, 'Info.plist');
      writeFileSync(plist, JSON.stringify({ CFBundleExecutable: 'Binary', UIBackgroundModes: ['remote-notification'] }));
      for (const app of [plain, empty]) assert.equal(assertActivityRefusals(dir, runApple, app), 0);
      assert.throws(() => assertActivityRefusals(dir, runApple, activity), /remote-notification/);
      assert.throws(() => assertActivityRefusals(dir, runApple, activity, granted), /remote-notification/);
      writeFileSync(plist, JSON.stringify({ CFBundleExecutable: 'Binary' }));
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// The async iOS step supplies its just-built bundle. Glue has no assembled iOS fixture yet.
test.skipIf(!process.env.EXACT_ACTIVITY_APP)('the assembled app and extension refuse push-to-start symbols, remote registration without device.push, and its background mode (LLP 1126.001 WP9; LLP 1133.004 §9 P8)', async () => {
  const { cpSync, readFileSync, readdirSync } = await import('node:fs');
  const { spawnSync } = await import('node:child_process');
  const { resolve } = await import('node:path');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-activity-refusals-'));
  const runApple = (cmd, args) => {
    const result = spawnSync(cmd, args, { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return result;
  };
  try {
    const app = resolve(process.env.EXACT_ACTIVITY_APP), declaration = resolveApp('caltrain');
    assert.ok(assertActivityRefusals(app, runApple, declaration) >= 3, 'app, extension and ActivityKit arm are scanned');
    cpSync(app, dir, { recursive: true });
    const extension = resolve(dir, 'PlugIns', readdirSync(resolve(dir, 'PlugIns')).find(n => n.endsWith('.appex')));
    for (const directory of [dir, extension]) {
      const plist = resolve(directory, 'Info.plist'), original = readFileSync(plist);
      const info = JSON.parse(runApple('plutil', ['-convert', 'json', '-o', '-', plist]).stdout);
      const binary = resolve(directory, info.CFBundleExecutable), bytes = readFileSync(binary);
      runApple('plutil', ['-replace', 'UIBackgroundModes', '-json', '["remote-notification"]', plist]);
      assert.throws(() => assertActivityRefusals(dir, runApple, declaration), /remote-notification/);
      writeFileSync(plist, original);
      const granted = 'device.notifications purpose.notifications\ndevice.push';
      for (const symbol of ['pushToStartTokenUpdates', 'registerForRemoteNotifications']) {
        writeFileSync(binary, Buffer.concat([bytes, Buffer.from(`\0${symbol}\0`)]));
        assert.throws(() => assertActivityRefusals(dir, runApple, declaration), new RegExp(symbol));
        // `device.push` admits registration beside the activities, never push-to-start (LLP 1133.004 §9 P8).
        if (symbol === 'pushToStartTokenUpdates') assert.throws(() => assertActivityRefusals(dir, runApple, declaration, granted), new RegExp(symbol));
        else assert.ok(assertActivityRefusals(dir, runApple, declaration, granted) >= 3);
      }
      writeFileSync(binary, bytes);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
