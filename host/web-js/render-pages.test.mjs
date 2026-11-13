import { expect, test } from 'bun:test';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { createServer } from 'node:net';
import { webCompiler } from './module.mjs';
import { buildTreeFile } from '../web/serve.mjs';

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

test('the render server preserves encoded separators before looking up an exact file', async () => {
  await builtPages(contract('productPages()'), `export const grants = '';
export function answer(name: string, args: any[]): any {
  return name === 'productPages' ? ['plain', 'plain/index.html'] : 'Product: ' + args[0];
}
`, async ({ status, output, dist }) => {
    expect(status, output).toBe(0);
    mkdirSync(resolve(dist, 'assets'), { recursive: true });
    writeFileSync(resolve(dist, 'assets/100%.txt'), 'an asset with a percent sign');
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
      for (const id of ['plain', 'plain/index.html']) {
        const response = await fetch(`http://127.0.0.1:${port}/product/${encodeURIComponent(id)}`);
        expect(response.status).toBe(200);
        const body = await response.text();
        expect(/<title>([^<]*)<\/title>/.exec(body)?.[1]).toBe(`Product: ${id}`);
        const checkpoint = JSON.parse(/<script type="application\/vnd.exact.checkpoint"[^>]*>([^<]*)<\/script>/.exec(body)[1]);
        expect(checkpoint.location).toBe('/product/' + encodeURIComponent(id));
      }
      const asset = await fetch(`http://127.0.0.1:${port}/assets/100%25.txt`);
      expect(await asset.text()).toBe('an asset with a percent sign');
      expect(buildTreeFile(dist, '/assets/100%25.txt')?.path).toBe(resolve(dist, 'assets/100%.txt'));
      // A directory of assets is named as its path decodes; only a build route's page keeps its escapes.
      mkdirSync(resolve(dist, 'deck/100%'), { recursive: true });
      writeFileSync(resolve(dist, 'deck/100%/index.html'), 'a deck');
      expect(buildTreeFile(dist, '/deck/100%25/', 'text/html')?.path).toBe(resolve(dist, 'deck/100%/index.html'));
      expect(buildTreeFile(dist, '/deck/100%25/')?.path).toBe(resolve(dist, 'deck/100%/index.html'));
      // A path below a file names no file and is routed as any other path naming nothing.
      expect(existsSync(resolve(dist, 'app.js'))).toBe(true);
      const below = await fetch(`http://127.0.0.1:${port}/app.js/x`), nothing = await fetch(`http://127.0.0.1:${port}/nothing/x`);
      expect(below.status).toBe(nothing.status);
    } finally {
      clearTimeout(timer);
      child.kill();
      await child.exited;
    }
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

test('the build rejects distinct static locations that choose the same page file', async () => {
  await builtPages(`routes nav
  home "/" render=client
  first "/a" render=build
  second "/%61" render=build

component App
  view
    text top(nav).name
`, null, ({ status, output, dist }) => {
    expect(status, output).not.toBe(0);
    expect(output).toContain('/a');
    expect(output).toContain('/%61');
    expect(output).toContain('same page file');
    expect(existsSync(resolve(dist, 'a/index.html'))).toBe(false);
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
