import type { Answer, Result, Sources } from './app.contract.d.ts';
import { pages } from './content/index.ts';
import nav from './content/nav.ts';
import moved from './content/redirects.ts';
import { search } from './constants/search.ts';
import type { markdownHeadings } from './tools/markdown-headings.ts';

export const appId = 'dev.exact.docs';
const SEARCH_HOST = 'https://SNB9MIQMWS-dsn.algolia.net';
export const grants = `net.fetch ${SEARCH_HOST}`;

type Page = Result<'loadPage'>;
type Marked = { value?: string };
type DocSearchHit = {
  objectID: string;
  url: string;
  hierarchy: Record<string, string | null>;
  _highlightResult?: { hierarchy?: Record<string, Marked> };
  _snippetResult?: { content?: Marked };
};
type SearchHit = Result<'searchDocs'>[number];

const MARK_START = '\u0001';
const MARK_END = '\u0002';
const LEVELS = ['lvl1', 'lvl2', 'lvl3', 'lvl4', 'lvl5', 'lvl6'];
const HITS_PER_PAGE = 6;

const markedParts = (id: string, marked: string) =>
  marked
    .split(MARK_START)
    .flatMap((chunk, i) => {
      const [inside, after] = i === 0 ? ['', chunk] : chunk.split(MARK_END);
      return [{ text: inside ?? '', mark: true }, { text: after ?? '', mark: false }];
    })
    .filter(part => part.text !== '')
    .map((part, i) => ({ id: `${id}-${i}`, ...part }));

const searchHit = (hit: DocSearchHit): SearchHit => {
  const levels = LEVELS.filter(level => hit.hierarchy[level]);
  const deepest = levels.at(-1) ?? 'lvl0';
  const title = hit._highlightResult?.hierarchy?.[deepest]?.value ?? hit.hierarchy[deepest] ?? '';
  const url = new URL(hit.url);
  return {
    id: hit.objectID,
    href: `${url.pathname}${url.hash}`,
    title: markedParts(`${hit.objectID}-t`, title),
    path: levels.length ? [hit.hierarchy.lvl0, ...levels.slice(0, -1).map(level => hit.hierarchy[level])].join(' › ') : '',
    snippet: markedParts(`${hit.objectID}-s`, hit._snippetResult?.content?.value ?? ''),
    nested: false,
  };
};

const pageOf = (hit: SearchHit) => hit.href.split('#')[0];

const groupedByPage = (hits: SearchHit[]): SearchHit[] =>
  [...new Set(hits.map(pageOf))].flatMap(page =>
    hits
      .filter(hit => pageOf(hit) === page)
      .sort((a, b) => Number(a.href.includes('#')) - Number(b.href.includes('#')))
      .slice(0, HITS_PER_PAGE)
      .map((hit, i) => ({ ...hit, nested: i > 0 })),
  );

const none = { title: '', href: '' };
const missing: Page = {
  slug: 'notfound',
  path: '',
  movedFrom: [],
  section: '',
  crumbs: [],
  title: 'Page Not Found',
  description: 'No page of the Exact docs is at this address.',
  subtitle: '',
  updated: '',
  prev: none,
  next: none,
  editUrl: '',
  feedbackUrl: '',
  markdownPath: '',
  claudeUrl: '',
  claudeCodeUrl: '',
  chatgptUrl: '',
  codexUrl: '',
  cursorUrl: '',
  t3Url: '',
  grokUrl: '',
  metaUrl: '',
  blocks: [],
  toc: [],
};

const words = (s: string) => s.toLowerCase().split(/[^a-z0-9]+/).filter(w => w.length > 2);

const cleaned = (url: string) => {
  const path = url.split(/[?#]/)[0].replace(/^\/+/, '/').replace(/\/index\.html$/, '/').replace(/\.(html|md)$/, '');
  return path.length > 1 ? path.replace(/\/+$/, '') : path;
};

// The not-found page's URL, as expo/docs' getRedirectPath reads it: no query or fragment, no index.html, .html
// or .md, no trailing slash. A listed old path gives its page; a page's own path (reached with a trailing slash
// or an extension) gives itself. Else the pages sharing the most words with the URL are suggested, as
// expo/docs' worker/url-recovery.ts suggests them.
function resolve(url: string): Result<'resolveMoved'> {
  if (!url) return { to: '', go: '', suggestions: [] };
  const clean = cleaned(url);
  const go = (to: string) => ({ to, go: `${to}?from=${encodeURIComponent(clean)}`, suggestions: [] });
  if (moved.redirects[clean]) return go(moved.redirects[clean]);
  if (clean !== url && moved.pages.some(p => p.path === clean)) return { to: clean, go: clean, suggestions: [] };
  const asked = new Set(words(clean));
  const suggestions = moved.pages
    .map(p => ({ p, score: words(`${p.path} ${p.title}`).filter(w => asked.has(w)).length }))
    .filter(x => x.score > 0)
    .sort((a, b) => b.score - a.score)
    .slice(0, 3)
    .map(({ p }) => ({ title: p.title, href: p.path }));
  return { to: '', go: '', suggestions };
}

const searchHits = async (body: Record<string, unknown>) => {
  const response = await fetch(`${SEARCH_HOST}/1/indexes/${search.index}/query`, {
    method: 'POST',
    headers: { 'X-Algolia-Application-Id': search.appId, 'X-Algolia-API-Key': search.searchKey },
    body: JSON.stringify(body),
  });
  if (!response.ok) throw new Error(`Search answered ${response.status}.`);
  return ((await response.json()) as { hits: DocSearchHit[] }).hits;
};

async function textAt(url: string, cache?: RequestCache) {
  const response = await fetch(url, { cache });
  if (!response.ok) throw new Error(`${url} answered ${response.status}.`);
  return await response.text();
}

function pageAt(given: string) {
  const here = new URL(import.meta.url);
  const url = (given.startsWith('/') || /^https?:\/\//i.test(given)) && URL.canParse(given, here) ? new URL(given, here) : null;
  if (!url || ![here.origin, moved.origin].includes(url.origin)) throw new Error('path must start with "/", or be a URL of these docs.');
  const clean = cleaned(url.pathname);
  const path = moved.redirects[clean] ?? (clean === '/index' ? '/' : clean);
  const page = moved.pages.find(page => page.path === path);
  if (!page) throw new Error(`No page is at ${clean}. search_docs and list_sections return the paths of the pages.`);
  return { ...page, location: `${path}${url.search}${url.hash}` };
}

const markdownAt = (route: string, cache?: RequestCache) => textAt(`/assets/markdown/${route}.md`, cache);

type PageInfo = {
  path: string;
  title: string;
  description: string;
  modificationDate: string;
  headings: ReturnType<typeof markdownHeadings>;
  digest: string;
};

async function pageInfoAt(route: string, cache?: RequestCache): Promise<PageInfo> {
  return JSON.parse(await textAt(`/assets/markdown/tools/${route}.json`, cache)) as PageInfo;
}

const plain = (heading: string) => heading.replaceAll('`', '').replace(/\s+/g, ' ').trim().toLowerCase();

function sectionsOf(markdown: string, headings: PageInfo['headings'], section: string, path: string) {
  const found = headings.flatMap((heading, index) => {
    if (plain(heading.text) !== plain(section)) return [];
    const parents: string[] = [];
    let level = heading.level;
    for (const earlier of headings.slice(0, index).reverse()) {
      if (earlier.level < level) {
        parents.unshift(markdown.slice(earlier.start, earlier.headingEnd).trimEnd());
        level = earlier.level;
      }
    }
    return [[...parents, markdown.slice(heading.start, heading.end)].join('\n').trimEnd()];
  });
  if (!found.length) throw new Error(`No heading on ${path} is "${section}". get_page_info returns the headings of a page.`);
  return found.join('\n\n');
}

function sectionsIn(area: string, section: string) {
  const inArea = (id: string) => nav.groups.filter(group => group.tab === id);
  if (!area) return nav.tabs.map(tab => ({ area: tab.id, sections: inArea(tab.id).map(group => group.title) }));
  if (!nav.tabs.some(tab => tab.id === area)) throw new Error(`No area is "${area}". The areas: ${nav.tabs.map(tab => tab.id).join(', ')}.`);
  const groups = inArea(area).filter(group => !section || group.title === section);
  if (!groups.length) throw new Error(`No section of ${area} is "${section}". Its sections: ${inArea(area).map(group => group.title).join(', ')}.`);
  return groups.flatMap(group =>
    group.items.map(item => {
      const parent = group.items.find(other => other.route === item.parent);
      return { section: group.title, ...(parent && { group: parent.title }), title: item.title, path: item.href };
    }),
  );
}

async function searchResults(query: string, limit: number) {
  if (!query.trim()) throw new Error('query must not be empty.');
  if (!Number.isInteger(limit) || limit < 1 || limit > 20) throw new Error('limit must be a whole number from 1 to 20.');
  const hits = await searchHits({ query: query.trim(), hitsPerPage: limit });
  return hits.map(hit => {
    const url = new URL(hit.url);
    return { title: ['lvl0', ...LEVELS].map(level => hit.hierarchy[level]).filter(Boolean).join(' > '), path: `${url.pathname}${url.hash}` };
  });
}

const tools: Record<string, (path: string, text: string, count: number) => Promise<string>> = {
  search_docs: async (_path, query, limit) => JSON.stringify(await searchResults(query, limit)),
  get_page_markdown: async (given, section) => {
    const { path, route } = pageAt(given);
    if (!section) return markdownAt(route);
    for (const cache of ['default', 'reload'] as const) {
      const { headings, digest } = await pageInfoAt(route, cache);
      const markdown = await markdownAt(route, cache);
      const hash = await crypto.subtle.digest('SHA-256', new TextEncoder().encode(markdown));
      const hex = Array.from(new Uint8Array(hash), byte => byte.toString(16).padStart(2, '0')).join('');
      if (hex === digest) return sectionsOf(markdown, headings, section, path);
    }
    throw new Error(`The Markdown for ${path} changed while it was being read. Try again.`);
  },
  get_page_info: async given => {
    const { route } = pageAt(given);
    const { path, title, description, modificationDate, headings } = await pageInfoAt(route);
    return JSON.stringify({ path, title, description, modificationDate, headings: headings.map(({ level, text }) => ({ level, text })) });
  },
  get_site_overview: () => textAt('/assets/llms.txt'),
  list_sections: async (area, section) => JSON.stringify(sectionsIn(area, section)),
};

const sources: Sources = {
  // Only a route with no page is missing. A page whose module fails to download rejects, so the
  // reader is told and can try again.
  loadPage: async ([route]): Promise<Page> => {
    const load = pages[route];
    return load ? ((await load()).default as Page) : missing;
  },
  loadNav: async (): Promise<Result<'loadNav'>> => nav,
  resolveMoved: async ([url]): Promise<Result<'resolveMoved'>> => resolve(url),
  resolveToolPage: async ([given]): Promise<Result<'resolveToolPage'>> => {
    try {
      return { url: pageAt(given).location, error: '' };
    } catch (error) {
      return { url: '', error: error instanceof Error ? error.message : String(error) };
    }
  },
  searchDocs: async ([query]): Promise<Result<'searchDocs'>> => {
    if (query.trim().length < 2) return [];
    const hits = await searchHits({ query, hitsPerPage: 20, attributesToSnippet: ['content:24'], highlightPreTag: MARK_START, highlightPostTag: MARK_END });
    return groupedByPage(hits.map(searchHit));
  },
  answerTool: async ([tool, path, text, count]): Promise<Result<'answerTool'>> => {
    const read = tools[tool];
    if (!read) return { failed: false, text: '' };
    try {
      return { failed: false, text: await read(path, text, count) };
    } catch (error) {
      return { failed: true, text: error instanceof Error ? error.message : String(error) };
    }
  },
  fetchMarkdown: async ([route]): Promise<Result<'fetchMarkdown'>> => {
    const response = await fetch(`/assets/markdown/${route}.md`);
    if (!response.ok) throw new Error(`The page's Markdown answered ${response.status}.`);
    return { text: await response.text() };
  },
  pageAddress: async ([path]): Promise<Result<'pageAddress'>> => ({ url: new URL(path, import.meta.url).href }),
};

export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);
