// Info.plist rows that `build.mjs`'s `infoPlist` and `macInfoPlist` write from the manifest, the grants and the app's
// packages.

import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { appleImplementations } from '../../js/bake/src/typescript.mjs';

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
      if (typeof strings[k] !== 'string') throw new Error(`host/apple: ${key} names "${k}", which strings/${base}.json lacks`);
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
