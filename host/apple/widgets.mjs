import { writePrivacyManifest } from './privacy.mjs';
// @ref LLP 1126 D2/D8 — one runtime framework, one generated widget bundle.
import { dirname, resolve } from 'node:path';
import { mkdirSync, readdirSync, existsSync, writeFileSync, readFileSync, copyFileSync, cpSync, rmSync, renameSync, statSync, lstatSync, utimesSync } from 'node:fs';
import { createHash, X509Certificate } from 'node:crypto';
import { spawnSync } from 'node:child_process';
import { plistFile, wrapFramework, frameworkLayout, frameworkInstallName, singleFramework } from './assets.mjs';
import { assertCapabilityFrameworks, stripProductionExecutable } from './link.mjs';
import { linkedPlanArgs } from '../../scripts/linked-plan.mjs';
import { appleLinkArgs } from '../../scripts/package-libraries.mjs';
import { constValueArgs, intentsMetadata } from './intents.mjs';
import { appexArgs, keepMain, extensionModuleCache } from './extensions.mjs';
import { coveringProfiles, missingDevelopmentProfile, installedProfiles, decodeProfile, profilesFor, profileMisfit, byExpiry, METHODS, iosProfile, allowsEntitlement, macHardwareID } from './devices.mjs';
import { fileURLToPath } from 'node:url';
const root = resolve(fileURLToPath(new URL('../..', import.meta.url))), pkg = resolve(root, 'host/apple');
export const accessoryFamilies = ['accessoryCircular', 'accessoryRectangular', 'accessoryInline', 'accessoryCorner'];
export function platformWidgetEntries(app, platform = 'ios') {
  return widgetEntries(app).flatMap(entry => {
    const families = (platform === 'watchos' ? entry.watchFamilies ?? entry.families : entry.families).filter(family => platform === 'watchos' ? accessoryFamilies.includes(family) : platform === 'macos' ? ['systemSmall', 'systemMedium', 'systemLarge', 'systemExtraLarge'].includes(family) : family !== 'accessoryCorner');
    return families.length ? [{ ...entry, families }] : [];
  });
}
export const widgetEntries = app => app.manifest.widgets ?? [];
export const activityEntries = app => app.manifest.live_activities ?? [];
/** Whether the app has an entry of either kind: what the extension, the shared framework, the App Group and the
 * Keychain group are gated on (LLP 1126.001 WP13). */
export const hasEntries = app => platformWidgetEntries(app).length > 0 || activityEntries(app).length > 0;
export const widgetGroup = app => `group.${app.widgetAppId ?? app.clipParent ?? app.id}`;
export const widgetId = app => `${app.id}.widgets`;

// LLP 1126.001 WP16: the Mac container is authorized by the signing team,
// independently of iOS's registered group and of restricted Keychain access.
export const macWidgetGroup = (app, team) => `${team}.${app.id}.widgets`;
export function widgetSigningTeam(identity, { distribution = false, certificates = null } = {}) {
  certificates ??= (spawnSync('security', ['find-certificate', '-a', '-p'], { encoding: 'utf8' }).stdout
    ?.match(/-----BEGIN CERTIFICATE-----[\s\S]*?-----END CERTIFICATE-----/g) ?? []).map(pem => new X509Certificate(pem));
  const kind = distribution ? 'Developer ID Application' : 'Apple Development';
  for (const cert of certificates) {
    const cn = /^CN=(.*)$/m.exec(cert.subject)?.[1];
    if (cert.fingerprint.replaceAll(':', '') === identity?.toUpperCase() || cn === identity) {
      if (!distribution && cn?.startsWith('Developer ID Application: ')) return null;
      if (!cn?.startsWith(kind + ': ')) throw new Error(`host/apple: Mac widgets need an ${kind} certificate for this build; EXACT_IDENTITY names ${cn}`);
      const team = /^OU=([A-Z0-9]{10})$/m.exec(cert.subject)?.[1];
      if (team) return team;
    }
  }
  throw new Error(`host/apple: Mac widgets need an ${kind} identity; name its SHA-1 or certificate name with EXACT_IDENTITY (security find-identity -v -p codesigning)`);
}
export { macHardwareID };
/** `also`: what the app's own profile must allow besides (presence.mjs `macSigning`: its grants' profile entitlements),
 * so one profile is chosen for both. */
export function macWidgetProfiles({ app, team, distribution = false, installed = installedProfiles(), named = process.env.EXACT_PROFILE ? decodeProfile(process.env.EXACT_PROFILE) : null, udid, hardwareID = macHardwareID, log = console.log, also = () => true }) {
  if (named && !named.platforms?.includes('OSX')) {
    log(`Mac widgets: ignoring EXACT_PROFILE (${named.name}) for ${named.platforms?.join(', ') || 'another platform'}; shared secrets need macOS profiles (LLP 1126.001 §5 Q5).`);
    named = null;
  }
  if (udid === undefined) udid = !distribution && installed.some(p => p.platforms?.includes('OSX') && p.team === team) ? hardwareID() : null;
  const group = macWidgetGroup(app, team);
  const fits = (p, id) => p.platforms?.includes('OSX') && p.team === team && p.dev === !distribution && !profileMisfit(p, udid)
    && profilesFor(id, [p]).length && (p.groups.includes(group) || p.groups.includes(`${team}.*`))
    && (p.keychain.includes(`${p.prefix}.${app.id}`) || p.keychain.includes(`${p.prefix}.*`));
  for (const owner of (named ? [named] : installed).filter(p => fits(p, app.id) && also(p)).sort(byExpiry)) {
    const extension = installed.filter(p => fits(p, widgetId(app)) && !profileMisfit(p, udid, owner)).sort(byExpiry)[0];
    if (extension) return { app: owner, extension, keychain: `${owner.prefix}.${app.id}` };
  }
  if (named) throw new Error(`host/apple: EXACT_PROFILE (${named.name}) needs a matching macOS profile for ${app.id} and ${widgetId(app)}, of team ${team}, granting ${group} (or ${team}.*) and the shared Keychain group <prefix>.${app.id}`);
  return null;
}
/** Decide before compilation. A machine without a signing identity still builds the ordinary app. */
export function macWidgetSigning(app, identity, { distribution = false, log = console.log, certificates, profiles } = {}) {
  if (!platformWidgetEntries(app, 'macos').length) return null;
  const team = identity && identity !== '-' ? widgetSigningTeam(identity, { distribution, certificates }) : null;
  if (!team) {
    const reason = identity && identity !== '-' ? 'EXACT_IDENTITY names a Developer ID identity' : 'no signing identity';
    log(`Mac widgets: ${reason}; building the app ad hoc without an extension. Set EXACT_IDENTITY to ${distribution ? 'a Developer ID Application' : 'an Apple Development'} identity to include widgets.`);
    return null;
  }
  return { team, profiles: macWidgetProfiles({ app, team, distribution, log, ...profiles }) };
}
/** `extra`: the app's own profile entitlements (presence.mjs `macProvisioning`), signed under the same profile. */
export function macWidgetEntitlements({ app, team, profiles = null, extension = false, extra = {} }) {
  const id = extension ? widgetId(app) : app.id;
  return plistFile({
    'com.apple.security.application-groups': [macWidgetGroup(app, team)],
    ...(extension ? { 'com.apple.security.app-sandbox': true, 'com.apple.security.network.client': true } : {}),
    ...(profiles ? { 'com.apple.application-identifier': `${profiles.app.prefix}.${id}`, 'com.apple.developer.team-identifier': team, 'keychain-access-groups': [profiles.keychain] } : {}),
    ...(extension ? {} : extra),
  });
}

/** The two profiles a device build of an app with entries embeds, the app's and its extension's (LLP 1126.001
 * WP13): of one team, App ID prefix and method, each granting the App Group and the one Keychain group both sign
 * (`<prefix>.<app id>`, or the team wildcard), the app's also `aps-environment` (as its method signs it) when an
 * activity takes pushes and `required` (the grants' signing entitlements, which the extension never signs); or
 * one refusal naming, for each App ID, the profiles that were close and what each lacked or failed. A build for a
 * phone (`udid`) takes development profiles, signed with the Apple Development identity; an archive (no phone)
 * takes distribution ones, the extension's of the named one's method and, for Ad Hoc, its devices. EXACT_PROFILE
 * names the app's, under the same conditions; the extension's is found by its own id, since a team wildcard
 * grants no App Group. `installed` and `named` are the decoded profiles, which a test replaces. */
export function entryProfiles({ app, udid, required = [], installed = installedProfiles(), named = process.env.EXACT_PROFILE ? decodeProfile(process.env.EXACT_PROFILE) : null, team = process.env.DEVELOPMENT_TEAM?.trim(), phoneName = udid }) {
  // installedProfiles also finds native Mac profiles; they cannot provision iOS.
  installed = installed.filter(iosProfile);
  if (udid && team && !named) {
    installed = installed.filter(p => p.team === team);
    if (!coveringProfiles(udid, app.id, installed).length) throw missingDevelopmentProfile(udid, app.id, team, phoneName);
  }
  const group = widgetGroup(app), extension = widgetId(app), push = activityEntries(app).some(entry => entry.push === true);
  const shared = prefix => `${prefix}.${app.id}`;
  const wants = (id, prefix) => [
    { has: p => p.groups.includes(group), missing: `the App Group ${group}`, portal: `App Groups (${group})` },
    { has: p => p.keychain.includes(`${p.prefix}.*`) || p.keychain.includes(shared(p.prefix)), missing: `the Keychain group ${shared(prefix)}`, portal: `Keychain Sharing (the group ${shared(prefix)}, or the team's ${prefix}.*)` },
    ...(id === app.id && push && !required.includes('aps-environment') ? [{ has: p => allowsEntitlement(p, 'aps-environment'), missing: 'aps-environment (development on a development profile, production otherwise; an activity takes pushes)', portal: 'Push Notifications' }] : []),
    ...(id === app.id ? required.map(name => ({ has: p => allowsEntitlement(p, name), missing: name === 'aps-environment' ? 'aps-environment (development on a development profile, production otherwise)' : name, portal: name === 'aps-environment' ? 'Push Notifications' : name })) : []),
  ];
  // The build's method, which a profile that `like` does not decide must have.
  const mode = p => udid ? (p.dev ? null : `is ${METHODS[p.method]}, not development`) : (p.dev ? 'is development; an archive takes a distribution profile' : null);
  // `candidates` are the profiles for the id; `like` the profile the choice must match, else the build's method;
  // `label` how a profile is named; `reject` a reason that stands before the rest (a named profile for another App ID).
  const assess = (id, candidates, like, { label = p => `"${p.name}"`, reject = () => null } = {}) => {
    const needs = wants(id, candidates[0]?.prefix ?? '<prefix>');
    const fits = [], misfits = [];
    for (const p of candidates) {
      const why = reject(p) ?? (like ? profileMisfit(p, udid, like) : mode(p) ?? profileMisfit(p, udid));
      if (why) misfits.push([p, why]); else fits.push(p);
    }
    fits.sort(byExpiry);
    const found = fits.find(p => needs.every(want => want.has(p)));
    if (found) return { found };
    // What to register: a grant no profile for the id makes, fit or not (a misfit profile already has the rest).
    const lacking = needs.filter(want => !candidates.some(p => want.has(p)));
    const fitsLack = needs.filter(want => !fits.some(p => want.has(p)));
    const parts = [];
    if (fits.length) parts.push(`${fits.map(label).join(', ')} ${fits.length > 1 ? 'cover' : 'covers'} it but ${fitsLack.length ? `none grants ${fitsLack.map(w => w.missing).join(', ')}` : 'none grants all of these at once'}`);
    for (const [p, why] of misfits) parts.push(`${label(p)} is for it but ${why}`);
    return { lacking, why: parts.join('; ') || 'no profile covers it' };
  };
  const forApp = named
    ? assess(app.id, [named], null, { label: p => `EXACT_PROFILE (${p.name})`, reject: p => !iosProfile(p) ? 'is not an iOS profile' : profilesFor(app.id, [p]).length ? null : `is for ${p.appId}, not ${app.id}` })
    : assess(app.id, profilesFor(app.id, installed), null);
  const like = forApp.found ?? null;
  const forExtension = assess(extension, profilesFor(extension, installed), like);
  if (like && forExtension.found) return { app: like, extension: forExtension.found };
  const method = like ? METHODS[like.method] : udid ? 'development' : 'distribution';
  const register = [[app.id, forApp], [extension, forExtension]].filter(([, choice]) => !choice.found && choice.lacking?.length)
    .map(([id, choice]) => `the App ID ${id} with ${[...new Set(choice.lacking.map(w => w.portal))].join(' and ')}`);
  throw new Error(`host/apple: a device build of an app with widgets or Live Activities embeds one ${method} profile in the app and one in its extension, of one team, App ID prefix and method, each granting the App Group and the Keychain group they share (LLP 1126.001 WP13). On this Mac${udid ? `, for this phone (${udid})` : ', for an archive'}:
  ${app.id}: ${forApp.found ? 'provisioned' : forApp.why}
  ${extension}: ${forExtension.found ? 'provisioned' : forExtension.why}
${register.length ? `In the Apple Developer portal, register ${register.join(', and ')}; then make` : 'Make'} one ${method} profile for each App ID${udid ? ' that includes this phone' : like?.devices.length ? ' naming the same devices' : ''}; install both (open the files, or Xcode ▸ Settings ▸ Accounts ▸ Download Manual Profiles). EXACT_PROFILE names the app's.`);
}
const tree = dir => readdirSync(dir, { withFileTypes: true }).flatMap(e => e.isDirectory() ? tree(resolve(dir, e.name)) : [resolve(dir, e.name)]);

/** SwiftPM owns the sources, settings, incremental objects and module. The wrapper's stable path depends on that
 * interface, never on the Rust archive or the app: every app of a scratch shares it, so ExactKit, compiled against
 * the framework's path, is compiled once for all of them. The app's identifier is written into its own copy
 * (`placeCoreFramework`). */
export function prepareWidgetCore({ directory, swiftBuildRoot, swiftArgs, swiftc, triple, device, platform = 'ios' }) {
  const scratch = resolve(swiftBuildRoot, 'core');
  const args = swiftArgs.map(a => a === swiftBuildRoot ? scratch : a);
  const key = createHash('sha256').update(JSON.stringify([args, swiftc, triple, 'versioned-framework-module-bundles']));
  for (const file of [resolve(pkg, 'Package.swift'), resolve(pkg, 'include/exact.h'), ...['ExactCore', 'ExactSnapshot', 'CExact'].flatMap(name => tree(resolve(pkg, 'Sources', name)))].sort()) key.update(file).update(readFileSync(file));
  const base = resolve(directory, key.digest('hex').slice(0, 20));
  return { base, scratch, args, triple, device, platform, framework: resolve(base, 'ExactCore.framework'), xcframework: resolve(base, 'ExactCore.xcframework') };
}
/** ExactCore's module and object an older build left in a products directory, in either SwiftPM layout
 * (the flat one and `Modules/`), which the product build would otherwise take over the framework's. */
export function dropLeftoverCore(bin) {
  for (const directory of [bin, resolve(bin, 'Modules')]) for (const name of ['ExactCore.swiftmodule', 'ExactCore.swiftdoc', 'ExactCore.abi.json', 'ExactCore.swiftsourceinfo', 'ExactCore.o']) rmSync(resolve(directory, name), { recursive: true, force: true });
}
export function sharedWidgetCore({ app, prepared, libDir, crate, env, runApple, linkedPlan = null, library = null }) {
  const { base, args, framework, xcframework, device, platform } = prepared;
  mkdirSync(base, { recursive: true });
  const coreEnv = { ...env, EXACT_WIDGET_CORE: '1' };
  delete coreEnv.EXACT_CORE_FRAMEWORK;
  const options = { cwd: pkg, env: coreEnv };
  const answer = resolve(base, 'bin-path');
  let bin = existsSync(answer) ? readFileSync(answer, 'utf8') : '';
  if (!bin || !existsSync(bin)) {
    bin = runApple('swift', [...args, '--show-bin-path'], { ...options, read: true }).stdout.trim();
    writeFileSync(answer, bin);
  }
  const dylib = resolve(bin, 'libExactCore.dylib');
  const archive = resolve(libDir, `lib${crate.replaceAll('-', '_')}.a`);
  const digest = createHash('sha256').update(readFileSync(archive)).update(linkedPlan?.digest ?? '').update(library?.sha256 ?? '').digest('hex');
  // SwiftPM compiles the whole scratch again when any argument in its plan changes, link arguments included, and the
  // archive and plan sat under the app's own directory: every switch between two widget apps recompiled about 32 s
  // of Swift (expo/exact-build#177). They are linked from fixed paths here instead, copied in when their bytes change.
  // The archive's name is fixed too (Package.swift force-loads lib<EXACT_LIB>.a), since apps' crates differ.
  const inputs = resolve(base, 'link-inputs'), stableArchive = resolve(inputs, 'libexact_widget_app.a'), stablePlan = linkedPlan && resolve(inputs, 'plan');
  const placed = resolve(inputs, 'digest');
  if (!existsSync(placed) || readFileSync(placed, 'utf8') !== digest || !existsSync(stableArchive)) {
    rmSync(inputs, { recursive: true, force: true });
    mkdirSync(inputs, { recursive: true });
    copyFileSync(archive, stableArchive);
    if (stablePlan) {
      mkdirSync(stablePlan);
      for (const file of ['app.plan', 'compat.json', 'app.hbc']) copyFileSync(resolve(linkedPlan.dir, file), resolve(stablePlan, file));
    }
    writeFileSync(placed, digest);
  }
  coreEnv.EXACT_LIB_DIR = inputs;
  coreEnv.EXACT_LIB = 'exact_widget_app';
  // A package's framework (LLP 1154 D3), which the archive's shim calls, found beside this one in Frameworks.
  const linkLibrary = appleLinkArgs(library, platform !== 'macos', platform === 'macos' ? '@loader_path/../../..' : '@loader_path/..');
  const stamp = resolve(base, 'archive');
  // SwiftPM tracks neither an external archive nor the linked plan. Invalidate only its link;
  // the objects/module stay incremental, including on a kernel-only edit.
  if (!existsSync(stamp) || readFileSync(stamp, 'utf8') !== digest) rmSync(dylib, { force: true });
  runApple('swift', [...args, ...linkLibrary, ...linkedPlanArgs(stablePlan), '--product', 'ExactCore'], options);
  // Only the extension links the snapshot target. Keeping it out of the shared
  // runtime keeps SwiftUI and WidgetKit out of an ordinary app launch.
  runApple('swift', [...args, ...linkedPlanArgs(stablePlan), '--product', 'ExactSnapshot'], options);
  const changed = !existsSync(stamp) || readFileSync(stamp, 'utf8') !== digest || !existsSync(resolve(framework, 'ExactCore'));
  if (changed) {
    const layout = frameworkLayout(framework, 'ExactCore', platform === 'macos' ? 'MacOSX' : 'iPhoneOS');
    const temporary = resolve(base, `ExactCore.${process.pid}.tmp`);
    copyFileSync(dylib, temporary);
    runApple('xcrun', ['install_name_tool', '-id', frameworkInstallName('ExactCore', platform === 'macos' ? 'MacOSX' : 'iPhoneOS'), temporary]);
    renameSync(temporary, layout.binary);
    // The modules are rewritten only when their bytes change: what is compiled against them (ExactKit) is then
    // not compiled again for another app's archive.
    let moduleTriple;
    for (const name of ['ExactCore', 'ExactSnapshot']) {
      const moduleName = `${name}.swiftmodule`;
      const modified = path => Math.max(...(statSync(path).isDirectory() ? tree(path).filter(file => file.endsWith('.swiftmodule')) : [path]).map(file => statSync(file).mtimeMs));
      const source = [resolve(bin, moduleName), resolve(bin, 'Modules', moduleName)].filter(existsSync).sort((a, b) => modified(b) - modified(a))[0] ?? resolve(bin, moduleName);
      const destination = resolve(layout.modules, moduleName);
      let files;
      if (statSync(source).isDirectory()) files = tree(source).map(file => [file, resolve(destination, file.slice(source.length + 1))]);
      else {
        moduleTriple ??= JSON.parse(runApple('xcrun', ['swiftc', '-print-target-info', '-target', prepared.triple], { read: true }).stdout).target.moduleTriple;
        files = ['swiftmodule', 'swiftdoc', 'abi.json', 'swiftsourceinfo'].map(suffix => [source.replace(/swiftmodule$/, suffix), resolve(destination, `${moduleTriple}.${suffix}`)])
          .filter(([file]) => existsSync(file));
      }
      const kept = new Set(files.map(([, to]) => to));
      if (existsSync(destination)) for (const file of tree(destination)) if (!kept.has(file)) rmSync(file);
      for (const [from, to] of files) syncFile(from, to);
    }
    // The identifier is the app's, written where the framework is placed for it (`placeCoreFramework`).
    writeIfChanged(layout.info, plistFile({ CFBundleIdentifier: SHARED_CORE_ID, CFBundleName: 'ExactCore', CFBundleExecutable: 'ExactCore', CFBundlePackageType: 'FMWK', CFBundleVersion: '1', CFBundleShortVersionString: '0.1.0', CFBundleSupportedPlatforms: [platform === 'macos' ? 'MacOSX' : platform === 'watchos' ? 'WatchSimulator' : device ? 'iPhoneOS' : 'iPhoneSimulator'], ...(platform === 'macos' ? { LSMinimumSystemVersion: app.manifest.host?.macos?.minimumOS ?? '14.0' } : { MinimumOSVersion: platform === 'watchos' ? '10.0' : app.manifest.host?.ios?.minimumOS ?? '17.0' }) }));
    if (!existsSync(resolve(xcframework, 'Info.plist'))) singleFramework(framework, xcframework, prepared.triple);
    else {
      const slice = readdirSync(xcframework).find(name => existsSync(resolve(xcframework, name, 'ExactCore.framework')));
      const copy = resolve(xcframework, slice, 'ExactCore.framework');
      for (const file of tree(framework)) if (!lstatSync(file).isSymbolicLink()) syncFile(file, resolve(copy, file.slice(framework.length + 1)));
    }
    writeFileSync(stamp, digest);
  }
  assertCapabilityFrameworks(resolve(framework, 'ExactCore'), []);
  return { ...prepared, snapshotArchive: resolve(bin, 'libExactSnapshot.a') };
}
/** The shared framework's placeholder identifier; no bundle ships it. */
const SHARED_CORE_ID = 'dev.exact.core.shared';
/** `framework` (the shared ExactCore) copied to `to` for `app`, with the app's own bundle identifier. */
export function placeCoreFramework(framework, to, app, platform = 'ios') {
  rmSync(to, { recursive: true, force: true });
  cpSync(framework, to, { recursive: true, verbatimSymlinks: true });
  const info = resolve(to, platform === 'macos' ? 'Resources/Info.plist' : 'Info.plist');
  writeFileSync(info, readFileSync(info, 'utf8').replace(`<string>${SHARED_CORE_ID}</string>`, `<string>${app.id}.runtime</string>`));
  if (!readFileSync(info, 'utf8').includes(`<string>${app.id}.runtime</string>`)) throw new Error(`host/apple: ${info} names no ExactCore identifier to replace`);
  return to;
}
/** `to` with `from`'s bytes and modification time, written only when the bytes differ. */
function syncFile(from, to) {
  const bytes = readFileSync(from);
  if (existsSync(to) && readFileSync(to).equals(bytes)) return;
  mkdirSync(dirname(to), { recursive: true });
  const temporary = `${to}.${process.pid}.tmp`;
  writeFileSync(temporary, bytes);
  const { atime, mtime } = statSync(from);
  utimesSync(temporary, atime, mtime);
  renameSync(temporary, to);
}
const writeIfChanged = (path, text) => { if (!existsSync(path) || readFileSync(path, 'utf8') !== text) writeFileSync(path, text); };
/** The reload artifact has no app-specific inputs. Reuse it until its source or target changes. */
export function widgetReload({ directory, triple, sdk, device, platform = 'ios', production = false, runApple, source = resolve(pkg, 'Sources/ExactCore/WidgetContainer.swift') }) {
  // One file per digest: apps sharing `directory` may ask for different triples at once.
  const digest = createHash('sha256').update(readFileSync(source)).update(JSON.stringify([triple, sdk, production])).digest('hex').slice(0, 16);
  const binary = resolve(directory, `widget-reload-${digest}`, 'libexact_widget_reload.dylib');
  if (existsSync(binary)) return binary;
  mkdirSync(dirname(binary), { recursive: true });
  runApple('xcrun', ['--sdk', platform === 'macos' ? 'macosx' : device ? 'iphoneos' : 'iphonesimulator', 'swiftc', '-parse-as-library', '-emit-library', '-O',
    '-D', 'EXACT_WIDGET_RELOAD', ...(!production ? ['-D', 'EXACT_INSPECT'] : []), '-module-name', 'ExactWidgetReload', '-target', triple, '-sdk', sdk,
    '-module-cache-path', extensionModuleCache, source, '-o', `${binary}.${process.pid}.tmp`]);
  renameSync(`${binary}.${process.pid}.tmp`, binary);
  return binary;
}
/** The extension's generated Swift: one `Widget` per entry, static or, with a `configuration`, an
 * `AppIntentConfiguration` whose intent has one `AppEnum` parameter per row (`widget/Timeline.swift`'s
 * `ExactIntentTimelineProvider` stamps its values on each entry). Labels and values are Swift string literals:
 * the manifest admits no control characters in them, and JSON's escapes are Swift's for the rest. */
export function widgetBundleSource(entries, { platform = 'ios', activities = false, alarmMember = '' } = {}) {
  const mac = platform === 'macos', literal = JSON.stringify;
  const names = entries.map((_, i) => `Entry${i}`);
  const human = kind => kind.replace(/[-_]+/g, ' ').replace(/\b\w/g, c => c.toUpperCase());
  const gamut = platform === 'ios' ? '.environment(\\.widgetDisplayGamut, UITraitCollection.current.displayGamut == .P3 ? "p3" : "srgb")' : mac ? '.environment(\\.widgetDisplayGamut, NSScreen.main?.colorSpace?.cgColorSpace?.isWideGamutRGB == true ? "p3" : "srgb")' : '';
  // LLP 1126: these identifiers survive reordering and distinguish punctuation. UTF-8 hex is injective.
  const identifier = text => Buffer.from(text, 'utf8').toString('hex');
  const intentName = entry => `Configure_${identifier(entry.kind)}`;
  const intent = (configuration, name) => {
    const parameters = configuration.parameters;
    const enums = parameters.map(p => `enum ${name}_${identifier(p.name)}: String, AppEnum {
    ${p.values.map(v => `case v${identifier(v.value)} = ${literal(identifier(v.value))}`).join('; ')}
    var exactValue: String { switch self { ${p.values.map(v => `case .v${identifier(v.value)}: ${literal(v.value)}`).join('; ')} } }
    static var typeDisplayRepresentation: TypeDisplayRepresentation = ${literal(p.title)}
    static var caseDisplayRepresentations: [Self: DisplayRepresentation] = [${p.values.map(v => `.v${identifier(v.value)}: ${literal(v.title)}`).join(', ')}]
}`);
    const fallback = p => `v${identifier(p.default ?? p.values[0].value)}`;
    // watchOS has no parameter form: offer every combination, default first.
    // The manifest caps watch entries at 64 combinations before this generation.
    const combinations = platform === 'watchos' ? parameters.reduce((rows, p) => {
      const value = p.default ?? p.values[0].value;
      const values = [...p.values.filter(v => v.value === value), ...p.values.filter(v => v.value !== value)];
      return rows.flatMap(row => values.map(v => [...row, { p, v }]));
    }, [[]]) : [];
    const labels = combinations.map(row => row.map(({ p, v }) => `${p.title}: ${v.title}${p.values.some(other => other !== v && other.title === v.title) ? ` (${v.value})` : ''}`).join(', '));
    // Swift compares canonically equivalent strings as equal. A suffix on every
    // label keeps authored text from colliding with a disambiguated label too.
    const repeated = new Set(labels.map(label => label.normalize('NFC'))).size !== labels.length;
    const recommendations = combinations.map((row, i) => {
      const label = labels[i] + (repeated ? ` [${row.map(({ v }) => identifier(v.value)).join('-')}]` : '');
      return `{ let intent = Self(); ${row.map(({ p, v }) => `intent.\`parameter_${p.name}\` = .v${identifier(v.value)}`).join('; ')}; return AppIntentRecommendation(intent: intent, description: ${literal(label)}) }()`;
    });
    return `${enums.join('\n')}
struct ${name}: WidgetConfigurationIntent, ExactConfigurationIntent {
    static var title: LocalizedStringResource = ${literal(configuration.title)}
    static var description = IntentDescription(${literal(configuration.description ?? configuration.title)})
    ${parameters.map(p => `@Parameter(title: ${literal(p.title)}, default: .${fallback(p)}) var \`parameter_${p.name}\`: ${name}_${identifier(p.name)}?`).join('\n    ')}
    ${platform === 'watchos' ? `static var exactRecommendations: [AppIntentRecommendation<Self>] { [\n        ${recommendations.join(',\n        ')}\n    ] }` : ''}
    var exactConfiguration: [String: String] { [${parameters.map(p => `${literal(p.name)}: (\`parameter_${p.name}\` ?? .${fallback(p)}).exactValue`).join(', ')}] }
}
`;
  };
  const generated = entries.map((entry, i) => {
    const configured = entry.configuration && intentName(entry);
    return `${configured ? intent(entry.configuration, configured) : ''}struct ${names[i]}: Widget {
    let kind = ${literal(entry.kind)}
    var body: some WidgetConfiguration {
        ${configured
          ? `AppIntentConfiguration(kind: kind, intent: ${configured}.self, provider: ExactIntentTimelineProvider<${configured}>(kind: kind))`
          : 'StaticConfiguration(kind: kind, provider: ExactTimelineProvider(kind: kind))'} { entry in
            ExactWidgetView(entry: entry)${gamut}
        }
        .configurationDisplayName(${literal(entry.title ?? human(entry.kind))}).description(${literal(entry.description ?? human(entry.kind))})
        .supportedFamilies([${entry.families.map(f => `.${f}`).join(', ')}]).contentMarginsDisabled()
    }
}`;
  }).join('\n');
  if (activities) names.push('ExactActivities');
  return `import AppIntents\nimport SwiftUI\nimport WidgetKit\n${platform === 'ios' ? 'import UIKit\n' : mac ? 'import AppKit\n' : ''}${generated}\n@main struct ExactWidgets: WidgetBundle { var body: some Widget { ${names.map(n => `${n}()`).join('; ')}${alarmMember} } }\n`;
}

/** One module name for compilation and metadata, including AlarmKit's shared attributes. */
export function widgetCompilation({ entries, platform, activities, alarm, source, values, directory }) {
  const configured = entries.some(entry => entry.configuration);
  const moduleName = platform === 'ios' && alarm ? alarm : 'ExactWidgets';
  const sources = [resolve(pkg, 'widget/Timeline.swift'), resolve(pkg, 'widget/Configuration.swift'),
    resolve(pkg, 'widget/WidgetTimelineWaiters.swift'), ...(activities ? [resolve(pkg, 'widget/Activity.swift')] : []),
    ...(platform === 'ios' && alarm ? alarmSources : []), source];
  const args = ['-module-name', moduleName, ...(configured ? ['-wmo', ...constValueArgs(values, directory)] : []),
    ...(platform === 'ios' && alarm ? ['-Xlinker', '-weak_framework', '-Xlinker', 'AlarmKit'] : [])];
  return { moduleName, sources, args };
}

export function packageWidgets({ app, bundle, shared, directory, sdk, triple, device, entitlements, sign, runApple, production, symbols = directory, profile = null, platform = 'ios', entries = platformWidgetEntries(app, platform), alarm = null }) {
  const mac = platform === 'macos', contents = mac ? resolve(bundle, 'Contents') : bundle;
  const resources = mac ? resolve(contents, 'Resources') : contents;
  const extension = resolve(contents, 'PlugIns/ExactWidgets.appex');
  const extensionContents = mac ? resolve(extension, 'Contents') : extension;
  const extensionResources = mac ? resolve(extensionContents, 'Resources') : extension;
  const extensionBinary = mac ? resolve(extensionContents, 'MacOS/ExactWidgets') : resolve(extension, 'ExactWidgets');
  mkdirSync(extensionResources, { recursive: true });
  if (mac) mkdirSync(resolve(extensionContents, 'MacOS'));
  // A phone installs the extension only with its own profile embedded (entryProfiles chose it).
  if (profile) copyFileSync(profile.path, resolve(extensionContents, mac ? 'embedded.provisionprofile' : 'embedded.mobileprovision'));
  // The widgets' plans and a TypeScript app's module pair, which the bake wrote or the build fails; an app with
  // activities alone has no `plans` tree until its entries bake (LLP 1126.001 WP8).
  for (const [tree, expected] of [['plans', entries.length > 0], ['typescript', platform !== 'watchos' && existsSync(resolve(app.dir, 'app.ts'))]]) {
    if (existsSync(resolve(resources, tree))) cpSync(resolve(resources, tree), resolve(extensionResources, tree), { recursive: true });
    else if (expected) throw new Error(`host/apple: the bake wrote no ${tree} tree for the widget extension (${resolve(resources, tree)})`);
  }
  const appInfo = JSON.parse(runApple('plutil', ['-convert', 'json', '-o', '-', resolve(contents, 'Info.plist')], { read: true }).stdout);
  const capabilities = appInfo.UIRequiredDeviceCapabilities ? { UIRequiredDeviceCapabilities: appInfo.UIRequiredDeviceCapabilities } : {};
  const versions = Object.fromEntries(['CFBundleVersion', 'CFBundleShortVersionString', mac ? 'LSMinimumSystemVersion' : 'MinimumOSVersion'].map(key => [key, appInfo[key]]));
  const runtimeInfo = resolve(contents, 'Frameworks/ExactCore.framework', mac ? 'Resources/Info.plist' : 'Info.plist');
  const runtime = JSON.parse(runApple('plutil', ['-convert', 'json', '-o', '-', runtimeInfo], { read: true }).stdout);
  writeFileSync(runtimeInfo, plistFile({ ...runtime, ...versions, ...capabilities }));
  const frameworks = resolve(contents, 'Frameworks'), reload = 'libexact_widget_reload.dylib';
  if (platform !== 'watchos') {
    copyFileSync(widgetReload({ directory: shared.base ?? directory, triple, sdk, device, platform, production, runApple }), resolve(frameworks, reload));
    wrapFramework(frameworks, reload, 'ExactWidgetReload', app, mac ? 'MacOSX' : device ? 'iPhoneOS' : 'iPhoneSimulator');
  }
  // The one ActivityConfiguration for every activity kind (`widget/Activity.swift`, LLP 1126.001 WP10): the
  // envelope's attributes name the kind and the generation at render time, so the generated bundle holds no
  // activity string at all, and an app with activities and no widgets has it as its one member.
  // ActivityKit exists on iOS alone: the watch's and the Mac's bundles have no activities.
  const activities = platform === 'ios' && activityEntries(app).length > 0;
  const source = resolve(directory, 'WidgetBundle.swift');
  // An alarm's countdown beside them, from iOS 26 (LLP 1133.030 D8 item 2).
  const alarmMember = alarm && platform === 'ios' ? '; if #available(iOS 26.0, *) { ExactAlarmCountdown() }' : '';
  writeFileSync(source, widgetBundleSource(entries, { platform, activities, alarmMember }));
  const entitlementPath = resolve(directory, 'widget-entitlements.plist');
  writeFileSync(entitlementPath, entitlements);
  writeFileSync(resolve(extensionContents, 'Info.plist'), plistFile({ CFBundleIdentifier: widgetId(app), CFBundleName: 'ExactWidgets', CFBundleDisplayName: app.displayName, CFBundleExecutable: 'ExactWidgets', CFBundlePackageType: 'XPC!', ...versions, ...(!mac ? { UIDeviceFamily: appInfo.UIDeviceFamily } : {}), ...capabilities,
    CFBundleSupportedPlatforms: mac ? ['MacOSX'] : appInfo.CFBundleSupportedPlatforms, ...(appInfo.DTPlatformName ? { DTPlatformName: appInfo.DTPlatformName } : {}), ...(appInfo.NSAppTransportSecurity ? { NSAppTransportSecurity: appInfo.NSAppTransportSecurity } : {}), ExactAppGroup: appInfo.ExactAppGroup ?? widgetGroup(app), ExactAppIdentifier: appInfo.ExactAppIdentifier ?? app.id, ExactWidgetTypeScript: platform !== 'watchos' && existsSync(resolve(app.dir, 'app.ts')), ...(appInfo.ExactKeychainAccessGroup ? { ExactKeychainAccessGroup: appInfo.ExactKeychainAccessGroup } : {}), ...(mac ? { ExactWidgetSecretsAvailable: !!appInfo.ExactKeychainAccessGroup, ExactWidgetRequiresSecrets: appInfo.ExactWidgetRequiresSecrets ?? [] } : {}), ExactWidgetGeneration: appInfo.ExactWidgetGeneration, ...(typeof app.origin === 'string' ? { ExactLinkBase: app.origin } : {}),
    NSExtension: { NSExtensionPointIdentifier: 'com.apple.widgetkit-extension' } }));
  // A configured entry's intent reaches the Edit Widget sheet through the extension's App Intents metadata, which
  // Xcode's pass extracts from the module's constant values (`intents.mjs`).
  const configured = entries.some(entry => entry.configuration);
  const intents = resolve(directory, 'widget-intents'), values = resolve(intents, 'ExactWidgets.swiftconstvalues');
  if (configured) { rmSync(intents, { recursive: true, force: true }); mkdirSync(intents, { recursive: true }); }
  const compilation = widgetCompilation({ entries, platform, activities, alarm: alarmMember ? alarm : null, source, values, directory: intents });
  const { moduleName, sources: swiftSources } = compilation;
  runApple('xcrun', ['--sdk', mac ? 'macosx' : platform === 'watchos' ? 'watchsimulator' : device ? 'iphoneos' : 'iphonesimulator', 'swiftc', '-parse-as-library', '-O', ...(production ? ['-g'] : []), ...appexArgs, ...keepMain, ...(!production ? ['-D', 'EXACT_INSPECT'] : []), '-target', triple, '-sdk', sdk,
    ...compilation.args,
    '-module-cache-path', extensionModuleCache, '-I', resolve(root, 'host/apple/Sources/CExact'), '-I', resolve(shared.framework, 'Modules'), '-F', resolve(shared.framework, '..'), '-framework', 'ExactCore',
    '-Xlinker', '-rpath', '-Xlinker', mac ? '@executable_path/../../../../Frameworks' : '@executable_path/../../Frameworks',
    ...(!device && !mac ? ['-Xlinker', '-sectcreate', '-Xlinker', '__TEXT', '-Xlinker', '__entitlements', '-Xlinker', entitlementPath] : []),
    shared.snapshotArchive, ...swiftSources, '-o', extensionBinary]);
  if (configured) {
    const metadata = intentsMetadata({ moduleName, dir: intents, sdk, triple, platform, deployment: versions[mac ? 'LSMinimumSystemVersion' : 'MinimumOSVersion'] ?? (mac ? '14.0' : platform === 'watchos' ? '10.0' : '17.0'),
      binary: extensionBinary, sources: swiftSources, values, runApple });
    cpSync(metadata, resolve(extensionResources, 'Metadata.appintents'), { recursive: true });
  }
  stripProductionExecutable(extensionBinary, { production, dsym: resolve(symbols, 'ExactWidgets.appex.dSYM'), sourceDsym: `${extensionBinary}.dSYM` });
  writePrivacyManifest(platform === 'macos' ? resolve(extension, 'Contents/Resources') : extension,
    [platform === 'macos' ? resolve(extension, 'Contents/MacOS/ExactWidgets') : resolve(extension, 'ExactWidgets')], { mac: platform === 'macos' });
  sign(extension, entitlementPath);
}

/** What compiles an alarm's countdown into a widget extension (LLP 1133.030 D8 items 1 and 2): the host's two files,
 * AlarmKit weak-linked (the app's minimum is below iOS 26), and the module named as the package's, which compiles the
 * same `ExactAlarmMetadata` into the app: ActivityKit names an attributes type by its module and type name, as
 * `activities.mjs` keeps the envelope's in `ExactSnapshot` on both sides. */
const alarmSources = [resolve(root, 'host/apple/widget/Alarm.swift'), resolve(root, 'host/apple/widget/AlarmAttributes.swift')];
const alarmArgs = module => ['-module-name', module, '-Xlinker', '-weak_framework', '-Xlinker', 'AlarmKit',
  ...alarmSources];

/** The package that asks the host for AlarmKit's countdown (`"apple": {"activity": "alarmkit"}`, LLP 1133.030 D12
 * item 3), by its Swift module's name; one at most, since the extension's module takes that name. */
export function alarmActivity(packages) {
  const asking = packages.filter(p => p.activity === 'alarmkit');
  if (asking.length > 1) throw new Error(`host/apple: ${asking.map(p => p.extension).join(' and ')} each ask for AlarmKit's countdown; one package may (LLP 1133.030 D12 item 3)`);
  return asking[0]?.module ?? null;
}

/** An app with no widgets and no Live Activities whose package asks for AlarmKit's countdown gets an extension that
 * holds the countdown alone (LLP 1133.030 D8 item 2): no ExactCore, no snapshot archive, no app group and no cohort,
 * so nothing of the app's state is reachable from it. */
export function packageAlarmExtension({ app, bundle, directory, sdk, triple, device, entitlements, sign, runApple, production, symbols = directory, profile = null, alarm }) {
  const extension = resolve(bundle, 'PlugIns/ExactWidgets.appex');
  mkdirSync(extension, { recursive: true });
  if (profile) copyFileSync(profile.path, resolve(extension, 'embedded.mobileprovision'));
  const appInfo = JSON.parse(runApple('plutil', ['-convert', 'json', '-o', '-', resolve(bundle, 'Info.plist')], { read: true }).stdout);
  const source = resolve(directory, 'AlarmWidgetBundle.swift');
  writeFileSync(source, 'import SwiftUI\nimport WidgetKit\n@main struct ExactWidgets: WidgetBundle { var body: some Widget { if #available(iOS 26.0, *) { ExactAlarmCountdown() } } }\n');
  const entitlementPath = resolve(directory, 'alarm-widget-entitlements.plist');
  writeFileSync(entitlementPath, entitlements);
  writeFileSync(resolve(extension, 'Info.plist'), plistFile({ CFBundleIdentifier: widgetId(app), CFBundleName: 'ExactWidgets', CFBundleDisplayName: app.displayName, CFBundleExecutable: 'ExactWidgets', CFBundlePackageType: 'XPC!',
    CFBundleVersion: appInfo.CFBundleVersion, CFBundleShortVersionString: appInfo.CFBundleShortVersionString, MinimumOSVersion: appInfo.MinimumOSVersion, UIDeviceFamily: appInfo.UIDeviceFamily,
    CFBundleSupportedPlatforms: appInfo.CFBundleSupportedPlatforms, ...(appInfo.DTPlatformName ? { DTPlatformName: appInfo.DTPlatformName } : {}),
    ...(appInfo.UIRequiredDeviceCapabilities ? { UIRequiredDeviceCapabilities: appInfo.UIRequiredDeviceCapabilities } : {}),
    NSExtension: { NSExtensionPointIdentifier: 'com.apple.widgetkit-extension' } }));
  runApple('xcrun', ['--sdk', device ? 'iphoneos' : 'iphonesimulator', 'swiftc', '-parse-as-library', '-O', ...(production ? ['-g'] : []), ...appexArgs, ...keepMain, '-target', triple, '-sdk', sdk,
    '-module-cache-path', extensionModuleCache, ...(!device ? ['-Xlinker', '-sectcreate', '-Xlinker', '__TEXT', '-Xlinker', '__entitlements', '-Xlinker', entitlementPath] : []),
    ...alarmArgs(alarm), source, '-o', resolve(extension, 'ExactWidgets')]);
  stripProductionExecutable(resolve(extension, 'ExactWidgets'), { production, dsym: resolve(symbols, 'ExactWidgets.appex.dSYM'), sourceDsym: resolve(extension, 'ExactWidgets.dSYM') });
  writePrivacyManifest(extension, [resolve(extension, 'ExactWidgets')]);
  sign(extension, entitlementPath);
}
