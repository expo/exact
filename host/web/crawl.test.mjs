import { expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { writeCrawlerFiles } from './crawl.mjs';

const ORIGIN = 'https://site.invalid';
const page = (location, more = {}) => ({ location, notfound: false, status: 200, robots: null, page: '<html>', ...more });
const written = (pages, origin = ORIGIN, before = () => {}, routes = []) => {
  const dir = mkdtempSync(resolve(tmpdir(), 'crawl-'));
  try {
    before(dir);
    const read = f => statSync(resolve(dir, f), { throwIfNoEntry: false })?.isFile() ? readFileSync(resolve(dir, f), 'utf8') : null;
    writeCrawlerFiles(dir, pages, origin, routes);
    return { robots: read('robots.txt'), sitemap: read('sitemap.xml') };
  } finally { rmSync(dir, { recursive: true, force: true }); }
};
const locs = sitemap => [...(sitemap ?? '').matchAll(/<loc>([^<]*)<\/loc>/g)].map(m => m[1]);

// The sitemap keeps what the render server's keeps (serve.rs `indexed`): a page whose status is 200 or 503 and that
// says neither `noindex` nor `none`. A canonical URL naming another page does not leave a page out, as on the server.
test('the sitemap lists the pages a crawler should index, against the origin', () => {
  const { robots, sitemap } = written([
    page('/'), page('/late', { status: 503 }), page('/gone', { status: 410 }), page('/missing', { notfound: true, status: 404 }),
    page('/hidden', { robots: 'noindex, follow' }), page('/none', { robots: 'none' }),
    page('/copy', { page: '<link rel="canonical" href="https://site.invalid/">' }),
    page('/self', { page: '<link rel="canonical" href="https://site.invalid/self">' }),
    page('/a%20b?x=1&y=2'),
  ], ORIGIN + '/');
  expect(locs(sitemap)).toEqual(['https://site.invalid/', 'https://site.invalid/late', 'https://site.invalid/copy', 'https://site.invalid/self', 'https://site.invalid/a%20b?x=1&amp;y=2']);
  expect(robots).toBe('User-agent: *\nAllow: /\nSitemap: https://site.invalid/sitemap.xml\n');
});

test('without an origin or a page to list there is no sitemap, and robots.txt names none', () => {
  for (const [pages, origin] of [[[page('/')], null], [[page('/missing', { notfound: true, status: 404 })], ORIGIN], [[], ORIGIN]]) {
    expect(written(pages, origin)).toEqual({ robots: 'User-agent: *\nAllow: /\n', sitemap: null });
  }
});

// A route at /sitemap.xml or /robots.txt is the app's page there: its directory stays, and nothing is written over it.
test('a route the app renders at either path keeps it', () => {
  const routed = dir => { for (const f of ['sitemap.xml', 'robots.txt']) mkdirSync(resolve(dir, f)); };
  expect(written([page('/')], ORIGIN, routed)).toEqual({ robots: null, sitemap: null });
});

test('declared request routes keep crawler paths even without built pages', () => {
  for (const path of ['/robots.txt', '/sitemap.xml']) {
    const routes = [{ pattern: path, render: 'request', notfound: false }];
    const files = written([page('/')], ORIGIN, () => {}, routes);
    expect(files[path.slice(1).split('.')[0]]).toBeNull();
  }
});

test('a route with a parameter does not claim either path', () => {
  const files = written([page('/')], ORIGIN, () => {}, [{ pattern: '/:slug', render: 'request', notfound: false }]);
  expect(files.robots).toContain('Allow: /');
  expect(locs(files.sitemap)).toEqual(['https://site.invalid/']);
});


test('robots directives use their names, not values', () => {
  const { sitemap } = written([
    page('/preview', { robots: 'max-image-preview:none' }),
    page('/other', { robots: 'follow, noarchive:none' }),
    page('/hidden', { robots: 'follow, NoIndex' }),
    page('/none', { robots: 'NoNe:example' }),
  ]);
  expect(locs(sitemap)).toEqual(['https://site.invalid/preview', 'https://site.invalid/other']);
});
