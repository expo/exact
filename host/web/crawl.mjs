// robots.txt and sitemap.xml for a site built ahead (LLP 1048.000 D2, LLP 1048 §9.9). The sitemap lists the pages
// made at build by the rule the render server's own sitemap keeps (serve.rs `indexed`): not the not-found page, a page
// whose status is not 200 or 503, or one whose robots meta says `noindex` (or `none`, which implies it). A page
// rendered per request is not made at build, so it is not listed.
import { existsSync, writeFileSync } from 'node:fs';
import path, { resolve } from 'node:path';
import { canonical, filePathOf, matchPattern } from '../web-js/route-pattern.js';

const xml = t => String(t).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;');

/** Choose every page file before either renderer writes one. Static lookup uses the same segment mapping. */
export function buildPageFiles(dir, pages, routes, paths = path, platform = process.platform) {
  const chosen = new Map();
  const files = pages.map(page => {
    let value;
    const at = matchPattern(routes, canonical(page.location), (_, parameter) => { value = parameter; });
    const route = page.notfound ? routes.find(r => r.notfound) : routes[at];
    const label = `route \`${route?.name ?? page.location}\` page ${JSON.stringify(page.location)}${value === undefined ? '' : ` (value ${JSON.stringify(value)})`}`;
    try {
      const decoded = decodeURIComponent(page.location);
      if (!page.location.startsWith('/') || /[?#]/.test(page.location) || /[\\\0]/.test(decoded)
          || decoded.split('/').some(s => s === '.' || s === '..')) throw new Error('unsafe page path');
      const file = page.notfound ? '404.html' : `${filePathOf(page.location).replace(/^\/|\/$/g, '')}/index.html`.replace(/^\//, '');
      const relative = paths.relative(paths.resolve(dir), paths.resolve(dir, file));
      if (!relative || relative === '..' || relative.startsWith('..' + paths.sep) || paths.isAbsolute(relative)) throw new Error('page leaves the build directory');
      const key = platform === 'darwin' ? relative.normalize('NFD').toLowerCase()
        : paths.sep === '\\' ? relative.toLowerCase() : relative;
      const prior = chosen.get(key);
      if (prior && (prior.page.location !== page.location || prior.page.notfound !== page.notfound)) throw new Error(`the same page file as ${JSON.stringify(prior.page.location)}: ${file}`);
      chosen.set(key, { page, label });
      return file;
    } catch (error) { throw new Error(`${label}: ${error.message}`); }
  });
  for (const [file, { label }] of chosen) {
    for (let parent = paths.dirname(file); parent !== '.'; parent = paths.dirname(parent)) {
      const prior = chosen.get(parent);
      if (prior) throw new Error(`${label}: page file is inside the page file for ${JSON.stringify(prior.page.location)}`);
    }
  }
  return files;
}

/** Writes crawler files for built `pages` against the manifest's `origin`. A route declared at either file's path
 * keeps it, whether rendered at build or per request; a route with a parameter does not claim it. Without an origin,
 * or a page to list, there is no sitemap. */
export function writeCrawlerFiles(dir, pages, origin, routes) {
  const site = origin?.replace(/\/+$/, '');
  const claimed = path => routes.some(r => !r.notfound && canonical(r.pattern) === path);
  const excluded = robots => (robots ?? '').split(',').some(directive => /^(noindex|none)$/i.test(directive.split(':', 1)[0].trim()));
  const listed = !site ? [] : pages.filter(p => !p.notfound && (p.status === 200 || p.status === 503)
    && !excluded(p.robots))
    .map(p => p.location);
  const sitemap = listed.length > 0 && !claimed('/sitemap.xml') && !existsSync(resolve(dir, 'sitemap.xml'));
  if (sitemap) writeFileSync(resolve(dir, 'sitemap.xml'), `<?xml version="1.0" encoding="UTF-8"?>\n<urlset xmlns="http://www.sitemaps.org/schemas/sitemap/0.9">\n${listed.map(l => `  <url><loc>${xml(site + l)}</loc></url>\n`).join('')}</urlset>\n`);
  if (!claimed('/robots.txt') && !existsSync(resolve(dir, 'robots.txt'))) writeFileSync(resolve(dir, 'robots.txt'), `User-agent: *\nAllow: /\n${sitemap ? `Sitemap: ${site}/sitemap.xml\n` : ''}`);
}
