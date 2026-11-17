// A production page that admits nothing gets the empty set's answers in place of the grant parser (LLP 1047.002 D3,
// host/web-js/build.mjs `emptyGrantAdmission`): the stub answers as the parser does for the empty set, exports what the
// admission module exports, and a grant, a development build or a module brings the parser back.
import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { webCompiler } from './module.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { builtFile } from './content-names.mjs';

const root = resolve(import.meta.dir, '../..');
// The parser's validation refusal, the driver's fault table, a fetch option's refusal, the asset path's pattern and
// an upload's refusal: text only admission, faults.js, ts-fetch.js's request paths, admission.js's asset check and
// uploads.js carry.
const PARSER = 'the grant set was not validated', FAULTS = '__exactFaults', REQUESTS = 'exactLongPoll asks again', ASSETS = '\\/assets\\/', UPLOADS = 'already sent: the upload';

test.each([false, true])('a production page with no grants, module or page module admits nothing without the parser (fetch: %s)', async usesFetch => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/web-js-empty-admission-'));
  let browser, server;
  try {
    mkdirSync(resolve(dir, 'assets'));
    writeFileSync(resolve(dir, 'assets/a.txt'), 'asset loaded');
    const app = (grants) => {
      writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Empty admission', id: 'test.empty-admission', app: { id: 'test.empty-admission', name: 'Empty admission' } }));
      writeFileSync(resolve(dir, 'app.contract'), 'shape Greeting\n  text: string\ncomponent App\n  resource greeting = greeting() as shape Greeting\n  view\n    text greeting.text testId="greeting"\n');
      writeFileSync(resolve(dir, 'app.ts'), `import type { Answer } from './app.contract.d.ts';
export const grants = ${JSON.stringify(grants)};
export const answer: Answer = ${usesFetch ? `async () => {
  // An asset's body is read in the answer's turn, which may draw randomness (LLP 1069.005 D3).
  const asset = await (await fetch('/assets/a.txt')).text().then(text => text + ' ' + crypto.randomUUID().length);
  try { await fetch('http://127.0.0.1:1/ungranted'); return { text: 'unexpected access' }; }
  catch (error) { return { text: asset + ': ' + String(error) }; }
}` : "() => ({ text: 'hi' })"};
`);
    };
    const build = (out, args) => {
      const result = spawnSync(process.execPath, ['host/web-js/build.mjs', 'empty-admission', '--out', out, '--render', usesFetch ? 'js' : 'none', ...args], { cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, encoding: 'utf8' });
      expect(result.status, result.stdout + result.stderr).toBe(0);
      return { admission: readFileSync(resolve(out, '.gen/grant-admission.js'), 'utf8'), entry: readFileSync(resolve(out, builtFile(out, 'app.js')), 'utf8'), module: () => import(pathToFileURL(resolve(out, '.gen/grant-admission.js')).href) };
    };
    app('');
    const production = build(resolve(dir, 'production'), ['--production']), development = build(resolve(dir, 'development'), []);
    expect(production.admission.startsWith('// Generated: the grant admission of a page that admits nothing')).toBe(true);
    expect(production.entry).not.toContain(PARSER);
    expect(production.entry).not.toContain(FAULTS);
    // Its sources' `fetch` is the asset path and the refusal (ts-fetch-refused.js), so the request paths are not linked.
    expect(existsSync(resolve(dir, 'production/.gen/ts-fetch-refused.js'))).toBe(true);
    expect(production.entry).not.toContain(REQUESTS);
    // Nor uploads, which only a stream makes, in either mode (LLP 1047.003 D5).
    expect(production.entry).not.toContain(UPLOADS);
    expect(development.entry).not.toContain(UPLOADS);
    expect(production.entry).toContain(ASSETS);
    // Development links the parser and the request paths (LLP 1047 D7).
    expect(development.admission).toContain('function validateGrantSet');
    expect(development.entry).toContain(PARSER);
    expect(development.entry).toContain(REQUESTS);
    // Parity for the empty set: the stub and the parser answer the same calls the same way.
    const stub = await production.module(), parser = await development.module();
    expect(Object.keys(stub).sort()).toEqual(Object.keys(parser).sort());
    const empty = parser.createGrantSet(JSON.parse(production.admission.match(/\.\.\.(\{.*?\}), entries:/)[1]));
    expect(parser.grantError(empty)).toBeNull();
    for (const [name, args] of [
      ['admitsNetwork', [empty, 'https://api.example/x', 'fetch']], ['admittingGrant', [empty, 'https://api.example/x', 'fetch']],
      ['admitsSecret', [empty, 'token']], ['coversPath', [empty, 'fs-read', 'app:/data/x']], ['hasGrant', [empty, 'net-fetch']],
      ['trimGrant', ['\u0085auth.session https://login.example\u0085']], ['trimGrant', ['\ufeffauth.session']],
      ['grantWords', ['secret.keep\u0085token user-presence optional']],
      ['grantError', [empty]], ['rawGrantText', [empty]], ['sameGrantDeclaration', [empty, 'app.ts']], ['sameGrantDeclaration', [empty, 42]],
    ]) expect(stub[name](...args), name).toEqual(parser[name](...args));
    for (const name of ['createGrantSet', 'scopedGrantSet', 'unionGrantSets']) expect(stub[name](empty, 'app.ts'), name).toEqual(empty);
    // A grant brings the parser back.
    app('net.fetch https://api.example');
    const granted = build(resolve(dir, 'granted'), ['--production']);
    expect(granted.admission).toContain('function validateGrantSet');
    expect(granted.entry).toContain(PARSER);
    expect(granted.entry).toContain(REQUESTS);
    expect(existsSync(resolve(dir, 'granted/.gen/ts-fetch-refused.js'))).toBe(false);
    if (!usesFetch) return;
    let ungranted = 0;
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
      const path = new URL(req.url).pathname;
      return new Response(Bun.file(resolve(dir, 'production', path === '/' ? 'index.html' : path.slice(1))));
    } });
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.route('**/ungranted', route => { ungranted++; return route.abort(); });
    await page.goto(server.url.href);
    await page.waitForFunction(() => document.querySelector('[data-testid=greeting]')?.textContent.includes('asset loaded'));
    expect(await page.getByTestId('greeting').textContent()).toBe("asset loaded 36: FetchError: outside the app's grants (net.fetch)");
    expect(ungranted).toBe(0);
  } finally {
    await browser?.close();
    server?.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);

// A page has the stream reader only where a module app.ts reaches spells `exactStream` (LLP 1047.003 D5), the same in
// both modes. Without it, `fetch` refuses the option by name. With it, a page with no network grant links the whole
// `fetch` in production too, so a stream is refused as development refuses it: its mapper sees the refusal.
test('a page has the stream reader only where a module spells exactStream, and refuses a stream the same in both modes', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/web-js-stream-names-')), servers = [];
  let browser;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Stream names', id: 'test.stream-names', app: { id: 'test.stream-names', name: 'Stream names' } }));
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  resource said = said() as shape string\n  view\n    text said testId="said"\n');
    const build = (out, args) => {
      const result = spawnSync(process.execPath, ['host/web-js/build.mjs', 'stream-names', '--out', out, '--render', 'none', ...args], { cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, encoding: 'utf8' });
      expect(result.status, result.stdout + result.stderr).toBe(0);
      return { out, entry: readFileSync(resolve(out, builtFile(out, 'app.js')), 'utf8'), reader: readdirSync(out).some(f => f.startsWith('ts-stream-')) };
    };
    // What the page built in `out` says, at its own origin.
    const said = async ({ out }) => {
      const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
        const path = new URL(req.url).pathname;
        return new Response(Bun.file(resolve(out, path === '/' ? 'index.html' : path.slice(1))));
      } });
      servers.push(server);
      const page = await (browser ??= await suiteBrowser()).newPage();
      await page.goto(server.url.href);
      await page.waitForFunction(() => document.querySelector('[data-testid=said]')?.textContent, null, { timeout: 30000 });
      const text = await page.getByTestId('said').textContent();
      await page.close();
      return text;
    };
    // No grant admits the URL.
    const url = JSON.stringify('http://127.0.0.1:1/events');
    // A name made at run time.
    writeFileSync(resolve(dir, 'app.ts'), `export const grants = '';
export const answer = async () => {
  try { await fetch(${url}, { ['exact' + 'Stream']: (e: any) => e.type } as RequestInit); return 'answered'; }
  catch (error) { return String(error); }
};
`);
    for (const args of [['--production'], []]) {
      const unnamed = build(resolve(dir, `unnamed${args.length}`), args);
      expect(unnamed.reader, args.join()).toBe(false);
      expect(unnamed.entry).not.toContain('ts-stream');
      expect(unnamed.entry).not.toContain(UPLOADS);
      expect(await said(unnamed)).toStartWith('TypeError: fetch() with exactStream: no module app.ts reaches spells exactStream');
    }
    // The name spelled only in a module app.ts imports.
    writeFileSync(resolve(dir, 'open.ts'), "export const open = (url: string) => fetch(url, { exactStream: (e: any) => [e.type, e.kind, e.message].join(' ') } as RequestInit);\n");
    writeFileSync(resolve(dir, 'app.ts'), `import { open } from './open';\nexport const grants = '';\nexport const answer = (): any => open(${url});\n`);
    for (const args of [['--production'], []]) {
      const named = build(resolve(dir, `named${args.length}`), args);
      expect(named.reader, args.join()).toBe(true);
      expect(named.entry).toContain(UPLOADS);
      expect(existsSync(resolve(named.out, '.gen/ts-fetch-refused.js'))).toBe(false);
      expect(await said(named)).toBe("error Refused outside the app's grants (net.fetch)");
    }
  } finally {
    await browser?.close();
    for (const server of servers) server.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 180000);

// The TypeScript seam is a chunk loaded at the page's first ask (LLP 1047.003 D6) where only a resource asks: a page
// made at build starts from its checkpoint and never runs it, a press that asks again loads it, the shell loads it
// before its first render, and one that does not load is said once while the page keeps what it shows. A plan that
// sends keeps it in the entry.
test('a production page loads its TypeScript seam at its first ask, and a shell before its first render', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/web-js-seam-'));
  const contract = sends => `routes nav
  home "/" render=build activate=idle
  notfound render=build activate=idle

component App
  state count = 0
  resource said = greeting(count) as shape string
${sends ? '  mutation saved as shape string\n' : ''}  action follow(url: string)
    nav = go(nav, url)
  action bump
    count = count + 1
${sends ? '  action save\n    send saved = keep()\n' : ''}  view
    main navigationKey=\`\${top(nav).id}\` navigate=follow
      for e in stack(nav) key=e.id
        column navigationKey=\`\${e.id}\`
          text said testId="said"
          button "More" press=bump testId="bump"
`;
  let browser;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Seam', id: 'test.seam', app: { id: 'test.seam', name: 'Seam' } }));
    writeFileSync(resolve(dir, 'app.ts'), "export const grants = '';\nexport const answer = (name: string, args: unknown[]): any => name === 'keep' ? 'kept' : 'said ' + String(args[0]);\n");
    const build = (out, sends) => {
      writeFileSync(resolve(dir, 'app.contract'), contract(sends));
      const result = spawnSync(process.execPath, ['host/web-js/build.mjs', 'seam', '--out', out, '--production'], { cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, encoding: 'utf8' });
      expect(result.status, result.stdout + result.stderr).toBe(0);
      return { modules: Object.keys(JSON.parse(readFileSync(resolve(out, '.gen/modules.json'), 'utf8'))), chunk: readdirSync(out).find(name => /^ts-data-.*\.js$/.test(name)) };
    };
    const sending = build(resolve(dir, 'sends'), true);
    expect(sending.modules).toContain('ts-data.js');
    expect(sending.chunk).toBeUndefined();
    const out = resolve(dir, 'asks'), { modules, chunk } = build(out, false);
    expect(modules).not.toContain('ts-data.js');
    expect(chunk).toBeDefined();
    expect(readFileSync(resolve(out, 'shell.html'), 'utf8')).toContain(`<link rel="modulepreload" href="./${chunk}" data-exact-asks>`);
    expect(readFileSync(resolve(out, 'index.html'), 'utf8')).not.toContain(chunk);
    // `/` as built, or as the shell a host with one fallback page serves; `gone` drops the seam's chunk.
    const serve = (shell, gone = false) => Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const path = new URL(request.url).pathname;
      if (gone && path === `/${chunk}`) return new Response('gone', { status: 404 });
      return new Response(Bun.file(resolve(out, path === '/' ? (shell ? 'shell.html' : 'index.html') : path.slice(1))));
    } });
    browser = await suiteBrowser();
    // The seam sets `exact.moduleReaders` as it loads (ts-data.js).
    const visit = async (shell, gone, press) => {
      const server = serve(shell, gone), page = await browser.newPage();
      try {
        await page.goto(server.url.href);
        await page.waitForFunction(() => globalThis.exact?.ready && document.querySelector('[data-testid=said]')?.textContent);
        const before = { said: await page.getByTestId('said').textContent(), loaded: await page.evaluate(() => typeof exact.moduleReaders === 'function') };
        if (!press) return before;
        await page.getByTestId('bump').click();
        await page.waitForFunction(() => document.querySelector('[data-testid=said]').textContent !== 'said 0' || exact.journal.some(l => l.includes('did not load')));
        const journal = await page.evaluate(() => exact.journal.filter(l => l.includes('did not load')).map(l => l.replace(/^t=[\d.]+ /, '').replace(/: Failed .*$/, '')));
        return { before, after: { said: await page.getByTestId('said').textContent(), loaded: await page.evaluate(() => typeof exact.moduleReaders === 'function'), journal } };
      } finally { await page.close(); server.stop(true); }
    };
    expect(await visit(false, false, true)).toEqual({ before: { said: 'said 0', loaded: false }, after: { said: 'said 1', loaded: true, journal: [] } });
    expect(await visit(true, false, false)).toEqual({ said: 'said 0', loaded: true });
    expect(await visit(false, true, true)).toEqual({ before: { said: 'said 0', loaded: false },
      after: { said: 'said 0', loaded: false, journal: ['the TypeScript module did not load', 'resource said failed: the TypeScript module did not load'] } });
  } finally {
    await browser?.close();
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);

// A production page whose sources hold no `fs.read`, `fs.write` or `sqlite.open` grant keeps ts-data.js's storage
// refusals and none of what a grant reaches: the adapters, the SQLite handles and the topics a write tells the app's
// other tabs (build.mjs `storageGrants`). Each call refuses as a development page, which links everything, refuses it.
test('a production page with no storage grant refuses storage as a development page does, without the granted paths', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/web-js-storage-refused-'));
  const GRANTED = ['storage-fs.js', 'storage-sqlite.js', 'exact.topics.'], servers = [];
  let browser;
  try {
    const app = (grants) => {
      writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Storage refused', id: 'test.storage-refused', app: { id: 'test.storage-refused', name: 'Storage refused' } }));
      writeFileSync(resolve(dir, 'app.contract'), 'shape Report\n  text: string\ncomponent App\n  resource report = report() as shape Report\n  view\n    text report.text testId="report"\n');
      writeFileSync(resolve(dir, 'app.ts'), `export const grants = ${JSON.stringify(grants)};
export const answer = async (name: string, args: unknown[], store: unknown, storage: any) => {
  const calls = [
    () => storage.fs.readFile('app:/data/a.txt'), () => storage.fs.writeFile('app:/data/a.txt', 'x'),
    () => storage.fs.readdir('app:/data'), () => storage.fs.rename('app:/data/a.txt', 'app:/data/b.txt'),
    () => storage.fs.readFile('doc:/1/a.txt'), () => storage.fs.copyFile('doc:/1/a.txt', 'app:/data/a.txt'),
    () => storage.fs.writeFile('app:/data/a.txt', 'x', { expect: {} }), () => storage.fs.writeFile('app:/data/a.txt', Symbol('body')),
    () => storage.fs.compressImage('app:/a.png', 'app:/b.jpg', { maxDimension: 10, quality: 0.5 }),
    () => storage.fs.compressImage('app:/a.png', 'app:/b.jpg', {}),
    () => storage.fs.transformImage('app:/a.png', 'app:/b.png', { rotate: 90 }),
    () => storage.fs.transformImage('app:/a.png', 'app:/b.png', { flip: 'up' }),
    () => storage.sqlite.open('app:/data/notes.db'),
    () => storage.fs.watch('app:/data/a.txt'),
  ];
  const said = [];
  for (const call of calls) {
    try { said.push('answered ' + JSON.stringify(await call())); }
    catch (e: any) { said.push([e?.name, e?.kind, e?.code, e?.message].join(' | ')); }
  }
  return { text: said.join('\\n') };
};
`);
    };
    const build = (out, args) => {
      const result = spawnSync(process.execPath, ['host/web-js/build.mjs', 'storage-refused', '--out', out, '--render', 'none', ...args], { cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, encoding: 'utf8' });
      expect(result.status, result.stdout + result.stderr).toBe(0);
      return readFileSync(resolve(out, builtFile(out, 'app.js')), 'utf8');
    };
    // `net.fetch` is no storage grant.
    app('net.fetch https://api.example');
    const production = build(resolve(dir, 'production'), ['--production']);
    build(resolve(dir, 'development'), []);
    for (const text of GRANTED) expect(production).not.toContain(text);
    // Each build at its own origin's root, as a page is served.
    const serve = build => Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
      const path = new URL(req.url).pathname;
      return new Response(Bun.file(resolve(dir, build, path === '/' ? 'index.html' : path.slice(1))));
    } });
    browser = await suiteBrowser();
    const report = async build => {
      const at = serve(build);
      servers.push(at);
      const page = await browser.newPage();
      await page.goto(at.url.href);
      await page.waitForFunction(() => document.querySelector('[data-testid=report]')?.textContent.includes('\n'), null, { timeout: 30000 });
      const text = await page.getByTestId('report').textContent();
      await page.close();
      return text.split('\n');
    };
    const refused = await report('production');
    expect(refused).toEqual(await report('development'));
    expect(refused).toHaveLength(14);
    expect(refused[0]).toBe('Error | Unavailable | denied | denied: fs.readFile');
    expect(refused[12]).toStartWith('Error | Unavailable | denied | denied: sqlite.open app:/data/notes.db: no grant covers it');
    // A storage grant links them.
    app('fs.read app:/data');
    const granted = build(resolve(dir, 'granted'), ['--production']);
    for (const text of GRANTED) expect(granted).toContain(text);
  } finally {
    await browser?.close();
    for (const server of servers) server.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 180000);

// A Rust data module is called only for a source the plan asks or a canvas it draws (build.mjs `rustLinked`):
// `apps/hello`'s plan does neither, so its production page links neither the module nor the grant parser.
test('a production page whose plan asks no Rust source links neither the module nor the grant parser', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/web-js-unasked-rust-'));
  let browser, server;
  try {
    const out = resolve(dir, 'production');
    const result = spawnSync(process.execPath, ['host/web-js/build.mjs', 'hello', '--out', out, '--render', 'none', '--production'], { cwd: root, encoding: 'utf8' });
    expect(result.status, result.stdout + result.stderr).toBe(0);
    const entry = readFileSync(resolve(out, builtFile(out, 'app.js')), 'utf8');
    expect(readFileSync(resolve(out, '.gen/grant-admission.js'), 'utf8').startsWith('// Generated: the grant admission of a page that admits nothing')).toBe(true);
    expect(entry).not.toContain(PARSER);
    expect(entry).not.toContain('rust-data');
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
      const path = new URL(req.url).pathname;
      return new Response(Bun.file(resolve(out, path === '/' ? 'index.html' : path.slice(1))));
    } });
    browser = await suiteBrowser();
    const page = await browser.newPage(), errors = [];
    page.on('pageerror', error => errors.push(error.message));
    await page.goto(server.url.href);
    await page.waitForFunction(() => document.getElementById('exact-root')?.textContent.includes('Hello world'));
    expect(await page.evaluate(() => globalThis.exact.journal.filter(line => /Rust module/.test(line)))).toEqual([]);
    expect(errors).toEqual([]);
  } finally {
    await browser?.close();
    server?.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);
