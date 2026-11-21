import { expect, test } from 'bun:test';
import { prefersMarkdown as serverPrefersMarkdown } from '../../../scripts/origin.mjs';
import worker, { prefersMarkdown } from '../worker/index.ts';

const HTML = { 'Content-Type': 'text/html; charset=utf-8' };
const MARKDOWN = { 'Content-Type': 'text/markdown; charset=utf-8' };
// Pages' answers: a file at its path, a page at its path without .html, and 404.html for anything else.
const FILES: Record<string, [string, Record<string, string>]> = {
  '/': ['home page', HTML],
  '/index.md': ['# Home', MARKDOWN],
  '/start': ['start page', HTML],
  '/start.md': ['# Start', MARKDOWN],
  '/.exact/install/': ['install page', HTML],
  '/404.md': ['# Page not found', MARKDOWN],
  '/app.plan': ['plan', { 'Content-Type': 'application/octet-stream' }],
};
const env = {
  ASSETS: {
    async fetch(request: Request) {
      const [body, headers] = FILES[new URL(request.url).pathname] ?? [];
      if (body === undefined) return new Response('not found page', { status: 404, headers: HTML });
      if (request.headers.get('If-None-Match') === '"current"') return new Response(null, { status: 304, headers });
      return new Response(body, { headers });
    },
  },
};

async function get(path: string, headers: Record<string, string> = {}, origin = 'https://docs.example.com') {
  const response = await worker.fetch(new Request(origin + path, { headers }), env);
  return { status: response.status, type: response.headers.get('Content-Type'), vary: response.headers.get('Vary'), robots: response.headers.get('X-Robots-Tag'), body: await response.text() };
}
const asksMarkdown = { Accept: 'text/markdown, text/html;q=0.8' };

test('a request that prefers text/markdown gets the page as Markdown, and any other its HTML', async () => {
  expect(await get('/start', asksMarkdown)).toMatchObject({ status: 200, type: MARKDOWN['Content-Type'], vary: 'Accept', body: '# Start' });
  expect(await get('/', asksMarkdown)).toMatchObject({ status: 200, body: '# Home' });
  expect(await get('/start', { Accept: 'text/html, text/markdown;q=0.5' })).toMatchObject({ status: 200, type: HTML['Content-Type'], vary: 'Accept', body: 'start page' });
  expect(await get('/start.md')).toMatchObject({ status: 200, vary: null, body: '# Start' });
  expect(await get('/start', { ...asksMarkdown, 'If-None-Match': '"current"' })).toMatchObject({ status: 304, vary: 'Accept' });
  expect(await get('/.exact/install/', asksMarkdown)).toMatchObject({ status: 200, vary: 'Accept', body: 'install page' });
  expect(await get('/app.plan', asksMarkdown)).toMatchObject({ status: 200, vary: null, body: 'plan' });
});

test('a path nothing answers gets 404.md where Markdown was asked for', async () => {
  const missing = { status: 404, type: MARKDOWN['Content-Type'], body: '# Page not found' };
  expect(await get('/nope', asksMarkdown)).toMatchObject({ ...missing, vary: 'Accept' });
  expect(await get('/nope.md')).toMatchObject(missing);
  expect(await get('/nope')).toMatchObject({ status: 404, type: HTML['Content-Type'], body: 'not found page' });
  expect(await get('/nope.md', { 'If-None-Match': '"current"' })).toMatchObject(missing);
  expect(await get('/nope', { ...asksMarkdown, 'If-None-Match': '"current"' })).toMatchObject(missing);
});

test('a request that is not a GET or a HEAD is not negotiated', async () => {
  const response = await worker.fetch(new Request('https://docs.example.com/start', { method: 'POST', body: 'text', headers: asksMarkdown }), env);
  expect([response.status, response.headers.get('Vary'), await response.text()]).toEqual([200, null, 'start page']);
});

test('the worker and the server read Accept alike', () => {
  for (const accept of ['', '*/*', 'text/html', 'text/markdown', 'text/markdown, */*', 'text/markdown;q=0.9, text/html', 'text/markdown;q=0.5, text/*;q=0.5', 'TEXT/MARKDOWN ; Q=0.7, */*;q=0.8', 'text/markdown;q=0, */*', 'text/plain, text/markdown;q=0.1', 'text/markdown;q=0, text/markdown', 'text/markdown, text/markdown;q=0', 'text/html;q=0, text/html, text/markdown;q=0.5']) {
    expect([accept, prefersMarkdown(accept)]).toEqual([accept, serverPrefersMarkdown(accept)]);
  }
});

test('only the site origin is indexed', async () => {
  const { site } = await import('../constants/site.ts');
  expect((await get('/start', {}, 'https://pr-12.exact-docs.pages.dev')).robots).toBe('noindex');
  if (site.origin) expect((await get('/start', {}, site.origin)).robots).toBeNull();
});
