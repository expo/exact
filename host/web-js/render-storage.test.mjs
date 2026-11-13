import { expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { open } from '../../scripts/agent.mjs';
import { webCompiler } from './module.mjs';

const root = resolve(import.meta.dir, '../..');
const scratch = () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const dir = mkdtempSync(resolve(root, 'target/render-storage-test-'));
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Render Storage', app: { id: 'test.render-storage', name: 'Render Storage' }, host: { web: {} } }));
  return dir;
};
// The app in `dir` built for the web with its pages rendered under Bun; its build names no failure.
async function build(dir) {
  const dist = resolve(dir, 'dist');
  const built = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'render-storage-test', '--out', dist, '--render', 'js'], {
    cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
  });
  const [status, stdout, stderr] = await Promise.all([built.exited, new Response(built.stdout).text(), new Response(built.stderr).text()]);
  expect(status, stdout + stderr).toBe(0);
  expect(stdout + stderr).not.toMatch(/failed|structuredClone|dynamic import/);
  return dist;
}

test('a caught canvas capture stays pending in a render and captures after adoption', async () => {
  const dir = scratch();
  let site, browser;
  try {
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  home "/" render=build

component App
  resource image = capture() as shape string else empty()
  view
    main
      canvas surface=ink() width=20 height=20
      text image testId="image"
      text (pending(image) ? "pending" : "answered") testId="state"
`);
    writeFileSync(resolve(dir, 'app.ts'), `import type { Ctx2D } from './app.contract.d.ts';
export const appId = 'test.render-storage';
export const grants = 'fs.write app:/tmp';
export const answer = () => captureCanvas('ink', 'app:/tmp/ink.png').then(c => c.path).catch(() => '');
export const surfaces = { ink: 0 };
export function draw(name: string, args: unknown[], ctx: Ctx2D) { ctx.fillStyle = 'red'; ctx.fillRect(0, 0, 20, 20); return false; }
`);
    const dist = await build(dir), html = readFileSync(resolve(dist, 'index.html'), 'utf8');
    const checkpoint = JSON.parse(/<script type="application\/vnd.exact.checkpoint"[^>]*>([^<]*)<\/script>/.exec(html)[1]);
    expect(checkpoint.pending).toEqual(['image']);
    expect(checkpoint.answers).toEqual([]);
    expect(html).toMatch(/data-testid="state"[^>]*>pending</);
    site = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(request) {
      const path = new URL(request.url).pathname;
      if (path === '/') return new Response(html, { headers: { 'content-type': 'text/html' } });
      const asset = Bun.file(resolve(dist, path.slice(1)));
      return await asset.exists() ? new Response(asset) : new Response(null, { status: 404 });
    } });
    browser = await open({ host: 'web', url: new URL('/?storage=render-capture', site.url).href });
    await browser.clock('settle');
    const journal = await browser.carrier.evaluate('globalThis.exact.journal');
    expect(journal.some(l => l.endsWith('adopted the document')), journal.join('\n')).toBe(true);
    expect(await browser.carrier.evaluate(`['image', 'state'].map(id => document.querySelector('[data-testid="' + id + '"]').textContent)`), journal.join('\n'))
      .toEqual(['app:/tmp/ink.png', 'answered']);
  } finally { await browser?.close(); site?.stop(true); rmSync(dir, { recursive: true, force: true }); }
}, 120000);

test.each(['TypeScript', 'Rust'])('a JavaScript render refuses %s store writes and deletions on every request', async language => {
  const dir = scratch();
  const api = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch: () => new Response('public') });
  try {
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  read "/" render=request
  write "/write" render=request
  forget "/forget" render=request
  caughtWrite "/caught-write" render=request
  caughtForget "/caught-forget" render=request
  later "/later" render=request
  parseWrite "/parse-write" render=request
  parseForget "/parse-forget" render=request
  ownError "/own-error" render=request
  badArguments "/bad-arguments" render=request
  unknownSource "/unknown-source" render=request
  exclusive "/exclusive" render=request
  keepKey "/keep-key" render=request

component App
  resource reader = reader() as shape string
  resource writer = writer(top(nav).name) as shape string else empty()
  view
    main
      text reader testId="reader"
      text writer testId="writer"
      text (failed(writer) ? "failed" : "answered") testId="state"
`);
    if (language === 'TypeScript') writeFileSync(resolve(dir, 'app.ts'), `export const grants = 'secret.keep token';
let reads = 0;
export function answer(name: string, [route]: string[], store: any): any {
  if (name === 'reader') return (store.get('token') ?? 'empty') + ':' + ++reads;
  const write = () => route === 'forget' || route === 'caughtForget' ? store.forget('token') : store.set('token', 'private');
  if (route === 'read') return store.get('token') ?? 'empty';
  if (route === 'exclusive') return store.exclusive('token', () => 'work ran').catch(() => 'refused:' + (store.get('token') ?? 'empty'));
  if (route === 'keepKey') return store.keepKey('token', {}).catch(() => 'refused:' + (store.get('token') ?? 'empty'));
  if (route === 'later') return Promise.resolve().then(write);
  if (route === 'ownError') { try { write(); } catch {} throw new Error('source failure'); }
  if (route.startsWith('parse')) return Promise.resolve().then(() => {
    try { route === 'parseForget' ? store.forget('token') : store.set('token', 'private'); }
    catch { return 'refused:' + (store.get('token') ?? 'empty'); }
    return 'written';
  });
  if (route.startsWith('caught')) {
    try { write(); } catch { return 'refused:' + (store.get('token') ?? 'empty'); }
  } else write();
  return 'written';
}
`);
    else {
      mkdirSync(resolve(dir, 'data/src'), { recursive: true });
      mkdirSync(resolve(dir, 'web'));
      writeFileSync(resolve(dir, 'web/build.rs'), 'fn main() { contract::bake(plan, render_store_data::Reader::default()); }\n');
      writeFileSync(resolve(dir, 'data/Cargo.toml'), `[package]
name = "render-store-data"
version = "0.1.0"
edition = "2021"
[dependencies]
exact-runner = { path = "${root}/runner" }
[workspace]
`);
      writeFileSync(resolve(dir, 'data/src/lib.rs'), `use exact_runner::{Answer, DataError, DataSource, Outcome, Request, Store, Value};

fn write(store: &mut Store, route: &str) -> Result<Answer, DataError> {
    let result = if route == "forget" || route.ends_with("Forget") {
        store.forget("token")
    } else { store.set("token", "private") };
    if route == "ownError" { return Err(DataError::Unavailable("source failure".into())); }
    if route.starts_with("caught") || route.starts_with("parse") {
        return Ok(Answer::Now(Value::str(&format!("{}:{}",
            if result.is_err() { "refused" } else { "written" }, store.get("token").unwrap_or("empty")))));
    }
    result?;
    Ok(Answer::Now(Value::str("written")))
}

#[derive(Default)]
pub struct Reader { reads: u32 }
impl DataSource for Reader {
    fn grants(&self) -> &str { "secret.keep token\nnet.fetch ${api.url.origin}" }
    fn query(&mut self, _: &str, _: &[Value]) -> Result<Value, DataError> { Ok(Value::str("baked")) }
    fn answer(&mut self, store: &mut Store, source: &str, args: &[Value]) -> Result<Answer, DataError> {
        if source == "reader" {
            self.reads += 1;
            return Ok(Answer::Now(Value::str(&format!("{}:{}", store.get("token").unwrap_or("empty"), self.reads))));
        }
        let route = args[0].as_str().unwrap();
        if route == "badArguments" { return Err(DataError::BadArguments("bad arguments".into())); }
        if route == "unknownSource" { return Err(DataError::UnknownSource("missing source".into())); }
        match route {
            "read" => return Ok(Answer::Now(Value::str(store.get("token").unwrap_or("empty")))),
            "parseWrite" | "parseForget" => return Ok(Answer::Later(Request::get("${api.url.origin}"))),
            _ => (),
        }
        write(store, route)
    }
    fn parse(&mut self, store: &mut Store, _: &str, args: &[Value], _: Outcome) -> Result<Answer, DataError> {
        write(store, args[0].as_str().unwrap())
    }
}
`);
    }
    const dist = await build(dir);
    const { renderer } = await import('./render.mjs');
    const paths = ['/own-error', '/caught-write', '/caught-forget', '/parse-write', '/parse-forget', '/', '/write', '/', '/forget', '/write', '/'];
    if (language === 'TypeScript') paths.push('/later', '/', '/exclusive', '/keep-key');
    else paths.push('/bad-arguments', '/unknown-source');
    for (const ahead of [false, true]) {
      const render = renderer(dist); render.ahead = ahead;
      for (const path of paths) {
        if (language === 'Rust' && ['/own-error', '/write', '/forget', '/bad-arguments', '/unknown-source'].includes(path)) {
          const message = { '/own-error': 'source failure', '/bad-arguments': 'bad arguments', '/unknown-source': 'missing source' }[path] ?? 'secret token is not granted';
          await expect(render(path)).rejects.toThrow(new RegExp('boot refused.*' + message));
          continue;
        }
        const reply = await render(path);
        const text = id => new RegExp(`data-testid="${id}"[^>]*>([^<]*)<`).exec(reply.html)[1];
        const failed = path !== '/' && !path.startsWith('/caught') && !path.startsWith('/parse') && !['/exclusive', '/keep-key'].includes(path);
        expect({ path, status: reply.status, settled: reply.settled, reader: text('reader'), writer: text('writer'), state: text('state'),
          failed: reply.failed.map(f => f.split(':')[0]) }).toEqual({ path, status: 200, settled: true, reader: 'empty:1',
          writer: failed ? '' : path === '/' ? 'empty' : 'refused:empty', state: failed ? 'failed' : 'answered', failed: failed ? ['writer'] : [] });
        if (failed) expect(reply.failed[0]).toContain(path === '/own-error' ? 'source failure' : language === 'TypeScript'
          ? "a render's store is empty and refuses writes" : 'secret token is not granted');
      }
      render.ahead = false;
    }
    if (language === 'Rust') {
      const path = resolve(dir, 'app.contract');
      writeFileSync(path, readFileSync(path, 'utf8').replace(' else empty()', ''));
      const bare = renderer(await build(dir)); bare.ahead = false;
      await expect(bare('/own-error')).rejects.toThrow(/boot refused.*source failure/);
    }
  } finally {
    api.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);

test('a render contains detached store refusals within their request', async () => {
  const dir = scratch();
  try {
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  read "/" render=request
  write "/write" render=request
  forget "/forget" render=request
  promiseWrite "/promise-write" render=request
  promiseForget "/promise-forget" render=request
  promiseAsync "/promise-async" render=request
  hostPromise "/host-promise" render=request
  caught "/caught" render=request

component App
  resource value = value(top(nav).name) as shape string
  view
    main
      text value testId="value"
`);
    writeFileSync(resolve(dir, 'app.ts'), `export const grants = 'secret.keep token';
export function answer(_: string, [route]: string[], store: any): string {
  if (route === 'write') queueMicrotask(() => store.set('token', 'private'));
  if (route === 'forget') queueMicrotask(async () => store.forget('token'));
  if (route === 'promiseWrite') void Promise.resolve().then(() => store.set('token', 'private'));
  if (route === 'promiseForget') void Promise.resolve().then(() => store.forget('token'));
  if (route === 'promiseAsync') void (async () => { await Promise.resolve(); store.set('token', 'private'); })();
  if (route === 'hostPromise') void new Response('body').text().then(() => store.set('token', 'private'));
  if (route === 'caught') void Promise.resolve().then(() => store.set('token', 'private')).catch(() => {});
  return store.get('token') ?? 'empty';
}
`);
    const dist = await build(dir);
    const child = Bun.spawn([process.execPath, '--eval', `
      import { renderer } from './host/web-js/render.mjs';
      const seen = [];
      for (const ahead of [false, true]) {
        const render = renderer(process.argv[1]); render.ahead = ahead;
        const page = async path => {
          const reply = await render(path);
          return { path, status: reply.status, settled: reply.settled, failed: reply.failed,
            text: /data-testid="value"[^>]*>([^<]*)</.exec(reply.html)[1] };
        };
        const paths = ['/write', '/', '/forget', '/', '/promise-write', '/', '/promise-forget', '/',
          '/promise-async', '/', '/host-promise', '/', '/caught'];
        for (const path of paths) seen.push(await page(path));
        seen.push(...await Promise.all(['/promise-write', '/', '/promise-forget', '/caught'].map(page)));
        render.ahead = false;
      }
      process.stdout.write(JSON.stringify(seen));
    `, dist], { cwd: root, stdout: 'pipe', stderr: 'pipe' });
    const [status, stdout, stderr] = await Promise.all([child.exited, new Response(child.stdout).text(), new Response(child.stderr).text()]);
    expect(status, stderr).toBe(0);
    const seen = JSON.parse(stdout);
    expect(seen).toHaveLength(34);
    for (const reply of seen) {
      expect([reply.status, reply.settled, reply.text]).toEqual([200, true, 'empty']);
      if (!['/', '/caught'].includes(reply.path)) {
        expect(reply.failed).toHaveLength(1);
        expect(reply.failed[0]).toContain("a render's store is empty and refuses writes");
      } else expect(reply.failed).toEqual([]);
    }
    // Containing a render's failures must preserve the host's fatal handling of an unrelated rejection.
    const unrelated = Bun.spawn([process.execPath, '--eval', `
      import { renderer } from './host/web-js/render.mjs';
      const render = renderer(process.argv[1]); render.ahead = false;
      await render('/');
      Promise.reject(new Error('outside a render'));
    `, dist], { cwd: root, stdout: 'pipe', stderr: 'pipe' });
    const [unrelatedStatus, unrelatedError] = await Promise.all([unrelated.exited, new Response(unrelated.stderr).text()]);
    expect(unrelatedStatus).toBe(1);
    expect(unrelatedError).toContain('outside a render');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);

// A page rendered on the server has no storage (LLP 1048.000 D9): a source that reads a file there is refused before
// any storage code runs, and its resource keeps its placeholder, pending, not failed and not in the checkpoint's
// answers; the render does not wait for it. The reader's page, once it adopts the document, asks it again.
test.each(['TypeScript', 'Rust'])('a page rendered on the server keeps a %s storage source pending, and the reader\'s page asks it', async language => {
  const dir = scratch();
  let site, browser;
  try {
    writeFileSync(resolve(dir, 'app.contract'), `shape Prefs
  name: string

routes nav
  home "/" render=build
    item "/item/:id" render=request

component App
  resource prefs = prefs() as shape Prefs else empty(name="placeholder")
  resource title = title(params(nav, "id")) as shape string
  view
    main
      head title=title
      text prefs.name testId="name"
      text (pending(prefs) ? "pending" : failed(prefs) ? "failed" : "answered") testId="state"
`);
    // On the reader's device the file is not there yet: the source says so, which shows that storage was asked.
    if (language === 'Rust') {
      mkdirSync(resolve(dir, 'data/src'), { recursive: true });
      mkdirSync(resolve(dir, 'web'));
      writeFileSync(resolve(dir, 'web/build.rs'), 'fn main() { contract::bake(plan, render_storage_data::Prefs); }\n');
      writeFileSync(resolve(dir, 'data/Cargo.toml'), `[package]
name = "render-storage-data"
version = "0.1.0"
edition = "2021"
[dependencies]
exact-runner = { path = "${root}/runner" }
[workspace]
`);
      writeFileSync(resolve(dir, 'data/src/lib.rs'), `use exact_runner::{Answer, DataError, DataSource, Outcome, Request, Store, Value};

pub struct Prefs;
impl DataSource for Prefs {
    fn app_id(&self) -> &str {
        "test.render-storage"
    }
    fn grants(&self) -> &str {
        "fs.read app:/data/prefs.json"
    }
    fn query(&mut self, source: &str, args: &[Value]) -> Result<Value, DataError> {
        if source != "title" {
            return Err(DataError::UnknownSource(source.into()));
        }
        let Value::List(ids) = &args[0] else {
            unreachable!()
        };
        Ok(Value::str(
            &ids.first()
                .map(|id| format!("Item {}", id.as_str().unwrap()))
                .unwrap_or("Home".into()),
        ))
    }
    fn answer(&mut self, _: &mut Store, source: &str, args: &[Value]) -> Result<Answer, DataError> {
        if source == "title" {
            return self.query(source, args).map(Answer::Now);
        }
        Ok(Answer::Later(Request::storage(
            br#"{"version":1,"op":"fs.readFile","args":{"path":"app:/data/prefs.json"}}"#.to_vec(),
        )))
    }
    fn parse(
        &mut self,
        _: &mut Store,
        _: &str,
        _: &[Value],
        outcome: Outcome,
    ) -> Result<Answer, DataError> {
        match outcome {
            Outcome::Storage(bytes)
                if String::from_utf8_lossy(&bytes)
                    .contains("no such file or directory: app:/data/prefs.json") =>
            {
                Ok(Answer::Now(Value::record(vec![Value::str(
                    "asked on the device",
                )])))
            }
            other => Err(DataError::Unavailable(format!(
                "unexpected storage outcome: {other:?}"
            ))),
        }
    }
}
`);
    } else writeFileSync(resolve(dir, 'app.ts'), `export const grants = 'fs.read app:/data/prefs.json';
export const answer = async (name: string, args: string[][], _store: unknown, storage: any): Promise<any> => {
  if (name === 'title') return args[0].length ? 'Item ' + args[0][0] : 'Home';
  try { return { name: new TextDecoder().decode(await storage.fs.readFile('app:/data/prefs.json')) }; }
  catch (e: any) { if (e.code === 'ENOENT') return { name: 'asked on the device' }; throw e; }
};
`);
    const dist = await build(dir);
    const pending = html => {
      const checkpoint = JSON.parse(/<script type="application\/vnd.exact.checkpoint"[^>]*>([^<]*)<\/script>/.exec(html)[1]);
      const text = id => new RegExp(`data-testid="${id}"[^>]*>([^<]*)<`).exec(html)[1];
      return { name: text('name'), state: text('state'), pending: checkpoint.pending, answered: checkpoint.answers.map(a => a[0]) };
    };
    // At build (`web-build`): the page written for the render=build route.
    expect(pending(readFileSync(resolve(dist, 'index.html'), 'utf8'))).toEqual({ name: 'placeholder', state: 'pending', pending: ['prefs'], answered: ['title'] });
    // Per request (`serve`): a settled 200 long before its deadline, which would be a 503, with nothing failed or logged as failed.
    const { pageServer, renderer } = await import('./render.mjs');
    const render = renderer(dist); render.ahead = false;
    const lines = [], page = pageServer(dist, render, line => lines.push(line));
    const reply = await page('/item/7', () => undefined);
    const html = new TextDecoder().decode(typeof reply.body === 'string' ? new TextEncoder().encode(reply.body) : reply.body);
    expect(reply.status).toBe(200);
    expect(html).toContain('<title>Item 7</title>');
    expect(pending(html)).toEqual({ name: 'placeholder', state: 'pending', pending: ['prefs'], answered: ['title'] });
    expect(html).not.toContain('"revalidate":true');
    expect(lines.join('\n')).not.toMatch(/failed|structuredClone|dynamic import/);
    const api = language === 'Rust' ? 'storage request' : 'storage.fs.readFile()';
    expect(lines.join('\n')).toContain(`resource prefs is pending: a page rendered on the server has no storage (${api})`);
    const direct = await render('/item/7', 30000);
    expect([direct.status, direct.settled, direct.failed]).toEqual([200, true, []]);
    // The reader's page adopts the document and asks the source, which now reads storage.
    site = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(request) {
      const path = new URL(request.url).pathname;
      if (path === '/item/7') return new Response(html, { headers: { 'content-type': 'text/html' } });
      const asset = Bun.file(resolve(dist, path.slice(1)));
      return await asset.exists() ? new Response(asset) : new Response(null, { status: 404 });
    } });
    browser = await open({ host: 'web', url: new URL('/item/7?storage=render-prefs', site.url).href });
    await browser.clock('settle');
    const journal = await browser.carrier.evaluate('globalThis.exact.journal');
    expect(journal.some(l => l.endsWith('adopted the document')), journal.join('\n')).toBe(true);
    expect(await browser.carrier.evaluate(`['name', 'state'].map(id => document.querySelector('[data-testid="' + id + '"]').textContent)`), journal.join('\n'))
      .toEqual(['asked on the device', 'answered']);
  } finally {
    await browser?.close(); site?.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);

// A resource that answered and is asked again during the render, whose second answer waits on storage: the page
// shows its first answer, and the reader's page asks it again once it adopts the document, as it asks any
// resource the render left pending.
test('a resource asked again during a render and refused its storage is asked again by the reader\'s page', async () => {
  const dir = scratch();
  let site, browser;
  try {
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  home "/" render=request

component App
  resource writer = writer() as shape number else empty()
  resource reader = reader(writer) as shape string
  view
    main
      text reader testId="reader"
      text writer.toString()
`);
    // The writer answers after the reader's first answer, changing its arguments; its second answer reads a file.
    writeFileSync(resolve(dir, 'app.ts'), `export const grants = 'fs.read app:/data/reader';
let readerCalls = 0;
export const answer = async (name: string, _args: unknown[], _store: unknown, storage: any): Promise<any> => {
  if (name === 'writer') { await Promise.resolve(); await Promise.resolve(); return 1; }
  if (++readerCalls === 1) return 'first';
  try { return new TextDecoder().decode(await storage.fs.readFile('app:/data/reader')); } catch { return 'asked on the device'; }
};
`);
    const dist = await build(dir);
    const { renderer } = await import('./render.mjs');
    const render = renderer(dist); render.ahead = false;
    const reply = await render('/', 10000);
    const checkpoint = JSON.parse(/<script type="application\/vnd.exact.checkpoint"[^>]*>([^<]*)<\/script>/.exec(reply.html)[1]);
    expect({ status: reply.status, settled: reply.settled, refused: reply.refused, pending: checkpoint.pending, answered: checkpoint.answers.map(a => a[0]) })
      .toEqual({ status: 200, settled: true, refused: [['reader', 'storage.fs.readFile()']], pending: ['reader'], answered: ['writer', 'reader'] });
    site = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(request) {
      const path = new URL(request.url).pathname;
      if (path === '/') return new Response(reply.html, { headers: { 'content-type': 'text/html' } });
      const asset = Bun.file(resolve(dist, path.slice(1)));
      return await asset.exists() ? new Response(asset) : new Response(null, { status: 404 });
    } });
    browser = await open({ host: 'web', url: site.url.href });
    await browser.clock('settle');
    const journal = await browser.carrier.evaluate('globalThis.exact.journal');
    expect(journal.some(l => l.endsWith('adopted the document')), journal.join('\n')).toBe(true);
    expect(journal.some(l => /^t=\d+ reader answered:/.test(l)), journal.join('\n')).toBe(true);
  } finally {
    await browser?.close(); site?.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);

test('answered sources retry unawaited storage reads and writes after adoption', async () => {
  const dir = scratch();
  let site, browser;
  try {
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  home "/" render=request

component App
  state inspection = 0
  resource writer = write() as shape string
  resource reader = read() as shape string
  resource seen = seen(inspection) as shape string
  action inspect
    inspection = inspection + 1
  view
    main
      text writer testId="writer"
      text reader testId="reader"
      text seen testId="seen"
      button press=inspect testId="inspect"
        text "Inspect storage results"
`);
    writeFileSync(resolve(dir, 'app.ts'), `export const grants = 'fs.read app:/data/prefs\\nfs.write app:/data/prefs\\nsecret.keep prefs\\nsecret.keep written';
export const answer = (name: string, _args: unknown[], store: any, storage: any): string => {
  if (name === 'write') {
    void storage.fs.writeFile('app:/data/prefs', new TextEncoder().encode('device prefs')).then(() => store.set('written', 'written'));
    return 'started';
  }
  if (name === 'read') {
    void storage.fs.readFile('app:/data/prefs').then((bytes: Uint8Array) => store.set('prefs', new TextDecoder().decode(bytes)));
    return 'started';
  }
  return (store.get('written') ?? 'none') + ': ' + (store.get('prefs') ?? 'none');
};
`);
    const dist = await build(dir);
    const { renderer } = await import('./render.mjs');
    const render = renderer(dist); render.ahead = false;
    const reply = await render('/', 10000);
    const checkpoint = JSON.parse(/<script type="application\/vnd.exact.checkpoint"[^>]*>([^<]*)<\/script>/.exec(reply.html)[1]);
    expect([reply.status, reply.settled, reply.failed, checkpoint.pending, checkpoint.revalidate ?? false])
      .toEqual([200, true, [], [], false]);
    expect(checkpoint.answers.map(a => a[0])).toEqual(['writer', 'reader', 'seen']);
    expect(reply.html).toContain('>none: none<');
    site = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(request) {
      const path = new URL(request.url).pathname;
      if (path === '/') return new Response(reply.html, { headers: { 'content-type': 'text/html' } });
      const asset = Bun.file(resolve(dist, path.slice(1)));
      return await asset.exists() ? new Response(asset) : new Response(null, { status: 404 });
    } });
    // The named store is confined to the driver's temporary browser profile.
    browser = await open({ host: 'web', url: new URL('/?storage=render-background', site.url).href });
    await browser.clock('settle');
    const journal = await browser.carrier.evaluate('globalThis.exact.journal');
    expect(journal.some(l => l.endsWith('adopted the document')), journal.join('\n')).toBe(true);
    // Inspect after storage settles, without relying on a baked store reader's subscriptions.
    await browser.tap('inspect');
    await browser.clock('settle');
    expect(await browser.carrier.evaluate(`['writer', 'reader', 'seen'].map(id => document.querySelector('[data-testid="' + id + '"]').textContent)`), (await browser.logs()).lines.join('\n'))
      .toEqual(['started', 'started', 'written: device prefs']);
    expect(checkpoint.open.map(k => checkpoint.answers[k][0])).toEqual(['writer', 'reader']);
  } finally {
    await browser?.close(); site?.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);

// Another answer's fetch, one after an await or one in a plain `.then` (tied to no answer), holds the page while it is
// out; once it ends, an answer left waiting only on a refused storage call holds the page no longer, whether the call
// was refused before that fetch began or while it was out.
test('a refused answer holds nothing once a fetch tied to no answer ends', async () => {
  const dir = scratch();
  const api = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch: request => {
    const wait = new URL(request.url).pathname === '/slow' ? 200 : 0;
    return new Promise(resolve => setTimeout(() => resolve(new Response('body')), wait));
  } });
  try {
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  twice "/" render=request
  reaction "/reaction" render=request
  during "/during" render=request

component App
  resource prefs = prefs(top(nav).name) as shape string else empty()
  resource body = body(top(nav).name) as shape string else empty()
  view
    main
      text prefs testId="prefs"
      text body testId="body"
`);
    writeFileSync(resolve(dir, 'app.ts'), `export const grants = 'fs.read app:/data/prefs\\nnet.fetch ${api.url.origin}';
const API = '${api.url.origin}';
export const answer = async (name: string, [route]: string[], _store: unknown, storage: any): Promise<any> => {
  const read = async () => new TextDecoder().decode(await storage.fs.readFile('app:/data/prefs'));
  if (name === 'prefs') {
    // On /during the call is refused after this answer's own fetch, while /body's slow fetch is out.
    if (route === 'during') await (await fetch(API + '/fast')).text();
    return read();
  }
  if (route === 'twice') { await (await fetch(API + '/a')).text(); return (await fetch(API + '/b')).text(); }
  if (route === 'reaction') return Promise.resolve().then(() => fetch(API + '/b')).then(r => r.text());
  return Promise.resolve().then(() => fetch(API + '/slow')).then(r => r.text());
};
`);
    const dist = await build(dir);
    const { renderer } = await import('./render.mjs');
    const render = renderer(dist); render.ahead = false;
    const seen = {}, wanted = {};
    for (const path of ['/', '/reaction', '/during']) {
      const reply = await render(path);
      const checkpoint = JSON.parse(/<script type="application\/vnd.exact.checkpoint"[^>]*>([^<]*)<\/script>/.exec(reply.html)[1]);
      const text = id => new RegExp(`data-testid="${id}"[^>]*>([^<]*)<`).exec(reply.html)[1];
      seen[path] = { status: reply.status, settled: reply.settled, refused: reply.refused, pending: checkpoint.pending,
        prefs: text('prefs'), body: text('body'), quick: reply.render < 1500 };
      wanted[path] = { status: 200, settled: true, refused: [['prefs', 'storage.fs.readFile()']], pending: ['prefs'], prefs: '', body: 'body', quick: true };
    }
    expect(seen).toEqual(wanted);
  } finally {
    api.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);

// Storage reached by another door, and work that is no count on its answer. A stream (and an upload, which streams its
// progress) holds the page beside a refused call as it does without one. A fetch whose body comes from a file or goes
// to one reads storage: it is refused as a storage call is, before any storage code loads, and its answer is pending.
// `storage.fs.watch` checks what it checks on the device first, so what throws there throws here; a watch that holds
// is the reader's to make, so its answer is asked again once the reader's page adopts the document.
test('a render holds streams and uploads beside a refused call, and refuses file bodies and watches as storage', async () => {
  const dir = scratch();
  const api = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(request) {
    const path = new URL(request.url).pathname;
    await new Promise(resolve => setTimeout(resolve, path === '/slow' ? 200 : 100));
    if (path === '/events') return new Response(new ReadableStream({ start(c) { c.enqueue(new TextEncoder().encode('data: message\n\n')); } }), { headers: { 'content-type': 'text/event-stream' } });
    await request.arrayBuffer();
    return new Response('done');
  } });
  try {
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  stream "/" render=request
  streamPlain "/stream-plain" render=request
  upload "/upload" render=request
  uploadPlain "/upload-plain" render=request
  bodyFrom "/body-from" render=request
  bodyTo "/body-to" render=request
  uploadFrom "/upload-from" render=request
  watch "/watch" render=request
  watchBad "/watch-bad" render=request
  watchThen "/watch-then" render=request
  watchAwait "/watch-await" render=request
  watchHelpers "/watch-helpers" render=request
  bodyFromAwait "/body-from-await" render=request
  bodyToAwait "/body-to-await" render=request
  timeout "/timeout" render=request
  aborted "/aborted" render=request
  streamVoid "/stream-void" render=request
  abortedFrom "/aborted-from" render=request
  abortedTo "/aborted-to" render=request
  expectBad "/expect-bad" render=request

component App
  resource prefs = prefs(top(nav).name) as shape string else empty()
  resource live = live(top(nav).name) as shape string else empty()
  view
    main
      text prefs testId="prefs"
      text live testId="live"
`);
    writeFileSync(resolve(dir, 'watch.ts'), `class Document {
  #path = 'doc:/notes';
  get path() { return this.#path; }
}
async function* paths() { yield new Document().path; }
export async function watch(storage: any): Promise<string> {
  await Promise.resolve();
  const { ignored, ...options } = { ...{ recursive: true }, ignored: 0 };
  for await (const path of paths()) storage.fs.watch(path, options);
  return options.recursive ? 'watched' : 'missing';
}
`);
    writeFileSync(resolve(dir, 'app.ts'), `import { watch } from './watch';
export const grants = 'fs.read app:/data\\nfs.write app:/data\\nnet.fetch ${api.url.origin}';
const API = '${api.url.origin}';
const stop = new AbortController();
export const answer = async (name: string, [route]: string[], _store: unknown, storage: any): Promise<any> => {
  if (name === 'prefs') {
    // A fetch out for 200 ms, so the render outlasts the file fetch's 50 ms timeout, or the abort comes after it.
    if (route === 'timeout' || route === 'aborted') { const text = await (await fetch(API + '/slow')).text(); stop.abort(); return text; }
    return route === 'stream' || route === 'upload' ? new TextDecoder().decode(await storage.fs.readFile('app:/data/p')) : 'plain';
  }
  if (route === 'timeout') return (await fetch(API + '/upload', { method: 'POST', exactBodyFrom: 'app:/data/b', exactTimeout: 50 } as RequestInit)).text();
  if (route === 'aborted') return (await fetch(API + '/upload', { method: 'POST', exactBodyFrom: 'app:/data/b', signal: stop.signal } as RequestInit)).text();
  if (route === 'expectBad') { try { await storage.fs.writeFile('app:/data/b', 'x', { expect: { modified: 0 } }); return 'written'; } catch (e: any) { return 'rejected ' + e.name; } }
  if (route === 'watchThen') return Promise.resolve().then(() => { storage.fs.watch('doc:/notes'); return 'watched'; });
  if (route === 'watchAwait') { await Promise.resolve(); storage.fs.watch('doc:/notes'); return 'watched'; }
  if (route === 'watchHelpers') return watch(storage);
  if (route === 'bodyFromAwait') { await Promise.resolve(); return (await fetch(API + '/upload', { method: 'POST', exactBodyFrom: 'app:/data/b' } as RequestInit)).text(); }
  if (route === 'bodyToAwait') { await Promise.resolve(); await fetch(API + '/upload', { exactBodyTo: 'app:/data/b' } as RequestInit); return 'saved'; }
  if (route === 'streamVoid') { void Promise.resolve().then(() => storage.fs.readFile('app:/data/p')); return fetch(API + '/events', { exactStream: (e: any) => e.data } as RequestInit); }
  if (route === 'abortedFrom' || route === 'abortedTo') {
    const gone = new AbortController();
    gone.abort(new Error('gone'));
    const init = route === 'abortedFrom' ? { method: 'POST', exactBodyFrom: 'app:/data/b' } : { exactBodyTo: 'app:/data/b' };
    try { await fetch(API + '/upload', { ...init, signal: gone.signal } as RequestInit); return 'sent'; } catch (e: any) { return 'caught ' + e.message; }
  }
  if (route.startsWith('stream')) return fetch(API + '/events', { exactStream: (e: any) => e.data } as RequestInit);
  if (route.startsWith('upload') && route !== 'uploadFrom') return fetch(API + '/upload', { method: 'POST', body: 'x', exactUploadProgress: true, exactStream: (e: any) => e.type === 'load' ? 'load ' + e.data : e.type } as RequestInit);
  if (route === 'bodyFrom') return (await fetch(API + '/upload', { method: 'POST', exactBodyFrom: 'app:/data/b' } as RequestInit)).text();
  if (route === 'bodyTo') { await fetch(API + '/upload', { exactBodyTo: 'app:/data/b' } as RequestInit); return 'saved'; }
  if (route === 'uploadFrom') return fetch(API + '/upload', { method: 'POST', exactBodyFrom: 'app:/data/b', exactUploadProgress: true, exactStream: (e: any) => e.type } as RequestInit);
  if (route === 'watch') { storage.fs.watch('doc:/notes'); return 'watched'; }
  storage.fs.watch('app:/data/b');
  return 'watched';
};
`);
    const dist = await build(dir);
    const { renderer } = await import('./render.mjs');
    const render = renderer(dist); render.ahead = false;
    const page = async path => {
      const reply = await render(path);
      const checkpoint = JSON.parse(/<script type="application\/vnd.exact.checkpoint"[^>]*>([^<]*)<\/script>/.exec(reply.html)[1]);
      const text = id => new RegExp(`data-testid="${id}"[^>]*>([^<]*)<`).exec(reply.html)[1];
      return { status: reply.status, settled: reply.settled, live: text('live'), pending: checkpoint.pending, open: (checkpoint.open ?? []).map(k => checkpoint.answers[k][0]),
        refused: reply.refused, failed: reply.failed.map(f => f.split(':')[0]), quick: reply.render < 1500 };
    };
    const seen = {}, wanted = {};
    // Beside a refused call, what a stream or an upload holds the page for is what it holds it for without one.
    for (const [path, plain] of [['/', '/stream-plain'], ['/upload', '/upload-plain']]) {
      const twin = await page(plain);
      wanted[path] = { ...twin, pending: ['prefs', ...twin.pending], refused: [['prefs', 'storage.fs.readFile()']] };
      seen[path] = await page(path);
    }
    const refused = api => ({ status: 200, settled: true, live: '', pending: ['live'], open: [], refused: [['live', api]], failed: [], quick: true });
    for (const [path, api] of [
      ['/body-from', 'exactBodyFrom app:/data/b'], ['/body-from-await', 'exactBodyFrom app:/data/b'],
      ['/body-to', 'exactBodyTo app:/data/b'], ['/body-to-await', 'exactBodyTo app:/data/b'],
      ['/upload-from', 'exactBodyFrom app:/data/b'],
    ]) {
      wanted[path] = refused(api);
      seen[path] = await page(path);
    }
    wanted['/watch'] = { status: 200, settled: true, live: 'watched', pending: [], open: ['live'], refused: [], failed: [], quick: true };
    seen['/watch'] = await page('/watch');
    for (const path of ['/watch-await', '/watch-helpers']) {
      wanted[path] = wanted['/watch'];
      seen[path] = await page(path);
    }
    wanted['/watch-bad'] = { status: 200, settled: true, live: '', pending: [], open: [], refused: [], failed: ['live'], quick: true };
    seen['/watch-bad'] = await page('/watch-bad');
    // A plain `.then` runs outside its answer in the reader's page too, so the watch throws in both.
    wanted['/watch-then'] = wanted['/watch-bad'];
    seen['/watch-then'] = await page('/watch-then');
    // A file body's refusal is no request in flight: neither the fetch's timeout nor its abort ends it, though the
    // page waits past both for the other source's 200 ms fetch, which answers (it is not pending).
    for (const path of ['/timeout', '/aborted']) {
      wanted[path] = refused('exactBodyFrom app:/data/b');
      seen[path] = await page(path);
    }
    // A stream holds the page until its first message, whatever storage calls are made around it.
    wanted['/stream-void'] = await page('/stream-plain');
    seen['/stream-void'] = await page('/stream-void');
    // A signal already aborted is the device's rejection before storage is touched: a source that catches it answers.
    for (const [path, said] of [['/aborted-from', 'caught gone'], ['/aborted-to', 'caught gone']]) {
      wanted[path] = { status: 200, settled: true, live: said, pending: [], open: [], refused: [], failed: [], quick: true };
      seen[path] = await page(path);
    }
    // What the device rejects before asking storage (an `expect` on an app file) rejects here too, so a source that
    // catches it answers as on the device.
    wanted['/expect-bad'] = { status: 200, settled: true, live: 'rejected TypeError', pending: [], open: [], refused: [], failed: [], quick: true };
    seen['/expect-bad'] = await page('/expect-bad');
    expect(seen).toEqual(wanted);
  } finally {
    api.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);

// A file-body fetch an app module makes as it loads is refused as any is, in a realm made ahead of its render too
// (`render.ahead`, the default), where the module loads well before the render starts: its resource stays pending.
test('a file-body fetch made as an app module loads is refused in a realm made ahead', async () => {
  const dir = scratch();
  try {
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  home "/" render=request

component App
  resource saved = saved() as shape string else empty()
  view
    main
      text saved testId="saved"
`);
    writeFileSync(resolve(dir, 'app.ts'), `export const grants = 'fs.write app:/data\\nnet.fetch http://127.0.0.1:9';
const saved = fetch('http://127.0.0.1:9/', { exactBodyTo: 'app:/data/f' } as RequestInit).then(() => 'saved', (e: any) => 'caught ' + e.name);
export const answer = (): Promise<string> => saved;
`);
    const dist = await build(dir);
    const { renderer } = await import('./render.mjs');
    const render = renderer(dist);
    const seen = [];
    for (let i = 0; i < 3; i++) {
      const reply = await render('/', 1000);
      const checkpoint = JSON.parse(/<script type="application\/vnd.exact.checkpoint"[^>]*>([^<]*)<\/script>/.exec(reply.html)[1]);
      seen.push([reply.status, reply.settled, checkpoint.pending, /data-testid="saved"[^>]*>([^<]*)</.exec(reply.html)[1]]);
      await new Promise(r => setTimeout(r, 50)); // the next realm is made once the last render is done (render.mjs `makeAhead`)
    }
    expect(seen).toEqual(Array(3).fill([200, true, ['saved'], '']));
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);

// An upload whose body is an app file is refused as its stream opens, before any deadline or signal is made: a
// render of it arms no host timer (`AbortSignal.timeout`) and composes no signal (`AbortSignal.any`) that would
// outlive it, whatever its `exactTimeout`.
test('an upload from an app file in a render arms no host timer or signal', async () => {
  const dir = scratch();
  const made = { timeout: 0, any: 0 }, { timeout, any } = AbortSignal;
  try {
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  home "/" render=request

component App
  resource sent = sent() as shape string else empty()
  view
    main
      text sent testId="sent"
`);
    writeFileSync(resolve(dir, 'app.ts'), `export const grants = 'fs.read app:/data\\nnet.fetch http://127.0.0.1:9';
export const answer = (): any => fetch('http://127.0.0.1:9/', { method: 'POST', exactBodyFrom: 'app:/data/b', exactUploadProgress: true,
  exactTimeout: 3600000, exactStream: (e: any) => e.type } as RequestInit);
`);
    const dist = await build(dir);
    const { renderer } = await import('./render.mjs');
    const render = renderer(dist); render.ahead = false;
    AbortSignal.timeout = ms => { made.timeout++; return timeout.call(AbortSignal, ms); };
    AbortSignal.any = signals => { made.any++; return any.call(AbortSignal, signals); };
    const reply = await render('/', 1000);
    const checkpoint = JSON.parse(/<script type="application\/vnd.exact.checkpoint"[^>]*>([^<]*)<\/script>/.exec(reply.html)[1]);
    expect({ status: reply.status, settled: reply.settled, pending: checkpoint.pending, refused: reply.refused, made })
      .toEqual({ status: 200, settled: true, pending: ['sent'], refused: [['sent', 'exactBodyFrom app:/data/b']], made: { timeout: 0, any: 0 } });
  } finally {
    Object.assign(AbortSignal, { timeout, any });
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);
