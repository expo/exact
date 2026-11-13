// What an Apple app links: the capabilities its composition names, the deployment
// targets it compiles and links for, and the SDK its executable records, with the
// device launch link of LLP 1047.001.001.
import { spawnSync } from './toolchain.mjs';
import { readFileSync, mkdirSync, rmSync, cpSync, statSync } from 'node:fs';
import { basename, dirname } from 'node:path';

/** Save a distributed image's symbols before removing its local symbol table.
 * `strip` keeps the image's UUID; `strip: false` writes the dSYM alone.
 * A direct Swift link supplies its dSYM: the driver's temporary objects are already gone. */
export function stripForDistribution(executable, dsym, { strip = true, sourceDsym = null } = {}) {
  // dsymutil reports each symbol it could not find in an object file on stderr; only a failure is said.
  const run = (cmd, args) => { const r = spawnSync(cmd, args, { stdio: ['ignore', 'ignore', 'pipe'] }); if (r.status !== 0) throw new Error(`${cmd} ${args.join(' ')} failed (${r.status ?? r.signal}): ${r.stderr ?? ''}`); };
  mkdirSync(dirname(dsym), { recursive: true });
  rmSync(dsym, { recursive: true, force: true });
  if (sourceDsym) {
    cpSync(sourceDsym, dsym, { recursive: true });
    rmSync(sourceDsym, { recursive: true });
  } else run('dsymutil', [executable, '-o', dsym]);
  if (!strip) return { dsym, saved: 0 };
  const before = statSync(executable).size;
  run('strip', ['-x', executable]);
  return { dsym, saved: before - statSync(executable).size };
}
/** Embedded products name a dSYM outside their shipped bundle. */
export function stripProductionExecutable(executable, { production, ipa = null, dsym = ipa ? `${ipa.replace(/\.ipa$/, '')}.dSYM` : `${executable}.dSYM`, sourceDsym = null, strip = true }) {
  if (!production) return null;
  return stripForDistribution(executable, dsym, { sourceDsym, strip });
}

/** Production compiler policy (LLP 1047.001 D9). tvOS shares host.ios. */
export function appleProduction(app, platform, args = []) {
  const game = !!app.manifest.game;
  const at = args.indexOf('--optimize');
  const optimize = at >= 0 ? args[at + 1]
    : app.manifest.host?.[platform === 'macos' ? 'macos' : 'ios']?.optimize
      ?? (game ? 'speed' : args.includes('--embed') || platform !== 'macos' ? 'size' : 'speed');
  if (!['speed', 'size'].includes(optimize)) throw new Error(`host/apple: --optimize is speed or size, not ${optimize}`);
  if (game && optimize === 'size') throw new Error('host/apple: games cannot optimize for size because their generated Cargo workspace has no size profile; use speed');
  return { optimize, profile: optimize === 'size' ? 'size' : 'release' };
}

/** A Linux builder can target an ARM simulator on another machine, independently of its host CPU. */
export function iosSimulatorTarget(arch = process.env.EXACT_IOS_SIM_ARCH ?? (process.platform === 'linux' ? 'arm64' : process.arch)) {
  const cpu = arch === 'arm64' ? 'arm64' : ['x64', 'x86_64'].includes(arch) ? 'x86_64' : null;
  if (!cpu) throw new Error(`host/apple: unsupported iOS simulator architecture ${arch}; EXACT_IOS_SIM_ARCH is arm64 or x86_64`);
  return { target: cpu === 'arm64' ? 'aarch64-apple-ios-sim' : 'x86_64-apple-ios', triple: `${cpu}-apple-ios17.0-simulator` };
}

const newer = (a, b) => (a.split('.').map(Number).reduce((x, n, i) => x || n - (b.split('.').map(Number)[i] ?? 0), 0) > 0 ? a : b);
/** The deployment targets an app builds for: its manifest's `minimumOS`, never
 * below the host's own floor (Package.swift's). They are what its Info.plist
 * declares, what its executable links for, and what its own Swift modules and
 * packages compile for (Swift refuses an API newer than the target at compile
 * time); `compiledTargets` is what the rest compiles for. */
export function deploymentTargets(app) {
  return {
    ios: newer(String(app.manifest.host?.ios?.minimumOS ?? '17.0'), '17.0'),
    macos: newer(String(app.manifest.host?.macos?.minimumOS ?? '14.0'), '14.0'),
  };
}
/** The deployment targets the SDK's units compile for: its Rust and C (the
 * app's crate with them, in one Cargo build), the Swift host, its arms and
 * extensions, and the filter kernels. Outside production trust they compile
 * for iOS's floor whatever the app's `minimumOS`, so every app shares one
 * compile of each, which a minimum of its own compiled cold (about 440 s).
 * Code built for 17.0 runs on every later iOS, and the app installs only where
 * its Info.plist says. Rust and C check no availability as they compile, so the
 * app's crate loses nothing at the floor. An app with `launch` modules keeps
 * its own minimum: their Swift compiles inside the host's package
 * (`ExactLaunchParts`), where Swift checks availability. */
export const compiledTargets = (app, trust = process.env.EXACT_UPDATE_TRUST ?? 'development') =>
  trust === 'production' || app.manifest.launch?.length ? deploymentTargets(app) : { ...deploymentTargets(app), ios: '17.0' };

/** The capabilities with a Swift half, by name, and the product of
 * `Package.swift` (its `capabilities` table) each is. */
const SWIFT_CAPABILITIES = [['grouped_lists', 'ExactGroupedLists'], ['markdown', 'ExactMarkdown'], ['surfaces', 'ExactSurfaces'], ['drag', 'ExactDrag'], ['media_picker', 'ExactMediaPicker'], ['auth_session', 'ExactAuthSession'], ['numerals', 'ExactNumerals'], ['tips', 'ExactTips']];

/** The capabilities a development build links only as production would
 * (LLP 1047.001.000 F): each links a framework that costs every iOS launch
 * its images (PhotosUI, AuthenticationServices, ~300 each; SwiftUI; TipKit),
 * and the agent's substitutes answer without the first two. */
const BY_USE_IN_DEVELOPMENT = ['media_picker', 'auth_session', 'numerals', 'tips'];

/** What the archive links (LLP 1047.001 D2, D4, D6; LLP 1047 D8): for a
 * production build, the plan's use-set, as the bake's graph names it, the
 * manifest's `link` (capabilities linked ahead of use) and, for an app
 * whose grants name `auth.session` (`granted`), the sign-in sheet; every
 * capability for every other build, but those of `BY_USE_IN_DEVELOPMENT`,
 * which it links as production would. `link` is `EXACT_APPLE_LINK` for
 * SwiftPM (the names, `none`, or `all`); `products` the Swift capability
 * products the composition links and installs, and an embedder links beside
 * ExactKit. Cargo is told `plan` or `all` before the bake, and the entry
 * derives the same set from the same plan, grants and manifest
 * (`exact_bake::apple_link`). */
export function appleComposition(graph, production, ahead = [], granted = false) {
  const uses = graph?.uses;
  const byUse = Array.isArray(uses) ? [...uses, ...ahead, ...(granted ? ['auth_session'] : [])] : null;
  const link = !byUse ? 'all'
    : production ? [...new Set(byUse)].join(',') || 'none'
    : SWIFT_CAPABILITIES.map(([name]) => name).filter((name) => !BY_USE_IN_DEVELOPMENT.includes(name) || byUse.includes(name)).join(',');
  const products = SWIFT_CAPABILITIES.filter(([name]) => link === 'all' || link.split(',').includes(name)).map(([, product]) => product);
  return { link, products };
}

/** The system frameworks only a capability's Swift half brings, each with
 * the products that may bring it, and so absent from an executable whose
 * composition links none of them (LLP 1047.001.000 §6, the check's first
 * form). SafariServices belongs to @exact/in-app-browser in ExactAppModules,
 * not a SwiftPM capability product. */
const CAPABILITY_FRAMEWORKS = [['SafariServices', ['@exact/in-app-browser']], ['PhotosUI', ['ExactMediaPicker']], ['AuthenticationServices', ['ExactAuthSession']], ['SwiftUI', ['ExactNumerals', 'ExactTips']], ['TipKit', ['ExactTips']], ['WidgetKit', ['ExactSnapshot']], ['ActivityKit', ['ExactSnapshot']]];

export function appleLaunchFrameworks(destination, product) {
  return destination === 'ios' && ['ExactIOS', 'ExactHostIOS'].includes(product) ? ['SafariServices'] : [];
}

export function appleLaunchLinkArgs(destination, product) {
  // UIKit's -[UIApplication _accessibilityInit] loads this closure at every launch;
  // dyld loads it cheaper (LLP 1047.001.001). Keep the link even with dylib dead stripping.
  return appleLaunchFrameworks(destination, product).flatMap(name => ['-Xlinker', '-needed_framework', '-Xlinker', name]);
}

/** The dylibs an executable's load commands name (`LC_LOAD_DYLIB` and its
 * weak, re-export, lazy and upward forms), every slice of a universal one.
 * Read from the file, since `otool` takes a path ending in `name(member)`
 * for an archive's member (#234). */
export function loadedDylibs(executable) {
  return [...new Set(machoCommands(executable).flatMap(commands => commands.flatMap(({ cmd, bytes }) => {
    if (![0xc, 0x80000018, 0x8000001f, 0x20, 0x80000023].includes(cmd)) return [];
    return [dylibName(executable, bytes)];
  })))];
}

function dylibName(executable, bytes) {
  const at = bytes.length >= 24 ? bytes.readUInt32LE(8) : -1, end = bytes.indexOf(0, at);
  if (at < 24 || at >= bytes.length || end < at) throw new Error(`host/apple: ${basename(executable)} has an invalid dylib load command`);
  return bytes.toString('utf8', at, end);
}

/** One LC_ID_DYLIB per slice; null means that slice is not a dynamic library. */
export function dylibInstallNames(executable) {
  return machoCommands(executable).map(commands => {
    const ids = commands.filter(({ cmd }) => cmd === 0xd);
    return ids.length === 1 ? dylibName(executable, ids[0].bytes) : null;
  });
}

/** Load commands bounded by their own slice, so build checks need no host's Mach-O tools. */
function machoCommands(executable) {
  let bytes;
  try { bytes = readFileSync(executable); } catch (e) { throw new Error(`host/apple: cannot read ${executable}: ${e.message}`); }
  const invalid = () => { throw new Error(`host/apple: ${basename(executable)} has an invalid 64-bit Mach-O`); };
  const slice = (at, size) => {
    if (!Number.isSafeInteger(at) || !Number.isSafeInteger(size) || at < 0 || size < 32 || at + size > bytes.length) invalid();
    if (bytes.readUInt32LE(at) !== 0xfeedfacf) invalid();
    const count = bytes.readUInt32LE(at + 16), end = at + 32 + bytes.readUInt32LE(at + 20), commands = [];
    if (end > at + size || count > (end - at - 32) / 8) invalid();
    let cursor = at + 32;
    for (let i = 0; i < count; i++) {
      if (cursor + 8 > end) invalid();
      const cmd = bytes.readUInt32LE(cursor), length = bytes.readUInt32LE(cursor + 4);
      if (length < 8 || length % 4 || cursor + length > end) invalid();
      commands.push({ cmd, bytes: bytes.subarray(cursor, cursor + length) });
      cursor += length;
    }
    if (cursor !== end) invalid();
    return commands;
  };
  if (bytes.length < 4) invalid();
  const magic = bytes.readUInt32BE(0);
  if (magic !== 0xcafebabe && magic !== 0xcafebabf) return [slice(0, bytes.length)];
  if (bytes.length < 8) invalid();
  const count = bytes.readUInt32BE(4), width = magic === 0xcafebabf ? 32 : 20;
  if (!count || count > (bytes.length - 8) / width) invalid();
  return Array.from({ length: count }, (_, i) => {
    const at = 8 + i * width;
    const offset = width === 32 ? Number(bytes.readBigUInt64BE(at + 8)) : bytes.readUInt32BE(at + 8);
    const size = width === 32 ? Number(bytes.readBigUInt64BE(at + 16)) : bytes.readUInt32BE(at + 12);
    if (offset < 8 + count * width) invalid();
    return slice(offset, size);
  });
}

/** An executable links no framework that only an unlinked capability
 * brings: the next import of one into the core fails the build by name
 * instead of costing every launch silently. */
export function assertCapabilityFrameworks(executable, products, allowedFrameworks = [], frameworks = CAPABILITY_FRAMEWORKS) {
  const loaded = loadedDylibs(executable);
  const stray = frameworks.filter(([framework, owners]) => !owners.some((owner) => products.includes(owner)) && !allowedFrameworks.includes(framework) && loaded.some((path) => path.includes(`/${framework}.framework/`) && path.endsWith(`/${framework}`)));
  if (stray.length) throw new Error(`host/apple: ${basename(executable)} links ${stray.map(([framework, owners]) => `${framework} (only ${owners.join(' or ')} may)`).join(', ')}, which its composition does not link: an import outside the capability's module brings it to every launch`);
}

/** Whether a build links by its plan (LLP 1047.001 D6): production always;
 * a development build with `--link plan`, so a smoke runs what ships
 * (development otherwise links everything, for the agent's whole vocabulary
 * and plan restarts). */
export function linksByPlan(production, args) {
  return production || (args.includes('--link') && args[args.indexOf('--link') + 1] === 'plan');
}

/** The SDK the linker recorded in an executable (`LC_BUILD_VERSION`) is the
 * one asked for: AppKit draws its design by that number, so a link that
 * records another (as SwiftPM's did, LLP 1069.011 §7) changes every app's
 * look without a word. Compare major.minor in every slice, reading the file
 * directly so the same check runs on Linux and accepts names ending in parentheses (#234). */
export function assertLinkedSdk(executable, expected) {
  const majorMinor = (v) => String(v).split('.').slice(0, 2).map(Number).join('.');
  for (const commands of machoCommands(executable)) {
    const versions = commands.filter(({ cmd }) => cmd === 0x32);
    const command = versions.length === 1 ? versions[0].bytes : null;
    const sdk = command?.length >= 24 ? command.readUInt32LE(16) : null;
    // As vtool prints it: the patch only when it is not 0.
    const recorded = sdk == null ? null : `${sdk >>> 16}.${(sdk >>> 8) & 255}${sdk & 255 ? `.${sdk & 255}` : ''}`;
    if (!recorded || majorMinor(recorded) !== majorMinor(expected)) {
      throw new Error(`host/apple: ${basename(executable)} records SDK ${recorded ?? '(none)'}, not ${expected}: AppKit would draw it in another design`);
    }
  }
}
