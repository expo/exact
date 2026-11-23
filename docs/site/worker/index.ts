import { site } from '../constants/site.ts';

// The Pages worker (dist.pages/_worker.js, which tools/pages.ts bundles from this file with the build's page
// paths). It gives a request the answer `bun exact.mjs serve` gives it: a page's Markdown where the request prefers
// text/markdown, and 404.md for a path nothing answers. As docs.expo.dev's worker does, it also gives a page's
// Markdown, at the page's own path, to an agent that fetches one page for a person in a chat. _routes.json keeps
// the site's other files away from it.

export type Env = { ASSETS: { fetch(request: Request): Promise<Response> } };

const HOST = site.origin && new URL(site.origin).host;
const MARKDOWN = 'text/markdown; charset=utf-8';
// Product names and versions are HTTP tokens (RFC 9110 §§5.6.2, 10.1.5).
const AI_USER_AGENTS = /(?<![\w!#$%&'*+.^`|~/-])(?:ChatGPT-User|Claude-User|Perplexity-User)(?![\w!#$%&'*+.^`|~-])/i;

// The rule of scripts/origin.mjs's prefersMarkdown, which imports what a worker cannot load: the type at a
// quality above zero and no lower than that of the most specific range matching text/html (RFC 9110 §12.5.1).
// The first of a repeated range counts.
function prefers(accept: string, type: string): boolean {
  const ranges = new Map<string, number>();
  for (const range of accept.split(',')) {
    const [type, ...params] = range.split(';').map(part => part.trim().toLowerCase());
    const q = params.find(param => param.startsWith('q='));
    if (!ranges.has(type)) ranges.set(type, q === undefined ? 1 : Math.max(0, Math.min(1, Number(q.slice(2)) || 0)));
  }
  const wanted = ranges.get(type) ?? 0;
  return wanted > 0 && wanted >= (ranges.get('text/html') ?? ranges.get('text/*') ?? ranges.get('*/*') ?? 0);
}

export const prefersMarkdown = (accept: string) => prefers(accept, 'text/markdown');

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
  const asked = prefersMarkdown(request.headers.get('Accept') ?? '');
  const offered = canonical && !asked && AI_USER_AGENTS.test(request.headers.get('User-Agent') ?? '');
  if (asked || offered) {
    const markdown = await asset(`${url.pathname.replace(/\/+$/, '') || '/index'}.md`);
    // 304 answers a conditional request for the Markdown.
    if (markdown.ok || markdown.status === 304) return withHeaders(markdown, markdown.status, vary);
  }
  const page = await env.ASSETS.fetch(request);
  return asked && page.status === 404 ? missing(vary) : withHeaders(page, page.status, vary);
}

function notFoundJson(request: Request, url: URL): Response {
  const body = {
    error: { status: 404, code: 'not_found', message: 'No documentation page exists at this path. Find a valid path in the documentation index or the sitemap.' },
    links: { index: `${url.origin}/llms.txt`, sitemap: `${url.origin}/sitemap.md`, home: `${url.origin}/` },
  };
  return new Response(request.method === 'HEAD' ? null : JSON.stringify(body), { status: 404, headers: { 'Content-Type': 'application/json; charset=utf-8', Vary: 'Accept' } });
}

export default function createWorker(pagePaths: readonly string[]) {
  const paths = new Set(pagePaths);
  return {
    async fetch(request: Request, env: Env): Promise<Response> {
      const url = new URL(request.url);
      const answered = await answer(request, env, url, paths.has(url.pathname));
      const missingPageAsHtml = answered.status === 404 && (request.method === 'GET' || request.method === 'HEAD') && !/\.[^/]+$/.test(url.pathname) && !(answered.headers.get('Content-Type') ?? '').startsWith('text/markdown');
      const response = missingPageAsHtml && prefers(request.headers.get('Accept') ?? '', 'application/json') ? notFoundJson(request, url) : answered;
      // Preview deployments and the project's pages.dev address stay out of search indexes.
      return url.host === HOST ? response : withHeaders(response, response.status, { 'X-Robots-Tag': 'noindex' });
    },
  };
}
