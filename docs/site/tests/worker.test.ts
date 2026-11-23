import { expect, test } from 'bun:test';
import { prefersMarkdown as serverPrefersMarkdown } from '../../../scripts/origin.mjs';
import createWorker, { prefersMarkdown } from '../worker/index.ts';

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
  '/sitemap.md': ['# Sitemap', MARKDOWN],
};
const worker = createWorker(['/', '/start']);
const env = {
  ASSETS: {
    async fetch(request: Request) {
      const { pathname } = new URL(request.url);
      if (pathname === '/start/') return new Response(null, { status: 301, headers: { Location: '/start' } });
      const [body, headers] = FILES[pathname] ?? [];
      if (body === undefined) return new Response('not found page', { status: 404, headers: HTML });
      if (request.headers.get('If-None-Match') === '"current"') return new Response(null, { status: 304, headers });
      return new Response(body, { headers });
    },
  },
};

async function get(path: string, headers: Record<string, string> = {}, origin = 'https://docs.example.com') {
  const response = await worker.fetch(new Request(origin + path, { headers }), env);
  return { status: response.status, type: response.headers.get('Content-Type'), vary: response.headers.get('Vary'), location: response.headers.get('Location'), robots: response.headers.get('X-Robots-Tag'), body: await response.text() };
}
const asksMarkdown = { Accept: 'text/markdown, text/html;q=0.8' };
const asksHtml = { Accept: 'text/html' };
const fetcher = { 'User-Agent': 'Claude-User/1.0' };

test('a request that prefers text/markdown gets the page as Markdown, and any other its HTML', async () => {
  expect(await get('/start', asksMarkdown)).toMatchObject({ status: 200, type: MARKDOWN['Content-Type'], body: '# Start' });
  expect(await get('/', asksMarkdown)).toMatchObject({ status: 200, body: '# Home' });
  expect(await get('/start', { Accept: 'text/html, text/markdown;q=0.5' })).toMatchObject({ status: 200, type: HTML['Content-Type'], body: 'start page' });
  expect(await get('/start.md')).toMatchObject({ status: 200, vary: null, body: '# Start' });
  expect(await get('/.exact/install/', asksMarkdown)).toMatchObject({ status: 200, vary: 'Accept', body: 'install page' });
  expect(await get('/app.plan', asksMarkdown)).toMatchObject({ status: 200, vary: null, body: 'plan' });
});

test('both page representations vary by Accept and User-Agent, including 304 responses', async () => {
  for (const headers of [asksHtml, asksMarkdown, { ...asksHtml, ...fetcher }]) {
    expect(await get('/start', headers)).toMatchObject({ status: 200, vary: 'Accept, User-Agent' });
    expect(await get('/start', { ...headers, 'If-None-Match': '"current"' })).toMatchObject({ status: 304, vary: 'Accept, User-Agent' });
  }
});

test('an agent that fetches a page for a person in a chat gets its Markdown without asking', async () => {
  for (const agent of ['Mozilla/5.0 AppleWebKit/537.36 (KHTML, like Gecko); compatible; ChatGPT-User/1.0; +https://openai.com/bot', 'Claude-User/1.0', 'Perplexity-User/1.0']) {
    expect(await get('/start', { ...asksHtml, 'User-Agent': agent })).toMatchObject({ status: 200, type: MARKDOWN['Content-Type'], body: '# Start' });
  }
  expect(await get('/start', { ...asksMarkdown, ...fetcher })).toMatchObject({ body: '# Start' });
});

test('a chat fetcher can omit Accept or send a wildcard range', async () => {
  expect(await get('/start', fetcher)).toMatchObject({ type: MARKDOWN['Content-Type'], body: '# Start' });
  expect(await get('/start', { ...fetcher, Accept: '*/*' })).toMatchObject({ type: MARKDOWN['Content-Type'], body: '# Start' });
});

test('a chat fetcher leaves a trailing slash for Pages to redirect', async () => {
  expect(await get('/start/', { ...asksHtml, ...fetcher })).toMatchObject({ status: 301, location: '/start' });
});

test('a chat fetcher leaves a standalone Markdown file at its own URL', async () => {
  expect(await get('/sitemap', { ...asksHtml, ...fetcher })).toMatchObject({ status: 404, type: HTML['Content-Type'] });
  expect(await get('/sitemap.md', { ...asksHtml, ...fetcher })).toMatchObject({ status: 200, body: '# Sitemap' });
});

test('a chat fetcher gets the Pages answer for installation and missing paths', async () => {
  expect(await get('/.exact/install/', fetcher)).toMatchObject({ status: 200, vary: 'Accept', body: 'install page' });
  expect(await get('/nope', fetcher)).toMatchObject({ status: 404, type: HTML['Content-Type'], body: 'not found page' });
});

test('a chat fetcher matches by its whole product name', async () => {
  for (const agent of ['ClaudeBot/1.0', 'Claude-User-Crawler/1.0', 'My-Perplexity-User/1.0', 'Claude-User.Crawler/1.0', 'Other/Claude-User']) {
    expect(await get('/start', { ...asksHtml, 'User-Agent': agent })).toMatchObject({ type: HTML['Content-Type'], body: 'start page' });
  }
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
