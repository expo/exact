import { expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, statSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { writeCrawlerFiles } from './crawl.mjs';

const ORIGIN = 'https://site.invalid';
const page = (location, more = {}) => ({ location, notfound: false, status: 200, robots: null, page: '<html>', ...more });
const written = (pages, origin = ORIGIN, before = () => {}) => {
  const dir = mkdtempSync(resolve(tmpdir(), 'crawl-'));
  try {
    before(dir);
    const read = f => statSync(resolve(dir, f), { throwIfNoEntry: false })?.isFile() ? readFileSync(resolve(dir, f), 'utf8') : null;
    writeCrawlerFiles(dir, pages, origin);
    return { robots: read('robots.txt'), sitemap: read('sitemap.xml') };
  } finally { rmSync(dir, { recursive: true, force: true }); }
};
const locs = sitemap => [...(sitemap ?? '').matchAll(/<loc>([^<]*)<\/loc>/g)].map(m => m[1]);

// The sitemap keeps what the render server's keeps (serve.rs `sitemap`): a page whose status is 200 or 503, that says
// neither `noindex` nor `none`, and whose canonical URL is its own.
test('the sitemap lists the pages a crawler should index, against the origin', () => {
  const { robots, sitemap } = written([
    page('/'), page('/late', { status: 503 }), page('/gone', { status: 410 }), page('/missing', { notfound: true, status: 404 }),
    page('/hidden', { robots: 'noindex, follow' }), page('/none', { robots: 'none' }),
    page('/copy', { page: '<link rel="canonical" href="https://site.invalid/">' }),
    page('/self', { page: '<link rel="canonical" href="https://site.invalid/self">' }),
    page('/a%20b?x=1&y=2'),
  ], ORIGIN + '/');
  expect(locs(sitemap)).toEqual(['https://site.invalid/', 'https://site.invalid/late', 'https://site.invalid/self', 'https://site.invalid/a%20b?x=1&amp;y=2']);
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
