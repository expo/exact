import { describe, expect, test } from 'bun:test';
import { createHash } from 'node:crypto';
import { execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { embedArmReach, embedEntitlements, generateEmbedComposition, stageEmbedIntents, stageEmbedWidgets, writeEmbedBundle } from './embed.mjs';
import { assertArchiveFrameworks } from './assets.mjs';
import { writeEmbedPrivacyManifest } from './privacy.mjs';

const mac = process.platform === 'darwin';

describe('LLP 1031.000 embed packaging helpers', () => {
  test('embedArmReach drives arms strictly from graph.loads and L=A manifest.link', () => {
    expect([...embedArmReach({ reaches: [], app: { manifest: {} }, level: '0' })]).toEqual([]);
    expect([...embedArmReach({ reaches: ['video', 'sound', 'unknown'], app: { manifest: {} }, level: '0' })].sort()).toEqual(['sound', 'video']);
    expect([...embedArmReach({ reaches: [], app: { manifest: { link: ['web_views', 'surfaces'] } }, level: '0' })]).toEqual([]);
    expect([...embedArmReach({ reaches: ['svg'], app: { manifest: { link: ['web_views', 'surfaces'] } }, level: 'A' })].sort()).toEqual(['canvas', 'svg', 'web']);
  });

  test('generateEmbedComposition emits deterministic zero-env-var Swift for listed products', () => {
    const embedded = generateEmbedComposition({ products: ['ExactKit', 'ExactNavigation', 'ExactDragIn'], composition: 'embedded' });
    expect(embedded).toContain('import ExactKit');
    expect(embedded).toContain('import ExactNavigation');
    expect(embedded).toContain('#if os(iOS)\nimport ExactDragIn\n#endif');
    expect(embedded).not.toContain('import ExactUpdates');
    expect(embedded).not.toContain('EXACT_LINK_');
    expect(embedded).not.toContain('EXACT_APP_UPDATING');
    expect(embedded).toContain('ExactNavigation.install()');
    expect(embedded).toContain('ExactDragIn.install()');

    const updating = generateEmbedComposition({ products: ['ExactKit', 'ExactNotifications', 'ExactToolDoor'], composition: 'updating' });
    expect(updating).toContain('import ExactUpdates');
    expect(updating).toContain('ExactUpdates.install(on: app)');
    expect(updating).toContain('installNotificationClicks');
    expect(updating).toContain('app.installToolDoor(boot: boot)');
    expect(updating).not.toContain('app.installContentIndex()');
  });

  test('embedEntitlements omits applinks from widget extension entitlements', () => {
    const app = { id: 'com.example.shop', manifest: { widgets: [{ kind: 'today', families: ['systemSmall'] }] } };
    const ent = embedEntitlements({ app, reach: null, hasWidgets: true, ios: true, claimedDomains: ['applinks:shop.example.com'] });
    expect(ent.app['com.apple.developer.associated-domains']).toEqual(['applinks:shop.example.com']);
    expect(ent.app['com.apple.security.application-groups']).toEqual(['group.com.example.shop']);
    expect(ent.widget['com.apple.security.application-groups']).toEqual(['group.com.example.shop']);
    expect(ent.widget['keychain-access-groups']).toEqual(['$(AppIdentifierPrefix)com.example.shop']);
    expect(ent.widget['com.apple.developer.associated-domains']).toBeUndefined();
  });

  test('writeEmbedPrivacyManifest declares ExactKit/Rust reasons and merges app manifests without nm/otool', () => {
    const dir = mkdtempSync(resolve(tmpdir(), 'exact-embed-privacy-'));
    try {
      const appPrivacy = resolve(dir, 'app-privacy.xcprivacy');
      writeFileSync(appPrivacy, `<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0"><dict>
  <key>NSPrivacyTracking</key><false/>
  <key>NSPrivacyAccessedAPITypes</key><array>
    <dict><key>NSPrivacyAccessedAPIType</key><string>NSPrivacyAccessedAPICategoryDiskSpace</string>
      <key>NSPrivacyAccessedAPITypeReasons</key><array><string>E174.1</string></array></dict>
  </array>
</dict></plist>`);
      const out = writeEmbedPrivacyManifest(resolve(dir, 'out'), { manifests: [{ path: appPrivacy, bytes: readFileSync(appPrivacy).toString('base64') }] });
      const text = readFileSync(out, 'utf8');
      expect(text).toContain('NSPrivacyAccessedAPICategoryUserDefaults');
      expect(text).toContain('NSPrivacyAccessedAPICategoryFileTimestamp');
      expect(text).toContain('NSPrivacyAccessedAPICategorySystemBootTime');
      expect(text).toContain('NSPrivacyAccessedAPICategoryActiveKeyboards');
      expect(text).toContain('NSPrivacyAccessedAPICategoryDiskSpace');
      expect(text).toContain('E174.1');
      if (mac) {
        const parsed = JSON.parse(execFileSync('plutil', ['-convert', 'json', '-o', '-', out], { encoding: 'utf8' }));
        expect(parsed.NSPrivacyTracking).toBe(false);
        expect(parsed.NSPrivacyAccessedAPITypes.length).toBeGreaterThanOrEqual(5);
      }
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });

  test('stageEmbedIntents emits ExactIntents.swift, launch parts and launchLink without ExactIntents.o', () => {
    const dir = mkdtempSync(resolve(tmpdir(), 'exact-embed-intents-'));
    try {
      const launchDir = resolve(dir, 'launch');
      mkdirSync(launchDir);
      writeFileSync(resolve(launchDir, 'Registry.swift'), 'public enum ExactLaunchParts {}\n');
      const compat = {
        inputs: {
          faces: [{
            signature: { name: 'addTask', description: 'Add a task', parameters: [['title', 'string']], consequence: 'writes' },
            face: { doors: ['apple'], title: 'Add Task', digest: 'abc123', parameters: { title: { title: 'Title' } } },
          }],
        },
      };
      const result = stageEmbedIntents({
        embed: resolve(dir, 'embed'),
        app: { manifest: { intents: {} } },
        compat,
        launchPartsDir: launchDir,
        nativeIntents: [null, { moduleName: 'ExactAppModules', binary: '/tmp/libexact_modules.dylib' }],
        ios: true,
      });
      expect(result.sources).toEqual(['intents/ExactIntents.swift', 'intents/LaunchParts/Registry.swift']);
      expect(result.tools).toEqual(['addTask']);
      expect(result.launchLink).toContain('Frameworks/ExactModules.framework/ExactModules');
      expect(existsSync(resolve(dir, 'embed/intents/ExactIntents.swift'))).toBe(true);
      expect(existsSync(resolve(dir, 'embed/intents/ExactIntents.o'))).toBe(false);
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });

  test.skipIf(!mac)('writeEmbedBundle wraps iOS dylibs into .framework bundles passing assertArchiveFrameworks and preserves capabilities.json shape', () => {
    const dir = mkdtempSync(resolve(tmpdir(), 'exact-embed-bundle-'));
    try {
      const libDir = resolve(dir, 'capture'), ns = resolve(dir, 'ns'), embedOut = resolve(ns, 'embed'), scratch = resolve(dir, 'scratch');
      mkdirSync(libDir, { recursive: true });
      mkdirSync(ns, { recursive: true });
      mkdirSync(scratch, { recursive: true });
      writeFileSync(resolve(libDir, 'libdemo_apple.a'), '!<arch>\n');
      // Build a real tiny arm64 iOS simulator dylib so wrapFramework (install_name_tool + nm/otool) runs end to end.
      const src = resolve(dir, 'tiny.c'), dylib = resolve(dir, 'libexact_video.dylib');
      writeFileSync(src, 'int exact_embed_tiny(void) { return 42; }\n');
      execFileSync('xcrun', ['--sdk', 'iphonesimulator', 'clang', '-dynamiclib', '-target', 'arm64-apple-ios17.0-simulator', src, '-o', dylib]);

      const app = { id: 'com.example.demo', name: 'demo', displayName: 'Demo', dir, manifest: { host: { ios: { minimumOS: '17.0' } } } };
      const bakedPlan = Buffer.from('plan');
      const compat = {
        id: '0123456789abcdef0123456789abcdef',
        inputs: { app: app.id, store: { L: '0' }, assets: {} },
        embedded: { plan: { sha256: createHash('sha256').update(bakedPlan).digest('hex'), bytes: bakedPlan.length }, assets: [] },
      };
      const cleanup = [];
      writeEmbedBundle({
        app, ios: true, device: false, tv: false, target: 'aarch64-apple-ios-sim',
        sdk: '/sdk', sdkName: 'iphonesimulator', triple: 'arm64-apple-ios17.0-simulator',
        crate: 'demo-apple', libDir, paths: { namespace: ns, capture: libDir, embed: embedOut }, cleanup,
        buildReceipt: { binary: { privacyManifests: [], metadata: {} }, graph: { loads: ['video'], artifacts: [] } },
        bakedCompat: compat, bakedPlan, composition: 'embedded', optimize: 'size', production: false, distribution: false,
        appleLink: 'all', swiftCapabilities: ['ExactNavigation', 'ExactVideo'], gpuArtifacts: [], hasGpu: false, loadName: 'libexact_gpu.dylib',
        frameworkBundles: true, frameworkPlatform: 'iPhoneSimulator', svgFilterBuilt: null, svgFilterLibraryName: 'ExactSvgFilters.metallib', library: null,
        looseArms: [[true, dylib, 'libexact_video.dylib', 'ExactVideo']],
        copyAppleGpuProducts: () => {}, copyAppleStaticTrees: () => {},
        placeAppleArtifact: (srcDir, dstDir) => { rmSync(dstDir, { recursive: true, force: true }); mkdirSync(dstDir, { recursive: true }); execFileSync('cp', ['-R', `${srcDir}/.`, dstDir]); },
        receipt: (_app, fields) => JSON.stringify(fields, null, 2), writeUsageStrings: () => {}, usageKeys: () => ({}), runApple: () => {},
      });

      assertArchiveFrameworks(resolve(embedOut, 'Frameworks'));
      expect(existsSync(resolve(embedOut, 'Frameworks/ExactVideo.framework/ExactVideo'))).toBe(true);
      expect(existsSync(resolve(embedOut, 'Frameworks/ExactVideo.framework/Info.plist'))).toBe(true);
      expect(existsSync(resolve(embedOut, 'Frameworks/ExactVideo.framework/PrivacyInfo.xcprivacy'))).toBe(true);
      expect(existsSync(resolve(embedOut, 'ExactEmbedComposition.swift'))).toBe(true);
      expect(existsSync(resolve(embedOut, 'PrivacyInfo.xcprivacy'))).toBe(true);
      const caps = JSON.parse(readFileSync(resolve(embedOut, 'capabilities.json'), 'utf8'));
      expect(caps.products).toEqual(['ExactKit', 'ExactNavigation', 'ExactVideo']);
      expect(caps.install).toEqual(['ExactNavigation.install()', 'ExactVideo.install()']);
      expect(caps.embed.link).toEqual({ mode: 'static-archive', archive: 'libdemo_apple.a', sharedCore: null, headers: 'include' });
      expect(caps.embed.frameworks).toEqual(['Frameworks/ExactVideo.framework']);
      expect(caps.embed.compositionSource).toBe('ExactEmbedComposition.swift');
      expect(caps.embed.privacyManifest).toBe('PrivacyInfo.xcprivacy');
      for (const tmp of cleanup) rmSync(tmp, { recursive: true, force: true });
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });

  test('stageEmbedWidgets emits consumer-owned sources and excludes applinks from widget entitlements', () => {
    const dir = mkdtempSync(resolve(tmpdir(), 'exact-embed-widgets-'));
    try {
      const embed = resolve(dir, 'embed'), capture = resolve(dir, 'capture');
      mkdirSync(resolve(capture, 'assets/fonts'), { recursive: true });
      mkdirSync(resolve(capture, 'plans'), { recursive: true });
      writeFileSync(resolve(capture, 'assets/fonts/Inter.ttf'), 'font');
      writeFileSync(resolve(capture, 'plans/today.plan'), 'plan');
      const app = {
        id: 'com.example.shop', dir,
        manifest: { widgets: [{ kind: 'today', families: ['systemSmall'], title: 'Today' }] },
      };
      const ent = embedEntitlements({ app, reach: null, hasWidgets: true, ios: true, claimedDomains: ['applinks:shop.example.com'] });
      const res = stageEmbedWidgets({
        embed, app, captureDir: capture, compat: { inputs: {} }, shared: null,
        triple: 'arm64-apple-ios17.0-simulator', sdk: '/sdk', device: false, production: false,
        runApple: () => {}, alarm: 'ExactAlarm', frameworkPlatform: 'iPhoneSimulator', generation: 'gen1',
        widgetEntitlements: ent.widget,
      });
      expect(res.moduleName).toBe('ExactAlarm');
      expect(res.weakFrameworks).toEqual(['AlarmKit']);
      expect(res.sources).toContain('widgets/Sources/WidgetBundle.swift');
      expect(res.sources).toContain('widgets/Sources/Alarm.swift');
      expect(res.entitlements['com.apple.developer.associated-domains']).toBeUndefined();
      expect(existsSync(resolve(embed, 'widgets/Sources/WidgetBundle.swift'))).toBe(true);
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });
});
