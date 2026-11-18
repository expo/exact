// App variants (LLP 1030.008): the app under another identity, installed beside it. `variants.<name>` in app.json
// gives an `idSuffix`, a `name`, optionally its icons (`icons`, and an Icon Composer project per Apple platform) and
// setting values (`env`); `--variant <name>` or EXACT_VARIANT selects one for the app a verb resolves, and
// `readManifest` returns that app's manifest resolved as the variant, so everything derived from `app.id` and
// `app.name` follows. contract/cli/src/manifest.rs resolves the same selection for the Rust readers.
import { realpathSync } from 'node:fs';
import { appIconProblems, projectName } from './app-icons.mjs';
import { executableName } from './app.mjs';
import { settingNames } from '../js/bake/src/__exact_env.mjs';

export { takeVariant } from './help.mjs';

/** Makes a requested variant (EXACT_VARIANT) this app's selection, unless another app already claimed it, and returns
 * the manifest `read` reads then: the verb that resolves the app (`resolveApp`) calls it, and every process below
 * inherits EXACT_VARIANT_APP. A selection names one app, so a variant exported in a shell neither renames nor refuses
 * any other app a process reads. The first app resolved as a variant says so, once a process. */
export function claimVariant(dir, read, env = process.env) {
  if (env.EXACT_VARIANT && !env.EXACT_VARIANT_APP) env.EXACT_VARIANT_APP = realpathSync(dir);
  const manifest = read();
  if (manifest.variant && !said) { said = true; console.error(`variant ${manifest.variant.name}: ${manifest.app.id}`); }
  return manifest;
}
let said = false;

/** `app`, which `exact <verb>` (run, install, uninstall, release, appstore) takes only without a variant: it names the
 * command and the ~/Applications entry after the app, and a second identity needs its own (LLP 1030.008 §6). */
export function withoutVariant(verb, app, args) {
  const variant = app.manifest.variant?.name ?? (args.includes('--variant') ? args[args.indexOf('--variant') + 1] ?? '<name>' : null);
  if (variant) throw new Error(`${verb} takes no variant (LLP 1030.008 §6): bun exact.mjs mac --variant ${variant} --run builds and opens one`);
  return app;
}

/** The variant selected for the app in `dir`, or undefined. */
export function selectedVariant(dir, env = process.env) {
  if (!env.EXACT_VARIANT || !env.EXACT_VARIANT_APP) return undefined;
  const real = (path) => { try { return realpathSync(path); } catch { return path; } };
  return real(env.EXACT_VARIANT_APP) === real(dir) ? env.EXACT_VARIANT : undefined;
}

/** What the manifest's `variants` get wrong beyond the schema, whether or not one is selected: a suffix or an installed
 * name another variant or the app has, a setting the manifest's `env` does not declare, an icon that is not there, an
 * Icon Composer project named as an alternate icon is, an App Clip id no suffix can make the variant's. */
export function variantProblems(manifest, dir) {
  const declared = settingNames(manifest.env), base = manifest.app?.id, clip = manifest.host?.ios?.appClip?.id;
  if (manifest.game !== undefined && manifest.variants) return ['variants: a game takes no variant (LLP 1030.008 §6)'];
  // Two installs under one id, or one file name (~/Applications, a crash report), cannot sit side by side; the file
  // system may ignore case.
  const file = (name) => executableName({ displayName: name ?? '', id: base ?? '' }).toLowerCase();
  const suffixes = new Map(), files = new Map([[file(manifest.app?.name), 'app.name']]);
  // Apple names the primary icon after its project, and a catalog holds one asset per name (LLP 1163.000 D1).
  const alternates = new Map((manifest.alternate_icons ?? []).map((entry, i) => [entry.name.toLowerCase(), `alternate_icons[${i}] (${JSON.stringify(entry.name)})`]));
  const namesake = (icon) => icon && alternates.get(projectName(icon).toLowerCase());
  return Object.entries(manifest.variants ?? {}).flatMap(([name, variant]) => [
    ...suffixes.has(variant.idSuffix) ? [`variants.${name}.idSuffix: variants.${suffixes.get(variant.idSuffix)} has ${variant.idSuffix} too`] : (suffixes.set(variant.idSuffix, name), []),
    ...files.has(file(variant.name)) ? [`variants.${name}.name: ${JSON.stringify(variant.name)} installs under the same name as ${files.get(file(variant.name))}`] : (files.set(file(variant.name), `variants.${name}.name`), []),
    ...Object.keys(variant.env ?? {}).filter((key) => !declared.includes(key))
      .map((key) => `variants.${name}.env.${key}: not a setting the manifest's "env" declares`),
    ...variant.icons ? appIconProblems({ icons: variant.icons }, dir).map((p) => `variants.${name}.${p}`) : [],
    ...namesake(variant.host?.ios?.icon) ? [`variants.${name}.host.ios.icon: ${variant.host.ios.icon} gives the primary icon the name of ${namesake(variant.host.ios.icon)} on Apple; rename one`] : [],
    ...clip && !clip.startsWith(`${base}.`) ? [`variants.${name}: host.ios.appClip.id ${clip} does not begin with ${base}., so it cannot be this variant's App Clip`] : [],
  ]);
}

/** `manifest` as the variant `name` has it, or as it is when none is named. The resolved manifest carries
 * `variant: { name, base }`, `base` being the unsuffixed `app.id`, and is never resolved again. */
export function resolveVariant(manifest, name) {
  if (!name || manifest.variant) return manifest;
  const variant = manifest.variants?.[name];
  if (!variant) {
    const known = Object.keys(manifest.variants ?? {});
    throw new Error(`variant ${name}: app.json has no variants.${name} (${known.length ? `it declares ${known.join(', ')}` : 'it declares none'})`);
  }
  const base = manifest.app.id, id = base + variant.idSuffix, host = { ...manifest.host };
  if (host.android?.id) host.android = { ...host.android, id: host.android.id + variant.idSuffix };
  if (host.ios?.appClip?.id) host.ios = { ...host.ios, appClip: { ...host.ios.appClip, id: id + host.ios.appClip.id.slice(base.length) } };
  const resolved = { ...manifest, app: { ...manifest.app, id, name: variant.name }, host, variant: { name, base } };
  if (variant.icons) {
    resolved.icons = variant.icons;
    if (resolved.color_scheme_dark?.icons) { const { icons: _, ...dark } = resolved.color_scheme_dark; resolved.color_scheme_dark = dark; }
    for (const platform of ['ios', 'macos']) if (host[platform]?.icon) { const { icon: _, ...rest } = host[platform]; host[platform] = rest; }
  }
  // The variant's own Icon Composer project wins on its platform, as the app's does (LLP 1163 D4).
  for (const platform of ['ios', 'macos']) if (variant.host?.[platform]?.icon) host[platform] = { ...host[platform], icon: variant.host[platform].icon };
  return resolved;
}
