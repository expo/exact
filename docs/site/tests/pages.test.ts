import { afterAll, beforeAll, expect, test } from 'bun:test';
import { copyFileSync, existsSync, mkdirSync, mkdtempSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:net';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { marked } from 'marked';
import pages from '../content/pages.json';
import { bySection } from '../tools/page-lists.ts';
import { promptOf } from '../tools/prompts.ts';

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

test('Recipes lists every Markdown recipe in name order', () => {
  const expected = readdirSync(resolve(site, '../recipes'), { withFileTypes: true })
    .filter(entry => entry.isFile() && entry.name.endsWith('.md'))
    .map(entry => `/recipes/${entry.name.slice(0, -3)}`)
    .sort();
  expect(pages.filter(page => page.section === 'Recipes').map(page => page.path)).toEqual(expected);
});

test("llms.txt lists the pages in the sidebar's order, under its sections, with a split guide's pages below it", async () => {
  const text = await Bun.file(resolve(site, 'dist.pages/llms.txt')).text();
  const listed = text.split('\n').filter(line => line.startsWith('## ') || /^ *- \[/.test(line))
    .map(line => line.startsWith('## ') ? line : `${/^ */.exec(line)![0]}${/\]\((?:https?:\/\/[^/)]+)?(\/[^)]*)\.md\)/.exec(line)![1]}`);
  const expected = [...new Set(pages.map(page => page.section))].flatMap(section => [
    `## ${section}`,
    ...pages.filter(page => page.section === section).map(page =>
      `${pages.find(guide => guide.source === page.source) === page ? '' : '    '}${page.path === '/' ? '/index' : page.path}`),
  ]);
  expect(listed).toEqual(expected);
});

test('a page nests below a guide only when it is one of the pages the guide was split into', () => {
  const page = (path: string, source: string) => ({ path, title: path, source, section: 'Learn' });
  const pages = [page('/guide', 'guide.md'), page('/guide/install', 'guide.md'), page('/guide/faq', 'faq.md')];
  expect(bySection(pages, p => p.path, () => '')).toBe('## Learn\n\n- [/guide](/guide)\n    - [/guide/install](/guide/install)\n- [/guide/faq](/guide/faq)');
});

test('every recipe has a Markdown prompt that names its source', async () => {
  const problems: string[] = [];
  for (const page of pages.filter(page => page.section === 'Recipes')) {
    const source = `docs${page.path}.md`;
    const markdown = await Bun.file(resolve(site, 'dist.pages', `${page.path.slice(1)}.md`)).text();
    const prompt = marked.lexer(markdown).find(token => token.type === 'code' && token.lang?.split(/\s+/, 1)[0] === 'prompt');
    if (!prompt?.text.includes(source)) problems.push(`${source} has no prompt naming its source`);
    if (prompt && /\{\{[^{}]*\}\}/.test(prompt.raw)) problems.push(`${source} has an unresolved prompt placeholder`);
  }
  expect(problems).toEqual([]);
});

test('a recipe uses its own prompt or expands both source references in the folder prompt', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-recipe-prompts-'));
  try {
    mkdirSync(join(dir, 'recipes'));
    const own = 'recipes/own.md';
    const inherited = 'recipes/new-$&.md';
    const dedicated = '```prompt Dedicated\nRead docs/recipes/own.md.\n```';
    writeFileSync(join(dir, own), dedicated);
    copyFileSync(join(site, 'prompts', 'recipes.md'), join(dir, 'recipes.md'));
    writeFileSync(join(dir, '..md'), 'A top-level document has no folder prompt.');

    const fallback = promptOf(dir, inherited);
    const prompt = marked.lexer(fallback.text).find(token => token.type === 'code' && token.lang?.split(/\s+/, 1)[0] === 'prompt');
    expect({
      dedicated: promptOf(dir, own),
      fromFolder: fallback.fromFolder,
      remoteSource: prompt?.text.includes(`https://github.com/expo/exact/blob/main/docs/${inherited}`),
      checkoutSource: prompt?.text.includes(` docs/${inherited}`),
      placeholders: prompt?.text.match(/\{\{[^{}]*\}\}/g) ?? [],
      topLevel: promptOf(dir, 'guide.md'),
    }).toEqual({
      dedicated: { text: `${dedicated}\n\n`, fromFolder: false },
      fromFolder: true,
      remoteSource: true,
      checkoutSource: true,
      placeholders: [],
      topLevel: { text: '', fromFolder: false },
    });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('a chat fetcher gets Markdown at a published page path', async () => {
  const fetcher = { Accept: 'text/html', 'User-Agent': 'Claude-User/1.0' };
  expect(await get('/learn/contract-for-humans/run-an-app', fetcher)).toEqual({ status: 200, type: 'text/markdown', location: null });
  expect(await get('/start/', fetcher)).toMatchObject({ status: 301, location: '/start' });
  expect(await get('/sitemap', fetcher)).toEqual({ status: 404, type: 'text/html', location: null });
});

test('a JSON HEAD 404 carries the Pages discovery headers', async () => {
  const response = await fetch(`${origin}/nope`, { method: 'HEAD', headers: { Accept: 'application/json' } });
  expect([response.status, response.headers.get('Content-Type'), response.headers.get('Vary'), response.headers.get('X-Llms-Txt')])
    .toEqual([404, 'application/json; charset=utf-8', 'Accept', '/llms.txt']);
  expect(response.headers.get('Link')).toContain('</llms.txt>; rel="llms-txt"');
  expect([response.headers.get('Access-Control-Allow-Origin'), response.headers.get('Cache-Control')]).toEqual(['*', 'no-store']);
  expect(await response.text()).toBe('');
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

test("a font file has a cache lifetime, so the browser does not request it again when the runtime replaces the page's sheet", async () => {
  const font = await fetch(`${origin}/assets/fonts/Inter-Regular.ttf`, { method: 'HEAD' });
  expect([font.status, font.headers.get('Cache-Control')]).toEqual([200, 'public, max-age=604800']);
});
