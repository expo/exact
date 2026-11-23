import { afterAll, beforeAll, expect, test } from 'bun:test';
import { existsSync } from 'node:fs';
import { createServer } from 'node:net';
import { resolve } from 'node:path';

// Pages' own rules meet the tree tools/pages.ts makes here: `wrangler pages dev` serves dist.pages as Cloudflare
// does, with its _redirects, _headers, _routes.json and _worker.js.

const site = resolve(import.meta.dir, '..');
let server: ReturnType<typeof Bun.spawn>, origin: string;

beforeAll(async () => {
  if (!existsSync(resolve(site, 'dist.pages/_worker.js'))) throw new Error('run bun run build:pages in docs/site before the Pages tests');
  const port = await new Promise<number>(done => {
    const probe = createServer().listen(0, '127.0.0.1', () => {
      const { port } = probe.address() as { port: number };
      probe.close(() => done(port));
    });
  });
  server = Bun.spawn(['bun', 'x', 'wrangler', 'pages', 'dev', '--ip', '127.0.0.1', '--port', String(port)], { cwd: site, stdout: 'pipe', stderr: 'inherit' });
  let output = '';
  for await (const chunk of server.stdout as ReadableStream<Uint8Array>) {
    output += new TextDecoder().decode(chunk);
    if (output.includes('Ready on')) break;
  }
  if (!output.includes('Ready on')) throw new Error(`wrangler pages dev did not start:\n${output}`);
  origin = `http://127.0.0.1:${port}`;
}, 60000);

afterAll(() => server?.kill());

async function get(path: string, headers: Record<string, string> = {}) {
  const response = await fetch(origin + path, { headers, redirect: 'manual' });
  await response.arrayBuffer();
  const location = response.headers.get('Location');
  return { status: response.status, type: response.headers.get('Content-Type')?.split(';')[0] ?? null, location: location && new URL(location, origin).pathname };
}

test('a page answers at its path, and every other spelling of it redirects there once', async () => {
  const page = { status: 200, type: 'text/html', location: null };
  expect(await get('/')).toEqual(page);
  expect(await get('/start')).toEqual(page);
  expect(await get('/learn/contract-for-humans')).toEqual(page);
  expect(await get('/learn/contract-for-humans/run-an-app')).toEqual(page);
  expect(await get('/start/')).toMatchObject({ status: 301, location: '/start' });
  expect(await get('/start.html')).toMatchObject({ status: 308, location: '/start' });
  expect(await get('/start-here')).toMatchObject({ status: 301, location: '/start' });
  expect(await get('/nope')).toEqual({ status: 404, type: 'text/html', location: null });
});

test('a path that is not a page redirects by its rule in constants/redirects.ts', async () => {
  expect(await get('/llms-full.txt')).toMatchObject({ status: 301, location: '/llms.txt' });
  expect(await get('/llms.txt')).toEqual({ status: 200, type: 'text/plain', location: null });
});

test('a chat fetcher gets Markdown at a published page path', async () => {
  const fetcher = { Accept: 'text/html', 'User-Agent': 'Claude-User/1.0' };
  expect(await get('/learn/contract-for-humans/run-an-app', fetcher)).toEqual({ status: 200, type: 'text/markdown', location: null });
  expect(await get('/start/', fetcher)).toMatchObject({ status: 301, location: '/start' });
  expect(await get('/sitemap', fetcher)).toEqual({ status: 404, type: 'text/html', location: null });
});

test('Markdown, the worker and the files it does not see', async () => {
  const response = await fetch(`${origin}/start`, { headers: { Accept: 'text/markdown' } });
  // The build writes sitemap.xml only with an origin, and names it only then.
  const link = existsSync(resolve(site, 'dist.pages/sitemap.xml')) ? '</llms.txt>; rel="llms-txt", </sitemap.xml>; rel="sitemap"' : '</llms.txt>; rel="llms-txt"';
  expect([response.status, response.headers.get('Content-Type'), response.headers.get('Vary'), response.headers.get('Link'), response.headers.get('X-Llms-Txt'), response.headers.get('X-Robots-Tag')])
    .toEqual([200, 'text/markdown; charset=utf-8', 'Accept, User-Agent', link, '/llms.txt', 'noindex']);
  expect((await response.text()).startsWith('---\ntitle: ')).toBe(true);
  expect(await get('/start.md')).toEqual({ status: 200, type: 'text/markdown', location: null });
  expect(await get('/nope.md')).toEqual({ status: 404, type: 'text/markdown', location: null });

  const file = await fetch(`${origin}/app.plan`);
  expect([file.status, file.headers.get('X-Robots-Tag')]).toEqual([200, null]);
  expect((await get('/.exact/install.json')).status).toBe(200);
  const install = await fetch(`${origin}/.exact/install/`);
  expect([install.status, install.headers.get('X-Robots-Tag')]).toEqual([200, 'noindex']);
});
