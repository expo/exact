// The origins' association files (LLP 1030 D1; LLP 1038 D8): the app identities allowed to use each domain. The
// origin a build deploys gets its files written into the build; every other origin a claim names gets them printed.
import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { EXACT_PATHS, appleComponents, appleCovers, baseIdentity, claimedOrigin, linkClaims, processedId, rawAppleDomains, webAuthCallback } from '../../scripts/links.mjs';

const HANDLE_ALL_URLS = 'delegate_permission/common.handle_all_urls', LOGIN_CREDS = 'delegate_permission/common.get_login_creds';

/** `app`'s manifest with its id and origin as resolved (a variant's id, `app.origin`). */
const view = (app) => ({ ...app.manifest, app: { ...app.manifest.app, id: app.id ?? app.manifest.app?.id, origin: app.origin ?? undefined } });

/** The origin's host, as the claims normalize it. */
const hostOf = (origin) => { try { return claimedOrigin(origin).host ?? new URL(origin).hostname; } catch { return null; } };
const ownOrigin = (app) => app.origin ? new URL(app.origin).origin : null;
const authOrigins = (callbacks) => [...new Set(callbacks.map((callback) => new URL(callback))
  .filter((url) => url.protocol === 'https:' && !webAuthCallback(url)).map((url) => url.origin))];

/** An `apple-app-site-association` for `components` (null: no `applinks`), its App Clip's `appclips` beside them,
 * and `webcredentials`. Each names `<host.ios.team>.<base bundle id>`, the base app alone (LLP 1030.008 D8). */
function appleFile(manifest, team, { components = null, clip = false, webcredentials = false }) {
  const base = baseIdentity(manifest);
  return {
    ...(components ? { applinks: { details: [{ appIDs: [`${team}.${base.appId}`], components }] } } : {}),
    ...(webcredentials ? { webcredentials: { apps: [`${team}.${base.appId}`] } } : {}),
    ...(components && clip && base.clipId ? { appclips: { apps: [`${team}.${base.clipId}`] } } : {}),
  };
}

/** The deployed origin's file for `app`, or null when it claims nothing there or names no team. `applinks` where
 * `handle_links` claims the origin (its `components` narrowed to `scope`, Exact's paths and the auth callbacks
 * excluded) or a raw `applinks:` entry covers its host (every path but Exact's), with its App Clip's `appclips` (Apple,
 * "Associating your App Clip with your website"); `webcredentials` for the origin's passkeys or auth callback (LLP
 * 1069.006 D2, LLP 1133.011 D6). */
export function appleAssociation(app, { webcredentials = false, callbacks = [] } = {}) {
  const manifest = view(app), team = manifest.host?.ios?.team, host = app.origin ? hostOf(app.origin) : null;
  webcredentials ||= authOrigins(callbacks).includes(ownOrigin(app));
  const claim = host && linkClaims(manifest, callbacks).find((c) => c.host === host);
  const raw = !!host && rawAppleDomains(manifest).some((d) => appleCovers(d, host));
  const components = claim ? appleComponents(claim) : raw ? appleComponents({ scope: '/', exclude: [EXACT_PATHS] }) : null;
  if (!team || !(components || webcredentials)) return null;
  return appleFile(manifest, team, { components, clip: true, webcredentials });
}

/** Writes `appleAssociation` into the build at `root`, when there is one. */
export function writeAppleAssociation(root, app, options) {
  const association = appleAssociation(app, options);
  if (!association) return;
  mkdirSync(resolve(root, '.well-known'), { recursive: true });
  writeFileSync(resolve(root, '.well-known/apple-app-site-association'), JSON.stringify(association) + '\n');
}

/** One `assetlinks.json` statement: `relation` for the base app's package and each `host.android.certificates`
 * fingerprint. Passkeys by default (LLP 1133.011 D6). */
export const androidAssociation = (manifest, relation = LOGIN_CREDS) => ({
  relation: [relation],
  target: { namespace: 'android_app', package_name: baseIdentity(manifest).androidPackage,
    sha256_cert_fingerprints: manifest.host?.android?.certificates ?? [] },
});

/** The deployed origin's `assetlinks.json` statements: `handle_all_urls` for navigation or native auth callbacks,
 * `get_login_creds` for passkeys. The app's filters narrow paths; the file names identities. */
export function androidStatements(app, { passkeys = false, callbacks = [] } = {}) {
  const manifest = view(app), host = app.origin ? hostOf(app.origin) : null;
  const links = !!host && (linkClaims(manifest).some((c) => c.host === host) || authOrigins(callbacks).includes(ownOrigin(app)));
  return [...(links ? [androidAssociation(manifest, HANDLE_ALL_URLS)] : []), ...(passkeys ? [androidAssociation(manifest)] : [])];
}

/** The files every other origin a claim names must serve, `[{ origin, files }]` by origin, each file by its name under
 * `/.well-known/`: an extension origin's `apple-app-site-association` and `assetlinks.json` when `handle_links`
 * claims it, and its `web-app-origin-association` (keyed by the processed manifest id, `{}` granting its whole
 * origin) whatever `handle_links` says; a raw `applinks:` host the claims do not derive, its Apple file (a wildcard's
 * names the domain Apple fetches it from). Native auth callback origins also need webcredentials and Android links;
 * passkey RP IDs need webcredentials and Android login credentials in those same files. */
export function elsewhereFiles(app, { callbacks = [], passkeys = [], webManifest = null } = {}) {
  const manifest = view(app), team = manifest.host?.ios?.team ?? '<host.ios.team>', own = app.origin ? hostOf(app.origin) : null;
  const byOrigin = new Map(), add = (origin, name, file) => { if (!byOrigin.has(origin)) byOrigin.set(origin, {}); byOrigin.get(origin)[name] = file; };
  const claims = linkClaims(manifest, callbacks);
  for (const claim of claims.filter((c) => c.host !== own)) {
    add(claim.origin, 'apple-app-site-association', appleFile(manifest, team, { components: appleComponents(claim), clip: true }));
    add(claim.origin, 'assetlinks.json', [androidAssociation(manifest, HANDLE_ALL_URLS)]);
  }
  const id = processedId(webManifest ? { ...webManifest, app: manifest.app } : manifest);
  for (const { origin } of manifest.scope_extensions ?? []) {
    const normalized = claimedOrigin(origin).origin;
    if (normalized && id) add(normalized, 'web-app-origin-association', { [id]: {} });
  }
  const derived = new Set(claims.map((c) => c.host));
  for (const domain of rawAppleDomains(manifest).filter((d) => d.startsWith('applinks:'))) {
    const name = domain.split('?')[0].slice('applinks:'.length), wildcard = name.startsWith('*.'), host = wildcard ? name.slice(2) : name;
    if (derived.has(host) || host === own) continue;
    add(`https://${host}`, 'apple-app-site-association', appleFile(manifest, team, { components: appleComponents({ scope: '/', exclude: [] }), clip: !wildcard }));
  }
  for (const origin of authOrigins(callbacks).filter((origin) => origin !== ownOrigin(app))) {
    const existing = byOrigin.get(origin)?.['apple-app-site-association'] ?? {};
    add(origin, 'apple-app-site-association', { ...existing, ...appleFile(manifest, team, { webcredentials: true }) });
    add(origin, 'assetlinks.json', [androidAssociation(manifest, HANDLE_ALL_URLS)]);
  }
  for (const rp of new Set(passkeys.filter((rp) => rp !== own && rp !== 'localhost'))) {
    const origin = `https://${rp}`, existing = byOrigin.get(origin) ?? {};
    add(origin, 'apple-app-site-association', { ...existing['apple-app-site-association'], ...appleFile(manifest, team, { webcredentials: true }) });
    add(origin, 'assetlinks.json', [...(existing['assetlinks.json'] ?? []), androidAssociation(manifest)]);
  }
  return [...byOrigin].map(([origin, files]) => ({ origin, files }));
}

/** Writes the deployed origin's link and credential files into `root` and returns the build's lines:
 * what it wrote, what it could not, and each file every other origin must serve. */
export function writeLinkFiles(root, app, { passkeys = [], callbacks = [], webManifest = null } = {}) {
  const own = app.origin ? hostOf(app.origin) : null, passkeyHere = passkeys.includes(own);
  writeAppleAssociation(root, app, { webcredentials: passkeyHere, callbacks });
  const manifest = view(app), statements = androidStatements(app, { passkeys: passkeyHere, callbacks }), certificates = manifest.host?.android?.certificates ?? [];
  if (statements.length && certificates.length) {
    mkdirSync(resolve(root, '.well-known'), { recursive: true });
    writeFileSync(resolve(root, '.well-known/assetlinks.json'), JSON.stringify(statements, null, 2) + '\n');
  }
  const claims = linkClaims(manifest, callbacks), lines = [];
  if (claims.length) {
    const missing = [!manifest.host?.ios?.team && 'host.ios.team (no apple-app-site-association: iOS and macOS verify none)',
      !certificates.length && 'host.android.certificates (no assetlinks.json: Android verifies none)'].filter(Boolean);
    lines.push(`links: ${claims.map((c) => `${c.host}${c.scope === '/' ? '' : c.scope}`).join(', ')} open the app${missing.length ? `; no ${missing.join(', no ')}` : ''}`);
  }
  const credentials = [passkeyHere && 'passkeys', authOrigins(callbacks).includes(ownOrigin(app)) && 'auth callbacks'].filter(Boolean);
  if (credentials.length) lines.push(`${credentials.join(', ')}: ${own}'s association files${manifest.host?.ios?.team ? '' : ' (no host.ios.team: no apple-app-site-association)'}${certificates.length ? '' : ' (no host.android.certificates: no assetlinks.json)'}`);
  for (const { origin, files } of elsewhereFiles(app, { callbacks, passkeys, webManifest })) {
    for (const [name, file] of Object.entries(files)) lines.push(`links: ${origin} must serve /.well-known/${name} ${JSON.stringify(file)}`);
  }
  if (manifest.host?.web?.base && manifest.host.web.base !== "/" && (appleAssociation(app, { webcredentials: passkeyHere, callbacks }) || statements.length && certificates.length))
    lines.push(`links: serve ${root}/.well-known/ at the origin root ${app.origin}/.well-known/, outside the site mount`);
  return lines;
}
