import { expect, test } from 'bun:test';
import { prefersMarkdown as serverPrefersMarkdown } from '../../../scripts/origin.mjs';
import createWorker, { prefersMarkdown } from '../worker/index.ts';

const HTML = { 'Content-Type': 'text/html; charset=utf-8' };
const MARKDOWN = { 'Content-Type': 'text/markdown; charset=utf-8' };
const JSON_TYPE = 'application/json; charset=utf-8';
const discovery = { Link: '</llms.txt>; rel="llms-txt"', 'X-Llms-Txt': '/llms.txt' };
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
      if (body === undefined) return new Response('not found page', { status: 404, headers: { ...HTML, ...discovery } });
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
const asksJson = { Accept: 'application/json' };
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

test('a path nothing answers gets a JSON 404 where JSON was asked for', async () => {
  const response = await worker.fetch(new Request('https://docs.example.com/nope', { headers: asksJson }), env);
  expect([response.status, response.headers.get('Content-Type'), response.headers.get('Vary')]).toEqual([404, JSON_TYPE, 'Accept']);
  expect(await response.json()).toMatchObject({ error: { status: 404, code: 'not_found' }, links: { index: 'https://docs.example.com/llms.txt', sitemap: 'https://docs.example.com/sitemap.md', home: 'https://docs.example.com/' } });
  const html = { status: 404, type: HTML['Content-Type'], body: 'not found page' };
  expect(await get('/nope', { Accept: 'text/html, application/json;q=0.9' })).toMatchObject(html);
  expect(await get('/nope', { Accept: '*/*' })).toMatchObject(html);
  expect(await get('/nope', { Accept: 'application/json, text/markdown' })).toMatchObject({ status: 404, type: MARKDOWN['Content-Type'], body: '# Page not found' });
  expect(await get('/start', asksJson)).toMatchObject({ status: 200, type: HTML['Content-Type'], body: 'start page' });
});

test('a JSON HEAD 404 carries the discovery headers and has an empty body', async () => {
  const request = new Request('https://docs.example.com/nope', { method: 'HEAD', headers: asksJson });
  const response = await worker.fetch(request, env);
  expect([response.status, response.headers.get('Content-Type'), response.headers.get('Vary')]).toEqual([404, JSON_TYPE, 'Accept']);
  expect([response.headers.get('Link'), response.headers.get('X-Llms-Txt')]).toEqual([discovery.Link, discovery['X-Llms-Txt']]);
  expect(await response.text()).toBe('');
});

test('a JSON 404 replaces the asset fallback regardless of its content type and cancels its body', async () => {
  let cancelled = false;
  const body = new ReadableStream({ cancel() { cancelled = true; } });
  const assets = { ASSETS: { async fetch() { return new Response(body, { status: 404, headers: MARKDOWN }); } } };
  const request = new Request('https://docs.example.com/nope', { headers: asksJson });
  const response = await createWorker(['/nope']).fetch(request, assets);
  expect([response.status, response.headers.get('Content-Type'), response.headers.get('Vary'), cancelled]).toEqual([404, JSON_TYPE, 'Accept, User-Agent', true]);
});

test('a JSON 404 keeps the asset 404\'s other headers and answers when its body has failed', async () => {
  const body = new ReadableStream({ start(controller) { controller.error(new Error('broken')); } });
  const pagesHeaders = { ...HTML, ...discovery, 'Access-Control-Allow-Origin': '*', 'Cache-Control': 'no-store', ETag: '"404"' };
  const assets = { ASSETS: { async fetch() { return new Response(body, { status: 404, headers: pagesHeaders }); } } };
  const response = await worker.fetch(new Request('https://docs.example.com/nope', { headers: asksJson }), assets);
  expect([response.status, response.headers.get('Content-Type'), response.headers.get('Access-Control-Allow-Origin'), response.headers.get('Cache-Control'), response.headers.get('ETag')])
    .toEqual([404, JSON_TYPE, '*', 'no-store', null]);
  expect((await response.json()).error.code).toBe('not_found');
});

test('Accept delimiters inside quoted parameters stay in the parameter', async () => {
  expect(await get('/nope', { Accept: 'text/html, text/plain;note="a,application/json,b"' })).toMatchObject({ status: 404, type: HTML['Content-Type'] });
  expect(await get('/nope', { Accept: 'text/markdown;note="a;q=0;b", text/html;q=0.8' })).toMatchObject({ status: 404, type: MARKDOWN['Content-Type'] });
});

test('JSON quality comes from the most specific matching Accept range', async () => {
  expect(await get('/nope', { Accept: 'application/*, text/html;q=0' })).toMatchObject({ status: 404, type: JSON_TYPE });
  expect(await get('/nope', { Accept: 'application/*, application/json;q=0, text/html;q=0.5' })).toMatchObject({ status: 404, type: HTML['Content-Type'] });
  expect(await get('/nope', { Accept: '*/*, text/html;q=0' })).toMatchObject({ status: 404, type: JSON_TYPE });
});

test('JSON wins a missing page when it ranks above Markdown', async () => {
  expect(await get('/nope', { Accept: 'application/json, text/markdown;q=0.5' })).toMatchObject({ status: 404, type: JSON_TYPE });
  expect(await get('/nope', { Accept: 'application/json;q=0.5, text/markdown' })).toMatchObject({ status: 404, type: MARKDOWN['Content-Type'], body: '# Page not found' });
  expect(await get('/start', { Accept: 'application/json, text/markdown;q=0.5' })).toMatchObject({ status: 200, type: MARKDOWN['Content-Type'], body: '# Start' });
});

test('a JSON request for a missing file gets the asset response', async () => {
  expect(await get('/nope.png', asksJson)).toMatchObject({ status: 404, type: HTML['Content-Type'], vary: null, body: 'not found page' });
});

test('a request that is not a GET or a HEAD is not negotiated', async () => {
  const response = await worker.fetch(new Request('https://docs.example.com/start', { method: 'POST', body: 'text', headers: asksMarkdown }), env);
  expect([response.status, response.headers.get('Vary'), await response.text()]).toEqual([200, null, 'start page']);
  const missing = await worker.fetch(new Request('https://docs.example.com/nope', { method: 'POST', headers: asksJson }), env);
  expect([missing.status, missing.headers.get('Content-Type'), missing.headers.get('Vary'), await missing.text()]).toEqual([404, HTML['Content-Type'], null, 'not found page']);
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
