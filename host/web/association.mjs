// The origin's association files (LLP 1030 D1): the app identities allowed to use this domain.
import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { androidPackageId, appClip, associatedDomains } from '../../scripts/app.mjs';

/** The file for `app`, or null when it claims nothing here. When the app's entitlements claim this origin's host as
 * `applinks:` (`associatedDomains`), it gets `applinks`, and its App Clip `appclips` (Apple, "Associating your App
 * Clip with your website"); `webcredentials` is the origin's passkeys or auth callback (LLP 1069.006 D2, LLP 1133.011
 * D6). Each names `<host.ios.team>.<bundle id>`, so with no team there is no file. */
export function appleAssociation(app, { webcredentials = false } = {}) {
  const team = app.manifest.host?.ios?.team, clip = appClip(app);
  const host = app.origin ? new URL(app.origin).host : null;
  // A wildcard entry (`applinks:*.example.com`) claims the domain it names and its subdomains (Apple, "Supporting
  // associated domains"; TN3155), so this origin serves the file for it too.
  const claims = (d) => { const name = d.split('?')[0]; return name === `applinks:${host}` || (name.startsWith('applinks:*.') && `.${host}`.endsWith(name.slice(10))); };
  const claimed = !!host && associatedDomains(app).some(claims);
  if (!team || !(claimed || webcredentials)) return null;
  return {
    ...(claimed ? { applinks: { details: [{ appIDs: [`${team}.${app.id}`], components: [{ '/': '*' }] }] } } : {}),
    ...(webcredentials ? { webcredentials: { apps: [`${team}.${app.id}`] } } : {}),
    ...(claimed && clip ? { appclips: { apps: [`${team}.${clip.id}`] } } : {}),
  };
}

/** Writes `appleAssociation` into the build at `root`, when there is one. */
export function writeAppleAssociation(root, app, options) {
  const association = appleAssociation(app, options);
  if (!association) return;
  mkdirSync(resolve(root, '.well-known'), { recursive: true });
  writeFileSync(resolve(root, '.well-known/apple-app-site-association'), JSON.stringify(association) + '\n');
}

/** The domain's passkey statement for Android (LLP 1133.011 D6). */
export const androidAssociation = manifest => ({
  relation: ['delegate_permission/common.get_login_creds'],
  target: { namespace: 'android_app', package_name: androidPackageId(manifest),
    sha256_cert_fingerprints: manifest.host?.android?.certificates ?? [] },
});
