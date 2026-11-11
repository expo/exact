// robots.txt and sitemap.xml for a site built ahead (LLP 1048.000 D2, LLP 1048 §9.9). The sitemap lists the pages
// made at build by the rule the render server's own sitemap keeps (serve.rs `sitemap`): not the not-found page, a page
// whose status is not 200 or 503, one whose robots meta says `noindex` or `none`, or one whose canonical URL is
// another. A page rendered per request is not made at build, so it is not listed.
import { existsSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';

const xml = t => String(t).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');
// The URL a page's `<link rel="canonical">` names, as both renderers write it (an attribute escaped as HTML's).
const canonicalOf = page => {
  const href = /<link rel="canonical" href="([^"]*)">/.exec(page ?? '')?.[1];
  return href?.replace(/&quot;/g, '"').replace(/&lt;/g, '<').replace(/&gt;/g, '>').replace(/&amp;/g, '&');
};

/** Writes both files into `dir` for `pages` (each `{ location, notfound, status, robots, page }`, its location
 * percent-encoded and `page` its HTML), against the manifest's `origin`. A file the app renders a route at, which
 * `dir` then holds as a directory, is left alone. Without an origin, or a page to list, there is no sitemap. */
export function writeCrawlerFiles(dir, pages, origin) {
  const site = origin?.replace(/\/+$/, '');
  const listed = !site ? [] : pages.filter(p => !p.notfound && (p.status === 200 || p.status === 503)
    && !/\b(noindex|none)\b/i.test(p.robots ?? '') && (canonicalOf(p.page) ?? site + p.location) === site + p.location)
    .map(p => p.location);
  const sitemap = listed.length > 0 && !existsSync(resolve(dir, 'sitemap.xml'));
  if (sitemap) writeFileSync(resolve(dir, 'sitemap.xml'), `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${listed.map(l => `  <url><loc>${xml(site + l)}</loc></url>\n`).join('')}</urlset>\n`);
  if (!existsSync(resolve(dir, 'robots.txt'))) writeFileSync(resolve(dir, 'robots.txt'), `User-agent: *\nAllow: /\n${sitemap ? `Sitemap: ${site}/sitemap.xml\n` : ''}`);
}
