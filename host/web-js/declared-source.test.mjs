// Sources declared in Contract on the JS target (fetches.js, LLP 1162.001): a page with no data module
// asks a server of this machine with the request the declaration writes, reads the reply into the shape field by
// field, says a failure as a TypeScript `fetch` would, and posts a send's parameters as JSON. A release admits the
// declaration's origin, though no module grants anything (build.mjs `admitsNothing`).
import { test, expect } from 'bun:test';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { runTests } from '../../scripts/agent-test.mjs';
import { webCompiler } from './module.mjs';

const contract = origin => `shape Current
  temperature_2m: number
  wind: option<number>

shape Ack
  id: string

source weather(place: string) as shape Current
  url "${origin}/forecast?place={place}"
  pointer "/current"

source save(title: string, done: bool) as shape Ack
  url "${origin}/items"
  method "POST"
  body json

component DeclaredSource
  state place = "Palo Alto & Co"
  resource now = weather(place) as shape Current
  mutation saved as shape Ack
  derive code = match failure(now) { case some(f) => f.code, case none => "" }
  derive why = match failure(now) { case some(f) => f.message, case none => "" }
  derive savedId = match saved { case some(a) => a.id, case none => "" }
  derive windText = match now.wind { case some(w) => w.toString(), case none => "none" }
  action move
    place = "down"
  action add
    send saved = save("Milk", false)
  view
    column
      text \`\${now.temperature_2m}\` testId="temp"
      text windText testId="wind"
      button press=move testId="move"
        text "Move"
      button press=add testId="add"
        text "Add"
`;

const TESTS = `test "a declared read and send, and a status that fails"
  clock data
  expect text "temp" == "12.5"
  expect text "wind" == "none"
  tap "add"
  clock data
  expect state savedId == "7"
  tap "move"
  clock data
  expect state code == "error"
  expect text "temp" == "12.5"
`;

test('a declared source asks, reads and fails as a TypeScript fetch would, on the JS target', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..');
  const seen = [];
  const cors = { 'access-control-allow-origin': '*', 'access-control-allow-headers': '*' };
  const api = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
    if (req.method === 'OPTIONS') return new Response(null, { headers: cors });
    const url = new URL(req.url);
    seen.push([req.method, url.pathname + url.search, req.headers.get('accept'), req.method === 'POST' ? await req.text() : '']);
    if (url.pathname === '/items') return Response.json({ id: '7', title: 'Milk' }, { status: 201, headers: cors });
    if (url.searchParams.get('place') === 'down') return new Response('down', { status: 503, headers: cors });
    return Response.json({ latitude: 1, current: { temperature_2m: 12.5, time: 'now' } }, { headers: cors });
  } });
  let dir, page;
  try {
    mkdirSync(resolve(root, 'target'), { recursive: true });
    dir = mkdtempSync(resolve(root, 'target/web-js-declared-source-'));
    const origin = `http://127.0.0.1:${api.port}`;
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Declared source', id: 'test.declared-source', app: { id: 'test.declared-source', name: 'Declared source' } }));
    writeFileSync(resolve(dir, 'app.contract'), contract(origin));
    const build = async (out, args) => {
      const run = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'declared-source', '--out', out, '--render', 'none', ...args], {
        cwd: root, env: { ...process.env, EXACT_APP_DIR: dir, ...(args.includes('--production') ? { EXACT_UPDATE_TRUST: 'production' } : {}) }, stdout: 'pipe', stderr: 'pipe',
      });
      const [status, stdout, stderr] = await Promise.all([run.exited, new Response(run.stdout).text(), new Response(run.stderr).text()]);
      expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    };
    // A release with no module keeps the grant parser: the declaration grants its origin.
    await build(resolve(dir, 'release'), ['--production']);
    expect(readFileSync(resolve(dir, 'release/.gen/grant-admission.js'), 'utf8')).not.toStartWith('// Generated: the grant admission of a page that admits nothing');
    await build(resolve(dir, 'dist'), []);
    page = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch(req) {
      const path = new URL(req.url).pathname;
      const file = Bun.file(resolve(dir, 'dist', path === '/' ? 'index.html' : path.slice(1)));
      return await file.exists() ? new Response(file) : new Response('not found', { status: 404 });
    } });
    const file = resolve(dir, 'app.test.contract');
    writeFileSync(file, TESTS);
    const ran = await runTests({ host: 'web', file, app: dir, url: page.url.href });
    expect(ran.results.map(r => [r.name, r.failures])).toEqual(ran.results.map(r => [r.name, []]));
    expect(ran.results.length).toBe(1);
    expect(seen).toEqual([
      ['GET', '/forecast?place=Palo%20Alto%20%26%20Co', 'application/json', ''],
      ['POST', '/items', 'application/json', '{"title":"Milk","done":false}'],
      ['GET', '/forecast?place=down', 'application/json', ''],
    ]);
  } finally {
    page?.stop(true);
    api.stop(true);
    if (dir) rmSync(dir, { recursive: true, force: true });
  }
}, 300_000);

// A render (`--render js`) waits for a declared source's reply, as for a TypeScript source's fetch (held.js
// `answersAtWork`): the page it serves holds the answer, not the placeholder.
test('a render waits for a declared source and serves its answer', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..');
  const api = Bun.serve({ hostname: '127.0.0.1', port: 0, async fetch() {
    await new Promise(r => setTimeout(r, 200));
    return Response.json({ current: { temperature_2m: 21.5 } });
  } });
  let dir;
  try {
    mkdirSync(resolve(root, 'target'), { recursive: true });
    dir = mkdtempSync(resolve(root, 'target/web-js-declared-render-'));
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Declared render', app: { id: 'test.declared-render', name: 'Declared render' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  home "/" render=request

shape Current
  temperature_2m: number

source weather() as shape Current
  url "http://127.0.0.1:${api.port}/forecast"
  pointer "/current"

component App
  resource now = weather() as shape Current
  view
    main
      text \`\${now.temperature_2m}\` testId="temp"
`);
    const dist = resolve(dir, 'dist');
    const built = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'declared-render', '--out', dist, '--render', 'js'], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, stdout, stderr] = await Promise.all([built.exited, new Response(built.stdout).text(), new Response(built.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    const { renderer } = await import('./render.mjs');
    const render = renderer(dist); render.ahead = false;
    const reply = await render('/', 3000);
    expect(reply.status).toBe(200);
    expect(/data-testid="temp"[^>]*>([^<]*)</.exec(reply.html)?.[1]).toBe('21.5');
  } finally {
    api.stop(true);
    if (dir) rmSync(dir, { recursive: true, force: true });
  }
}, 300_000);
