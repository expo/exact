import { writePrivacyManifest, privacyManifestForBinaries, privacyAPIManifest } from './privacy.mjs';
// The app's icon and asset catalog, from its manifest, for an Apple bundle
// (host/apple/build.mjs assembles it).
import { spawnSync } from './toolchain.mjs';
import { createHash } from 'node:crypto';
import { tmpdir } from 'node:os';
import { basename, dirname, relative, resolve, isAbsolute } from 'node:path';
import { chmodSync, closeSync, lstatSync, openSync, readlinkSync, readSync, copyFileSync, realpathSync, cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, readdirSync, renameSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
// The build's stamp, which build.mjs merges into Info.plist beside the icon keys.
export { buildInfo } from './buildinfo.mjs';
import { iconDigests, pickIcon, placeRgba, writeTinted } from '../../scripts/app-icons.mjs';
import { encodePng } from '../../scripts/png.mjs';

const run = (cmd, args, opts = {}) => {
  const r = spawnSync(cmd, args, { ...opts, stdio: opts.stdio === 'ignore' ? ['ignore', 'ignore', 'pipe'] : opts.stdio ?? 'inherit' });
  if (r.status !== 0) throw new Error(`${cmd} failed (${r.status ?? r.error?.message})${r.stderr?.length ? ': ' + String(r.stderr).trim() : ''}`);
  return r;
};
const sized = (source, width, height, out) => run('sips', ['-z', String(height), String(width), source, '--out', out], { stdio: 'ignore' });

/** The manifest's icon for `platform`'s `slot` (LLP 1163 D6), unless an Icon Composer project
 * replaces it there (`composerIcon`): the light icon, a square `maskable`, else `any`, which iOS
 * and watchOS need opaque; the dark one from `color_scheme_dark.icons`; the tinted one from a `monochrome`
 * glyph. watchOS takes the light icon alone. */
const ICON_SLOTS = {
  light: (platform) => ({ purposes: ['maskable', 'any'], ...(platform === 'macos' ? {} : { opaque: 'refuse' }), name: `the ${{ ios: 'iOS', macos: 'macOS', watchos: 'watchOS' }[platform]} icon` }),
  dark: () => ({ scheme: 'dark', purposes: ['any'], name: "iOS's dark icon" }),
  tinted: () => ({ purposes: ['monochrome'], glyph: true, name: "iOS's tinted icon" }),
};
const icon = (app, platform, slot = 'light') => composerIcon(app, platform) ? null
  : pickIcon(app.manifest, app.dir, { px: 1024, formats: ['png'], native: true, ...ICON_SLOTS[slot](platform) });

/** The Icon Composer project that replaces the manifest's icons on `platform` (LLP 1163 D4):
 * `host.ios.icon` or `host.macos.icon`, and on watchOS `host.ios.icon` when it draws a watch
 * icon, which Icon Composer records by listing watchOS in `icon.json`'s
 * `supported-platforms.circles`. tvOS takes none. */
function composerIcon(app, platform) {
  const selected = app.manifest.host?.[platform === 'watchos' ? 'ios' : platform]?.icon ?? null;
  if (platform !== 'watchos' || !selected) return selected;
  const json = resolve(app.dir, selected, 'icon.json');
  let circles;
  try { circles = existsSync(json) ? JSON.parse(readFileSync(json, 'utf8'))['supported-platforms']?.circles : null; } catch { throw new Error(`host.ios.icon: ${selected}/icon.json is not JSON`); }
  return Array.isArray(circles) && circles.includes('watchOS') ? selected : null;
}

/** tvOS's icon (LLP 1163 D6): the brand assets `actool` compiles into the Home Screen icon and
 * the Top Shelf images. The app icon is a stack of two to five layers at 400×240 pt, and
 * 1280×768 px for the App Store; the manifest's is a 5:3 `any` image. A Top Shelf image is 8:3
 * at 1920×720 pt, the wide one 29:9 at 2320×720 pt (HIG, "Top shelf";
 * https://developer.apple.com/library/archive/documentation/Xcode/Reference/xcode_ref-Asset_Catalog_Format/BrandAssetsType.html). */
const TV_BRAND = 'App Icon & Top Shelf Image';
const TV_ICON = { purposes: ['any'], shape: 5 / 3, px: 1280, formats: ['png'], native: true, opaque: 'refuse', name: 'the tvOS icon' };
const TV_STACKS = [['App Icon - App Store', 1280, 768, [1]], ['App Icon', 400, 240, [1, 2]]];
const TOP_SHELF = [['Top Shelf Image', 'top-shelf-image', 1920, 8 / 3], ['Top Shelf Image Wide', 'top-shelf-image-wide', 2320, 29 / 9]];
/** The app's own brand assets among its catalogs, by name. */
const brandAssets = catalogs => [...new Set(catalogs.flatMap(entry => entry.files
  .map(file => /(?:^|\/)([^/]+)\.brandassets\/Contents\.json$/.exec(file.name)?.[1]).filter(Boolean)))].sort();

/** The app icon from the manifest: on iOS an `AppIcon.appiconset` in `catalog` with the light
 * image and, where the manifest has them, the dark and tinted ones, which only an asset catalog
 * carries; `actool` writes the bundle's fallback PNGs and `CFBundleIcons` from it. On watchOS
 * the same set with the light image alone, which the system masks to a circle. On macOS an
 * `.icns` built by `iconutil`, whose plist key this returns. Nothing when the app declares no
 * such icon. */
export function appIcon(app, dir, platform, { catalog = false } = {}) {
  const light = icon(app, platform);
  if (!light) return {};
  if (platform === 'ios' || platform === 'watchos') {
    if (!catalog) return {};
    const set = resolve(catalog, 'AppIcon.appiconset');
    mkdirSync(set, { recursive: true });
    const images = [];
    const add = (filename, appearance) => images.push({ ...(appearance ? { appearances: [{ appearance: 'luminosity', value: appearance }] } : {}), filename, idiom: 'universal', platform, size: '1024x1024' });
    sized(light.path, 1024, 1024, resolve(set, 'icon.png'));
    add('icon.png');
    const dark = platform === 'ios' && icon(app, platform, 'dark');
    if (dark) { sized(dark.path, 1024, 1024, resolve(set, 'icon-dark.png')); add('icon-dark.png', 'dark'); }
    const tinted = platform === 'ios' && icon(app, platform, 'tinted');
    if (tinted) {
      sized(tinted.path, 1024, 1024, resolve(set, 'glyph.png'));
      writeTinted(resolve(set, 'glyph.png'), resolve(set, 'icon-tinted.png'));
      rmSync(resolve(set, 'glyph.png'));
      add('icon-tinted.png', 'tinted');
    }
    writeFileSync(resolve(set, 'Contents.json'), JSON.stringify({ images, info: { author: 'exact', version: 1 } }));
    return {};
  }
  const work = mkdtempSync(resolve(tmpdir(), 'exact-icns-')), set = resolve(work, 'AppIcon.iconset');
  try {
    mkdirSync(set);
    for (const base of [16, 32, 128, 256, 512]) {
      sized(light.path, base, base, resolve(set, `icon_${base}x${base}.png`));
      sized(light.path, base * 2, base * 2, resolve(set, `icon_${base}x${base}@2x.png`));
    }
    run('iconutil', ['-c', 'icns', set, '-o', resolve(dir, 'AppIcon.icns')], { stdio: 'ignore' });
  } finally { rmSync(work, { recursive: true, force: true }); }
  return { CFBundleIconFile: 'AppIcon' };
}

/** tvOS's brand assets from the manifest (`TV_BRAND` in `catalog`), returning their name, or
 * null when it has no 5:3 icon. Each stack has the fewest layers tvOS takes, two: the image at
 * the back and a transparent layer in front. A Top Shelf image, which `actool` does not require,
 * is written only from a file at least its 1x size, and its 2x image only from a file at least
 * that large, since a native slot is never scaled up (LLP 1163 D5): the system scales a lone 1x. */
function tvIcon(app, catalog) {
  const source = pickIcon(app.manifest, app.dir, TV_ICON);
  if (!source) return null;
  const brand = resolve(catalog, `${TV_BRAND}.brandassets`), assets = [];
  const contents = (dir, value) => { mkdirSync(dir, { recursive: true }); writeFileSync(resolve(dir, 'Contents.json'), JSON.stringify({ ...value, info: { author: 'exact', version: 1 } })); };
  const imageset = (dir, scales, write) => {
    mkdirSync(dir, { recursive: true });
    for (const scale of scales) write(resolve(dir, `${scale}x.png`), scale);
    contents(dir, { images: scales.map(scale => ({ filename: `${scale}x.png`, idiom: 'tv', scale: `${scale}x` })) });
  };
  for (const [name, width, height, scales] of TV_STACKS) {
    const stack = resolve(brand, `${name}.imagestack`);
    // Front to back, as Xcode lists them.
    contents(stack, { layers: [{ filename: 'Front.imagestacklayer' }, { filename: 'Back.imagestacklayer' }] });
    contents(resolve(stack, 'Front.imagestacklayer'), {});
    imageset(resolve(stack, 'Front.imagestacklayer/Content.imageset'), scales, (out, scale) => writeFileSync(out, encodePng(placeRgba(width * scale, height * scale))));
    contents(resolve(stack, 'Back.imagestacklayer'), {});
    imageset(resolve(stack, 'Back.imagestacklayer/Content.imageset'), scales, (out, scale) => sized(source.path, width * scale, height * scale, out));
    assets.push({ filename: `${name}.imagestack`, idiom: 'tv', role: 'primary-app-icon', size: `${width}x${height}` });
  }
  for (const [name, role, width, ratio] of TOP_SHELF) {
    const file = pickIcon(app.manifest, app.dir, { purposes: ['any'], shape: ratio, px: 2 * width, formats: ['png'], name });
    if (!file || file.width < width) continue;
    imageset(resolve(brand, `${name}.imageset`), file.width >= 2 * width ? [1, 2] : [1], (out, scale) => sized(file.path, width * scale, 720 * scale, out));
    assets.push({ filename: `${name}.imageset`, idiom: 'tv', role, size: `${width}x720` });
  }
  contents(brand, { assets });
  return TV_BRAND;
}

/** The iOS launch screen's picture (`host.ios.launchImage`) with the SHA-256
 * of its bytes, or null: a binary input, as the asset catalogs are, which the
 * bake records and packaging draws from the bytes it recorded. */
export function launchImageInventory(app) {
  const image = app.manifest.host?.ios?.launchImage;
  if (!image) return null;
  const source = resolve(app.dir, image.src);
  if (!existsSync(source)) throw new Error(`host/apple: ${app.name}'s launch image ${image.src} does not exist`);
  return { src: image.src, width: image.width, sha256: createHash('sha256').update(readFileSync(source)).digest('hex') };
}

/** Catalogs follow the app's native module directory selection; Icon Composer
 * projects are explicitly selected per platform. Their bytes
 * are native binary inputs, not portable plan assets or live update payloads. */
export function appleAssetCatalogInventory(app, platform) {
  const own = resolve(app.dir, platform === 'tvos' ? 'ios' : platform, 'modules');
  const folder = existsSync(own) ? own : resolve(app.dir, 'modules/apple');
  // The watch app compiles its icon alone.
  const catalogs = platform === 'watchos' ? [] : (existsSync(folder) ? readdirSync(folder) : []).filter(name => name.endsWith('.xcassets')).sort().map(name => {
    const path = resolve(folder, name);
    return { path: relative(app.dir, path), files: catalogFiles(path) };
  });
  const section = platform === 'watchos' ? 'ios' : platform, selected = ['ios', 'macos'].includes(section) && app.manifest?.host?.[section]?.icon;
  if (selected) {
    const path = resolve(app.dir, selected), field = `host.${section}.icon: ${selected}`;
    if (!existsSync(path)) throw new Error(`${field} does not exist`);
    if (!selected.endsWith('.icon') || !lstatSync(path).isDirectory()) throw new Error(`${field} must name an .icon directory`);
    if (isAbsolute(selected) || !path.startsWith(resolve(app.dir) + '/') || !realpathSync(path).startsWith(realpathSync(app.dir) + '/')) throw new Error(`${field} must be inside the app`);
    // `AppIcon` is the primary set's name (`iosAssets`), and a set named like the project would take its name;
    // any other `.appiconset` is an alternate icon, which actool compiles beside the project.
    const taken = new Set(['AppIcon.appiconset', `${basename(selected, '.icon')}.appiconset`]);
    for (const entry of catalogs) {
      const primary = readdirSync(resolve(app.dir, entry.path), { recursive: true }).sort().find(name => taken.has(basename(name)));
      if (primary) throw new Error(`${field} conflicts with ${entry.path}/${primary}; select one primary icon`);
    }
    if (composerIcon(app, platform)) catalogs.push({ path: relative(app.dir, path), files: catalogFiles(path) });
  }
  return catalogs;
}
function catalogFiles(root) {
  const files = [];
  const walk = path => {
    const info = lstatSync(path);
    if (info.isSymbolicLink()) throw new Error(`Apple asset catalogs must not contain symlinks: ${path}`);
    if (info.isDirectory()) for (const name of readdirSync(path).sort()) walk(resolve(path, name));
    else if (info.isFile()) files.push({ name: relative(root, path), sha256: createHash('sha256').update(readFileSync(path)).digest('hex') });
    else throw new Error(`Unsupported Apple asset catalog input: ${path}`);
  };
  if (!lstatSync(root).isDirectory()) throw new Error(`Apple asset catalog must be a directory: ${root}`);
  walk(root);
  return files;
}

/** The alternate icons among an iOS app's catalogs: each `.appiconset` but
 * the primary, which is `AppIcon` or the selected Icon Composer project's. */
const alternates = catalogs => {
  const selected = catalogs.find(entry => entry.path.endsWith('.icon'));
  return [...new Set(catalogs.flatMap(entry => entry.files
    .map(file => /(?:^|\/)([^/]+)\.appiconset\/Contents\.json$/.exec(file.name)?.[1])
    .filter(name => name && name !== 'AppIcon' && name !== (selected && basename(selected.path, '.icon')))))].sort();
};

/** Why an iOS bundle cannot have its primary icon, or null: a manifest icon
 * the slots refuse (`icon`), or none where one is needed. A distributed
 * bundle (`catalog`) needs one in Assets.car, and so do alternate icons;
 * the primary is `host.ios.icon` or the manifest's. `catalogs` is
 * `appleAssetCatalogInventory`'s. */
function iconRefusal(app, catalogs, catalog) {
  let primary;
  try {
    // The watch app's icon is built with the phone's, from the same manifest.
    if (app.manifest.watch) icon(app, 'watchos');
    if (catalogs.some(entry => entry.path.endsWith('.icon'))) return null;
    primary = icon(app, 'ios'); icon(app, 'ios', 'dark'); icon(app, 'ios', 'tinted');
  } catch (e) { return `host/apple: ${app.name}: ${e.message}`; }
  if (primary) return null;
  if (catalog) return `host/apple: ${app.name}'s distribution bundle requires an AppIcon; declare host.ios.icon or a square icon of at least 1024 px`;
  if (alternates(catalogs).length) return `host/apple: ${app.name}'s alternate icons require a primary; declare host.ios.icon or a square icon of at least 1024 px`;
  return null;
}

/** The same for a tvOS bundle, whose icon is the brand assets of the app's catalogs, else the
 * manifest's (`tvIcon`): `actool` takes one as the app icon. */
function tvIconRefusal(app, catalogs, catalog) {
  const own = brandAssets(catalogs);
  if (own.length > 1) return `host/apple: ${app.name}'s catalogs hold several brand assets (${own.join(', ')}); keep one, the tvOS app icon`;
  if (own.length) return null;
  let primary;
  try { primary = pickIcon(app.manifest, app.dir, TV_ICON); } catch (e) { return `host/apple: ${app.name}: ${e.message}`; }
  if (primary || !catalog) return null;
  return `host/apple: ${app.name}'s distribution bundle requires a tvOS app icon; declare a 5:3 icon of at least 1280×768 px, or brand assets in a catalog`;
}
/** The same for a Mac bundle, whose icon is optional: a manifest icon its slot refuses. */
function macIconRefusal(app) {
  try { icon(app, 'macos'); } catch (e) { return `host/apple: ${app.name}: ${e.message}`; }
  return null;
}
const ICON_REFUSALS = { ios: iconRefusal, tvos: tvIconRefusal, macos: macIconRefusal };

/** What packaging `app` for `platform` would refuse from its manifest and
 * catalogs alone, so the build says it before it compiles anything (a cold
 * device build packages after half an hour); packaging and the bake refuse
 * the same through these same functions. `archive`: a distributed iOS or tvOS
 * bundle (`--archive`). The manifest's own files are checked when it is read
 * (scripts/app-icons.mjs `appIconProblems`, scripts/app.mjs `launchImageProblems`). */
export function appleRefusals(app, platform, { archive = false } = {}) {
  let catalogs;
  try { catalogs = appleAssetCatalogInventory(app, platform); } catch (e) { return [e.message]; }
  const refused = [ICON_REFUSALS[platform]?.(app, catalogs, archive)];
  // Info.plist's ExactModuleConfig is `moduleConfig` as written: refused before anything compiles.
  try { plist({ moduleConfig: app.manifest.moduleConfig ?? {} }); } catch (e) { refused.push(`${app.name}: app.json's ${e.message.slice('plist: '.length)}`); }
  return refused.filter(Boolean);
}

/** One actool pass includes the app's catalogs, selected or generated icon and launch
 * colours: separate passes silently replace Assets.car. No unchanged build
 * reruns actool; the cache key includes source bytes, target and toolchain. */
export function iosAssets(app, dir, device, options = {}) {
  return appleAssets(app, dir, { ...options, device, platform: options.tv ? 'tvos' : 'ios' });
}
export function appleAssets(app, dir, { platform = 'macos', device = false, catalog = false, requireIcon = catalog, kept = null, expected, expectedLaunchImage, launch = 'system' } = {}) {
  // tvOS takes the manifest's iOS section; the watch app's deployment target is watch.mjs's.
  const minimum = platform === 'watchos' ? '10.0' : app.manifest.host?.[platform === 'tvos' ? 'ios' : platform]?.minimumOS ?? (platform === 'macos' ? '14.0' : '17.0');
  const catalogs = appleAssetCatalogInventory(app, platform);
  const selected = catalogs.find(entry => entry.path.endsWith('.icon'));
  if (expected !== undefined && JSON.stringify(catalogs) !== JSON.stringify(expected))
    throw new Error('Apple asset catalogs changed after the bake; rebuild before packaging');
  const launchImage = platform === 'ios' ? launchImageInventory(app) : null;
  if (expectedLaunchImage !== undefined && JSON.stringify(launchImage) !== JSON.stringify(expectedLaunchImage))
    throw new Error('the launch image changed after the bake; rebuild before packaging');
  const refused = ICON_REFUSALS[platform]?.(app, catalogs, requireIcon);
  if (refused) throw new Error(refused);
  if (platform === 'macos' && !catalogs.length) { rmSync(resolve(dir, 'Assets.car'), { force: true }); return {}; }
  if (kept) {
    const key = createHash('sha256').update(JSON.stringify([catalogs, iconDigests(app.manifest, app.dir), app.manifest.background_color ?? null, app.manifest.color_scheme_dark?.background_color ?? null, launch, launchImage,
      minimum, platform, device, catalog, kept.stamp,
      // What the compile itself writes: a cache from before `--accent-color` named no accent,
      // one from before `ExactLaunchImageSource` no launch image's source or digest,
      // one from before `digest` launch assets under fixed names
      'live-icons-with-manifest-fallback', 'accent-color', 'launch-image-source', 'launch-image-digest', 'launch-names-digest', 'alternate-app-icons', 'icon-appearances', 'tv-brand-assets', 'launch-color-components'])).digest('hex').slice(0, 16);
    const made = resolve(kept.dir, `assets-${platform}-${key}`);
    if (!existsSync(resolve(made, 'keys.json'))) {
      const making = `${made}.${process.pid}.tmp`;
      rmSync(making, { recursive: true, force: true });
      mkdirSync(making, { recursive: true });
      try {
        writeFileSync(resolve(making, 'keys.json'), JSON.stringify(appleAssets(app, making, { platform, device, catalog, requireIcon, expected: catalogs, expectedLaunchImage: launchImage, launch })));
        rmSync(made, { recursive: true, force: true });
        renameSync(making, made);
      } finally { rmSync(making, { recursive: true, force: true }); }
      for (const old of readdirSync(kept.dir)) if (old.startsWith(`assets-${platform}-`) && resolve(kept.dir, old) !== made) rmSync(resolve(kept.dir, old), { recursive: true, force: true });
    }
    rmSync(resolve(dir, 'Assets.car'), { force: true });
    for (const file of readdirSync(made)) if (file !== 'keys.json') cpSync(resolve(made, file), resolve(dir, file), { recursive: true });
    return JSON.parse(readFileSync(resolve(made, 'keys.json'), 'utf8'));
  }
  const work = mkdtempSync(resolve(tmpdir(), 'exact-apple-assets-'));
  try {
    const sources = catalogs.map(entry => {
      const copied = resolve(work, 'app', entry.path);
      cpSync(resolve(app.dir, entry.path), copied, { recursive: true });
      if (JSON.stringify(catalogFiles(copied)) !== JSON.stringify(entry.files))
        throw new Error('Apple asset catalogs changed while capturing them; rebuild before packaging');
      return copied;
    });
    const assets = resolve(work, 'Generated.xcassets');
    // The launch image drawn is the bytes the inventory hashed, copied once.
    let picture = null;
    if (launchImage) {
      const bytes = readFileSync(resolve(app.dir, launchImage.src));
      if (createHash('sha256').update(bytes).digest('hex') !== launchImage.sha256) throw new Error('the launch image changed while capturing it; rebuild before packaging');
      picture = { path: resolve(work, 'launch.png'), width: launchImage.width, sha256: launchImage.sha256 };
      writeFileSync(picture.path, bytes);
    }
    const alternateIcons = platform === 'ios' ? alternates(catalogs) : [];
    // The icon is compiled in every build: only Assets.car carries its dark and tinted appearances, and actool
    // writes the bundle's fallback PNGs and CFBundleIcons from it (LLP 1163 D6).
    const keys = platform === 'ios' ? { ...appIcon(app, dir, 'ios', { catalog: assets }), ...launchScreen(app, assets, launch, picture) }
      : platform === 'watchos' ? appIcon(app, dir, 'watchos', { catalog: assets }) : {};
    // tvOS's icon is the brand assets of the app's catalogs, else the manifest's.
    const own = platform === 'tvos' ? brandAssets(catalogs)[0] : null, brand = platform === 'tvos' && !own ? tvIcon(app, assets) : null;
    const hasIcon = existsSync(resolve(assets, 'AppIcon.appiconset'));
    const iconName = selected ? basename(selected.path, '.icon') : own ?? brand ?? (hasIcon ? 'AppIcon' : null);
    if (hasIcon || brand || keys.UILaunchScreen) {
      writeFileSync(resolve(assets, 'Contents.json'), JSON.stringify({ info: { author: 'exact', version: 1 } }));
      sources.push(assets);
    }
    rmSync(resolve(dir, 'Assets.car'), { force: true });
    // An `AccentColor` colour set is the app's accent, as Xcode's default
    // names it: actool writes `NSAccentColorName`, which AppKit applies and
    // ExactKit gives every UIKit window (`AppAccent`) under an `auto` accent.
    const accent = catalogs.some(c => c.files.some(f => /(^|\/)AccentColor\.colorset\/Contents\.json$/.test(f.name)));
    if (sources.length) {
      const partial = resolve(work, 'partial.plist');
      const target = { macos: 'macosx', ios: device ? 'iphoneos' : 'iphonesimulator', tvos: device ? 'appletvos' : 'appletvsimulator', watchos: device ? 'watchos' : 'watchsimulator' }[platform];
      run('xcrun', ['actool', ...sources, '--compile', dir, '--platform', target, '--minimum-deployment-target', minimum,
        ...{ macos: [], ios: ['iphone', 'ipad'], tvos: ['tv'], watchos: ['watch'] }[platform].flatMap(family => ['--target-device', family]),
        ...(iconName ? ['--app-icon', iconName] : []),
        ...(selected && platform === 'ios' && process.platform === 'linux' ? (() => {
          const fallback = (app.manifest.icons ?? []).find(icon => /^([0-9]+)x\1$/.test(icon.sizes ?? '') && Number(icon.sizes.split('x')[0]) >= 512 && icon.src?.toLowerCase().endsWith('.png'));
          return fallback ? ['--exact-icon-fallback', resolve(app.dir, fallback.src)] : [];
        })() : []),
        ...alternateIcons.flatMap(name => ['--alternate-app-icon', name]),
        ...(accent ? ['--accent-color', 'AccentColor'] : []),
        '--output-partial-info-plist', partial, '--output-format', 'human-readable-text'], { stdio: 'ignore' });
      Object.assign(keys, JSON.parse(run('plutil', ['-convert', 'json', '-o', '-', partial], { encoding: 'utf8', stdio: 'pipe' }).stdout));
    }
    // actool reports an icon it cannot compile and still succeeds, so a distributed bundle's is looked for in Assets.car:
    // on tvOS under the name of the brand assets' app icon stack, which the partial plist gives.
    if (catalog && (platform === 'ios' || platform === 'tvos') && iconName) {
      const name = platform === 'tvos' ? keys.CFBundleIcons?.CFBundlePrimaryIcon : iconName;
      const contents = JSON.parse(run('xcrun', ['assetutil', '--info', resolve(dir, 'Assets.car')], { encoding: 'utf8', stdio: 'pipe' }).stdout);
      if (!name || !contents.some(asset => asset.Name === name)) throw new Error(`host/apple: ${dir}/Assets.car has no ${name ?? iconName}`);
    }
    return keys;
  } finally { rmSync(work, { recursive: true, force: true }); }
}

/** The launch screen in the app's own background, light and dark
 * (the manifest's `background_color` and `color_scheme_dark.background_color`): iOS crossfades
 * from the launch screen to the first frame, and between two screens of one
 * colour that crossfade is invisible, so the app opens on its first frame.
 * Where the manifest names none, the app's unsaid background is the platform's
 * (LLP 1115 D2): `systemBackground`, which an empty `UILaunchScreen` already is
 * in both schemes, or, where the first route scrolls a grouped list (`launch`,
 * the bake's `launchBackground`, LLP 1116.011 D2.3), `systemGroupedBackground`,
 * which the system shows as #F2F2F7 and black. That is a colour set of its own
 * (a reference to the system colour compiles to its light value alone), so the
 * window under the first frame (`ExactLaunchColor`, the same components) is the
 * same grey and nothing lighter shows between the launch screen and the first
 * frame (stopwatch, round 13: "blank white screen for ~1 s").
 * `UILaunchScreen` names colours and images only from an asset catalog, so
 * this writes its colour set, and the manifest's `host.ios.launchImage`
 * (`picture`: its captured bytes' file and width) as an image set at its
 * width in points (1x, 2x and 3x), which iOS centres on that colour, for the
 * shared compile. Returns the plist keys to merge. */
function launchScreen(app, catalog, launch = 'system', picture = null) {
  const named = app.manifest.background_color;
  const light = named ?? (launch === 'grouped' ? '#F2F2F7' : undefined);
  const dark = named ? app.manifest.color_scheme_dark?.background_color : launch === 'grouped' ? '#000000' : undefined;
  const keys = {};
  if (picture) {
    keys.UIImageName = `ExactLaunchImage-${digest([picture.sha256, picture.width])}`;
    const set = resolve(catalog, `${keys.UIImageName}.imageset`);
    mkdirSync(set, { recursive: true });
    const images = [1, 2, 3].map(scale => {
      const filename = `launch@${scale}x.png`;
      run('sips', ['--resampleWidth', String(Math.round(picture.width * scale)), picture.path, '--out', resolve(set, filename)], { stdio: 'ignore' });
      return { idiom: 'universal', filename, scale: `${scale}x` };
    });
    writeFileSync(resolve(set, 'Contents.json'), JSON.stringify({ images, info: { author: 'exact', version: 1 } }));
  }
  // The plan's name for the picture and its bytes' SHA-256, so an image of
  // those bytes decodes from the catalog (`RasterInput.catalog`) and loads no
  // ImageIO reader; other bytes under the name (an update's) are decoded.
  const source = picture ? { ExactLaunchImageSource: app.manifest.host.ios.launchImage.src, ExactLaunchImageSHA256: picture.sha256 } : {};
  if (!light) return picture ? { UILaunchScreen: keys, ...source } : {};
  const components = (hex, field) => {
    const m = /^#([0-9a-f]{3}|[0-9a-f]{6}|[0-9a-f]{8})$/i.exec(hex ?? '');
    if (!m) throw new Error(`${field} must be a #RGB, #RRGGBB or #RRGGBBAA colour, not ${JSON.stringify(hex)}`);
    const h = m[1].length === 3 ? [...m[1]].map((c) => c + c).join('') : m[1];
    const a = h.length === 8 ? parseInt(h.slice(6), 16) : 255;
    return { 'color-space': 'srgb', components: { red: `0x${h.slice(0, 2)}`, green: `0x${h.slice(2, 4)}`, blue: `0x${h.slice(4, 6)}`, alpha: (a / 255).toFixed(3) } };
  };
  const colors = [{ idiom: 'universal', color: components(light, 'background_color') }];
  if (dark) colors.push({ idiom: 'universal', appearances: [{ appearance: 'luminosity', value: 'dark' }], color: components(dark, 'color_scheme_dark.background_color') });
  const name = `ExactLaunch-${digest(colors)}`;
  mkdirSync(resolve(catalog, `${name}.colorset`), { recursive: true });
  writeFileSync(resolve(catalog, `${name}.colorset`, 'Contents.json'), JSON.stringify({ colors, info: { author: 'exact', version: 1 } }));
  // The host uses the same sRGB components as the catalogue, including its
  // rounded alpha, without loading Assets.car before UIApplicationMain.
  const rgba = ({ color: { components: c } }) => [parseInt(c.red, 16) / 255, parseInt(c.green, 16) / 255, parseInt(c.blue, 16) / 255, Number(c.alpha)];
  return { UILaunchScreen: { UIColorName: name, ...keys }, ExactLaunchColor: { light: rgba(colors[0]), ...(dark ? { dark: rgba(colors[1]) } : {}) }, ...source };
}

/** A launch asset's name ends in a digest of what it draws: iOS keeps the launch
 * screen it drew for an app's `UILaunchScreen` and draws it again while those keys
 * are the same, through a reinstall of a new build of the same version, so a new
 * colour or picture under an unchanged name launched on the old one (the euxy
 * port's `background_color`, white on a simulator that had launched a white build). */
const digest = (content) => createHash('sha256').update(JSON.stringify(content)).digest('hex').slice(0, 8);

const NESTED_BUNDLE = /\.(app|appex|bundle|framework|plugin|xpc|systemextension)$/;

/** Whether a file is Mach-O code, by its magic and not its name: a helper
 *  executable or a `.node` addon has no `.dylib` to go by. Thin 32- and 64-bit,
 *  either byte order, and universal (fat) files. A fat magic is also a Java
 *  class file's, told apart by what follows it: a fat header's architecture
 *  count is small, a class file's version is not. */
function machO(path) {
  const head = Buffer.alloc(8);
  let fd;
  try {
    fd = openSync(path, 'r');
    if (readSync(fd, head, 0, 8, 0) < 8) return false;
  } catch { return false; } finally { if (fd !== undefined) closeSync(fd); }
  const magic = head.readUInt32BE(0);
  if ([0xfeedface, 0xfeedfacf, 0xcefaedfe, 0xcffaedfe].includes(magic)) return true;
  if (magic === 0xcafebabe || magic === 0xcafebabf) return head.readUInt32BE(4) < 32;
  if (magic === 0xbebafeca || magic === 0xbfbafeca) return head.readUInt32LE(4) < 32;
  return false;
}

/** A bundle's main executable, which codesign signs when it signs the bundle
 *  (and signing that path alone seals the bundle, before what it holds): an
 *  app's `Contents/MacOS/<name>`, a versioned framework's `Versions/<current>/<name>`
 *  as the walk reaches it (not through the `Current` link), a shallow bundle's
 *  `<name>`. The plist is XML or, through `plutil`, binary. */
function bundleExecutable(bundle) {
  const executable = (plist) => {
    if (!existsSync(plist)) return null;
    const xml = /<key>CFBundleExecutable<\/key>\s*<string>([^<]+)<\/string>/.exec(readFileSync(plist, 'utf8'))?.[1];
    if (xml) return xml;
    const read = spawnSync('plutil', ['-extract', 'CFBundleExecutable', 'raw', '-o', '-', plist], { encoding: 'utf8' });
    return read.status === 0 ? read.stdout.trim() || null : null;
  };
  let version = null;
  try { version = readlinkSync(resolve(bundle, 'Versions/Current')); } catch {}
  for (const [plist, dir] of [['Contents/Info.plist', 'Contents/MacOS'], ...(version ? [[`Versions/${version}/Resources/Info.plist`, `Versions/${version}`]] : []), ['Info.plist', '.']]) {
    const name = executable(resolve(bundle, plist));
    if (name) return resolve(bundle, dir, name);
  }
  return null;
}

/** Everything in a bundle that carries its own signature, innermost first:
 *  every Mach-O file (an executable, a library, an addon — wherever it sits)
 *  and every nested bundle after what it holds. A bundle is sealed over its
 *  contents, so code re-signed after its container invalidates the container.
 *  Symlinks are skipped: a framework's `Versions/Current` names what is signed
 *  through its real path. Baked code keeps its verified digest and signature;
 *  static assets cannot contain code a release would have to re-sign. */
export function signingOrder(bundle, compat = null, identity = null) {
  const preserved = new Map();
  const embedded = compat?.embedded;
  for (const card of embedded?.assets ?? []) {
    const path = resolve(bundle, 'Contents/Resources', card.name);
    if (!machO(path)) continue;
    if (!card.name.startsWith('rust/')) throw new Error(`Mach-O asset ${card.name}: move code to host.macos.resources before releasing`);
    preserved.set(path, card.sha256);
  }
  // GPU cards name the code digest, signature left out (codeDigest).
  const gpu = new Set();
  const gpuCards = [...(embedded?.gpu ? [['libexact_gpu.dylib', embedded.gpu]] : []),
    ...Object.entries(embedded?.gpuModules ?? {}).map(([name, card]) => [`libexact_gpu_${name.replaceAll('-', '_')}.dylib`, card])];
  for (const [file, card] of gpuCards) {
    const path = resolve(bundle, 'Contents/MacOS', file);
    gpu.add(path); preserved.set(path, card.sha256);
  }
  for (const [path, digest] of preserved) {
    if (!existsSync(path) || (gpu.has(path) ? codeDigest(readFileSync(path)) : createHash('sha256').update(readFileSync(path)).digest('hex')) !== digest)
      throw new Error(`baked code ${path}: digest differs; rebuild before signing`);
    run('codesign', ['--verify', '--strict', ...(identity && identity !== '-' ? ['-R', `=certificate leaf = H"${identity}"`] : []), path], { stdio: 'ignore' });
  }
  const order = [];
  const walk = (dir, main) => {
    for (const name of readdirSync(dir, { withFileTypes: true })) {
      const path = resolve(dir, name.name);
      if (name.isDirectory() && NESTED_BUNDLE.test(name.name) && bundleExecutable(path)) {
        walk(path, bundleExecutable(path));
        order.push(path);
      } else if (name.isDirectory()) walk(path, main);
      else if (name.isFile() && path.normalize('NFD') !== main?.normalize('NFD') && !preserved.has(path) && machO(path)) order.push(path);
    }
  };
  walk(resolve(bundle, 'Contents'), bundleExecutable(bundle));
  return [...order, bundle];
}

/** The Mach-O file type of a thin file, or of a universal file's first
 *  architecture (`mach-o/loader.h`: 2 is `MH_EXECUTE`, 6 `MH_DYLIB`). */
export function machOType(path) {
  const read = (at, n) => { const b = Buffer.alloc(n), fd = openSync(path, 'r'); try { return readSync(fd, b, 0, n, at) === n ? b : null; } finally { closeSync(fd); } };
  let head = read(0, 16);
  if (!head) return null;
  const fat = head.readUInt32BE(0);
  if (fat === 0xcafebabe || fat === 0xcafebabf) {
    const arch = read(8, 16);
    head = arch && read(fat === 0xcafebabe ? arch.readUInt32BE(8) : Number(arch.readBigUInt64BE(8)), 16);
    if (!head) return null;
  }
  const magic = head.readUInt32BE(0);
  if (magic === 0xfeedface || magic === 0xfeedfacf) return head.readUInt32BE(12);
  if (magic === 0xcefaedfe || magic === 0xcffaedfe) return head.readUInt32LE(12);
  return null;
}

/** The hardened-runtime entitlements `exact release` signs the Mac app with:
 * the bake's derivation from the app's `device.*` grants (a usage string
 * alone is not enough; a hardened app without the entitlement is silently
 * refused). Runtime exceptions never reach the app's own executable (LLP
 * 1069.008 D9). Null when the app grants no such device. */
export const macReleaseEntitlements = (compat, widgets = {}) => {
  const names = compat?.reach?.entitlements ?? [];
  // An https auth callback's `webcredentials:` domain (LLP 1069.006 D2),
  // which a Developer ID build signs only with a profile that grants it.
  const domains = compat?.reach?.auth?.associatedDomains ?? [];
  if (!names.length && !domains.length && !Object.keys(widgets).length) return null;
  return plistFile({ ...widgets, ...Object.fromEntries(names.map((name) => [name, true])),
    ...(domains.length ? { 'com.apple.developer.associated-domains': domains } : {}) });
};

/** The keys a macOS provisioning profile authorizes (presence.mjs `macProfileEntitlements`). */
export const PROFILE_KEYS = ['com.apple.application-identifier', 'com.apple.developer.team-identifier', 'keychain-access-groups',
  'com.apple.developer.aps-environment', 'com.apple.developer.devicecheck.appattest-environment', 'com.apple.developer.declared-age-range',
  'com.apple.developer.associated-domains'];

/** Of `bundle`'s signed entitlements, those a profile authorizes; `signed`: its entitlements as a plist (a test's). */
export function profileEntitlements(bundle, signed = run('codesign', ['-d', '--entitlements', '-', '--xml', bundle], { encoding: 'utf8', stdio: 'pipe' }).stdout) {
  if (!signed?.includes('<plist')) return {};
  const json = JSON.parse(run('plutil', ['-convert', 'json', '-o', '-', '-'], { input: signed, encoding: 'utf8', stdio: 'pipe' }).stdout);
  return Object.fromEntries(Object.entries(json).filter(([key]) => PROFILE_KEYS.includes(key)));
}

/** The executable a bundle runs, as Foundation resolves it (`NSBundle`
 *  `executablePath`, the resolver `codesign` shares): it honours
 *  `Info-macos.plist`, XML comments and entities, which `bundleExecutable`'s
 *  quick read does not. Real path, or null. */
function resolvedExecutable(bundle) {
  const read = spawnSync('osascript', ['-l', 'JavaScript', '-e',
    'ObjC.import("Foundation"); function run(argv) { const b = $.NSBundle.bundleWithPath(argv[0]); return b.isNil() ? "" : (ObjC.unwrap(b.executablePath) || ""); }',
    bundle], { encoding: 'utf8' });
  const path = read.status === 0 ? read.stdout.replace(/\n$/, '') : '';
  return path && existsSync(path) ? realpathSync(path) : null;
}

/** Each helper a `runtime.<exception> <path>` grant names (LLP 1069.008 D9),
 *  keyed by the path `signingOrder` signs it as (a nested bundle, for the
 *  executable that bundle runs), mapped to its entitlements plist. A target
 *  must be a regular Mach-O executable inside a `host.macos.resources`
 *  destination, reached through no symbolic link, with no other hard link
 *  (a signature written to it is every link's), and signed by the walk;
 *  every target that is not is refused, all in one error. No grant: an
 *  empty map. */
export function helperEntitlements(bundle, manifest, compat, order) {
  const declared = Object.entries(compat?.reach?.helperEntitlements ?? {});
  const signed = new Map(), errors = [];
  if (!declared.length) return signed;
  const roots = nativeResourceMappings(manifest).map(({ to }) => to);
  const contents = resolve(bundle, 'Contents');
  // Signing a nested bundle signs the executable Foundation resolves for it,
  // whichever file the walk took for its main one; so a target that is that
  // executable takes its grant through the bundle, which the walk signs after
  // anything it signed the file as on its own.
  const runs = new Map();
  for (const p of order) if (p !== bundle && NESTED_BUNDLE.test(p)) {
    const executable = resolvedExecutable(p);
    if (executable) runs.set(executable, p);
  }
  for (const [path, keys] of declared) {
    const file = resolve(contents, path), info = lstatSync(file, { throwIfNoEntry: false });
    const real = info?.isFile() ? realpathSync(file) : null;
    const at = runs.get(real) ?? (order.includes(file) ? file : null);
    const why = !roots.some((to) => path.startsWith(to + '/')) ? `is not inside a host.macos.resources destination (${roots.join(', ') || 'none declared'})`
      : !info?.isFile() ? 'is not a file in the bundle'
      : real !== resolve(realpathSync(contents), path) ? 'is reached through a link'
      : info.nlink !== 1 ? 'has another hard link, which its signature would reach'
      : machOType(file) !== 2 ? 'is not a Mach-O executable'
      : !at ? 'is not code the release signs' : null;
    if (why) errors.push(`runtime grant: Contents/${path} ${why}`);
    else signed.set(at, plistFile(Object.fromEntries(keys.map((key) => [key, true]))));
  }
  if (errors.length) throw new Error(errors.join('\n'));
  return signed;
}

/** The release's signature walk: every path `signingOrder` names, innermost
 *  first, with `identity`, the hardened runtime and `timestamp`. The app
 *  carries `macReleaseEntitlements`, each helper a runtime grant names its
 *  own (`helperEntitlements`), and a widget extension keeps its signing entitlements. The plists
 *  are written into `scratch`. */
export function signRelease(bundle, { compat, manifest, identity, id, scratch, timestamp = ['--timestamp'] }) {
  const order = signingOrder(bundle, compat, identity);
  const plists = helperEntitlements(bundle, manifest, compat, order);
  const info = JSON.parse(run('plutil', ['-convert', 'json', '-o', '-', resolve(bundle, 'Contents/Info.plist')], { encoding: 'utf8', stdio: 'pipe' }).stdout);
  const widgets = info.ExactAppGroup ? { 'com.apple.security.application-groups': [info.ExactAppGroup] } : {};
  if (info.ExactAppGroup && existsSync(resolve(bundle, 'Contents/embedded.provisionprofile'))) {
    if (!info.ExactKeychainAccessGroup) throw new Error('Mac widget profile has no restricted Keychain group in the app plist');
    Object.assign(widgets, { 'keychain-access-groups': [info.ExactKeychainAccessGroup],
      'com.apple.application-identifier': info.ExactKeychainAccessGroup,
      'com.apple.developer.team-identifier': info.ExactAppGroup.split('.')[0] });
  }
  const extension = resolve(bundle, 'Contents/PlugIns/ExactWidgets.appex');
  if (info.ExactAppGroup && existsSync(extension)) {
    const signed = run('codesign', ['-d', '--entitlements', '-', '--xml', extension], { encoding: 'utf8', stdio: 'pipe' }).stdout;
    if (!signed?.includes('<plist')) throw new Error('Mac widget extension has no entitlements to preserve');
    plists.set(extension, signed);
  }
  // What the build signed under its embedded macOS profile (LLP 1133.010 D7), kept as the build chose it.
  if (existsSync(resolve(bundle, 'Contents/embedded.provisionprofile'))) Object.assign(widgets, profileEntitlements(bundle));
  const own = macReleaseEntitlements(compat, widgets);
  if (own) plists.set(bundle, own);
  let n = 0;
  for (const path of order) {
    const plist = plists.get(path), file = plist && resolve(scratch, `entitlements-${n++}.plist`);
    if (plist) writeFileSync(file, plist);
    run('codesign', ['--force', '--sign', identity, '--options', 'runtime', ...timestamp,
      ...(path === bundle ? ['--identifier', id] : []), ...(plist ? ['--entitlements', file] : []), path]);
  }
}

/** The folder of an iOS or tvOS bundle that holds the trees `host.ios.resources`
 *  declares and nothing else: what `bundle:/` names there (LLP 1133.007 D6). Their
 *  `to` is the Mac's `Resources/…`, so one `fs.read bundle:/<to>` line reads the
 *  same tree on both. */
export const IOS_RESOURCE_ROOT = 'ExactResources';

/** Refuse an iOS or tvOS executable name (`executableName` of the app's name, or of
 *  the App Clip's: `key` says which) that is, ignoring case as the volume may, a name
 *  the build writes beside it at the top of the flat bundle: `IOS_RESOURCE_ROOT` when
 *  trees are declared, and what every bundle holds. One written over the other fails
 *  the build part-way. */
export function iosExecutableRefusal(manifest, name, key = 'app.name') {
  const written = [...(manifest.host?.ios?.resources?.length ? [IOS_RESOURCE_ROOT] : []), 'Frameworks', 'Info.plist', 'PrivacyInfo.xcprivacy', 'receipt.json', '_CodeSignature'];
  const taken = written.find(n => n.toLowerCase() === name.toLowerCase());
  if (taken) throw new Error(`app.json's ${key} "${name}" names an iOS executable, which its bundle's own "${taken}" would take the place of: rename it`);
}

/** The trees every Apple bundle gets from the build's capture (`copyAppleStaticTrees`):
 *  under the Mac's `Contents/Resources`, at the top of an iOS bundle. */
export const APPLE_STATIC_TREES = ['assets', 'deck', 'shaders', 'exact-metal', 'rust', 'plans', 'typescript', 'activities'];

/** What the Mac build writes under `Contents/Resources` (bundle.mjs, intents.mjs) beside
 *  the trees an app declares, which may not take one of these places; and a
 *  `<locale>.lproj` for each usage string (`writeUsageStrings`). */
export const MAC_RESOURCES_WRITTEN = [...APPLE_STATIC_TREES, 'receipt.json', 'Assets.car', 'AppIcon.icns', 'Metadata.appintents', 'PrivacyInfo.xcprivacy'];

/** Which file `path` is, read without following a link at its end: its device and
 *  inode. Two directories are never one identity, whatever their names; a name the
 *  volume resolves in another case or Unicode normalization is the same one. */
export const fileIdentity = (path) => {
  const info = lstatSync(path, { throwIfNoEntry: false, bigint: true });
  return info ? `${info.dev}:${info.ino}` : null;
};

/** The identities of the app directories `host.macos.resources` and
 *  `host.ios.resources` copy into a bundle (`from`, relative to app.json), which no
 *  TypeScript capture reads (host/web-js/build.mjs; js/bake/src/lib.rs `sources`
 *  reads the same two keys, by the same rule). */
export const resourceIdentities = (appDir, manifest) => new Set(['macos', 'ios']
  .flatMap(platform => (manifest.host?.[platform]?.resources ?? []).map(({ from }) => fileIdentity(resolve(appDir, from))))
  .filter(Boolean));

/** Native files are binary inputs, never plan assets or TypeScript sources:
 *  `host.macos.resources` under the Mac's `Contents`, `host.ios.resources`
 *  (iOS and tvOS) under `IOS_RESOURCE_ROOT`, as resources only. */
export function nativeResourceMappings(manifest, platform = 'macos') {
  const key = `host.${platform}.resources`, mac = platform === 'macos';
  const mappings = manifest.host?.[platform]?.resources ?? [];
  const safe = path => typeof path === 'string' && path.length && !isAbsolute(path)
    && !path.includes('\\') && !path.includes('\0') && path.split('/').every(p => p && p !== '.' && p !== '..');
  const reserved = new Set(['assets', 'deck', 'gpu', 'fonts', 'strings', 'web', 'apple', 'linux', 'src', 'data', 'modules', 'node_modules', 'target', 'dist']);
  const overlap = (a, b) => a === b || a.startsWith(b + '/') || b.startsWith(a + '/');
  for (const [i, entry] of mappings.entries()) {
    // A root Exact owns says which, and what to name instead (the chat benchmark's `data` was refused unexplained).
    const root = typeof entry.from === 'string' ? entry.from.split('/')[0] : '';
    if (safe(entry.from) && reserved.has(root))
      throw new Error(`${key}.from "${entry.from}" is under \`${root}/\`, a directory Exact owns (each of ${[...reserved].map(name => `\`${name}/\``).join(', ')} holds sources, assets or build output); put the files in a directory of their own, such as \`${root === 'data' ? 'data-files' : `${root}-files`}/\``);
    if (!safe(entry.from) || entry.from.startsWith('.'))
      throw new Error(`${key}.from must name a separate app-relative resource directory, outside source, asset and output roots`);
    if (!safe(entry.to) || !(mac ? /^(Resources|Helpers|Frameworks)\/.+/ : /^Resources\/.+/).test(entry.to)
      || MAC_RESOURCES_WRITTEN.some(name => overlap(entry.to, `Resources/${name}`))
      || entry.to.split('/').some(p => p.endsWith('.lproj')))
      throw new Error(`${key}.to must name a private path under ${mac ? 'Resources/, Helpers/ or Frameworks/' : 'Resources/'}`);
    if (mappings.slice(0, i).some(other => overlap(other.from, entry.from) || overlap(other.to, entry.to)))
      throw new Error(`${key} entries must not overlap`);
    if (Object.keys(manifest.typescript?.sources ?? {}).some(name => overlap(name, entry.from)))
      throw new Error(`${key} cannot overlap typescript.sources`);
  }
  return mappings;
}

/** Refuse an iOS or tvOS app's `fs.read bundle:/Resources/…` line that reaches no
 *  tree either platform declares, as a forgotten `host.ios.resources` entry: there
 *  `bundle:/` holds the declared trees and nothing else (LLP 1133.007 D6). The grants
 *  are every host's, so any other line is left, as a `home:` line is: one naming the
 *  Mac's `Contents` (`bundle:/Info.plist`, `bundle:/Helpers/…`, what the Mac build
 *  writes under `Resources/`, `MAC_RESOURCES_WRITTEN`) or a tree only the Mac declares reads nothing on iOS,
 *  and the read fails there at run time. */
export function iosBundleGrants(manifest, grants) {
  const declared = (platform) => (manifest.host?.[platform]?.resources ?? []).map(({ to }) => to.split('/'));
  const ios = nativeResourceMappings(manifest, 'ios').map(({ to }) => to), trees = [...declared('ios'), ...declared('macos')];
  const refused = [];
  for (const line of (grants ?? '').split('\n')) {
    const [, verb, spelt] = /^\s*(\S+)\s+(.+?)\s*$/.exec(line) ?? [];
    let target = spelt ?? '';
    try { if (target.startsWith('"')) target = JSON.parse(target); } catch { continue; }
    if (verb !== 'fs.read' || !target.startsWith('bundle:/')) continue;
    // By whole names, empty ones dropped, as the grant parser reads a target.
    const names = target.slice('bundle:/'.length).split('/').filter(Boolean);
    // `resources` in another case opens `Resources` on a Mac's volume and nothing on iOS: it is held to the trees too.
    if (names[0]?.toLowerCase() !== 'resources' || names.length < 2 || (names[0] === 'Resources' && (MAC_RESOURCES_WRITTEN.includes(names[1]) || names[1].endsWith('.lproj')))) continue;
    const within = (to) => names.every((name, i) => i >= to.length || name === to[i]);
    if (!trees.some(within)) refused.push(line.trim());
  }
  if (refused.length) throw new Error(`${refused.map(line => `\`${line}\``).join(', ')}: on iOS and tvOS, bundle:/ holds only the trees host.ios.resources declares (${ios.map(to => `bundle:/${to}`).join(', ') || 'none declared'}), and no host declares one this reaches: declare the tree in app.json's host.ios.resources (and host.macos.resources for the Mac), or grant a path under one`);
}

/** Give a bundle beside the app's (the App Clip's, the sample host's) the app's
 *  declared trees: its `IOS_RESOURCE_ROOT` folder, when there are trees, and not a
 *  file of that name, such as an executable an app named so leaves there. */
export function shareIosResources(manifest, from, to) {
  const root = resolve(from, IOS_RESOURCE_ROOT);
  if (manifest.host?.ios?.resources?.length && lstatSync(root, { throwIfNoEntry: false })?.isDirectory())
    cpSync(root, resolve(to, IOS_RESOURCE_ROOT), { recursive: true });
}

/** The fallback PNGs actool writes beside Assets.car for the `CFBundleIconFiles` of the icon keys it returned
 *  (`AppIcon60x60@2x.png`, `AppIcon76x76@2x~ipad.png`, an alternate's), by name in `dir`: a bundle that takes
 *  those keys (the App Clip's) takes these files too, since iOS looks for them in the bundle that names them. */
export function iconFiles(keys, dir) {
  const names = [keys.CFBundleIcons, keys['CFBundleIcons~ipad']].flatMap(icons => icons ? [icons.CFBundlePrimaryIcon, ...Object.values(icons.CFBundleAlternateIcons ?? {})] : [])
    .flatMap(icon => icon?.CFBundleIconFiles ?? []);
  return readdirSync(dir).filter(file => names.some(name => file.startsWith(name) && /^(@[0-9]x)?(~ipad)?\.png$/.test(file.slice(name.length))));
}

/** Validate links before copying. Relative links stay inside their declared
 * tree, so a copied helper cannot accidentally depend on the producer machine.
 * An iOS tree holds neither links, which `bundle:` follows none of, nor
 * Mach-O code, which an app cannot run from its resources. */
export function nativeResourceInventory(app, contents = null, platform = 'macos') {
  const mappings = nativeResourceMappings(app.manifest, platform), mac = platform === 'macos';
  if (!mappings.length) return [];
  const result = [], block = Buffer.alloc(1024 * 1024);
  const inside = (root, path) => path === root || path.startsWith(root + '/');
  const appRoot = realpathSync(contents ?? app.dir);
  for (const { from, to } of mappings) {
    const source = resolve(contents ?? app.dir, contents ? to : from), root = realpathSync(source);
    if (!inside(appRoot, root) || lstatSync(source).isSymbolicLink() || !lstatSync(source).isDirectory())
      throw new Error(`native resource root must be a directory inside the app: ${from}`);
    const walk = path => {
      const stat = lstatSync(path), name = relative(source, path), mode = stat.mode & 0o777;
      const item = { from, to, name, mode };
      if (!mac && stat.isSymbolicLink())
        throw new Error(`host.${platform}.resources: ${from}/${name} is a link, and bundle: follows none (LLP 1133.007 §9): put the file it names there`);
      if (!mac && stat.isFile() && machO(path))
        throw new Error(`host.${platform}.resources: ${from}/${name} is Mach-O code, which an app cannot run from its resources: ship code as a native module`);
      if (stat.isSymbolicLink()) {
        item.link = readlinkSync(path);
        if (isAbsolute(item.link) || !inside(source, resolve(dirname(path), item.link)) || !inside(root, realpathSync(path)))
          throw new Error(`native resource link escapes its tree: ${path}`);
      } else if (stat.isDirectory()) item.directory = true;
      else if (stat.isFile()) {
        const hash = createHash('sha256'), fd = openSync(path, 'r');
        try { let n; while ((n = readSync(fd, block, 0, block.length, null))) hash.update(block.subarray(0, n)); }
        finally { closeSync(fd); }
        item.sha256 = hash.digest('hex');
        // Everything under Contents/Helpers is signed as code; a file that is not, outside a nested bundle (a helper app's
        // own Info.plist), fails only at the final codesign, "code object is not signed at all" (LLP 1156.004 D2).
        const placed = `${to}/${name}`.split('/');
        if (mac && placed[0] === 'Helpers' && !placed.slice(0, -1).some(part => NESTED_BUNDLE.test(part)) && !machO(path))
          throw new Error(`native resource ${from}/${name} is not code, and everything placed under Contents/Helpers is signed as code: put a file a helper reads under Resources/ (host.macos.resources "to": "Resources/…")`);
      } else throw new Error(`native resource is not a file, directory or link: ${path}`);
      result.push(item);
      if (item.directory) for (const child of readdirSync(path).sort()) walk(resolve(path, child));
    };
    walk(source);
  }
  return result;
}

/** Copy the declared trees under `contents` (the Mac's `Contents`, an iOS
 *  bundle's `IOS_RESOURCE_ROOT`), as `expected` recorded them. */
export function copyNativeResources(app, contents, expected = null, platform = 'macos') {
  const inventory = nativeResourceInventory(app, null, platform);
  if (expected && JSON.stringify(inventory) !== JSON.stringify(expected))
    throw new Error('native resources changed after the binary receipt; rebuild');
  for (const { from, to } of nativeResourceMappings(app.manifest, platform)) {
    const destination = resolve(contents, to);
    // Never merge with generated bundle contents or traverse an existing link.
    for (let at = destination; at !== resolve(contents); at = dirname(at)) {
      const info = lstatSync(at, { throwIfNoEntry: false });
      if (info && (at === destination || info.isSymbolicLink()))
        throw new Error(`native resource destination already exists: ${at}`);
    }
    mkdirSync(dirname(destination), { recursive: true });
    cpSync(resolve(app.dir, from), destination, { recursive: true, verbatimSymlinks: true });
  }
  // cp preserves regular-file modes; explicitly retain directory modes too.
  for (const item of inventory.toReversed()) if (!Object.hasOwn(item, 'link'))
    chmodSync(resolve(contents, item.to, item.name), item.mode);
  if (JSON.stringify(nativeResourceInventory(app, contents, platform)) !== JSON.stringify(inventory))
    throw new Error('native resources changed during bundle copy; rebuild');
  return inventory;
}

const xml = (text) => text.replace(/&/g, '&amp;').replace(/</g, '&lt;');
/** A plist from a plain object: strings, booleans, numbers, arrays, and objects, the four spellings Apple's DTD has.
 *  A value a property list cannot hold (null, a number that is not finite, an integer JavaScript does not hold
 *  exactly) is refused by its key path, `path`. */
const plist = (value, indent = '  ', path = '') => {
  const refuse = (what) => { throw new Error(`plist: ${path || 'the root'} is ${what}, which a property list cannot hold`); };
  if (typeof value === 'string') return `<string>${xml(value)}</string>`;
  if (typeof value === 'boolean') return value ? '<true/>' : '<false/>';
  if (typeof value === 'number') {
    if (!Number.isFinite(value) || (Number.isInteger(value) && !Number.isSafeInteger(value))) refuse(String(value));
    return Number.isInteger(value) ? `<integer>${value}</integer>` : `<real>${value}</real>`;
  }
  if (Array.isArray(value)) return `<array>${value.map((v, i) => plist(v, indent, `${path}[${i}]`)).join('')}</array>`;
  if (value === null || typeof value !== 'object') refuse(value === null ? 'null' : typeof value);
  return `<dict>\n${Object.entries(value).map(([k, v]) => `${indent}<key>${xml(k)}</key>${plist(v, indent + '  ', path ? `${path}.${k}` : k)}`).join('\n')}\n${indent.slice(2)}</dict>`;
};
export const plistFile = (dict) => `<?xml version="1.0" encoding="UTF-8"?>\n<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">\n<plist version="1.0">${plist(dict)}</plist>\n`;

/** Mac frameworks use the versioned layout; callers keep root aliases for loading and Swift modules. */
export const frameworkInstallName = (name, platform) => `@rpath/${name}.framework/${platform === 'MacOSX' ? 'Versions/A/' : ''}${name}`;
export function frameworkLayout(dir, name, platform) {
  const version = platform === 'MacOSX' ? resolve(dir, 'Versions/A') : dir;
  const resources = platform === 'MacOSX' ? resolve(version, 'Resources') : version;
  mkdirSync(resources, { recursive: true });
  mkdirSync(resolve(version, 'Modules'), { recursive: true });
  if (platform === 'MacOSX') {
    for (const [link, target] of [['Versions/Current', 'A'], [name, `Versions/Current/${name}`], ['Resources', 'Versions/Current/Resources'], ['Modules', 'Versions/Current/Modules']]) {
      if (!lstatSync(resolve(dir, link), { throwIfNoEntry: false })) symlinkSync(target, resolve(dir, link));
    }
  }
  return { binary: resolve(version, name), modules: resolve(version, 'Modules'), info: resolve(resources, 'Info.plist') };
}

/** The shared runtime has one architecture and SDK per build. SwiftPM reads this standard XCFramework layout. */
export function singleFramework(framework, output, triple) {
  const target = /^(arm64(?:_32)?|x86_64)-apple-(ios|macosx|watchos)[\d.]+(-simulator)?$/.exec(triple);
  if (!target) throw new Error(`host/apple: cannot package a framework for ${triple}`);
  const [, arch, os, variant] = target, platform = os === 'macosx' ? 'macos' : os;
  const identifier = `${platform}-${arch}${variant ?? ''}`, name = basename(framework, '.framework');
  // A directory without its Info.plist is an interrupted run's: start again.
  if (existsSync(output) && !existsSync(resolve(output, 'Info.plist'))) rmSync(output, { recursive: true, force: true });
  if (existsSync(output)) throw new Error(`host/apple: framework output already exists: ${output}`);
  mkdirSync(output, { recursive: true });
  try {
    cpSync(framework, resolve(output, identifier, basename(framework)), { recursive: true, verbatimSymlinks: true });
    writeFileSync(resolve(output, 'Info.plist'), plistFile({ CFBundlePackageType: 'XFWK', XCFrameworkFormatVersion: '1.0', AvailableLibraries: [{
      BinaryPath: `${name}.framework/${platform === 'macos' ? 'Versions/A/' : ''}${name}`, LibraryIdentifier: identifier,
      LibraryPath: `${name}.framework`, SupportedArchitectures: [arch], SupportedPlatform: platform,
      ...(variant ? { SupportedPlatformVariant: 'simulator' } : {}),
    }] }));
  } catch (error) { rmSync(output, { recursive: true, force: true }); throw error; }
}

/** A loose `Frameworks/lib….dylib` as `Frameworks/<name>.framework/<name>`,
 * which is the only form App Store Connect accepts for an embedded library
 * (ITMS-90171, ITMS-90432). The presenter loads either (`embeddedModule` in ExactKit). */
export function wrapFramework(frameworks, loose, name, app, platform = 'iPhoneOS', privacy = {}) {
  const from = resolve(frameworks, loose);
  if (!existsSync(from)) return;
  const dir = resolve(frameworks, `${name}.framework`);
  const layout = frameworkLayout(dir, name, platform);
  renameSync(from, layout.binary);
  run('install_name_tool', ['-id', frameworkInstallName(name, platform), layout.binary], { stdio: 'ignore' });
  writeFileSync(layout.info, plistFile(frameworkInfo(name, app, platform)));
  writePrivacyManifest(dirname(layout.info), [layout.binary], { mac: platform === 'MacOSX', ...privacy });
}

/** The Info.plist `wrapFramework` writes. `signedGpuProduct` keys its signed copy on it, so
 * a change to what a framework declares makes the next build package it again. */
export function frameworkInfo(name, app, platform) {
  return {
    CFBundleExecutable: name, CFBundleIdentifier: `${app.id}.${name.toLowerCase().replaceAll('_', '-')}`, CFBundleName: name,
    CFBundlePackageType: 'FMWK', CFBundleVersion: process.env.EXACT_BUILD_NUMBER ?? '1', CFBundleShortVersionString: process.env.EXACT_VERSION ?? '0.1.0',
    ...(platform === 'iPhoneOS' ? { UIRequiredDeviceCapabilities: ['arm64'] } : {}),
    CFBundleSupportedPlatforms: [platform], ...(platform === 'MacOSX' ? { LSMinimumSystemVersion: app.manifest.host?.macos?.minimumOS ?? '14.0' } : { MinimumOSVersion: app.manifest.host?.ios?.minimumOS ?? '17.0' }),
  };
}

/** The bake's GPU digest (update/src/gpu.rs, `code_digest`): the bytes, less a signed 64-bit
 *  Mach-O's code signature and the three sizes that follow it, so a re-signer (AppDrop) keeps it. */
export function codeDigest(bytes) {
  const word = (at) => at + 4 <= bytes.length ? bytes.readUInt32LE(at) : null;
  const code = () => {
    if (word(0) !== 0xfeedfacf || word(16) === null) return null;
    let at = 32, signature = null;
    const zero = [];
    for (let i = 0; i < word(16); i++) {
      const cmd = word(at), size = word(at + 4);
      if (cmd === null || size === null) return null;
      if (cmd === 0x1d) { signature = word(at + 8); zero.push([at + 12, 4]); }
      else if (cmd === 0x19 && bytes.subarray(at + 8, at + 24).equals(Buffer.from('__LINKEDIT\0\0\0\0\0\0', 'latin1'))) zero.push([at + 32, 8], [at + 48, 8]);
      at += Math.max(size, 8);
    }
    if (signature === null || signature > bytes.length) return null;
    const out = Buffer.from(bytes.subarray(0, signature));
    for (const [z, len] of zero) { if (z + len > out.length) return null; out.fill(0, z, z + len); }
    return out;
  };
  return createHash('sha256').update(code() ?? bytes).digest('hex');
}

export function signedGpuProduct(product, { identity = '-', framework = null, app, platform = 'iPhoneOS', hardened = false, privacyManifest = null } = {}) {
  // Cargo puts its own unsigned file back on every build, and a signature
  // carries its signing time: signing in place made the app's bake (which
  // names this product's digest) run on every build. Sign a copy beside it,
  // again only when its bytes, the signing options or the framework's Info.plist change.
  const directory = resolve(dirname(product), 'signed', ...(framework ? ['frameworks', basename(product)] : []));
  const container = resolve(directory, framework ? `${framework}.framework` : basename(product));
  const signed = framework ? resolve(container, framework) : container, record = `${container}.source`;
  const source = JSON.stringify([createHash('sha256').update(readFileSync(product)).digest('hex'), identity ?? '-',
    hardened, framework, framework ? frameworkInfo(framework, app, platform) : null,
    framework ? privacyManifestForBinaries([product], (privacyManifest ? [privacyAPIManifest(privacyManifest)] : []), { mac: platform === 'MacOSX' }) : null]);
  // An ad-hoc signature (`-`: every simulator build) names no certificate
  // to require; asked for one, the verification failed, and the copy was
  // signed again by every build, which reran the app's bake and compile.
  const current = existsSync(signed) && existsSync(record) && readFileSync(record, 'utf8') === source
    && spawnSync('codesign', ['--verify', '--strict', ...(identity && identity !== '-' ? ['-R', `=certificate leaf = H"${identity}"`] : []), container]).status === 0;
  if (!current) {
    mkdirSync(directory, { recursive: true });
    if (framework) rmSync(container, { recursive: true, force: true });
    copyFileSync(product, resolve(directory, basename(product)));
    if (framework) wrapFramework(directory, basename(product), framework, app, platform, { apiManifests: privacyManifest ? [privacyManifest] : [] });
    run('codesign', ['--force', '--sign', identity ?? '-', ...(hardened ? ['--options', 'runtime'] : []),
      hardened && identity && identity !== '-' ? '--timestamp' : '--timestamp=none', container], {stdio:'ignore'});
    writeFileSync(record, source);
  }
  return signed;
}

export function assertArchiveFrameworks(directory) {
  const loose = readdirSync(directory).filter(name => name.endsWith('.dylib'));
  if (loose.length) throw new Error(`archive has loose libraries in Frameworks: ${loose.join(', ')}`);
}
