import { site } from '../constants/site.ts';

// The Pages worker (dist.pages/_worker.js, which tools/pages.ts bundles from this file with the build's page
// paths). It serves a page's Markdown when requested, as `bun exact.mjs serve` does, and also offers it at
// the page's own path to a chat fetcher. A missing page can return 404.md or a JSON 404. _routes.json keeps
// the site's other files away from the worker.

export type Env = { ASSETS: { fetch(request: Request): Promise<Response> } };

const HOST = site.origin && new URL(site.origin).host;
const MARKDOWN = 'text/markdown; charset=utf-8';
// Product names and versions are HTTP tokens (RFC 9110 §§5.6.2, 10.1.5).
const AI_USER_AGENTS = /(?<![\w!#$%&'*+.^`|~/-])(?:ChatGPT-User|Claude-User|Perplexity-User)(?![\w!#$%&'*+.^`|~-])/i;

function preferences(accept: string) {
  // As in scripts/origin.mjs, quoted parameters can contain escaped quotes and delimiters.
  const ranges: string[][][] = [[[]]];
  let quoted = false;
  for (let i = 0; i < accept.length; i++) {
    const c = accept[i], range = ranges.at(-1)!, param = range.at(-1)!;
    if (quoted && c === '\\') { param.push(c, accept[++i] ?? ''); continue; }
    if (c === '"') quoted = !quoted;
    if (!quoted && c === ',') { ranges.push([[]]); continue; }
    if (!quoted && c === ';') { range.push([]); continue; }
    param.push(c);
  }
  const qualities = new Map<string, number>();
  for (const range of ranges) {
    const [media, ...params] = range.map(part => part.join('').trim().toLowerCase());
    const q = params.find(param => param.startsWith('q='));
    if (!qualities.has(media)) qualities.set(media, q === undefined ? 1 : Math.max(0, Math.min(1, Number(q.slice(2)) || 0)));
  }
  // Use the most specific matching range (RFC 9110 §12.5.1). Markdown requires an explicit range.
  const wildcard = qualities.get('*/*') ?? 0;
  const html = qualities.get('text/html') ?? qualities.get('text/*') ?? wildcard;
  const markdown = qualities.get('text/markdown') ?? 0;
  const jsonRange = qualities.get('application/json') ?? qualities.get('application/*');
  const json = jsonRange ?? wildcard;
  return {
    markdown: markdown > 0 && markdown >= html,
    // HTML wins a tie when the client accepts JSON through */* alone.
    json: json > 0 && json > markdown && (jsonRange === undefined ? json > html : json >= html),
  };
}

export const prefersMarkdown = (accept: string) => preferences(accept).markdown;

function withHeaders(response: Response, status: number, headers: Record<string, string>): Response {
  const out = new Response(response.body, { status, headers: response.headers });
  for (const [name, value] of Object.entries(headers)) name === 'Vary' ? out.headers.append(name, value) : out.headers.set(name, value);
  return out;
}

async function answer(request: Request, env: Env, url: URL, canonical: boolean): Promise<Response> {
  const asset = (pathname: string) => env.ASSETS.fetch(new Request(new URL(pathname, url), request));
  // 404.md is asked for without the request's headers: its conditions are about the path it named.
  const missing = async (headers: Record<string, string>) =>
    withHeaders(await env.ASSETS.fetch(new Request(new URL('/404.md', url), { method: request.method })), 404, { 'Content-Type': MARKDOWN, ...headers });

  // Only a GET or a HEAD has an answer to choose; any other request may carry a body, which is sent once.
  if (request.method !== 'GET' && request.method !== 'HEAD') return env.ASSETS.fetch(request);
  if (url.pathname.endsWith('.md')) {
    const file = await env.ASSETS.fetch(request);
    return file.status === 404 ? missing({}) : file;
  }
  // A page's path has no extension; a file's answer does not depend on Accept.
  if (/\.[^/]+$/.test(url.pathname)) return env.ASSETS.fetch(request);

  const vary = { Vary: canonical ? 'Accept, User-Agent' : 'Accept' };
  const { markdown: asked, json: wantsJson } = preferences(request.headers.get('Accept') ?? '');
  const offered = canonical && !asked && AI_USER_AGENTS.test(request.headers.get('User-Agent') ?? '');
  if (asked || offered) {
    const markdown = await asset(`${url.pathname.replace(/\/+$/, '') || '/index'}.md`);
    // 304 answers a conditional request for the Markdown.
    if (markdown.ok || markdown.status === 304) return withHeaders(markdown, markdown.status, vary);
  }
  const page = await env.ASSETS.fetch(request);
  if (page.status === 404) {
    if (wantsJson) {
      // Keep what Pages and _headers add to every answer (CORS, caching, discovery); only the body is new.
      const headers = new Headers(page.headers);
      for (const name of ['Content-Length', 'Content-Encoding', 'ETag', 'Last-Modified']) headers.delete(name);
      headers.set('Content-Type', 'application/json; charset=utf-8');
      headers.append('Vary', vary.Vary);
      // A body that already failed has nothing left to release.
      await page.body?.cancel().catch(() => {});
      const body = {
        error: { status: 404, code: 'not_found', message: 'No documentation page exists at this path. Find a valid path in the documentation index or the sitemap.' },
        links: { index: `${url.origin}/llms.txt`, sitemap: `${url.origin}/sitemap.md`, home: `${url.origin}/` },
      };
      return new Response(request.method === 'HEAD' ? null : JSON.stringify(body), { status: 404, headers });
    }
    if (asked) return missing(vary);
  }
  return withHeaders(page, page.status, vary);
}

export default function createWorker(pagePaths: readonly string[]) {
  const paths = new Set(pagePaths);
  return {
    async fetch(request: Request, env: Env): Promise<Response> {
      const url = new URL(request.url);
      const response = await answer(request, env, url, paths.has(url.pathname));
      // Preview deployments and the project's pages.dev address stay out of search indexes.
      return url.host === HOST ? response : withHeaders(response, response.status, { 'X-Robots-Tag': 'noindex' });
    },
  };
}
