// Mounted builds alone import this boundary. Contract locations and route patterns stay app-relative.
import { canonical as canonicalPath, mountedHref, withBase, withoutBase } from './route-pattern.js';

const base = __EXACT_SITE_BASE__;
const absolute = value => /^[a-z][a-z0-9+.-]*:/i.test(value);

// A shell served outside its mount has no app-relative location. Keep the absolute URL for notfound and
// history, so it cannot accidentally match an app route with the same path or acquire the mount on output.
export const appLocation = value => withoutBase(value, base) ?? new URL(value, globalThis.location.href).href;
export const canonical = value => absolute(value) ? value.split('#')[0] : canonicalPath(value);
export const siteLocation = value => absolute(value) ? value : withBase(value, base);
export const siteCanonical = value => siteLocation(canonical(appLocation(value)));

/** A link's authored path, including a lowered path() result; fragments and external URLs keep HTML's meaning. */
export const siteHref = value => mountedHref(value, base);

/** Bundled media belongs to the document's asset base, which may pin a generation. */
export function siteAsset(key, value) {
  if (key === 'href') return siteHref(value);
  if (key === 'src' || key === 'poster') {
    const url = siteHref(value), path = url && withoutBase(url, base);
    return /^\/(assets|deck|shaders)\//.test(path ?? '') ? '.' + path : url;
  }
  return value;
}
