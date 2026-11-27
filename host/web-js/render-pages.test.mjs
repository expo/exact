import { expect, test } from 'bun:test';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createServer } from 'node:net';
import { webCompiler } from './module.mjs';
import { buildTreeFile } from '../web/serve.mjs';
import { generationOf } from '../web/generations.mjs';
import { builtFile } from './content-names.mjs';

const root = resolve(import.meta.dir, '../..');
const contract = (call, extra = '') => `routes nav
  home "/" render=client
    product "/product/:id" render=build pages=${call}
${extra}
component App
  resource product = product(top(nav).params.id) as shape string else empty()
  view
    main
      head title=product
      text product testId="product"
`;

async function builtPages(source, data, check) {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const dir = mkdtempSync(resolve(root, 'target/render-pages-test-')), dist = resolve(dir, 'dist');
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Render Pages', app: { id: 'test.render-pages', name: 'Render Pages', origin: 'https://pages.invalid' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), source);
    if (data !== null) writeFileSync(resolve(dir, 'app.ts'), data);
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'render-pages-test', '--out', dist], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    await check({ status, output: stdout + stderr, dist });
  } finally { rmSync(dir, { recursive: true, force: true }); }
}

test('the JS build enumerates its only build route with the source arguments and renders encoded parameters', async () => {
  const samples = [
    ['plain', 'plain'], ['plain/', 'plain%2F'], ['plain/index.html', 'plain%2Findex.html'],
    ['two words', 'two words'], ['café', 'café'],
    ['a/b?c#d%', 'a%2Fb%3Fc#d%25'], ["a!b'c(d)*~", "a!b'c(d)%2A~"], ['%2F', '%252F'],
    ['a/b', 'a%2Fb'], ['a%2Fb', 'a%252Fb'], ['last.', 'last%2E'], ['last ', 'last%20'],
  ];
  const ids = samples.map(([id]) => id);
  await builtPages(contract('productPages("public", 2, true)'), `
export const grants = '';
let calls = 0;
export async function answer(name: string, args: any[], store: any): Promise<any> {
  if (name === 'productPages') {
    if (++calls !== 1 || JSON.stringify(args) !== '["public",2,true]') throw new Error('wrong enumeration call');
    if (store.get('token') != null) throw new Error('enumeration read a secret');
    return ${JSON.stringify(ids)};
  }
  if (name === 'product' && args[0]) return 'Product: ' + args[0];
  throw new Error('unknown source: ' + name);
}
`, ({ status, output, dist }) => {
    expect(status, output).toBe(0);
    expect(output).toContain('no render-pages-test-render entry');
    const sitemap = readFileSync(resolve(dist, 'sitemap.xml'), 'utf8');
    for (const [id, directory] of samples) {
      const file = resolve(dist, 'product', directory, 'index.html');
      const page = readFileSync(file, 'utf8');
      expect(page).toContain(`<title>Product: ${id}</title>`);
      expect(/data-testid="product"[^>]*>([^<]*)</.exec(page)?.[1]).toBe(`Product: ${id}`);
      const checkpoint = JSON.parse(/<script type="application\/vnd.exact.checkpoint"[^>]*>([^<]*)<\/script>/.exec(page)[1]);
      expect(checkpoint.location).toBe('/product/' + encodeURIComponent(id));
      expect(buildTreeFile(dist, checkpoint.location, 'text/html')?.path).toBe(file);
      expect(sitemap).toContain('/product/' + encodeURIComponent(id));
    }
    // A value that decodes to another value's page file is not that page.
    expect(buildTreeFile(dist, '/product/' + encodeURIComponent('%252F'), 'text/html')?.path).not.toBe(resolve(dist, 'product/%252F/index.html'));
  });
}, 120000);

// Runs `check` with a fetch against `render.mjs --serve` on `dist`, and stops the server after.
async function renderServer(dist, check) {
  const probe = createServer();
  await new Promise(ok => probe.listen(0, '127.0.0.1', ok));
  const port = probe.address().port;
  await new Promise(ok => probe.close(ok));
  const child = Bun.spawn([process.execPath, 'host/web-js/render.mjs', dist, '--serve', '--port', String(port)], {
    cwd: root, stdout: 'pipe', stderr: 'pipe',
  });
  const errors = new Response(child.stderr).text();
  let timer;
  try {
    const ready = (async () => {
      let said = '';
      for await (const bytes of child.stdout) {
        said += new TextDecoder().decode(bytes);
        if (said.includes('serving ')) return;
      }
      throw new Error(`render server exited: ${await errors}`);
    })();
    await Promise.race([ready, new Promise((_, no) => {
      timer = setTimeout(() => no(new Error('render server did not start')), 30000);
    })]);
    clearTimeout(timer);
    await check((path, headers) => fetch(`http://127.0.0.1:${port}${path}`, { headers }));
  } finally {
    clearTimeout(timer);
    child.kill();
    await child.exited;
  }
}

test('the render server preserves encoded separators before looking up an exact file', async () => {
  await builtPages(contract('productPages()'), `export const grants = '';
export function answer(name: string, args: any[]): any {
  return name === 'productPages' ? ['plain', 'plain/index.html'] : 'Product: ' + args[0];
}
`, async ({ status, output, dist }) => {
    expect(status, output).toBe(0);
    mkdirSync(resolve(dist, 'assets'), { recursive: true });
    writeFileSync(resolve(dist, 'assets/100%.txt'), 'an asset with a percent sign');
    await renderServer(dist, async get => {
      for (const id of ['plain', 'plain/index.html']) {
        const response = await get(`/product/${encodeURIComponent(id)}`);
        expect(response.status).toBe(200);
        const body = await response.text();
        expect(/<title>([^<]*)<\/title>/.exec(body)?.[1]).toBe(`Product: ${id}`);
        const checkpoint = JSON.parse(/<script type="application\/vnd.exact.checkpoint"[^>]*>([^<]*)<\/script>/.exec(body)[1]);
        expect(checkpoint.location).toBe('/product/' + encodeURIComponent(id));
      }
      const asset = await get('/assets/100%25.txt');
      expect(await asset.text()).toBe('an asset with a percent sign');
      expect(buildTreeFile(dist, '/assets/100%25.txt')?.path).toBe(resolve(dist, 'assets/100%.txt'));
      // A directory of assets is named as its path decodes; only a build route's page keeps its escapes.
      mkdirSync(resolve(dist, 'deck/100%'), { recursive: true });
      writeFileSync(resolve(dist, 'deck/100%/index.html'), 'a deck');
      expect(buildTreeFile(dist, '/deck/100%25/', 'text/html')?.path).toBe(resolve(dist, 'deck/100%/index.html'));
      expect(buildTreeFile(dist, '/deck/100%25/')?.path).toBe(resolve(dist, 'deck/100%/index.html'));
      // A path below a file names no file and is routed as any other path naming nothing.
      expect(existsSync(resolve(dist, builtFile(dist, 'app.js')))).toBe(true);
      const below = await get(`/${builtFile(dist, 'app.js')}/x`), nothing = await get('/nothing/x');
      expect(below.status).toBe(nothing.status);
      // The crawler files the build wrote are served as files.
      const robots = await get('/robots.txt');
      expect(robots.headers.get('content-type')).toContain('text/plain');
      expect(await robots.text()).toBe(readFileSync(resolve(dist, 'robots.txt'), 'utf8'));
    });
  });
}, 120000);

test('routes declared at the crawler files\' paths keep them, built and served as pages', async () => {
  await builtPages(`routes nav
  home "/" render=build
  robots "/robots.txt" render=build
  sitemap "/sitemap.xml" render=build
  help "/robots.txt/help" render=request

component App
  view
    text \`Authored \${top(nav).name}\`
`, null, async ({ status, output, dist }) => {
    expect(status, output).toBe(0);
    await renderServer(dist, async get => {
      for (const path of ['/robots.txt', '/sitemap.xml']) {
        const html = readFileSync(resolve(dist, '.' + path, 'index.html'), 'utf8');
        expect(html).toContain(`Authored ${path.slice(1, -4)}`);
        // Compressed or not: a dist file is compressed in the server's static lookup, which a directory must not reach.
        for (const encoding of ['identity', 'br']) {
          const response = await get(path, { 'accept-encoding': encoding });
          expect(response.status, `${path} ${encoding}`).toBe(200);
          expect(response.headers.get('content-type')).toContain('text/html');
          expect(await response.text()).toBe(html);
        }
        expect(buildTreeFile(dist, path, 'text/html')?.path).toBe(resolve(dist, '.' + path, 'index.html'));
      }
      expect(await (await get('/robots.txt/help')).text()).toContain('Authored help');
    });
  });
}, 120000);

test('the JS build renders hidden static paths and parameter values', async () => {
  await builtPages(contract('productPages()', '  security "/.well-known/security.txt" render=build\n'), `export const grants = '';
export function answer(name: string, args: any[]): any {
  return name === 'productPages' ? ['.draft'] : 'Product: ' + args[0];
}
`, ({ status, output, dist }) => {
    expect(status, output).toBe(0);
    expect(readFileSync(resolve(dist, '.well-known/security.txt/index.html'), 'utf8')).toContain('data-testid="product"');
    expect(readFileSync(resolve(dist, 'product/.draft/index.html'), 'utf8')).toContain('<title>Product: .draft</title>');
    for (const location of ['/.well-known/security.txt', '/product/.draft']) {
      expect(buildTreeFile(dist, location, 'text/html')?.path).toBe(resolve(dist, '.' + location, 'index.html'));
    }
  });
}, 120000);

test.each(['../', 'foo/..', 'a/./b'])('the build rejects a traversal parameter %s before writing any page', async value => {
  await builtPages(contract('productPages()'), `export const grants = '';
export function answer(name: string, args: any[]): any {
  return name === 'productPages' ? ['plain', ${JSON.stringify(value)}] : 'Product: ' + args[0];
}
`, ({ status, output, dist }) => {
    expect(status, output).not.toBe(0);
    expect(output).toContain('product');
    expect(output).toContain(JSON.stringify(value));
    expect(readFileSync(resolve(dist, 'index.html'), 'utf8')).toBe(readFileSync(resolve(dist, 'shell.html'), 'utf8'));
    expect(existsSync(resolve(dist, 'product/plain/index.html'))).toBe(false);
  });
}, 120000);

// Two such spellings would choose one page file; `buildPageFiles` refuses that too (crawl.test.mjs).
test('the build refuses a static location that escapes a character a URL carries as itself', async () => {
  await builtPages(`routes nav
  home "/" render=client
  first "/a" render=build
  second "/%61" render=build

component App
  view
    text top(nav).name
`, null, ({ status, output, dist }) => {
    expect(status, output).not.toBe(0);
    expect(output).toContain('`%61` escapes `a`');
    expect(existsSync(resolve(dist, 'a/index.html'))).toBe(false);
  });
}, 120000);

// The build makes no page whose render drew randomness, a listed page's or the not-found page's included, and names each
// such route once every page has rendered (LLP 1048.000 D7). The runtime's own facts, which an inline time asks for, draw
// nothing.
test('the build makes no page whose render drew randomness, and names each such route', async () => {
  await builtPages(`routes nav
  home "/" render=build
    product "/product/:id" render=build pages=productPages()
  notfound render=build

component App
  resource note = note(top(nav).name) as shape string
  view
    main
      text note
      text "Updated "
        time datetime=1792209600000 format="date-medium"
`, `export const grants = '';
export function answer(name: string, args: any[]): any {
  if (name === 'productPages') return ['a'];
  if (args[0] !== 'home') crypto.getRandomValues(new Uint8Array(1));
  return 'a note';
}
`, ({ status, output, dist }) => {
    const refusal = route => `route \`${route}\` is render=build, but its render drew randomness, and a page made at build would give every reader the same random values (LLP 1069.005 D2). Declare the route render=request, rendered for each request, or render=client, rendered in the browser`;
    expect([status, output.split('\n').filter(line => line.includes('drew randomness'))], output)
      .toEqual([1, [`/product/a: ${refusal('product')}`, `/404.html: ${refusal('notfound')}`]]);
    expect([readFileSync(resolve(dist, 'index.html'), 'utf8').includes('a note'), existsSync(resolve(dist, 'product/a/index.html')), existsSync(resolve(dist, '404.html'))])
      .toEqual([true, false, false]);
  });
}, 120000);

const failures = [
  ["throw new Error('catalog unavailable')", 'catalog unavailable'],
  ["return Promise.reject(new Error('catalog rejected'))", 'catalog rejected'],
  ["return 'not a list'", 'shape'],
  ['return [1]', 'shape'],
  ...['', '.', '..'].map(value => [`return [${JSON.stringify(value)}]`, 'a path parameter cannot be empty']),
  ['return new Promise(() => {})', "it didn't answer before the deadline"],
];
test.each(failures)('a pages source that executes %s fails the build with its route and source', async (body, message) => {
  await builtPages(contract('productPages()'), `export const grants = '';
export function answer(name: string): any {
  if (name === 'productPages') { ${body}; }
  return '';
}
`, ({ status, output }) => {
    expect(status, output).not.toBe(0);
    expect(output).toContain("route `product`'s pages (productPages)");
    expect(output).toContain(message);
  });
}, 120000);

// A listed value is its route's page only where the router gives the location back to that route (LLP 1048.000 D2), and
// the not-found document renders at /404.html, which a pattern that matches it would make its own page (D7). The build
// names each value a route declared earlier takes, each the pattern can't hold, and the not-found document's clash,
// renders the rest and then fails.
test('a build names each listed value an earlier route answers and the not-found clash, renders the rest, and fails', async () => {
  await builtPages(`routes nav
  home "/" render=client
    special "/product/special" render=request
    product "/product/:id" render=build pages=productPages()
    fresh "/story/new" render=request
    story "/story/:id" render=build pages=storyPages()
    post "/:slug" render=request
  notfound render=build

component App
  view
    main
      text "a page"
`, `export const grants = '';
export function answer(name: string): any {
  if (name === 'productPages') return ['7', 'special', ''];
  if (name === 'storyPages') return ['new', '1'];
  return '';
}
`, ({ status, output, dist }) => {
    const refusal = (route, source, value, location, other) => `route \`${route}\`'s pages (${source}): the value "${value}" is at ${location}, which route \`${other}\`, declared before \`${route}\`, answers, so the page there is \`${other}\`'s, not \`${route}\`'s. Leave the value out of the list, or give one of the two routes another path`;
    expect([status, output.split('\n').filter(line => line.includes('declared before'))], output).toEqual([1, [
      refusal('product', 'productPages', 'special', '/product/special', 'special'),
      refusal('story', 'storyPages', 'new', '/story/new', 'fresh'),
    ]]);
    // A value the pattern can't hold, and the not-found document's clash, are reported in the same run
    expect(output).toContain("route `product`'s pages (productPages): the value \"\" can't be a path segment: a path parameter cannot be empty, `.` or `..`. Leave the value out of the list");
    expect(output).toContain("the not-found document renders at /404.html, which route `post` matches, so it would be that route's page. Give `post` a pattern that doesn't match /404.html, or declare the not-found route render=request or render=client");
    expect([readFileSync(resolve(dist, 'product/7/index.html'), 'utf8').includes('a page'), existsSync(resolve(dist, 'story/1/index.html')), existsSync(resolve(dist, 'product/special')), existsSync(resolve(dist, '404.html'))])
      .toEqual([true, true, false, false]);
  });
}, 120000);

test('an empty enumeration keeps the static and not-found build pages', async () => {
  await builtPages(contract('productPages()', '  about "/about" render=build\n  notfound render=build\n'), `export const grants = '';
export function answer(name: string): any { return name === 'productPages' ? [] : 'Static page'; }
`, ({ status, output, dist }) => {
    expect(status, output).toBe(0);
    expect(existsSync(resolve(dist, 'product'))).toBe(false);
    expect(readFileSync(resolve(dist, 'about/index.html'), 'utf8')).toContain('<title>Static page</title>');
    expect(readFileSync(resolve(dist, '404.html'), 'utf8')).toContain('Static page');
    expect(readFileSync(resolve(dist, 'sitemap.xml'), 'utf8')).toContain('https://pages.invalid/about');
  });
}, 120000);

test('a declared pages source enumerates without booting the app or loading a data module', async () => {
  const seen = [];
  const api = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
    seen.push(new URL(req.url).pathname + new URL(req.url).search);
    return Response.json(['declared page']);
  } });
  try {
    await builtPages(`routes nav
  home "/" render=client
    product "/product/:id" render=build pages=productPages("public")

source productPages(group: string) as shape string[]
  url "http://127.0.0.1:${api.port}/pages?group={group}"

component App
  view
    main
      text top(nav).params.id
`, null, ({ status, output, dist }) => {
      expect(status, output).toBe(0);
      expect(seen).toEqual(['/pages?group=public']);
      expect(readFileSync(resolve(dist, 'product/declared page/index.html'), 'utf8')).toContain('>declared page</');
    });
  } finally { api.stop(true); }
}, 120000);

test('the render server sends a navigation the head of a request page before its data settles, based on its generation', async () => {
  const source = `routes nav
  home "/" render=request

component App
  resource greeting = greeting() as shape string else empty()
  view
    main
      text greeting testId="greeting"
`;
  // The data source's answer comes from a server that holds it for 600 ms: a render waits for a fetch.
  const slow = Bun.serve({ port: 0, hostname: '127.0.0.1', fetch: () => new Promise(done => setTimeout(() => done(new Response('Hello')), 600)) });
  await builtPages(source, `export const grants = 'net.fetch http://127.0.0.1:${slow.port}';
export async function answer(name: string): Promise<any> {
  return await (await fetch('http://127.0.0.1:${slow.port}/')).text();
}
`, async ({ status, output, dist }) => {
    expect(status, output).toBe(0);
    // The receipt `web-build` writes, which names the build's generation.
    writeFileSync(resolve(dist, '.exact-build.json'), JSON.stringify({ exactBuild: 1, target: 'js', files: [] }));
    const id = generationOf(dist);
    expect(id).toMatch(/^[0-9a-f]{32}$/);
    const probe = createServer();
    await new Promise(ok => probe.listen(0, '127.0.0.1', ok));
    const port = probe.address().port;
    await new Promise(ok => probe.close(ok));
    const child = Bun.spawn([process.execPath, 'host/web-js/render.mjs', dist, '--serve', '--port', String(port)], { cwd: root, stdout: 'pipe', stderr: 'pipe' });
    try {
      for await (const bytes of child.stdout) if (new TextDecoder().decode(bytes).includes('serving ')) break;
      for (const encoding of ['identity', 'br']) {
        const response = await fetch(`http://127.0.0.1:${port}/`, { headers: { accept: 'text/html', 'sec-fetch-dest': 'document', 'accept-encoding': encoding } });
        expect(response.status).toBe(200);
        const reader = response.body.getReader(), decoder = new TextDecoder();
        // The first part, before the data source answers: the head, its base the generation's, and no greeting.
        const first = decoder.decode((await reader.read()).value, { stream: true });
        expect(first).toContain(`<base href="/.exact/gen/${id}/">`);
        expect(first).not.toContain('Hello');
        let page = first;
        for (let part; !(part = await reader.read()).done;) page += decoder.decode(part.value, { stream: true });
        expect(/data-testid="greeting"[^>]*>([^<]*)</.exec(page)?.[1]).toBe('Hello');
      }
    } finally { child.kill(); await child.exited; }
  }).finally(() => slow.stop(true));
}, 120000);

test('build-time prerender succeeds without a CSS global for bound line-height, translate, scale and url() refs', async () => {
  const source = `routes nav
  home "/" render=build

component App
  state leading = "24px"
  state calcLh = "calc(12px + 12px)"
  state clampLh = "clamp(20px, 4vw, 32px)"
  state minLh = "min(24px, 30px)"
  state remCalcLh = "calc(1rem + 4px)"
  state commentCalcLh = "calc(1px/**/ + 1px)"
  state escapeCalcLh = "calc(1\\\\70 x + 1px)"
  state lhUnitCalc = "calc(1lh + 4px)"
  state chUnitCalc = "calc(1ch + 4px)"
  state clampNoneLh = "clamp(none, 24px, 32px)"
  state eofCalcLh = "calc((20px"
  state eofMinLh = "min(20px, 30px"
  state eofClampNoneLh = "clamp(none, 24px, none"
  state badNbspCalcLh = "\\u{00a0}calc(20px)"
  state badBomCalcLh = "\\u{feff}calc(20px)"
  state clampNonePos = "clamp(none, 10px, 20px) 5px"
  state badPercent = "150%"
  state badComment = "1.5/**/"
  state badUpperPx = "24PX"
  state badInitial = "initial"
  state badNormalComment = "normal/**/"
  state badNumCalcLh = "calc(1 + 0.5)"
  state badPctCalcLh = "calc(100% + 4px)"
  state badEmCalcLh = "calc(1.5em + 4px)"
  state badPiEmCalcLh = "calc(pi * 1em)"
  state badClampNoneEm = "clamp(none, 1em, none)"
  state badSyntaxCalcLh = "calc(10px +)"
  state badEnvCalcLh = "calc(10px + env(safe-area-inset-top))"
  state badVarCalcLh = "calc(10px + var(--x))"
  state badTooManyLh = "calc(${Array(33).fill('1px').join(' + ')})"
  state badTooDeepLh = "calc(${'('.repeat(32)}1px${')'.repeat(32)})"
  state pos = "10px 20px"
  state mathPos = "calc(10px + 20px) 5px"
  state adjPos = "calc(10px)calc(20px)"
  state viPos = "10vi 5px"
  state badPos = "10px 20px 30px 40px"
  state badCalcPos = "calc(bogus) 20px"
  state zoom = "1.25 0.5"
  state mathZoom = "calc(1 + 1)"
  state sqrtZoom = "sqrt(4)"
  state badZoom = "2 3 4 5"
  state offset = "4px 6px"
  state badOffset = "none 2px"
  state filtered = true
  action noop
    let x = 1
  view
    main
      svg width=20 height=20
        defs
          filter id="mask-box"
            feGaussianBlur stdDeviation=1
        rect width=10 height=10 filter=(filtered ? "url(#mask-box)" : "none") testId="filtered"
      text "ok" testId="ok" line-height=leading
      text "calc-lh" testId="calc-lh" line-height=calcLh
      text "clamp-lh" testId="clamp-lh" line-height=clampLh
      text "min-lh" testId="min-lh" line-height=minLh
      text "rem-calc-lh" testId="rem-calc-lh" line-height=remCalcLh
      text "comment-calc-lh" testId="comment-calc-lh" line-height=commentCalcLh
      text "escape-calc-lh" testId="escape-calc-lh" line-height=escapeCalcLh
      text "lh-unit-calc" testId="lh-unit-calc" line-height=lhUnitCalc
      text "ch-unit-calc" testId="ch-unit-calc" line-height=chUnitCalc
      text "clamp-none-lh" testId="clamp-none-lh" line-height=clampNoneLh
      text "eof-calc-lh" testId="eof-calc-lh" line-height=eofCalcLh
      text "eof-min-lh" testId="eof-min-lh" line-height=eofMinLh
      text "eof-clamp-none-lh" testId="eof-clamp-none-lh" line-height=eofClampNoneLh
      text "bad-nbsp-calc-lh" testId="bad-nbsp-calc-lh" line-height=badNbspCalcLh
      text "bad-bom-calc-lh" testId="bad-bom-calc-lh" line-height=badBomCalcLh
      view press=noop testId="clamp-none-moved" translate=clampNonePos scale=zoom
        :active
          translate = offset
          -exact-press-scale = 0.96
      text "bad-percent" testId="bad-percent" line-height=badPercent
      text "bad-comment" testId="bad-comment" line-height=badComment
      text "bad-upper" testId="bad-upper" line-height=badUpperPx
      text "bad-initial" testId="bad-initial" line-height=badInitial
      text "bad-normal-comment" testId="bad-normal-comment" line-height=badNormalComment
      text "bad-num-calc-lh" testId="bad-num-calc-lh" line-height=badNumCalcLh
      text "bad-pct-calc-lh" testId="bad-pct-calc-lh" line-height=badPctCalcLh
      text "bad-em-calc-lh" testId="bad-em-calc-lh" line-height=badEmCalcLh
      text "bad-pi-em-calc-lh" testId="bad-pi-em-calc-lh" line-height=badPiEmCalcLh
      text "bad-clamp-none-em" testId="bad-clamp-none-em" line-height=badClampNoneEm
      text "bad-syntax-calc-lh" testId="bad-syntax-calc-lh" line-height=badSyntaxCalcLh
      text "bad-env-calc-lh" testId="bad-env-calc-lh" line-height=badEnvCalcLh
      text "bad-var-calc-lh" testId="bad-var-calc-lh" line-height=badVarCalcLh
      text "bad-too-many-lh" testId="bad-too-many-lh" line-height=badTooManyLh
      text "bad-too-deep-lh" testId="bad-too-deep-lh" line-height=badTooDeepLh
      view press=noop testId="moved" translate=pos scale=zoom
        :active
          translate = offset
          -exact-press-scale = 0.96
      view press=noop testId="math-moved" translate=mathPos scale=mathZoom
        :active
          translate = offset
          -exact-press-scale = 0.96
      view press=noop testId="adj-moved" translate=adjPos scale=sqrtZoom
        :active
          translate = offset
          -exact-press-scale = 0.96
      view press=noop testId="vi-moved" translate=viPos scale=zoom
        :active
          translate = offset
          -exact-press-scale = 0.96
      view press=noop testId="invalid" translate=badPos scale=badZoom
        :active
          translate = badOffset
          -exact-press-scale = 0.96
      view press=noop testId="invalid-calc" translate=badCalcPos scale=badZoom
        :active
          translate = badOffset
          -exact-press-scale = 0.96
`;
  await builtPages(source, null, ({ status, output, dist }) => {
    expect(status, output).toBe(0);
    const page = readFileSync(resolve(dist, 'index.html'), 'utf8');
    const styleOf = (id) => new RegExp(`data-testid="${id}"[^>]*style="([^"]*)"`).exec(page)?.[1] ?? '';
    expect(styleOf('ok')).toContain('line-height:24px');
    expect(styleOf('calc-lh')).toContain('line-height:calc(12px + 12px)');
    expect(styleOf('clamp-lh')).toContain('line-height:clamp(20px, 4vw, 32px)');
    expect(styleOf('min-lh')).toContain('line-height:min(24px, 30px)');
    expect(styleOf('rem-calc-lh')).toContain('line-height:calc(1rem + 4px)');
    expect(styleOf('comment-calc-lh')).toContain('line-height:calc(1px + 1px)');
    expect(styleOf('escape-calc-lh')).toContain('line-height:calc(1px + 1px)');
    expect(styleOf('lh-unit-calc')).toContain('line-height:calc(16px + 4px)');
    expect(styleOf('ch-unit-calc')).toContain('line-height:calc(8px + 4px)');
    expect(styleOf('clamp-none-lh')).toContain('line-height:clamp(none, 24px, 32px)');
    expect(styleOf('eof-calc-lh')).toContain('line-height:calc((20px))');
    expect(styleOf('eof-min-lh')).toContain('line-height:min(20px, 30px)');
    expect(styleOf('eof-clamp-none-lh')).toContain('line-height:clamp(none, 24px, none)');
    expect(styleOf('clamp-none-moved')).toContain('--exact-translate-x:clamp(none, 10px, 20px)');
    for (const id of [
      'bad-percent', 'bad-comment', 'bad-upper', 'bad-initial', 'bad-normal-comment',
      'bad-num-calc-lh', 'bad-pct-calc-lh', 'bad-em-calc-lh', 'bad-pi-em-calc-lh',
      'bad-clamp-none-em', 'bad-syntax-calc-lh', 'bad-env-calc-lh', 'bad-var-calc-lh',
      'bad-too-many-lh', 'bad-too-deep-lh', 'bad-nbsp-calc-lh', 'bad-bom-calc-lh',
    ]) {
      expect(styleOf(id), id).toContain('line-height:unset');
    }
    expect(styleOf('moved')).toContain('--exact-translate-x:10px');
    expect(styleOf('moved')).toContain('--exact-translate-y:20px');
    expect(styleOf('moved')).toContain('--exact-base-translated:1');
    expect(styleOf('moved')).toContain('--exact-scale:1.25');
    expect(styleOf('moved')).toContain('--exact-scale-y:0.5');
    expect(styleOf('moved')).toContain('--exact-press-translate:4px 6px');
    expect(styleOf('math-moved')).toContain('--exact-translate-x:calc(10px + 20px)');
    expect(styleOf('math-moved')).toContain('--exact-translate-y:5px');
    expect(styleOf('math-moved')).toContain('--exact-base-translated:1');
    expect(styleOf('math-moved')).toContain('--exact-scale:calc(1 + 1)');
    expect(styleOf('math-moved')).toContain('--exact-scale-y:calc(1 + 1)');
    expect(styleOf('adj-moved')).toContain('--exact-translate-x:calc(10px)');
    expect(styleOf('adj-moved')).toContain('--exact-translate-y:calc(20px)');
    expect(styleOf('adj-moved')).toContain('--exact-base-translated:1');
    expect(styleOf('adj-moved')).toContain('--exact-scale:sqrt(4)');
    expect(styleOf('vi-moved')).toContain('--exact-translate-x:10vi');
    expect(styleOf('vi-moved')).toContain('--exact-translate-y:5px');
    expect(styleOf('vi-moved')).toContain('--exact-base-translated:1');
    for (const id of ['invalid', 'invalid-calc']) {
      expect(styleOf(id), id).toContain('--exact-translate-x:0px');
      expect(styleOf(id), id).toContain('--exact-translate-y:0px');
      expect(styleOf(id), id).not.toContain('--exact-base-translated:1');
      expect(styleOf(id), id).toContain('--exact-scale:1');
      expect(styleOf(id), id).toContain('--exact-scale-y:1');
      expect(styleOf(id), id).not.toContain('--exact-press-translate:');
    }
    const filterId = /<filter[^>]*id="([^"]+)"[^>]*data-exact-id="mask-box"|<filter[^>]*data-exact-id="mask-box"[^>]*id="([^"]+)"/.exec(page);
    const generated = filterId?.[1] ?? filterId?.[2];
    expect(generated).toBeTruthy();
    expect(styleOf('filtered')).toContain(`filter:url(#${generated})`);
  });
}, 120000);
