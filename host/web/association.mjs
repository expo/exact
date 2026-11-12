// The origin's `apple-app-site-association` (LLP 1030 D1, "The deep-link association files"; Apple, "Supporting
// associated domains"): what the manifest's iOS host claims on this domain, as a file Apple's CDN fetches.
import { mkdirSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { appClip, associatedDomains } from '../../scripts/app.mjs';

/** The file for `app`, or null when it claims nothing here. When the app's entitlements claim this origin's host as
 * `applinks:` (`associatedDomains`), it gets `applinks`, and its App Clip `appclips` (Apple, "Associating your App
 * Clip with your website"); `webcredentials` is the origin's passkeys or auth callback (LLP 1069.006 D2, LLP 1133.011
 * D6). Each names `<host.ios.team>.<bundle id>`, so with no team there is no file. */
export function appleAssociation(app, { webcredentials = false } = {}) {
  const team = app.manifest.host?.ios?.team, clip = appClip(app);
  const host = app.origin ? new URL(app.origin).host : null;
  const claimed = !!host && associatedDomains(app).some((d) => d.split('?')[0] === `applinks:${host}`);
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
