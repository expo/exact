// Info.plist rows that `build.mjs`'s `infoPlist` and `macInfoPlist` write from the manifest, the grants and the app's
// packages.

import { readFileSync, readdirSync } from 'node:fs';
import { resolve } from 'node:path';
import { grantLines } from '../../scripts/grant-advice.mjs';
import { appleImplementations } from '../../js/bake/src/typescript.mjs';

// Python's standard plistlib handles Apple's XML and binary encodings on a Linux builder.
const PLIST_TOOL = String.raw`
import base64, datetime, json, pathlib, plistlib, re, sys

def json_value(value):
    if isinstance(value, bytes): return base64.b64encode(value).decode('ascii')
    if isinstance(value, datetime.datetime): return value.isoformat() + ('Z' if value.tzinfo is None else '')
    raise ValueError('unsupported plist value')

def main(args):
    operation = args.pop(0)
    key = args.pop(0) if operation in ['-extract', '-insert', '-replace', '-remove'] else None
    kind = args.pop(0) if operation in ['-convert', '-extract', '-insert', '-replace'] else None
    value = args.pop(0) if operation in ['-insert', '-replace'] else None
    output = None
    if '-o' in args:
        at = args.index('-o'); output = args[at + 1]; del args[at:at + 2]
    if args[:1] == ['--']: args.pop(0)
    if len(args) != 1: raise ValueError('expected one plist file')
    path = pathlib.Path(args[0]); data = sys.stdin.buffer.read() if args[0] == '-' else path.read_bytes()
    try: document = plistlib.loads(data)
    except plistlib.InvalidFileException: document = json.loads(data)
    result = document
    if key is not None:
        keys = [word.replace(r'\.', '.').replace(r'\\', '\\') for word in re.split(r'(?<!\\)\.', key)]
        parent = document
        for word in keys[:-1]: parent = parent[int(word) if isinstance(parent, list) else word]
        leaf = int(keys[-1]) if isinstance(parent, list) else keys[-1]
        if operation == '-extract': result = parent[leaf]
        elif operation == '-remove': del parent[leaf]
        else:
            if operation == '-insert' and not isinstance(parent, list) and leaf in parent: raise ValueError('key already exists')
            if kind == '-string': parsed = value
            elif kind == '-bool':
                if value.lower() not in ['true', 'false', 'yes', 'no', '1', '0']: raise ValueError('invalid boolean')
                parsed = value.lower() in ['true', 'yes', '1']
            elif kind == '-integer': parsed = int(value)
            elif kind == '-float': parsed = float(value)
            elif kind == '-json': parsed = json.loads(value)
            else: raise ValueError('unsupported plist mutation type')
            if operation == '-insert' and isinstance(parent, list): parent.insert(leaf, parsed)
            else: parent[leaf] = parsed
    if operation not in ['-convert', '-extract', '-insert', '-replace', '-remove']: raise ValueError('unsupported plist operation')
    if operation in ['-insert', '-replace', '-remove']:
        fmt = plistlib.FMT_BINARY if data.startswith(b'bplist') else plistlib.FMT_XML
        path.write_bytes(plistlib.dumps(document, fmt=fmt, sort_keys=False)); return
    if kind == 'json': encoded = json.dumps(result, default=json_value, ensure_ascii=False, allow_nan=False).encode()
    elif kind in ['xml1', 'binary1']: encoded = plistlib.dumps(result, fmt=plistlib.FMT_XML if kind == 'xml1' else plistlib.FMT_BINARY, sort_keys=False)
    elif kind == 'raw' and isinstance(result, (str, int, float, bool)):
        encoded = (str(result).lower() if isinstance(result, bool) else str(result)).encode()
    else: raise ValueError('unsupported plist output type')
    if output == '-': sys.stdout.buffer.write(encoded)
    else: pathlib.Path(output or path).write_bytes(encoded)

try: main(sys.argv[1:])
except Exception as error:
    print('plist: ' + str(error), file=sys.stderr); sys.exit(1)
`;

export const plistCommand = args => ['python3', ['-c', PLIST_TOOL, ...args]];

// The global exceptions narrower than `NSAllowsArbitraryLoads`: ATS ignores that key when any of them is present
// (Apple, "NSAllowsArbitraryLoads"), and it already allows what each of them would.
const NARROWER = ['NSAllowsArbitraryLoadsForMedia', 'NSAllowsArbitraryLoadsInWebContent', 'NSAllowsLocalNetworking'];

/** `NSAppTransportSecurity` from `host.<platform>.appTransportSecurity` and the build's own `keys`; none of either, no
 * key. `allowsArbitraryLoadsInWebContent` relaxes ATS for web views only (an `iframe` loads `http://` from a named
 * host, as a browser does); a module's `URLSession` stays under ATS. `allowsArbitraryLoads` turns ATS off for every
 * connection the app makes, web views included, and is written without the narrower keys. `web: false` (tvOS, which
 * has no WebKit) leaves out the web-content key. */
export const transportSecurity = (section, keys = {}, { web = true } = {}) => {
  const typed = section?.appTransportSecurity ?? {};
  const ats = { ...(web && typed.allowsArbitraryLoadsInWebContent ? { NSAllowsArbitraryLoadsInWebContent: true } : {}), ...keys };
  if (typed.allowsArbitraryLoads) {
    for (const key of NARROWER) delete ats[key];
    ats.NSAllowsArbitraryLoads = true;
  }
  return Object.keys(ats).length ? { NSAppTransportSecurity: ats } : {};
};

/** `NSAllowsLocalNetworking`, which lets `URLSession` load `http:` from a local host: the development client's
 * `host.ios.localNetworking`, or a `net.fetch local-network` or `net.websocket local-network` line
 * (`reach.localNetworking`, LLP 1133.036 D5). */
export const localNetworking = (section, reach) => (section?.localNetworking || reach?.localNetworking ? { NSAllowsLocalNetworking: true } : {});

/** The app's declared origin (`app.origin`), which a link's Copy Link and Share put a path in the app on. */
export const declaredOrigin = ({ origin }) => (origin ? { ExactOrigin: new URL(origin).origin } : {});

/** A launch no link started opens at the manifest's `start_url`, as an installed web app does (W3C App Manifest): resolved
 * against the manifest's URL, on the app's declared origin when it has one, and baked when it is a place in the app (on
 * that origin) other than the root. */
export const startLocation = ({ manifest: { start_url: at }, origin: declared }) => {
  if (typeof at !== 'string' || !at) return {};
  const origin = declared ? new URL(declared).origin : 'https://exact.invalid';
  const url = new URL(at, `${origin}/manifest.json`), place = url.pathname + url.search + url.hash;
  return url.origin === origin && place !== '/' ? { ExactStartURL: place } : {};
};

/** The manifest's `userInterfaceStyle` (Expo's key): a fixed `light` or `dark` is UIKit's `UIUserInterfaceStyle`,
 * which the launch screen, every window and the system's own chrome take as the device's appearance; `automatic` or
 * none writes nothing. AppKit has no key that fixes dark (`NSRequiresAquaSystemAppearance` fixes only light), so the
 * Mac's row is Exact's `ExactUserInterfaceStyle`, which ExactMac sets as `NSApp.appearance` before its first window. */
export const interfaceStyle = ({ manifest: { userInterfaceStyle: style } }, mac = false) =>
  style === 'light' || style === 'dark' ? (mac ? { ExactUserInterfaceStyle: style } : { UIUserInterfaceStyle: style === 'dark' ? 'Dark' : 'Light' }) : {};

/** Each value `categories` admits, with the category Apple files it under (`LSApplicationCategoryType`'s values,
 * without `public.app-category.`), or null where Apple has none. The W3C's standardized categories (the manifest
 * wiki's "Categories" page) come first; the rest are Apple's own, each a category the W3C list has no name for. The
 * schema's enum is this list (`store-keys.test.mjs`). */
export const CATEGORIES = new Map([
  ...['business', 'education', 'entertainment', 'finance', 'games', 'lifestyle', 'medical', 'music', 'news', 'productivity', 'sports', 'travel', 'utilities', 'weather'].map((c) => [c, c]),
  ['fitness', 'healthcare-fitness'], ['health', 'healthcare-fitness'], ['photo', 'photography'], ['social', 'social-networking'],
  ...['books', 'food', 'government', 'kids', 'magazines', 'navigation', 'personalization', 'politics', 'security', 'shopping'].map((c) => [c, null]),
  ...['developer-tools', 'graphics-design', 'reference', 'video'].map((c) => [c, c]),
  ...['action', 'adventure', 'arcade', 'board', 'card', 'casino', 'dice', 'educational', 'family', 'kids', 'music', 'puzzle', 'racing', 'role-playing', 'simulation', 'sports', 'strategy', 'trivia', 'word'].map((g) => [`${g}-games`, `${g}-games`]),
]);

/** Apple's category for the manifest: its first `categories` entry Apple has one for, else `games` for a game the
 * game shell builds (`game`), else none. */
export function appleCategory({ categories = [], game }) {
  const named = categories.map((c) => CATEGORIES.get(c)).find(Boolean);
  return named ?? (game ? 'games' : null);
}

const atLeast = (version, floor) => {
  const [a, b] = [String(version), floor].map((v) => v.split('.').map(Number));
  return (a[0] - b[0] || (a[1] ?? 0) - (b[1] ?? 0) || (a[2] ?? 0) - (b[2] ?? 0)) >= 0;
};

/** The rows that file the app under its category and offer Game Mode, on `platform` (`ios` or `macos`).
 * `LSApplicationCategoryType` is the category the Mac App Store, Finder and Launchpad file the app under, and the one
 * App Store Connect requires of a Mac upload; Apple documents it for macOS, and the iOS bundle carries it too. Game
 * Mode (Apple, "LSSupportsGameMode": iOS 18.6, macOS 26) gives a game priority on the CPU and GPU and lower Bluetooth
 * latency, and a game without the key "might not" get it: so a game (an Apple category of `games` or a genre of game)
 * is written `YES` unless `host.<platform>.gameMode` is false, and any app may ask for it. iOS 18.0 to 18.5 read
 * `GCSupportsGameMode` instead, which an app whose `minimumOS` is below 18.6 carries too. */
export function storeKeys({ manifest }, platform) {
  const category = appleCategory(manifest), section = manifest.host?.[platform] ?? {};
  const game = category === 'games' || !!category?.endsWith('-games');
  const mode = section.gameMode ?? (game ? true : undefined);
  const older = platform === 'ios' && mode !== undefined && !atLeast(section.minimumOS ?? '17.0', '18.6');
  return { ...(category ? { LSApplicationCategoryType: `public.app-category.${category}` } : {}),
    ...(mode !== undefined ? { LSSupportsGameMode: mode } : {}), ...(older ? { GCSupportsGameMode: mode } : {}) };
}

export const MEMORY_ENTITLEMENT = 'com.apple.developer.kernel.increased-memory-limit';

/** `host.ios.increasedMemoryLimit`'s entitlement, which lets the app exceed the default memory limit on the iPhone
 * and iPad models that allow it (Apple, "Increased Memory Limit", iOS 15). The app's own: not an extension's or an App
 * Clip's, and not tvOS's, which Apple does not list for it. It is a capability of the App ID, so a device build's
 * profile must grant it. */
export const memoryEntitlements = (app) => app.manifest.host?.ios?.increasedMemoryLimit === true
  && !app.widgetTV && !app.widgetHost && !app.widgetAppId && !app.clipParent ? [MEMORY_ENTITLEMENT] : [];

/** When a downloaded update applies and how often the app checks (`deploy.activate`, `deploy.check.interval`). */
export const updateActivation = (app) => ({ ...(app.manifest.deploy?.activate ? { ExactUpdateActivate: app.manifest.deploy.activate } : {}),
  ...(app.manifest.deploy?.check?.interval != null ? { ExactUpdateCheckInterval: app.manifest.deploy.check.interval } : {}) });

/** The iOS rows for background work (LLP 1133.017 D14): `UIBackgroundModes`, the manifest's
 * `host.ios.backgroundModes` with the modes the grant lines derive (`reach.backgroundModes`), and
 * `BGTaskSchedulerPermittedIdentifiers`, the identifiers they permit after the bundle id `id`. `fetch` and `processing` come only from a grant line, so a mode has one way to be declared. `only`: the
 * manifest's modes alone, for tvOS, whose refresh is later (D17), and an App Clip, which has no background tasks. */
export function backgroundKeys(ios, reach, id, only) {
  const declared = ios.backgroundModes ?? [];
  const derived = declared.filter((mode) => mode === 'fetch' || mode === 'processing');
  if (derived.length) throw new Error(`host.ios.backgroundModes: ${derived.join(' and ')} comes from a grant line (device.background-sync, device.periodic-background-sync; LLP 1133.017 D14), not the manifest`);
  const modes = [...new Set([...declared, ...(only ? [] : reach?.backgroundModes ?? [])])];
  const tasks = only ? [] : (reach?.tasks ?? []).map((task) => `${id}.${task}`);
  return { ...(modes.length ? { UIBackgroundModes: modes } : {}), ...(tasks.length ? { BGTaskSchedulerPermittedIdentifiers: tasks } : {}) };
}

/** The tag reader session's lists from `host.ios.nfc` (LLP 1133.035 D11, D16 item 2): the ISO 7816 application
 * identifiers it selects and the FeliCa system codes it polls for. Core NFC reports no ISO 7816 tag whose identifier
 * the app does not list ("add the list of the application identifiers supported in your app", NFCTagReaderSession).
 * Nothing on tvOS, which has no Core NFC. */
export function nfcKeys(ios, tv) {
  const nfc = tv ? {} : ios.nfc ?? {};
  return {
    ...(nfc.iso7816Identifiers?.length ? { 'com.apple.developer.nfc.readersession.iso7816.select-identifiers': nfc.iso7816Identifiers } : {}),
    ...(nfc.felicaSystemCodes?.length ? { 'com.apple.developer.nfc.readersession.felica.systemcodes': nfc.felicaSystemCodes } : {}),
  };
}

/** The Info.plist keys a package's Apple implementation may declare in its `exact.json` `"plistKeys"`, by name, with
 * the one value each may take and the platforms whose plist takes it, as Android's `METADATA`
 * (scripts/android-manifest.mjs) bounds a manifest fragment's meta-data. Only a named key passes, so a package can set
 * no key the build writes, no usage string and nothing that changes trust. A package that needs another key amends
 * this list. */
export const PLIST_KEYS = new Map([
  // StoreKit then keeps finished consumables in `Transaction.all`, `Transaction.latest(for:)` and
  // `Product.latestTransaction` (Apple, "SKIncludeConsumableInAppPurchaseHistory": iOS 18, macOS 15, tvOS 18), so
  // `@exact/purchases`' `finish`, which looks in `Transaction.all`, answers a consumable finished before.
  ['SKIncludeConsumableInAppPurchaseHistory', { value: true, platforms: ['ios', 'macos', 'tvos'] }],
]);

/** The keys the implementations `bound` (`appleImplementations`' list) declare in `"plistKeys"`, as
 * `{ extension, key, value }`, those `target`'s plist takes. Throws, naming the package, for a key or value not on
 * [PLIST_KEYS]. */
export function packageKeys(bound, target) {
  const allowed = [...PLIST_KEYS].map(([key, { value }]) => `${key}: ${JSON.stringify(value)}`).join(', ');
  const keys = [];
  for (const impl of bound) {
    const declared = impl.plistKeys;
    if (declared == null) continue;
    const refuse = (what) => { throw new Error(`${impl.extension}'s Apple implementation: its plistKeys ${what} (the list allows only ${allowed}; host/apple/plist.mjs PLIST_KEYS)`); };
    if (typeof declared !== 'object' || Array.isArray(declared)) refuse('is not an object');
    for (const [key, value] of Object.entries(declared)) {
      const entry = PLIST_KEYS.get(key);
      if (!entry) refuse(`names ${key}, which is not on the list`);
      if (entry.value !== value) refuse(`gives ${key} = ${JSON.stringify(value)}, which is not on the list`);
      if (entry.platforms.includes(target)) keys.push({ extension: impl.extension, key, value });
    }
  }
  return keys;
}

/** `reach` with what the app's packages' implementations for `target` add to Info.plist (`packages`, as
 * `{ extension, key, value }`), whether or not the app grants a device: each `"plistKeys"` key, and each `"plist"`
 * dictionary, keyed by the strings keys the app passes and valued by their base text (LLP 1133.002 D10). A key two
 * packages declare is written once. */
export const modulePlist = (app, reach, target, extensions, contractSources = []) => {
  const bound = appleImplementations(app.dir, target, extensions, contractSources);
  const dictionaries = bound.flatMap((p) => p.plist.map((d) => ({ ...d, extension: p.extension })));
  const packages = packageKeys(bound, target).filter((k, i, all) => all.findIndex((o) => o.key === k.key) === i);
  if (dictionaries.length) {
    const base = reach?.base ?? 'en', strings = JSON.parse(readFileSync(resolve(app.dir, `strings/${base}.json`), 'utf8'));
    for (const { extension, key, keys } of dictionaries) packages.push({ extension, key, value: Object.fromEntries(keys.map((k) => {
      // A blank text is no purpose, as for a grant's (bake/src/reach.rs).
      if (typeof strings[k] !== 'string' || !strings[k].trim()) throw new Error(`host/apple: ${key} names "${k}", which strings/${base}.json lacks or leaves blank`);
      return [k, strings[k]];
    })) });
  }
  return packages.length ? { ...reach, packages } : reach;
};

/** `dict`, the plist the build wrote, with `reach.packages` (from [modulePlist]) added. Throws, naming the package,
 * for a key the build already wrote. */
export function withPackageKeys(dict, reach) {
  for (const { extension, key, value } of reach?.packages ?? []) {
    if (Object.hasOwn(dict, key)) throw new Error(`${extension}'s Apple implementation declares the Info.plist key ${key}, which the build writes`);
    dict[key] = value;
  }
  return dict;
}

/** Native tags that must be available before the first frame, from the qualified module roster. */
export const firstFrameKeys = (roster, tv = false) => {
  const tags = Object.keys(roster).filter(tag => tag.includes('-') && roster[tag].firstFrame === true);
  return tags.length && !tv ? { ExactFirstFrameModules: tags } : {};
};

/** AlarmKit's countdown is a Live Activity on iOS. */
export const alarmKeys = (alarm, tv = false) => alarm && !tv ? { NSSupportsLiveActivities: true, ExactAlarmCountdown: true } : {};

let metadataOwners;
/** Known derived keys; only advice exercised through the bake and platform writers names an owner. */
export function appleMetadataOwners() {
  if (metadataOwners) return metadataOwners;
  const hosts = Object.fromEntries(['ios', 'macos'].map(platform => [platform, { infoPlist: new Map(), entitlements: new Map() }]));
  const add = (map, keys, owner) => {
    for (const key of keys) map.set(key, [...new Set([...(map.get(key) ?? []), ...(owner ? [owner] : [])])]);
  };
  const groups = [
    [['ios', 'macos'], ['CFBundleIdentifier']],
    [['ios', 'macos'], ['CFBundleName', 'CFBundleDisplayName', 'CFBundleExecutable']],
    [['ios', 'macos'], ['CFBundleShortVersionString']],
    [['ios', 'macos'], ['CFBundleVersion']],
    [['ios', 'macos'], ['CFBundlePackageType']],
    [['ios'], [ 'DTCompiler', 'DTPlatformBuild', 'DTPlatformVersion', 'DTSDKBuild', 'DTSDKName', 'DTXcode', 'DTXcodeBuild', 'BuildMachineOSBuild']],
    [['ios'], ['CFBundleSupportedPlatforms', 'DTPlatformName', 'CADisableMinimumFrameDurationOnPhone', 'UIApplicationSceneManifest', 'UIRequiredDeviceCapabilities']],
    [['macos'], ['NSHighResolutionCapable']],
    [['ios'], ['MinimumOSVersion']],
    [['macos'], ['LSMinimumSystemVersion']],
    [['ios'], ['UIDeviceFamily']],
    [['ios'], ['UISupportedInterfaceOrientations', 'UISupportedInterfaceOrientations~ipad']],
    [['ios'], ['UIUserInterfaceStyle']],
    [['macos'], ['ExactUserInterfaceStyle']],
    [['ios', 'macos'], ['NSAppTransportSecurity']],
    [['ios'], ['LSApplicationQueriesSchemes']],
    [['ios'], ['com.apple.developer.nfc.readersession.iso7816.select-identifiers']],
    [['ios'], ['com.apple.developer.nfc.readersession.felica.systemcodes']],
    [['ios'], ['UIBackgroundModes']],
    [['ios', 'macos'], ['ExactModuleConfig']],
    [['ios'], ['ExactAudioSession']],
    [['ios', 'macos'], ['CFBundleDocumentTypes', 'UTExportedTypeDeclarations']],
    [['ios'], ['LSSupportsOpeningDocumentsInPlace', 'UTImportedTypeDeclarations']],
    [['ios', 'macos'], ['ExactProtocolHandlers', 'CFBundleURLTypes']],
    [['ios', 'macos'], ['CFBundleURLTypes']],
    [['ios', 'macos'], ['ExactOpenURLSchemes']],
    [['ios', 'macos'], ['ExactDevelopmentURLScheme', 'ExactDevelopmentOrigins', 'ExactDevelopmentToken']],
    [['ios', 'macos'], ['ExactOrigin']],
    [['ios', 'macos'], ['ExactStartURL']],
    [['macos'], ['ExactLaunchMode']],
    [['ios', 'macos'], ['ExactUpdateActivate']],
    [['ios', 'macos'], ['ExactUpdateCheckInterval']],
    [['ios', 'macos'], ['LSApplicationCategoryType']],
    [['ios', 'macos'], ['LSSupportsGameMode']],
    [['ios'], ['GCSupportsGameMode']],
    [['ios'], ['UIApplicationShortcutItems']],
    [['macos'], ['ExactShortcutItems']],
    [['ios'], ['NSAppClip']],
    [['ios', 'macos'], ['ExactAppGroup', 'ExactAppIdentifier', 'ExactWidgetKinds', 'ExactWidgetTypeScript', 'ExactWidgetGeneration', 'ExactKeychainAccessGroup']],
    [['ios'], ['NSSupportsLiveActivities', 'NSSupportsLiveActivitiesFrequentUpdates']],
    [['macos'], ['ExactWidgetRequiresSecrets']],
    [['ios', 'macos'], ['ExactFirstFrameModules']],
    [['macos'], ['LSUIElement']],
    [['macos'], ['ExactWindow']],
    [['macos'], ['ExactDisplayOverride']],
    [['macos'], ['NSAutoFillRequiresTextContentTypeForOneTimeCodeOnMac']],
    [['macos'], ['CFBundleIconFile']],
    [['ios'], ['CFBundleIcons', 'CFBundleIcons~ipad']],
    [['macos'], ['CFBundleIconName']],
    [['ios', 'macos'], ['NSAccentColorName']],
    [['ios'], ['UILaunchScreen', 'ExactLaunchColor', 'ExactLaunchImageSource', 'ExactLaunchImageSHA256']],
    [['ios', 'macos'], ['CFBundleDevelopmentRegion', 'CFBundleLocalizations']],
    [['ios', 'macos'], ['NSBonjourServices']],
    [['ios', 'macos'], ['ExactShareTarget', 'ExactShareGroup']],
    [['ios', 'macos'], ['ExactFrameHealthMode']],
    [['ios', 'macos'], ['ExactReleaseNotes']],
    [['ios', 'macos'], ['ExactDevMenu']],
    [['ios', 'macos'], ['ExactDistributionRevision']],
    [['ios'], ['ITSAppUsesNonExemptEncryption']],
    [['ios', 'macos'], ['ExactBuildTime', 'ExactBuildKind', 'ExactBuildHost', 'ExactBuildXcode', 'ExactCommit', 'ExactCommitDirty', 'ExactBranch', 'ExactAppCommit', 'ExactAppCommitDirty', 'ExactAppBranch', 'ExactMetalSources']],
    [['ios', 'macos'], ['NSLocationTemporaryUsageDescriptionDictionary']],
  ];
  for (const [platform, { infoPlist }] of Object.entries(hosts)) {
    for (const [platforms, keys] of groups) if (platforms.includes(platform)) add(infoPlist, keys);
    for (const [key, { platforms }] of PLIST_KEYS) if (platforms.includes(platform)) add(infoPlist, [key]);
  }
  add(hosts.macos.infoPlist, ['ExactWidgetSecretsAvailable']);
  add(hosts.ios.infoPlist, ['ExactAlarmCountdown', 'NSSupportsLiveActivities']);
  for (const [platform, keys] of [
    ['ios', ['application-identifier', 'com.apple.developer.team-identifier', 'keychain-access-groups', 'get-task-allow',
      'com.apple.security.application-groups', 'com.apple.developer.parent-application-identifiers',
      'com.apple.developer.associated-domains', 'com.apple.developer.web-browser', MEMORY_ENTITLEMENT,
      'com.apple.developer.applesignin', 'com.apple.developer.in-app-payments']],
    ['macos', ['com.apple.application-identifier', 'com.apple.developer.team-identifier', 'com.apple.security.application-groups',
      'com.apple.developer.associated-domains', 'keychain-access-groups', 'com.apple.security.app-sandbox',
      'com.apple.security.network.client', 'com.apple.security.get-task-allow']],
  ]) add(hosts[platform].entitlements, keys);
  const table = readFileSync(new URL('../../runner/src/device.rs', import.meta.url), 'utf8');
  const advice = grant => `write ${grantLines([grant], table)[0].map(line => `\`${line}\``).join(' beside ')}`;
  for (const [, name, row] of table.matchAll(/name: "([^"]+)",([\s\S]*?)\.\.ROW/g)) {
    const keys = field => [...(new RegExp(`\\b${field}: &\\[([^\\]]*)\\]`).exec(row)?.[1] ?? '').matchAll(/"([^"]+)"/g)].map(m => m[1]);
    for (const [platform, { infoPlist }] of Object.entries(hosts)) {
      const usage = keys(platform);
      const companion = usage.length ? [...row.matchAll(/companion: Some\("([^"\n]+)"\)/g)].map(m => m[1]) : [];
      add(infoPlist, [...usage, ...companion], advice(`device.${name}`));
      if (platform === 'ios') add(infoPlist, keys('ios_flags'));
    }
    if (keys('ios_modes').length) add(hosts.ios.infoPlist, ['UIBackgroundModes']);
    if (keys('ios_tasks').length) add(hosts.ios.infoPlist, ['BGTaskSchedulerPermittedIdentifiers']);
    add(hosts.ios.entitlements, keys('ios_entitlements'));
    add(hosts.macos.entitlements, [...keys('macos_profile'), ...[...row.matchAll(/hardened: Some\("([^"\n]+)"\)/g)].map(m => m[1])]);
  }
  add(hosts.ios.entitlements, ['aps-environment'], advice('device.push'));
  add(hosts.ios.entitlements, ['com.apple.developer.applesignin'], advice('auth.apple'));
  const hardened = readFileSync(new URL('../../runner/src/hardened.rs', import.meta.url), 'utf8');
  add(hosts.macos.entitlements, [...hardened.matchAll(/entitlement: "([^"]+)"/g)].map(m => m[1]));
  add(hosts.ios.infoPlist, ['WKApplication', 'WKCompanionAppBundleIdentifier', 'WKRunsIndependentlyOfCompanionApp']);
  const packages = new URL('../../packages/', import.meta.url);
  for (const directory of readdirSync(packages, { withFileTypes: true })) {
    if (!directory.isDirectory()) continue;
    let spec;
    try { spec = JSON.parse(readFileSync(new URL(`${directory.name}/exact.json`, packages), 'utf8')); }
    catch (error) { if (error.code === 'ENOENT') continue; throw error; }
    for (const declaration of spec.names ?? []) for (const [platform, { infoPlist }] of Object.entries(hosts)) {
      const addDerived = value => {
        if (Array.isArray(value)) add(infoPlist, value);
        else if (typeof value === 'string') add(infoPlist, [value]);
        else if (value && typeof value === 'object') for (const child of Object.values(value)) addDerived(child);
      };
      addDerived(declaration.derives?.[platform]);
    }
  }
  return metadataOwners = hosts;
}

/** Refusal-only manifest dictionaries: name every supplied key, including keys no writer owns yet. */
export function appleMetadataProblem(problem) {
  const match = /^(host\.(ios|macos)\.(infoPlist|entitlements))\.([\s\S]*): not a known key$/.exec(problem);
  if (!match) return problem;
  const [, path, platform, kind, key] = match;
  const owners = appleMetadataOwners()[platform][kind].get(key);
  const advice = owners?.length ? (owners.length > 1 ? `choose one: ${owners.join(' or ')}` : owners[0]) : owners
    ? 'Exact writes this key itself from its declarations (docs/reference.md#apple-bundle-metadata), so it cannot be set here'
    : `not supported on ${platform}: Exact does not write this key; file a \`bun exact.mjs feedback note\` from the app directory naming ${JSON.stringify(key)} and what the native module needs`;
  return `${path}.${key}: refused; ${advice}`;
}
