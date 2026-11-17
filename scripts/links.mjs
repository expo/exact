// Verified https links (LLP 1038 D8, amended 2026-11-15): whether an app's web addresses open the installed app is one
// declaration in Web App Manifest vocabulary, `handle_links`, `scope` and `scope_extensions`, from which each native
// bake derives its platform's claim and each origin's association files are written or printed. Nothing here reads a
// file: the manifest is the input, so the web, Apple and Android builds derive the same claims.

/** Exact's own paths (the web's sign-in callback page, `host/web/build.mjs`): never claimed, so a sign-in that returns
 * there in a browser stays in the browser. */
import { withBase } from '../host/web-js/route-pattern.js';

export const EXACT_PATHS = '/.exact/';

/** `text` as an origin a claim may name, or why it cannot be one. The URL Standard's parser serializes it: the host
 * lower-cased and written as A-labels, `:443` dropped. Another port is refused: Apple's entitlement is
 * `<service>:<fully qualified domain>` (Apple, "Associated Domains Entitlement") and Android fetches `assetlinks.json`
 * from the host itself. */
export function claimedOrigin(text) {
  let url;
  try { url = new URL(text); } catch { return { problem: 'is not a URL' }; }
  if (url.protocol !== 'https:') return { problem: 'is not https' };
  if (url.hostname.includes('*')) return { problem: 'is a wildcard origin, which scope_extensions does not have; claim every subdomain on Apple with applinks:*.<domain> in host.ios.associatedDomains' };
  if (url.port) return { problem: `names port ${url.port}: Apple's applinks: has no port and Android verifies the host itself` };
  if (url.username || url.password || url.pathname !== '/' || /[?#]/.test(text)) return { problem: 'carries a path, query, fragment or user: name the origin alone' };
  return { origin: url.origin, host: url.hostname };
}

/** The manifest's default `scope`: `start_url` resolved against `.`, its file name, query and fragment removed. */
export const defaultScope = (startUrl = '/') => new URL('.', new URL(startUrl, 'https://scope.invalid')).pathname;

/** The `scope` the app's claim on `app.origin` is narrowed to: as declared, else the default. */
export const effectiveScope = (manifest) => manifest.scope ?? defaultScope(manifest.start_url);

/** The manifest's processed `id` (Web App Manifest, "Processing the id member"): `id` resolved against `start_url`'s
 * origin, or `start_url` when there is none, without its fragment. Null without `app.origin`, which `start_url` is
 * resolved against. */
export function processedId(manifest) {
  const origin = manifest.app?.origin && claimedOrigin(manifest.app.origin).origin;
  if (!origin) return null;
  const start = new URL(manifest.start_url ?? '/', origin);
  const id = manifest.id === undefined ? start : new URL(manifest.id, start.origin);
  id.hash = '';
  return id.href;
}

/** The base app's identities, which the association files name whatever variant a build is for (LLP 1030.008 D8). */
export function baseIdentity(manifest) {
  const id = manifest.app.id, base = manifest.variant?.base ?? id, suffix = id.slice(base.length);
  const unsuffix = (value) => suffix && value?.endsWith(suffix) ? value.slice(0, -suffix.length) : value;
  const android = manifest.host?.android?.id ? unsuffix(manifest.host.android.id) : base;
  // A variant's clip id is its id followed by the base clip id's remainder (`resolveVariant`).
  const clipId = manifest.host?.ios?.appClip?.id, clip = !manifest.host?.ios?.appClip ? null : clipId ? base + clipId.slice(id.length) : `${base}.Clip`;
  return { appId: base, androidPackage: android.replaceAll('-', '_'), clipId: clip };
}

/** The raw Apple list, `host.ios.associatedDomains`, for what has no web name. */
export const rawAppleDomains = (manifest) => Array.isArray(manifest.host?.ios?.associatedDomains) ? manifest.host.ios.associatedDomains : [];

/** Whether the raw entry `domain` (`applinks:<host>` or `applinks:*.<domain>`, a `?mode=` aside) covers `host`: a
 * wildcard claims the domain it names and its subdomains (Apple, "Supporting associated domains"; TN3155). */
export function appleCovers(domain, host) {
  const name = domain.split('?')[0];
  return name === `applinks:${host}` || (name.startsWith('applinks:*.') && `.${host}`.endsWith(name.slice('applinks:*'.length)));
}

/** The https callback paths on `origin` (`auth.callback` grants), outside Exact's own paths, which a claim excludes. */
const callbackPaths = (origin, callbacks) => [...new Set(callbacks.filter((c) => c.startsWith(`${origin}/`))
  .map((c) => new URL(c).pathname).filter((p) => !p.startsWith(EXACT_PATHS)))];

/** What `handle_links: "preferred"` claims, each `{ origin, host, scope, exclude }`: `app.origin` narrowed to `scope`,
 * then each extension origin whole. `exclude` lists the paths the claim leaves to the browser: `/.exact/` on
 * `app.origin`, and each https auth callback on the origin (`callbacks`, the build's `auth.callback` grants). Empty for
 * any other value, or a manifest that does not conform (`linkProblems` refuses it). */
export function linkClaims(manifest, callbacks = []) {
  if (manifest.handle_links !== 'preferred' || !manifest.app?.origin) return [];
  const own = claimedOrigin(manifest.app.origin);
  if (!own.origin) return [];
  const extensions = (manifest.scope_extensions ?? []).map((e) => claimedOrigin(e.origin)).filter((e) => e.origin);
  const base = manifest.host?.web?.base ?? '/';
  return [{ ...own, scope: withBase(effectiveScope(manifest), base), exclude: [withBase(EXACT_PATHS, base), ...callbackPaths(own.origin, callbacks)] },
    ...extensions.map((e) => ({ ...e, scope: '/', exclude: callbackPaths(e.origin, callbacks) }))];
}

/** The `applinks:` entries the claims derive, for iOS, iPadOS and macOS. */
export const appleLinkDomains = (manifest) => linkClaims(manifest).map((c) => `applinks:${c.host}`);

/** An `apple-app-site-association` `applinks` entry's `components` for `claim`: each excluded path first, then the
 * scope (Apple, "Supporting associated domains": the first matching component decides). */
export const appleComponents = (claim) => [...claim.exclude.map((p) => ({ '/': p.endsWith(EXACT_PATHS) ? `${p}*` : p, exclude: true })), { '/': `${claim.scope}*` }];

/** Android's App Link filters for the claims, each `{ host, pathPrefix, exclude, allow, claimed }`: `pathPrefix` the
 * filter's own path (none for `/`), `exclude` the paths its `uri-relative-filter-group`s leave to the browser before
 * the group that allows `allow`, and `claimed` the excluded paths it cannot leave. A filter's own `<data>` paths are
 * matched before its groups and accept what they match (Android, "<uri-relative-filter-group>"), so an excluded path
 * inside a narrower scope is left out only when the groups carry the scope instead, which releases before 15 ignore:
 * that is done only where the app supports no release before 15 (`host.android.minimumOS`, default 12). */
export function androidLinkFilters(manifest, callbacks = []) {
  const from15 = Number(manifest.host?.android?.minimumOS ?? '12') >= 15;
  return linkClaims(manifest, callbacks).map((c) => {
    const inside = c.exclude.filter((p) => p.startsWith(c.scope)), grouped = inside.length > 0 && (c.scope === '/' || from15);
    return { host: c.host, pathPrefix: c.scope === '/' || grouped ? null : c.scope, exclude: grouped ? inside : [], allow: c.scope, claimed: grouped ? [] : inside };
  });
}

/** `host.ios.associatedDomains`'s removed boolean, refused by name. */
export const REMOVED_DOMAINS = {
  true: 'host.ios.associatedDomains: true was removed: declare "handle_links": "preferred", which claims app.origin on iOS, macOS and Android',
  false: 'host.ios.associatedDomains: false was removed: delete it; nothing is claimed unless handle_links is "preferred"',
};

/** What `handle_links`, `scope` and `scope_extensions` get wrong beyond the schema (LLP 1038 D8), every problem. */
export function linkProblems(manifest) {
  const problems = [], { handle_links: handle, scope, scope_extensions: extensions = [] } = manifest;
  const appOrigin = manifest.app?.origin;
  if (scope !== undefined) {
    if (!/^\/(?!\/)/.test(scope) || /[?#]/.test(scope)) problems.push(`scope: ${JSON.stringify(scope)} is not an absolute path on app.origin (one leading /, no query or fragment)`);
    else if (/[*?]/.test(scope)) problems.push(`scope: ${JSON.stringify(scope)} contains *, which is literal on the web and in Android's pathPrefix and a wildcard in Apple's components`);
    else if (/(^|\/)\.\.?(\/|$)/.test(scope)) problems.push(`scope: ${JSON.stringify(scope)} has a dot segment`);
    else {
      const start = new URL(manifest.start_url ?? '/', 'https://scope.invalid');
      if (start.origin !== 'https://scope.invalid' || !start.pathname.startsWith(scope)) problems.push(`start_url: ${manifest.start_url ?? '/'} is outside scope ${scope}, so the web manifest would ignore the scope`);
    }
  }
  const own = appOrigin ? claimedOrigin(appOrigin) : null;
  if (handle === 'preferred') {
    if (!appOrigin) problems.push('handle_links: "preferred" claims app.origin, which the manifest does not declare');
    else if (own.problem) problems.push(`app.origin: ${appOrigin} ${own.problem}`);
  }
  const seen = new Set(own?.origin ? [own.origin] : []);
  extensions.forEach((entry, i) => {
    if (entry.type !== 'origin') return problems.push(`scope_extensions[${i}].type: ${JSON.stringify(entry.type)} is not "origin", the only type there is`);
    const { origin, problem } = claimedOrigin(entry.origin);
    if (problem) return problems.push(`scope_extensions[${i}].origin: ${entry.origin} ${problem}`);
    if (seen.has(origin)) return problems.push(`scope_extensions[${i}].origin: ${entry.origin} repeats ${own?.origin === origin ? 'app.origin' : 'another entry'}`);
    seen.add(origin);
  });
  const raw = rawAppleDomains(manifest).filter((d) => d.startsWith('applinks:'));
  if (handle === 'not-preferred' && raw.length) problems.push(`handle_links: "not-preferred" beside host.ios.associatedDomains ${raw.join(', ')}: the two disagree on whether links open the app`);
  if (!problems.length) {
    for (const { host } of linkClaims(manifest)) for (const d of raw.filter((d) => appleCovers(d, host))) {
      problems.push(`host.ios.associatedDomains: ${d} covers ${host}, which handle_links claims: one source decides each host's paths, so delete it`);
    }
  }
  return problems;
}

/** Where a build cannot claim what the manifest asks: one line per host that claims nothing natively, or null. */
export function unclaimedNote(manifest, host) {
  if (manifest.handle_links !== 'preferred') return null;
  const why = {
    tvos: 'tvOS claims no links until an app needs them (LLP 1038 D8)', watchos: 'watchOS claims no links until an app needs them (LLP 1038 D8)',
    windows: 'Windows claims no links: its app URI handlers need a packaged app (LLP 1038 D8)',
    linux: 'Linux has no verified link registration (LLP 1038 D8)' }[host];
  return `handle_links "preferred": ${why}, so links open in the browser`;
}

/** What an APK's App Links cannot do on the releases it supports (`host.android.minimumOS`, default 12): below 12, one
 * host that fails verification leaves every autoVerify filter of the app unverified (Android, "Verify App Links");
 * below 15, the excluded paths open the app; an excluded path a narrower scope keeps (`claimed`) opens it on every
 * release. Each is said when the app signs in (`signsIn`), the case the exclusions are for. */
export function androidLinkNotes(manifest, filters, { signsIn = false } = {}) {
  const minimum = Number(manifest.host?.android?.minimumOS ?? '12'), notes = [];
  if (filters.length > 1 && minimum < 12) notes.push(`App Links: below Android 12 one unverified host of ${filters.map((f) => f.host).join(', ')} leaves all of them unverified`);
  const paths = (key) => filters.flatMap((f) => f[key].map((p) => `${f.host}${p}`)), excluded = paths('exclude'), claimed = paths('claimed');
  const opens = (list) => `${list.join(', ')} open${list.length === 1 ? 's' : ''} the app rather than the browser`;
  if (signsIn && excluded.length && minimum < 15) notes.push(`App Links: below Android 15 ${opens(excluded)}, since a filter cannot exclude a path there`);
  if (signsIn && claimed.length) notes.push(`App Links: ${opens(claimed)} on every Android release, since the filter's own path, the scope, matches before an exclusion; with host.android.minimumOS "15" the filter leaves ${claimed.length === 1 ? 'it' : 'them'} to the browser`);
  return notes;
}
