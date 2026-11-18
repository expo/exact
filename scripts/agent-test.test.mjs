// Authored expects over a tree (agent-test.mjs): a node the tree carries but the
// shipped app does not show — a header node the iOS navigation bar leaves out,
// which the agent's own chrome still paints — fails, and says why (LLP 1116 D2).
import { afterAll, test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import { createInterface } from 'node:readline';
import { Readable } from 'node:stream';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { failureMark, backendFetchPrefixes, churnLines, disabledExpectation, focusedExpectation, layoutExpectation, nativeFetchOrigins, NO_FAILURES, perfExpectation, pickHint, propertyHolds, testMutationClock, refusalExpectation, refuseDisabled, runTests, serverRequest, testScope, textExpectation, treeExpectation, unarranged } from './agent-test.mjs';
import { axFindings } from './agent-ax.mjs';
import { withEpoch } from './agent-launch.mjs';
import { serveBuildTree } from '../host/web/serve.mjs';
import { sitePages } from '../host/web-js/render.mjs';
import { fileURLToPath } from 'node:url';
import { engineTests } from './coverage.mjs';

const engineTest = await engineTests();
// A fixture's build can fail before the `try` that removes its directory.
const scratches = [];
const scratch = (prefix) => { const dir = mkdtempSync(join(tmpdir(), prefix)); scratches.push(dir); return dir; };
afterAll(() => { for (const dir of scratches) rmSync(dir, { recursive: true, force: true }); });

test('always predicates preserve JSON types, option kinds and the previous state', () => {
  const state = { slots: { n: 2, flag: false, absent: null, answer: { ok: true }, empty: [], text: '2' }, derives: { doubled: 4 } };
  const check = (name, test, rest = {}, previous) => propertyHolds({ name, test, ...rest }, state, previous);
  expect(check('n', 'whole')).toBe(true);
  expect(check('text', 'whole')).toBe(false);
  expect(check('doubled', 'at-least', { number: 4 })).toBe(true);
  expect(check('doubled', 'at-most', { number: 3 })).toBe(false);
  expect(check('text', 'at-least', { number: 1 })).toBe(false);
  expect(check('n', 'in', { values: [{ kind: 'number', value: 2 }] })).toBe(true);
  expect(check('text', 'in', { values: [{ kind: 'number', value: 2 }] })).toBe(false);
  expect(check('absent', 'in', { values: [{ kind: 'none' }] })).toBe(true);
  expect(check('answer', 'in', { values: [{ kind: 'some' }] })).toBe(true);
  expect(check('absent', 'in', { values: [{ kind: 'some' }] })).toBe(false);
  expect(check('empty', 'equal', { value: { kind: 'empty-list' } })).toBe(true);
  expect(check('missing', 'in', { values: [{ kind: 'some' }] })).toBe(false);
  expect(check('flag', 'stays', { value: { kind: 'bool', value: true } }, { slots: { flag: true } })).toBe(false);
  expect(check('flag', 'stays', { value: { kind: 'bool', value: true } }, { slots: { flag: false } })).toBe(true);
  expect(check('flag', 'stays', { value: { kind: 'bool', value: true } })).toBe(true);
});

test('device properties see boot, typing and clock changes, and reset stays between tests', async () => {
  const dir = scratch('exact-properties-'), contract = join(dir, 'app.contract'), file = join(dir, 'app.test.contract');
  const plan = join(dir, 'app.plan'), dist = join(dir, 'dist');
  writeFileSync(contract, `component App
  state ready = true
  state sessionReady = false
  action enable
    sessionReady = true
  state textValue = ""
  state n = 0
  derive doubled = n * 2
  task tick mount
    every(100, step)
  action step
    n = n + 1
  action toggle
    ready = not ready
  action edit(v: string)
    textValue = v
  view
    column
      button testId="toggle" press=toggle
        text "Toggle"
      input testId="field" value=textValue input=edit
      button "Enable" testId="enable" press=enable
`);
  writeFileSync(file, `always "ready stays"
  expect state ready stays true
always "session readiness survives reload"
  expect state sessionReady stays true
always "short input"
  expect state textValue in "" "ok"
always "small derive"
  expect state doubled <= 1
after "toggle" "unchecked"
  writes none
  command focus "field"
never refuses "toggle"
test "the three observations"
  tap "toggle"
  type "field" "bad"
  clock +100
test "a fresh history"
  type "field" "ok"
test "reload is observed"
  tap "enable"
  reload
`);
  const root = fileURLToPath(new URL('..', import.meta.url));
  const compiler = join(process.env.CARGO_TARGET_DIR ?? join(root, 'target'), 'debug', 'contract');
  const compile = spawnSync(compiler, ['build', contract, '-o', plan], { cwd: root, encoding: 'utf8' });
  expect(compile.status, compile.stderr).toBe(0);
  const build = spawnSync(process.execPath, ['host/web-js/build.mjs', 'caltrain', '--plan', plan, '--out', dist, '--render', 'none'], { cwd: root, encoding: 'utf8' });
  expect(build.status, build.stderr).toBe(0);
  const server = createServer((req, res) => serveBuildTree(dist, req, res));
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  try {
    // Each step is said before it runs and each result as it lands, in order (agent.mjs prints them as they come).
    const heard = [];
    const r = await runTests({ host: 'web', file, app: 'caltrain', url: `http://127.0.0.1:${server.address().port}/`,
      atLine: ({ index, line, last, op }) => heard.push(`${index} ${line}/${last} ${op}`), atResult: (t) => heard.push(`= ${t.name}`) });
    expect(heard).toEqual(['0 14/16 tap', '0 15/16 type', '0 16/16 clock', '= the three observations', '1 18/18 type', '= a fresh history', '2 20/21 tap', '2 21/21 reload', '= reload is observed']);
    expect(heard.filter(h => h.startsWith('= ')).length).toBe(r.results.length);
    expect([r.passed, r.failed]).toEqual([1, 2]);
    const failures = r.results[0].failures.join('\n');
    expect(failures).toContain('PROPERTY "ready stays" is false on web');
    expect(failures).toContain('at step 1: tap "toggle"');
    expect(failures).toContain('before web: slot ready true');
    expect(failures).toContain('PROPERTY "short input" is false on web');
    expect(failures).toContain('at step 3: clock +100');
    expect(failures).toContain('web: derive doubled 2');
    expect(failures).not.toContain('PROPERTY "unchecked"');
    expect(r.results[1].failures).toEqual([]);
    expect(r.results[2].failures.join('\n')).toContain('at step 2: reload');
    expect(r.results[2].failures.join('\n')).toContain('PROPERTY "session readiness survives reload" is false on web');
    writeFileSync(file, `always "false at boot"
  expect state n == 1
test "boot only"
  expect state n == 0
`);
    const boot = await runTests({ host: 'web', file, app: 'caltrain', url: `http://127.0.0.1:${server.address().port}/` });
    expect(boot.failed).toBe(1);
    writeFileSync(file, `test "bounded clock"
  clock +100
  expect state n >= 1
  expect state n <= 1
  expect state n in 0 1
  expect state textValue in "" "ok"
test "wrong lower bound"
  expect state n >= 1
`);
    const bounds = await runTests({ host: 'web', file, app: 'caltrain', url: `http://127.0.0.1:${server.address().port}/` });
    expect([bounds.passed, bounds.failed]).toEqual([1, 1]);
    expect(bounds.results[1].failures.join('\n')).toContain('n is 0, expected >= 1');

    expect(boot.results[0].failures.join('\n')).toContain('at step 0: boot');
  } finally { server.close(); rmSync(dir, { recursive: true, force: true }); }
}, 180_000);

test('the fetch-loading-error recipe passes its own test', async () => {
  const root = fileURLToPath(new URL('..', import.meta.url));
  const dir = mkdtempSync(join(root, 'target/fetch-recipe-'));
  const previousApp = process.env.EXACT_APP_DIR;
  const dist = join(dir, 'dist'), file = join(dir, 'app.test.contract');
  const posts = Array.from({ length: 10 }, (_, i) => ({ title: `Post ${i}` }));
  let requests = 0;
  const server = createServer((req, res) => {
    if (req.url === '/posts?_limit=10') {
      requests++;
      res.writeHead(200, { 'content-type': 'application/json', 'cache-control': 'no-store' });
      res.end(JSON.stringify(posts));
    } else serveBuildTree(dist, req, res);
  });
  try {
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    const origin = `http://127.0.0.1:${server.address().port}`;
    const recipe = readFileSync(join(root, 'docs/recipes/fetch-loading-error.md'), 'utf8')
      .replaceAll('https://jsonplaceholder.typicode.com', origin);
    for (const [language, name] of [['contract', 'app.contract'], ['ts', 'app.ts'], ['contract-test', 'app.test.contract']]) {
      const blocks = [...recipe.matchAll(new RegExp('^```' + language + '\\n([\\s\\S]*?)^```$', 'gm'))];
      expect(blocks.length, language).toBe(1);
      writeFileSync(join(dir, name), blocks[0][1]);
    }
    writeFileSync(join(dir, 'app.json'), JSON.stringify({
      name: 'Fetch Recipe', app: { id: 'test.fetch-recipe', name: 'Fetch Recipe' }, host: { web: {} }, rust: false,
    }));
    const build = spawnSync(process.execPath, ['host/web-js/build.mjs', 'fetch-recipe', '--out', dist, '--render', 'none'], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, encoding: 'utf8',
    });
    expect(build.status, build.stdout + build.stderr).toBe(0);
    const result = await runTests({
      host: 'web', app: dir, file, webDist: dist, url: `${origin}/`, env: { XDG_CACHE_HOME: join(dir, 'cache') },
    });
    expect(result.results.map(t => t.failures)).toEqual([[]]);
    expect([result.passed, result.failed]).toEqual([1, 0]);
    expect(requests).toBe(2);
  } finally {
    if (previousApp === undefined) delete process.env.EXACT_APP_DIR;
    else process.env.EXACT_APP_DIR = previousApp;
    await new Promise(resolve => server.close(resolve));
    rmSync(dir, { recursive: true, force: true });
  }
}, 180_000);

test('authored HTTP fixtures precede the first resource load, survive reload and reset between tests', async () => {
  const root = fileURLToPath(new URL('..', import.meta.url)), dir = scratch('exact-http-fixture-');
  const dist = join(dir, 'dist'), file = join(dir, 'app.test.contract');
  let requests = 0;
  const server = createServer((req, res) => {
    if (req.url === '/greeting') { requests++; res.writeHead(200, {'content-type': 'application/json', 'cache-control': 'no-store'}); res.end('{"text":"Live greeting"}'); }
    else serveBuildTree(dist, req, res);
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const origin = `http://127.0.0.1:${server.address().port}`, prefix = `${origin}/greeting`;
  try {
    writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'HTTP Fixture', app: {id: 'test.http-fixture', name: 'HTTP Fixture'}, host: {web: {}}, rust: false }));
    writeFileSync(join(dir, 'app.contract'), `routes nav
  home "/" render=build
shape Greeting
  text: string
component Rest
  resource greeting = greeting() as shape Greeting
  action retry
    refresh greeting
  view
    column
      if failed(greeting)
        text "Offline" testId="greeting"
      else
        text greeting.text testId="greeting"
      button "Retry" testId="retry" press=retry
`);
    writeFileSync(join(dir, 'app.ts'), `export const grants = 'net.fetch ${origin}';
export async function answer() {
  const response = await fetch('${prefix}');
  if (!response.ok) return {text: 'HTTP ' + response.status + ': ' + await response.text()};
  return await response.json();
}`);
    writeFileSync(file, `test "fixture before boot and after reload"
  answer fetch "${prefix}" with 200 header "x-fixture" "yes" {"text":"Stubbed greeting"}
  expect text "greeting" == "Stubbed greeting"
  reload
  expect text "greeting" == "Stubbed greeting"
  fail fetch "${prefix}"
  tap "retry"
  clock data
  expect text "greeting" == "Offline"
  reload
  expect text "greeting" == "Offline"
  answer fetch "${prefix}" with 503 "unavailable"
  tap "retry"
  clock data
  expect text "greeting" == "HTTP 503: unavailable"
  expect no failures
  reload
  expect text "greeting" == "HTTP 503: unavailable"
  pass fetch "${prefix}"
  reload
  expect text "greeting" == "Live greeting"
test "fresh fixture"
  answer fetch "${prefix}" with 200 {"text":"Second fixture"}
  expect text "greeting" == "Second fixture"
test "no fixture leaks"
  expect text "greeting" == "Live greeting"
test "unrelated fault preserves the rendered answer"
  fail fetch "${origin}/unrelated"
  expect text "greeting" == "Live greeting"
  reload
  expect text "greeting" == "Live greeting"
`);
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'http-fixture', '--out', dist, '--render', 'js'], {cwd: root, env: {...process.env, EXACT_APP_DIR: dir}, stdout: 'pipe', stderr: 'pipe'});
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect(status, stdout + stderr).toBe(0);
    const html = readFileSync(join(dist, 'index.html'), 'utf8');
    expect(html).toContain('Live greeting');
    expect(html).toContain('application/vnd.exact.checkpoint');
    expect(requests).toBe(1);
    const result = await runTests({host: 'web', app: dir, file, webDist: dist, url: `${origin}/`, env: {XDG_CACHE_HOME: join(dir, 'cache')}});
    expect(result.results.map(t => t.failures)).toEqual([[], [], [], []]);
    expect([result.passed, result.failed, requests]).toEqual([4, 0, 2]);
    writeFileSync(file, `test "successful transport with an invalid source answer"
  answer fetch "${prefix}" with 200 {"wrong":true}
  expect no failures
`);
    const refused = await runTests({host: 'web', app: dir, file, webDist: dist, url: `${origin}/`, env: {XDG_CACHE_HOME: join(dir, 'cache')}});
    expect(refused.failed).toBe(1);
    expect(refused.results[0].failures.join(' ')).toContain('greeting');
  } finally { await new Promise(resolve => server.close(resolve)); }
}, 180_000);

test.each(['declared', 'promise'])('%s rendered sources use HTTP fixtures at launch and reload', async kind => {
  const root = fileURLToPath(new URL('..', import.meta.url)), dir = scratch('exact-rendered-fixture-');
  const dist = join(dir, 'dist'), file = join(dir, 'app.test.contract');
  let requests = 0;
  const server = createServer((req, res) => {
    if (req.url === '/greeting/hello%20world') { requests++; res.writeHead(200, {'content-type': 'application/json', 'cache-control': 'no-store'}); res.end('{"text":"Live greeting"}'); }
    else serveBuildTree(dist, req, res);
  });
  await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
  const origin = `http://127.0.0.1:${server.address().port}`, url = `${origin}/greeting/hello%20world`;
  try {
    writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Rendered Fixture', app: {id: 'test.rendered-fixture', name: 'Rendered Fixture'}, host: {web: {}}, rust: false }));
    writeFileSync(join(dir, 'app.contract'), `routes nav
  home "/" render=build
shape Greeting
  text: string
${kind === 'declared' ? `source greeting(name: string) as shape Greeting
  url "${origin}/greeting/{name}"
` : ''}component Rest
  resource greeting = greeting("hello world") as shape Greeting
  view
    text greeting.text testId="greeting"
`);
    if (kind === 'promise') writeFileSync(join(dir, 'app.ts'), `export const grants = 'net.fetch ${origin}';
export function answer() { return Promise.resolve().then(() => fetch('${url}')).then(r => r.json()); }
`);
    writeFileSync(file, `test "fixture at launch and reload"
  answer fetch "${url}" with 200 {"text":"Stubbed greeting"}
  expect text "greeting" == "Stubbed greeting"
  reload
  expect text "greeting" == "Stubbed greeting"
`);
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'rendered-fixture', '--out', dist, '--render', 'js'], {cwd: root, env: {...process.env, EXACT_APP_DIR: dir}, stdout: 'pipe', stderr: 'pipe'});
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect(status, stdout + stderr).toBe(0);
    expect(readFileSync(join(dist, 'index.html'), 'utf8')).toContain('Live greeting');
    expect(requests).toBe(1);
    const result = await runTests({host: 'web', app: dir, file, webDist: dist, url: `${origin}/`, env: {XDG_CACHE_HOME: join(dir, 'cache')}});
    expect(result.results.map(t => t.failures)).toEqual([[]]);
    expect([result.passed, result.failed, requests]).toEqual([1, 0, 1]);
  } finally { await new Promise(resolve => server.close(resolve)); }
}, 180_000);

for (const [how, padding, browser, policy = 'request'] of [['rust', 0, 'chrome'], ['rust', 20 * 1024, 'chrome'], ['rust', 20 * 1024, 'firefox'], ['rust', 20 * 1024, 'webkit'], ['js', 0, 'chrome'], ['rust', 60 * 1024, 'chrome', 'cached'], ['js', 60 * 1024, 'chrome', 'cached']]) engineTest[browser](`${how} request rendering keeps ${padding}-byte fixtures local on ${browser} through launch and reload`, async () => {
  const root = fileURLToPath(new URL('..', import.meta.url)), dir = scratch('exact-rust-fixture-');
  const dist = join(dir, 'dist'), file = join(dir, 'app.test.contract');
  let requests = 0, configs = 0, renderer, lines, server;
  const heard = [];
  let report;
  const reported = new Promise(resolve => { report = resolve; });
  const probe = `<script>
    const present = Object.hasOwn(globalThis, '__exactLaunchFaults');
    if (top === self) fetch('/report?present=' + present);
    else parent.postMessage({fixtureProbe: true, present}, '*');
  </script>`;
  const backend = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
    if (new URL(req.url).pathname === '/probe') return new Response(probe, {headers: {'content-type': 'text/html'}});
    if (new URL(req.url).pathname === '/report') { report(new URL(req.url).searchParams.get('present') === 'true'); return new Response('ok'); }
    if (new URL(req.url).pathname === '/config') { configs++; return Response.json({text: 'Server-only config'}); }
    requests++; return Response.json({text: 'Live greeting'}, {headers: {'access-control-allow-origin': '*', 'cache-control': 'public, max-age=60'}});
  } });
  const prefix = new URL('greeting', backend.url).href;
  const stop = async () => {
    if (renderer) { renderer.kill('SIGTERM'); await renderer.exited; renderer = null; }
    lines?.close();
    if (server) { await new Promise(resolve => server.close(resolve)); server = null; }
  };
  try {
    writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Rust Fixture', app: {id: 'com.exact.caltrain', name: 'Rust Fixture'}, host: {web: {csp: false}}, rust: false }));
    writeFileSync(join(dir, 'app.contract'), `routes nav
  home "/" render=${policy}
shape Greeting
  text: string
source greeting() as shape Greeting
  url "${prefix}"
source config() as shape Greeting
  url "${new URL('config', backend.url).href}"
component App
  resource config = config() as shape Greeting
  resource greeting = greeting() as shape Greeting
  view
    column
      text greeting.text testId="greeting"
      text config.text testId="config"
`);
    const build = async production => {
      const child = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'rust-fixture', '--out', dist, '--render', how === 'js' ? 'js' : 'none', ...(production ? ['--production'] : [])], {cwd: root, env: {...process.env, EXACT_APP_DIR: dir}, stdout: 'pipe', stderr: 'pipe'});
      const [status, stdout, stderr] = await Promise.all([child.exited, new Response(child.stdout).text(), new Response(child.stderr).text()]);
      expect(status, stdout + stderr).toBe(0);
    };
    const start = async () => {
      if (how === 'js') {
        const pages = sitePages(dist);
        server = createServer({maxHeaderSize: 128 * 1024}, (req, res) => serveBuildTree(dist, req, res, {pages}));
        await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
        return `http://127.0.0.1:${server.address().port}/`;
      }
      const binary = join(process.env.CARGO_TARGET_DIR ?? join(root, 'target'), 'host-dev', 'caltrain-render');
      renderer = Bun.spawn([binary, '--serve', dist, '--port', '0'], {cwd: root, stdout: 'pipe', stderr: 'pipe'});
      const child = renderer, errors = new Response(child.stderr).text();
      lines = createInterface({input: Readable.fromWeb(child.stdout)});
      return new Promise((resolve, reject) => {
        lines.on('line', line => { heard.push(line); const url = /http:\/\/[^/]+\//.exec(line)?.[0]; if (url) resolve(url); });
        child.exited.then(async () => reject(new Error(`renderer ended: ${await errors}`)));
      });
    };
    if (how === 'rust') {
      const child = Bun.spawn(['cargo', 'build', '--profile', 'host-dev', '-p', 'caltrain-linux', '--bin', 'caltrain-render'], {cwd: root, stdout: 'pipe', stderr: 'pipe'});
      const [status, stdout, stderr] = await Promise.all([child.exited, new Response(child.stdout).text(), new Response(child.stderr).text()]);
      expect(status, stdout + stderr).toBe(0);
    }
    await build(false);
    mkdirSync(join(dist, '.exact', 'auth'), {recursive: true});
    writeFileSync(join(dist, '.exact', 'auth', 'probe.html'), probe);
    const url = await start();
    expect(await (await fetch(new URL('/.exact/auth/probe.html', url))).text()).toBe(probe);
    const live = await fetch(url);
    const html = await live.text();
    expect(live.status, html + heard.join('\n')).toBe(200);
    expect(html).toContain('Live greeting');
    expect(requests).toBe(1);
    requests = 0;
    const shell = await fetch(new URL('/?agent=1&failFetch=1', url));
    expect(shell.status).toBe(400);
    expect(await shell.text()).not.toContain('<script type="application/vnd.exact.checkpoint"');
    expect(requests).toBe(0);
    const response = {status: 200, headers: {'content-type': 'application/json', 'cache-control': 'public, max-age=60'}, body: '{"text":"Rendered fixture"}'};
    const headers = {'x-exact-faults': Buffer.from([prefix, '-', '-', 0, 1, JSON.stringify(response), 0].join('\t')).toString('base64')};
    const rendered = await fetch(new URL('/?agent=1&failFetch=1', url), {headers});
    const renderedHTML = await rendered.text();
    expect(rendered.status, renderedHTML).toBe(200);
    expect(rendered.headers.get('cache-control')).toBe('no-store');
    expect(renderedHTML).toContain('Rendered fixture');
    expect(renderedHTML).toContain('Server-only config');
    expect(renderedHTML).toContain('<script type="application/vnd.exact.checkpoint"');
    expect(requests).toBe(0);
    const body = JSON.stringify({text: 'Stubbed greeting', padding: 'é&'.repeat(padding / 3)});
    writeFileSync(file, `test "request fixture at launch and reload"
  answer fetch "${prefix}" with 200 header "cache-control" "public, max-age=60" ${body}
  expect text "greeting" == "Stubbed greeting"
  expect text "config" == "Server-only config"
  reload
  expect text "greeting" == "Stubbed greeting"
  expect text "config" == "Server-only config"
  answer fetch "${prefix}" with 200 {"text":"Replacement greeting"}
  reload
  expect text "greeting" == "Replacement greeting"
  expect text "config" == "Server-only config"
`);
    const result = await runTests({host: 'web', browser, app: dir, file, webDist: dist, url: new URL('/?appQuery=kept#anchor', url).href, env: {XDG_CACHE_HOME: join(dir, 'cache')}, atEnd: async session => {
      await session.carrier.reset({session: true});
      await session.clock('data');
      const text = await session.carrier.evaluate('document.body.textContent');
      expect(await session.carrier.evaluate('globalThis.__exactLaunchFaults')).toContain('Stubbed greeting');
      const frames = await session.carrier.evaluate(`Promise.all(${JSON.stringify([new URL('/.exact/auth/probe.html', url).href, new URL('/probe', backend.url).href])}.map(src => new Promise(resolve => {
        const frame = document.createElement('iframe');
        const receive = event => {
          if (event.source !== frame.contentWindow || !event.data.fixtureProbe) return;
          removeEventListener('message', receive); frame.remove(); resolve(event.data.present);
        };
        addEventListener('message', receive); frame.src = src; document.body.append(frame);
      })))`);
      await session.carrier.evaluate(`location.assign(${JSON.stringify(new URL('/probe', backend.url).href)})`);
      return {text, frames, external: await reported};
    }});
    expect(result.results.map(t => t.failures)).toEqual([[]]);
    expect(result.results[0].end.error).toBeUndefined();
    expect(result.results[0].end.text).toContain('Stubbed greeting');
    expect([result.results[0].end.frames, result.results[0].end.external]).toEqual([[false, false], false]);
    expect([result.passed, result.failed, requests]).toEqual([1, 0, 0]);
    expect(await (await fetch(url)).text()).toContain('Live greeting');
    expect(configs).toBeGreaterThan(1);
    if (!padding) {
      await stop();
      await build(true);
      const production = await fetch(new URL('/?agent=1&failFetch=1', await start()), {headers});
      expect(production.status).toBe(200);
      expect(await production.text()).toContain('Live greeting');
      expect(requests).toBe(1);
    }
  } finally { await stop(); backend.stop(true); }
}, 180_000);

test('successful HTTP fixtures do not arrange resource failures', async () => {
  const mark = await failureMark({
    state: async () => ({ faults: [{prefix: 'https://api.test/', hits: 1, response: {status: 200}}],
      failures: { greeting: {count: 1} }, failed: {greeting: 'Invalid greeting'} }),
    op: async () => ({next: 0}),
  });
  expect(unarranged(NO_FAILURES, mark)).toEqual(['resource greeting was refused once (Invalid greeting)']);
});

test('a pick with no picker held names the step that holds one on its host', () => {
  expect(pickHint('web')).toContain('tap the visible file input or a button');
  for (const host of ['macos', 'ios', 'linux']) {
    expect(pickHint(host)).not.toContain('tap the visible file input');
    expect(pickHint(host)).toContain('tap a button whose action calls showPicker("<input id>")');
  }
});

test('modal ancestors remain structural while exposed siblings are findings', () => {
  const elements = [
    { i: 0, parent: null, id: 10, via: 'self', role: 'main', name: '' },
    { i: 1, parent: 0, id: 11, via: 'self', role: 'group', name: '' },
    { i: 2, parent: 1, id: 12, via: 'self', role: 'dialog', name: 'Setup' },
    { i: 3, parent: 2, id: 13, via: 'self', role: 'button', name: 'Save' },
    { i: 4, parent: 1, id: 14, via: 'self', role: 'button', name: 'Behind' },
    { i: 5, parent: 0, id: 15, via: 'self', role: 'group', name: 'Sidebar' },
  ];
  for (const source of ['chrome-cdp', 'appkit']) {
    const ax = { source, elements, modal: { present: true, element: 2 } };
    expect(axFindings({ ax }).filter(f => f.kind === 'outside-modal').map(f => f.id)).toEqual([14, 15]);
  }
});

test('a presented popover belongs to its AppKit modal for outside-modal findings', () => {
  const elements = [
    { i: 0, parent: null, id: null, via: 'none', role: 'scrollarea', name: '' },
    { i: 1, parent: 0, id: 10, via: 'self', role: 'dialog', name: 'Setup' },
    { i: 2, parent: 1, id: 11, via: 'self', role: 'button', name: 'Options' },
    { i: 3, parent: 0, id: 12, via: 'self', role: 'group', name: 'Options' },
    { i: 4, parent: 3, id: 13, via: 'self', role: 'textbox', name: 'Filter' },
  ];
  const modal = { present: true, by: 'aria-modal', element: 1, presented: [3] };
  expect(axFindings({ ax: { source: 'appkit', modal, elements } }).filter(f => f.kind === 'outside-modal')).toEqual([]);
  expect(axFindings({ ax: { source: 'appkit', modal: { ...modal, presented: [] }, elements } }).filter(f => f.kind === 'outside-modal').map(f => f.id)).toEqual([12, 13]);
});

const why = 'the iOS navigation bar does not show this node; in the app it is not visible';
const nodes = [
  { id: 3, depth: 0, type: 'View', props: {} },
  { id: 4, depth: 1, type: 'Text', props: { text: 'Shopping List' } },
  { id: 5, depth: 1, type: 'Text', props: { text: '2 items left', testId: 'count' } },
  { id: 6, depth: 1, type: 'Text', props: { text: 'Shared', testId: 'tagline' }, unshown: why },
];

test('a text the bar shows passes; one it leaves out fails naming why', () => {
  expect(textExpectation(nodes, 'count', '2 items left')).toBeNull();
  expect(textExpectation(nodes, 'count', '1 item left').unshown).toBeUndefined();
  const f = textExpectation(nodes, 'tagline', 'Shared');
  expect(f.unshown).toBe(true);
  expect(f.message).toBe(`text of "tagline" is "Shared", but ${why}`);
  // A wrong text is the ordinary failure, which a clock step may still settle.
  expect(textExpectation(nodes, 'tagline', 'Other').unshown).toBeUndefined();
});

test('present means shown in the app; missing is the tree\'s', () => {
  expect(treeExpectation(nodes, 'count', true)).toBeNull();
  expect(treeExpectation(nodes, 'tagline', true)).toEqual({ message: `testId "tagline" is in the tree, but ${why}`, unshown: true });
  expect(treeExpectation(nodes, 'gone', true).message).toBe('expected testId "gone" present, it was absent');
  expect(treeExpectation(nodes, 'gone', false)).toBeNull();
  expect(treeExpectation(nodes, 'tagline', false).message).toBe('expected testId "tagline" absent, it was present');
});

test('expect focused reads the node every host\'s tree marks focused, and names what holds the focus instead', () => {
  const tree = [{ id: 1, type: 'Pressable', props: { testId: 'picker' } }, { id: 2, type: 'Pressable', props: { testId: 'row' }, focused: true }, { id: 3, type: 'Pressable', props: { testId: 'row' } }, { id: 4, type: 'View', props: {} }];
  expect(focusedExpectation(tree, 'row')).toBeNull();
  expect(focusedExpectation(tree, 'picker').message).toBe('expected "picker" focused, "row" has it');
  expect(focusedExpectation(tree, 'gone').message).toBe('expected "gone" focused, no view carries that testId');
  expect(focusedExpectation([tree[0], { ...tree[3], focused: true }], 'picker').message).toBe('expected "picker" focused, View#4 has it');
  expect(focusedExpectation([tree[0]], 'picker').message).toBe('expected "picker" focused, nothing has the focus');
});

test('expect layout bounds the width or height `layout <target>` prints, and says the size it found', async () => {
  const tree = [{ id: 2, type: 'Image', props: { testId: 'status-mark' } }, { id: 6, type: 'Image', props: { testId: 'fixed-mark' } }, { id: 7, type: 'Text', props: { testId: 'tagline' }, unshown: why }];
  const boxes = { 2: { x: 0, y: 0, w: 420, h: 309.778 }, 6: { x: 0, y: 0, w: 47, h: 19 } }, asked = [];
  const s = { layout: async (id) => { asked.push(id); return { node: { space: boxes[id] ? { viewport: boxes[id] } : id === 8 ? { unavailable: 'native swipe action is not revealed' } : {} } }; } };
  const step = (target, dimension, comparison, value) => layoutExpectation(s, tree, { target, dimension, comparison, value });
  expect(await step('fixed-mark', 'height', 'at-most', 24)).toBeNull();
  expect(await step('fixed-mark', 'width', 'at-least', 47)).toBeNull();
  expect(await step('fixed-mark', 'height', 'equal', 19)).toBeNull();
  expect(await step('status-mark', 'height', 'equal', 309.78)).toBeNull(); // the two decimals a layout reply carries
  expect((await step('status-mark', 'height', 'at-most', 24)).message).toBe('height of "status-mark" is 309.78, expected <= 24 (its box is 420×309.78)');
  expect((await step('status-mark', 'width', 'equal', 47)).message).toBe('width of "status-mark" is 420, expected 47 (its box is 420×309.78)');
  expect((await step('fixed-mark', 'width', 'at-least', 64)).message).toBe('width of "fixed-mark" is 47, expected >= 64 (its box is 47×19)');
  expect(asked).toEqual([6, 6, 6, 2, 2, 2, 6]);
  expect((await step('gone', 'height', 'at-most', 24)).message).toBe('height of "gone" is absent (no view carries that testId)');
  expect(await step('tagline', 'height', 'at-most', 24)).toEqual({ message: `testId "tagline" is in the tree, but ${why}`, unshown: true });
  const boxless = (id) => layoutExpectation(s, [{ id, type: 'View', props: { testId: 'boxless' } }], { target: 'boxless', dimension: 'width', comparison: 'at-most', value: 1 });
  expect((await boxless(8)).message).toBe('width of "boxless" is unknown: native swipe action is not revealed');
  expect((await boxless(9)).message).toBe('width of "boxless" is unknown: the host reports no box for it');
});

// LLP 1116.010 D3 (chat-rooms and password-generator, round 12): a disabled control is asserted, and pressing one fails the step.
test('expect disabled and enabled read the prop every host\'s tree carries; a press is refused while context eligibility follows the element type', async () => {
  const tree = [{ id: 1, type: 'Pressable', props: { testId: 'send', disabled: true } }, { id: 2, props: { testId: 'arm' } }, { id: 3, props: { testId: 'native', disabled: 'true' } }, { id: 4, props: { testId: 'update', accessibilityDisabled: true } },
    { id: 5, type: 'View', props: { disabled: true } }, { id: 6, type: 'Pressable', props: { disabled: true, href: '/help' } },
    { id: 7, type: 'TextInput', props: { disabled: true } }, { id: 8, type: 'Control', props: { disabled: true } }, { id: 9, type: 'Pressable', props: { disabled: true, href: '' } }];
  expect(disabledExpectation(tree, 'send', true)).toBeNull();
  expect(disabledExpectation(tree, 'native', true)).toBeNull();
  // `aria-disabled` reads as disabled for the expectation…
  expect(disabledExpectation(tree, 'update', true)).toBeNull();
  expect(disabledExpectation(tree, 'update', false).message).toBe('expected "update" enabled, it was disabled');
  expect(disabledExpectation(tree, 'arm', false)).toBeNull();
  expect(disabledExpectation(tree, 'send', false).message).toBe('expected "send" enabled, it was disabled');
  expect(disabledExpectation(tree, 'arm', true).message).toBe('expected "arm" disabled, it was enabled');
  expect(disabledExpectation(tree, 'gone', true).message).toBe('expected "gone" disabled, no view carries that testId');
  const s = { op: async (req) => ({ nodes: tree.filter((n) => n.id === req.target) }) };
  await expect(refuseDisabled(s, { id: 1 }, 'send', {})).rejects.toThrow('send is disabled: a press does nothing');
  await expect(refuseDisabled(s, { id: 3 }, 'native', { dblclick: true })).rejects.toThrow('native is disabled');
  await refuseDisabled(s, { id: 2 }, 'arm', {});
  // …but its press still dispatches, so the tap is not refused.
  await refuseDisabled(s, { id: 4 }, 'update', {});
  await refuseDisabled(s, { id: 1 }, 'send', { hover: true });
  for (const id of [5, 6]) await refuseDisabled(s, { id }, String(id), { contextmenu: true });
  for (const id of [1, 7, 8, 9]) await expect(refuseDisabled(s, { id }, String(id), { contextmenu: true })).rejects.toThrow('is disabled');
  await expect(refuseDisabled(s, { id: 5 }, 'container', { contextmenu: false })).rejects.toThrow('container is disabled');
  await refuseDisabled(s, { id: 1, entity: 'unit' }, 'send', {});
});

test('an active screen\'s node is read before a covered one\'s', () => {
  const covered = [{ id: 9, depth: 0, type: 'Text', props: { text: 'x', testId: 'count' }, inactive: true, unshown: why }, ...nodes];
  expect(textExpectation(covered, 'count', '2 items left')).toBeNull();
});

// LLP 1116.006 D8: a test's `request` line is the driver calling the app's server, the persona's name its bearer token.
test('a request line sends the method, the JSON body and the persona\'s token under the server URL; a refusal says its status and body', async () => {
  const seen = [];
  const server = Bun.serve({ port: 0, hostname: '127.0.0.1', async fetch(req) {
    const url = new URL(req.url);
    seen.push([req.method, url.pathname, req.headers.get('authorization'), req.headers.get('content-type'), await req.text()]);
    return url.pathname === '/refused' ? Response.json({ error: 'Sign in again.' }, { status: 401 }) : url.pathname === '/gone' ? new Response(null, { status: 404 }) : Response.json({ ok: true }, { status: 201 });
  } });
  const base = `http://127.0.0.1:${server.port}`;
  try {
    await serverRequest(base, { method: 'POST', path: '/polls/1/votes', body: '{"option":2}', as: 'bob' });
    await serverRequest(base, { method: 'GET', path: `${base}/health`, body: null, as: null });
    expect(seen).toEqual([['POST', '/polls/1/votes', 'Bearer bob', 'application/json', '{"option":2}'], ['GET', '/health', null, null, '']]);
    await expect(serverRequest(base, { method: 'PATCH', path: '/refused', body: '{}', as: 'mallory' })).rejects.toThrow(`request PATCH ${base}/refused as mallory: the server answered 401 {"error":"Sign in again."}`);
    await expect(serverRequest(base, { method: 'DELETE', path: '/gone', body: null, as: 'bob' })).rejects.toThrow(`request DELETE ${base}/gone as bob: the server answered 404`);
    await expect(serverRequest(null, { method: 'GET', path: '/items', body: null, as: null })).rejects.toThrow('app.ts calls no local server');
    // LLP 1116.010 D3 (polls, round 12): `expect <status>` passes on that status and fails on any other, a success included.
    await serverRequest(base, { method: 'DELETE', path: '/gone', body: null, as: 'bob', expect: 404 });
    await serverRequest(base, { method: 'PATCH', path: '/refused', body: '{}', as: 'mallory', expect: 401 });
    await expect(serverRequest(base, { method: 'DELETE', path: '/gone', body: null, as: 'bob', expect: 409 })).rejects.toThrow(`request DELETE ${base}/gone as bob: the server answered 404, expected 409`);
    await expect(serverRequest(base, { method: 'POST', path: '/items', body: '{}', as: 'bob', expect: 409 })).rejects.toThrow('the server answered 201 {"ok":true}, expected 409');
  } finally { await server.stop(true); }
  await expect(serverRequest(base, { method: 'GET', path: '/items', body: null, as: null })).rejects.toThrow(/nothing answered/);
});

// LLP 1116.009 D1: a `reload` relaunches at the virtual time the test had reached (the date it launched at plus every
// `clock +N`), not at its launch date: a phone restarted at 9:05 does not wake at 9:00 (habit-tracker, rounds 10 and 11).
test('the page address a reload loads takes the date it relaunches at, and keeps the rest', () => {
  const href = 'http://127.0.0.1:9/detail/3?agent=1&seed=1&epoch=1773489600000&timeZone=UTC&q=a';
  expect(new URL(withEpoch(href, 1773576000000)).searchParams.get('epoch')).toBe('1773576000000');
  expect(new URL(withEpoch(href, 1773576000000)).searchParams.get('q')).toBe('a');
  expect(new URL(withEpoch('http://127.0.0.1:9/', 5)).searchParams.get('epoch')).toBe('5');
  expect(withEpoch(href, undefined)).toBe(href);
});

test('a reload keeps the clock the test had reached, again and again, and the run ends with it', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-reload-clock-')), contract = join(dir, 'app.contract'), plan = join(dir, 'app.plan'), dist = join(dir, 'dist'), file = join(dir, 'app.test.contract');
  writeFileSync(contract, `shape Time
  epochAtZero: number
  utcOffset: number

component Day
  state now = 0
  resource time = exactTime() as shape Time
  derive day = formatDate(time.epochAtZero + now, time.utcOffset, "iso")
  task tick mount
    every(60000, step)
  action step
    now = performanceNow()
  view
    column testId="root"
      text day testId="day"
`);
  writeFileSync(file, `test "a relaunch is on the day the test reached"
  epoch "2026-03-14T12:00:00Z"
  expect text "day" == "2026-03-14"
  clock +86400000
  clock data
  expect text "day" == "2026-03-15"
  reload
  expect text "day" == "2026-03-15"
  clock +86400000
  clock data
  expect text "day" == "2026-03-16"
  reload
  expect text "day" == "2026-03-16"
`);
  const root = fileURLToPath(new URL('..', import.meta.url));
  const compiler = join(process.env.CARGO_TARGET_DIR ?? join(root, 'target'), 'debug', process.platform === 'win32' ? 'contract.exe' : 'contract');
  const compile = spawnSync(compiler, ['build', contract, '-o', plan], { cwd: root, encoding: 'utf8' });
  expect(compile.status, compile.stderr).toBe(0);
  const build = spawnSync(process.execPath, ['host/web-js/build.mjs', 'caltrain', '--plan', plan, '--out', dist, '--render', 'none'], { cwd: root, encoding: 'utf8' });
  expect(build.status, build.stderr).toBe(0);
  const server = createServer((request, response) => serveBuildTree(dist, request, response));
  await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
  const ends = [];
  try {
    const r = await runTests({ host: 'web', file, app: 'caltrain', url: `http://127.0.0.1:${server.address().port}/`, size: [420, 900], atEnd: async (s, t) => { ends.push([t.launch.epoch, t.reached]); return {}; } });
    expect(r.results.map((x) => x.failures)).toEqual([[]]);
    expect(r.passed).toBe(1);
    // verify's relaunch starts where the test left off too: the test names the date it launched at and the date it had reached.
    expect(ends).toEqual([[Date.UTC(2026, 2, 14, 12), Date.UTC(2026, 2, 16, 12)]]);
  } finally {
    server.close();
    rmSync(dir, { recursive: true, force: true });
  }
}, 180_000);


test('a web reload preserves browser history and a new test isolates it', async () => {
  const root = fileURLToPath(new URL('..', import.meta.url));
  const dir = mkdtempSync(join(root, 'target/reload-history-'));
  const contract = join(dir, 'app.contract'), plan = join(dir, 'app.plan'), dist = join(dir, 'dist'), file = join(dir, 'app.test.contract');
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'History', app: { id: 'test.exact.history', name: 'History' }, host: {}, deploy: {} }));
  writeFileSync(contract, `routes nav
  home "/"
    item "/item/:id"

component App
  action navigate(url: string)
    nav = go(nav, url)
  derive url = top(nav).url
  derive depth = stack(nav).length
  view
    main testId="root" navigationKey=\`\${top(nav).id}\` navigationBack="back" navigate=navigate
      for e in stack(nav) key=e.id
        column navigationKey=\`\${e.id}\`
          text e.url
`);
  // A reload restores the earlier visits as well as the top route (#527).
  writeFileSync(file, `test "reload keeps earlier visits" on web
  type "root" "/item/one"
  expect state depth == 2
  type "root" "/item/two"
  expect state depth == 3
  tap "root" history -1
  expect state url == "/item/one"
  tap "root" history 1
  expect state url == "/item/two"
  clock +60000
  reload
  expect state url == "/item/two"
  expect state depth == 3
  tap "root" history -1
  expect state url == "/item/one"
  tap "root" history -1
  expect state url == "/"
  tap "root" history 2
  expect state url == "/item/two"
  tap "root" history -1
  expect state url == "/item/one"
  reload
  tap "root" history 1
  expect state url == "/item/two"
test "a separate test has its own history" on web
  expect state url == "/"
`);
  const compiler = join(process.env.CARGO_TARGET_DIR ?? join(root, 'target'), 'debug', 'contract');
  let server;
  try {
    const compile = spawnSync(compiler, ['build', contract, '-o', plan], { cwd: root, encoding: 'utf8' });
    expect(compile.status, compile.stderr).toBe(0);
    const build = spawnSync(process.execPath, ['host/web-js/build.mjs', 'history', '--plan', plan, '--out', dist, '--render', 'none'], { cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, encoding: 'utf8' });
    expect(build.status, build.stderr).toBe(0);
    server = createServer((request, response) => serveBuildTree(dist, request, response));
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    const histories = [], ends = [];
    const result = await runTests({ host: 'web', file, app: 'caltrain', url: `http://127.0.0.1:${server.address().port}/`,
      atStep: async (s, step) => { if (['type', 'reload'].includes(step.op)) histories.push(await s.carrier.call('Page.getNavigationHistory')); },
      atEnd: async (s, t) => {
        ends.push(await s.carrier.call('Page.getNavigationHistory'));
        if (t.index === 0) {
          await s.carrier.reset({ session: true });
          ends.push(await s.carrier.call('Page.getNavigationHistory'));
        }
        return {};
      },
    });
    expect(result.results.map(t => t.failures)).toEqual([[], []]);
    expect(result.results.map(t => t.end)).toEqual([{}, {}]);
    const paths = h => h.entries.map(e => new URL(e.url).pathname);
    expect(paths(histories[0])).toEqual(['/', '/item/one']);
    expect(paths(histories[1])).toEqual(['/', '/item/one', '/item/two']);
    for (const history of histories.slice(2)) {
      expect(paths(history)).toEqual(paths(histories[1]));
      expect(history.entries.map(e => e.id)).toEqual(histories[1].entries.map(e => e.id));
    }
    expect(histories.map(h => h.currentIndex)).toEqual([1, 2, 2, 1]);
    expect(paths(ends[1])).toEqual(['/']);
    expect(paths(ends[2])).toEqual(['/']);
    const { open } = await import('./agent.mjs');
    const session = await open({ host: 'web', url: `http://127.0.0.1:${server.address().port}/item/7#moves` });
    try {
      expect((await session.state()).derives.url).toBe('/item/7');
      await session.carrier.evaluate('globalThis.beforeSessionReset = true');
      await session.carrier.reset({ session: true });
      expect(await session.carrier.evaluate('globalThis.beforeSessionReset ?? false')).toBe(false);
      expect((await session.state()).derives.url).toBe('/item/7');
      const history = await session.carrier.call('Page.getNavigationHistory');
      expect(history.currentIndex).toBe(0);
      expect(history.entries.map(e => new URL(e.url).pathname + new URL(e.url).hash)).toEqual(['/item/7#moves']);
    } finally { await session.close(); }
  } finally { server?.close(); rmSync(dir, { recursive: true, force: true }); }
}, 180_000);

test('expect refusal asserts an observed mutation failure and preserves unrelated wiring findings', async () => {
  const {refusalsOf} = await import('./verify.mjs');
  const lines=[
    't=1 refused reply saved; wall 3 ms: Not permitted',
    't=1 request 4 (saved) failed and is no longer pending: it ends unsent: Not permitted',
    't=2 refused reply saved; wall 2 ms: outside its shape',
    't=2 request 5 (saved) failed and is no longer pending: it is not retried: outside its shape',
    't=3 refused reply other; wall 1 ms: Not permitted',
    't=3 request 6 (other) failed and is no longer pending: it ends unsent: Not permitted',
    't=4 showModal absent refused: no dialog has that id',
    't=5 request 7 (resource) failed and is no longer pending: it keeps its last value: Offline',
  ];
  const first=refusalExpectation({lines:lines.slice(0,2)},'saved');
  expect(first).toEqual({expected:[lines[1],lines[0]]});
  expect(refusalsOf({lines},first.expected)).toEqual([
    'refused reply saved; wall 2 ms: outside its shape',
    'refused reply other; wall 1 ms: Not permitted',
    'showModal absent refused: no dialog has that id',
  ]);
  expect(refusalExpectation({lines},'resource').message).toContain('no refused reply was observed');
  expect(refusalExpectation({lines:[]},'saved').message).toContain('clock data');
  // The same assertion cannot consume a refusal before the preceding assertion of the same slot's log cursor.
  expect(refusalExpectation({lines:lines.slice(2)},'saved').expected).toEqual([lines[3],lines[2]]);
  const synchronous='t=6 send saved failed: Refused on purpose; it ends unsent';
  const sync=refusalExpectation({lines:[...lines,synchronous]},'saved');
  expect(sync).toEqual({expected:[synchronous]});
  expect(refusalsOf({lines:[synchronous]},sync.expected)).toEqual([]);
  const repeated=[...lines.slice(0,2),...lines.slice(0,2)];
  expect(refusalsOf({lines:repeated},first.expected)).toEqual(['refused reply saved; wall 3 ms: Not permitted']);
  const mirror='exact: refused reply saved; wall 3 ms: Not permitted';
  expect(refusalsOf({lines:lines.slice(0,2),host:[mirror]},first.expected)).toEqual([]);
  expect(refusalsOf({lines:lines.slice(0,2),host:[mirror,mirror]},first.expected)).toEqual(['refused reply saved; wall 3 ms: Not permitted']);
  expect(refusalsOf({lines:repeated,host:[mirror,mirror,mirror]},[...first.expected,...first.expected])).toEqual(['refused reply saved; wall 3 ms: Not permitted']);
});


test('an authored expected refusal keeps a prior send value, skips then, and is accepted by verify', async () => {
  const {refusalsOf} = await import('./verify.mjs');
  const dir=mkdtempSync(join(tmpdir(),'exact-expected-refusal-')), dist=join(dir,'dist'), file=join(dir,'app.test.contract');
  writeFileSync(join(dir,'app.json'),JSON.stringify({name:'Refusal',app:{id:'test.exact.refusal',name:'Refusal'},host:{},deploy:{}}));
  writeFileSync(join(dir,'app.contract'),`shape Saved
  value: string
component App
  state completions = 0
  mutation saved as shape Saved then completed
  derive stored = match saved { case some(value) => value.value, case none => "empty" }
  derive error = match failure(saved) { case some(value) => value.code, case none => "none" }
  action completed
    completions = completions + 1
  action accept
    send saved = write(false)
  action refuse
    send saved = write(true)
  action refuseNow
    send saved = reject()
  view
    column
      button "Accept" press=accept testId="accept"
      button "Refuse" press=refuse testId="refuse"
      button "Refuse immediately" press=refuseNow testId="refuse-now"
      text stored testId="stored"
      text error testId="error"
`);
  writeFileSync(join(dir,'app.ts'),`import type {Answer} from './app.contract.d.ts';
export const appId='test.exact.refusal';
export const grants='';
export const answer: Answer=(_source,args)=>{
  const refused=()=>Object.assign(new Error('Refused on purpose'),{reason:'DENIED'});
  if(_source==='reject') throw refused();
  return Promise.resolve().then(()=>{
    if(args[0]) throw refused();
    return {value:'kept'};
  });
};
`);
  writeFileSync(file,`test "a refused send keeps the previous reply"
  tap "accept"
  clock data
  expect text "stored" == "kept"
  expect state completions == 1
  tap "refuse"
  clock data
  expect refusal "saved"
  expect text "stored" == "kept"
  expect text "error" == "DENIED"
  expect state completions == 1

test "an expected refusal before reload does not accept the next journal"
  tap "refuse-now"
  clock data
  expect refusal "saved"
  reload
  tap "refuse-now"
  clock data
  expect text "error" == "DENIED"
`);
  const root=fileURLToPath(new URL('..',import.meta.url));
  const build=spawnSync(process.execPath,['host/web-js/build.mjs','refusal','--out',dist,'--render','none'],{cwd:root,env:{...process.env,EXACT_APP_DIR:dir},encoding:'utf8'});
  expect(build.status,build.stderr).toBe(0);
  const server=createServer((req,res)=>serveBuildTree(dist,req,res));
  await new Promise(ok=>server.listen(0,'127.0.0.1',ok));
  const findings=[], observed=[];
  try {
    const r=await runTests({host:'web',file,app:'caltrain',url:`http://127.0.0.1:${server.address().port}/`,atEnd:async(s,t)=>{
      const logs=await s.logs();
      observed.push(refusalsOf(logs).length);
      findings.push({expected:t.expectedRefusals,refused:refusalsOf(logs,t.expectedRefusals),lines:logs.lines});
      return {};
    }});
    expect(r.results.map(t=>t.failures)).toEqual([[],[]]);
    expect(r.passed).toBe(2);
    expect(observed).toHaveLength(2);
    expect(observed.every(count=>count>0)).toBe(true);
    expect(r.results.map(t=>t.end?.error)).toEqual([undefined,undefined]);
    expect(findings[0].refused).toEqual([]);
    expect(findings[1].expected).toEqual([]);
    expect(findings[1].refused).toEqual(['send saved failed: Refused on purpose; it ends unsent']);
    expect(findings[1].lines).toContain('t=0 send saved failed: Refused on purpose; it ends unsent');
  } finally { server.close(); rmSync(dir,{recursive:true,force:true}); }
},180_000);

test('authored preferences use the drive parser, override by name, and survive reload', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-test-media-'));
  const contract = join(dir, 'app.contract'), plan = join(dir, 'app.plan'), dist = join(dir, 'dist'), file = join(dir, 'app.test.contract');
  writeFileSync(contract, `shape Viewport
  prefersColorScheme: string
  prefersReducedMotion: bool
  displayMode: string
shape Page
  onLine: bool
component App
  resource page = exactPage() as shape Page
  resource viewport = exactViewport() as shape Viewport
  view
    column
      text viewport.prefersColorScheme testId="scheme"

`);
  writeFileSync(file, `prefer prefers-color-scheme dark
prefer prefers-reduced-motion reduce

test "inherits"
  before data
  expect text "scheme" == "dark"
  expect state viewport.prefersReducedMotion == true
  reload
  expect text "scheme" == "dark"
  prefer prefers-color-scheme light
  prefer display-mode standalone
  expect text "scheme" == "light"
  expect state viewport.displayMode == "standalone"
  reload
  expect text "scheme" == "light"
  expect state viewport.displayMode == "standalone"
  prefer prefers-color-scheme dark
  expect text "scheme" == "dark"
  prefer prefers-color-scheme light

test "overrides only scheme"
  prefer prefers-color-scheme light
  expect text "scheme" == "light"
  expect state viewport.prefersReducedMotion == true

test "invalid launch value"
  prefer prefers-color-scheme banana
  expect text "scheme" == "dark"

test "invalid launch name"
  prefer unknown dark
  expect text "scheme" == "dark"

test "invalid later value"
  expect text "scheme" == "dark"
  prefer prefers-color-scheme banana

test "page facts use the drive parser"
  prefer online false
  expect state page.onLine == false
  reload
  expect state page.onLine == false
  prefer online true
  expect state page.onLine == true
  reload
  expect state page.onLine == true
  prefer online false

test "a failure does not stop the next test"
  expect text "scheme" == "dark"

test "large text at launch survives reload"
  prefer root-font-size "24"
  expect text "scheme" == "dark"
  reload

test "large text can change between steps"
  expect text "scheme" == "dark"
  prefer root-font-size "28px"
  reload

test "invalid text size"
  prefer root-font-size "0"

test "text size does not leak into another test"
  expect text "scheme" == "dark"

test "a host refuses an unsupported page fact"
  prefer keyboard-navigation true
`);
  const root = fileURLToPath(new URL('..', import.meta.url));
  const compile = spawnSync(join(process.env.CARGO_TARGET_DIR ?? join(root, 'target'), 'debug', 'contract'), ['build', contract, '-o', plan], { cwd: root, encoding: 'utf8' });
  expect(compile.status, compile.stderr).toBe(0);
  const build = spawnSync(process.execPath, ['host/web-js/build.mjs', 'caltrain', '--plan', plan, '--out', dist, '--render', 'none'], { cwd: root, encoding: 'utf8' });
  expect(build.status, build.stderr).toBe(0);
  const server = createServer((request, response) => serveBuildTree(dist, request, response));
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  const ends = [];
  try {
    const r = await runTests({ host: 'web', file, app: 'caltrain', url: `http://127.0.0.1:${server.address().port}/`, atEnd: async (s, t) => {
      if (t.failures.length) return {};
      const dark = t.launch.media['prefers-color-scheme'] === 'dark';
      expect(await s.carrier.evaluate("matchMedia('(prefers-color-scheme: dark)').matches")).toBe(dark);
      const fontSize = parseFloat(t.launch.media['root-font-size'] ?? '16');
      const rootSize = () => s.carrier.evaluate('parseFloat(getComputedStyle(document.documentElement).fontSize)');
      expect(await rootSize()).toBe(fontSize);
      await t.restart();
      expect(textExpectation((await s.tree()).nodes, 'scheme', dark ? 'dark' : 'light')).toBeNull();
      expect(await rootSize()).toBe(fontSize);
      if (t.launch.media.online != null) expect((await s.state()).resources.page.onLine).toBe(false);
      ends.push(t.launch.media);
      return {};
    } });
    expect(r.results.map(t => t.end?.error)).toEqual(Array(12).fill(undefined));
    expect(r.results.map(t => t.failures.length)).toEqual([0, 0, 1, 1, 1, 0, 0, 0, 0, 1, 0, 1]);
    for (const [index, line, name, value] of [[2, 27, 'prefers-color-scheme', 'banana'], [3, 31, 'unknown', 'dark'], [4, 36, 'prefers-color-scheme', 'banana']]) {
      const hint = name === 'unknown' ? 'did you mean `prefer prefers-color-scheme dark`? ' : '';
      expect(r.results[index].failures[0]).toContain(`line ${line}: prefer: ${name} ${value}: ${hint}expected`);
    }
    expect(r.results[9].failures[0]).toContain('prefer: root-font-size 0: expected');
    expect(r.results[11].failures[0]).toContain('prefer: keyboard-navigation: true is not a page fact this host sets');
    expect(r.results[11].failures[0]).not.toContain('test prefer:');
    expect(ends).toEqual([
      { 'prefers-color-scheme': 'light', 'prefers-reduced-motion': 'reduce', 'display-mode': 'standalone' },
      { 'prefers-color-scheme': 'light', 'prefers-reduced-motion': 'reduce' },
      { 'prefers-color-scheme': 'dark', 'prefers-reduced-motion': 'reduce', online: false },
      { 'prefers-color-scheme': 'dark', 'prefers-reduced-motion': 'reduce' },
      { 'prefers-color-scheme': 'dark', 'prefers-reduced-motion': 'reduce', 'root-font-size': 24 },
      { 'prefers-color-scheme': 'dark', 'prefers-reduced-motion': 'reduce', 'root-font-size': 28 },
      { 'prefers-color-scheme': 'dark', 'prefers-reduced-motion': 'reduce' },
    ]);
    for (const [inherited, own] of [['displayScale', 'display-scale'], ['display-scale', 'displayScale']]) {
      writeFileSync(file, `prefer ${inherited} "2"
test "scale override before launch"
  prefer ${own} "1"
  expect text "scheme" == "light"
  reload
  expect text "scheme" == "light"
`);
      const overridden = await runTests({ host: 'web', file, app: 'caltrain', url: `http://127.0.0.1:${server.address().port}/` });
      expect(overridden.results.map(t => t.failures)).toEqual([[]]);
    }
  } finally { server.close(); rmSync(dir, { recursive: true, force: true }); }
}, 180_000);

test('authored wakelock expectations follow the holder record and name holders on failure', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-test-wakelock-'));
  const contract = join(dir, 'app.contract'), plan = join(dir, 'app.plan'), dist = join(dir, 'dist'), file = join(dir, 'app.test.contract');
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Wake Lock', app: { id: 'test.exact.wakelock', name: 'Wake Lock' }, host: {}, deploy: {} }));
  writeFileSync(contract, `component App
  state on = true
  action toggle
    on = !on
  view
    column wakelock=(on ? "screen" : "none") testId="awake"
      button "Toggle" press=toggle testId="toggle"
`);
  writeFileSync(file, `test "toggle the holder"
  expect wakelock held
  tap "toggle"
  expect wakelock released
  tap "toggle"
  expect wakelock held

test "a holder prevents release"
  expect wakelock released

test "nothing holds after toggling off"
  tap "toggle"
  expect wakelock held
`);
  const root = fileURLToPath(new URL('..', import.meta.url));
  const compiler = join(process.env.CARGO_TARGET_DIR ?? join(root, 'target'), 'debug', 'contract');
  try {
    const compile = spawnSync(compiler, ['build', contract, '-o', plan], { cwd: root, encoding: 'utf8' });
    expect(compile.status, compile.stderr).toBe(0);
    const build = spawnSync(process.execPath, ['host/web-js/build.mjs', 'wakelock', '--plan', plan, '--out', dist, '--render', 'none'], { cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, encoding: 'utf8' });
    expect(build.status, build.stderr).toBe(0);
    const server = createServer((request, response) => serveBuildTree(dist, request, response));
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    try {
      const r = await runTests({ host: 'web', file, app: 'caltrain', url: `http://127.0.0.1:${server.address().port}/` });
      expect(r.passed).toBe(1);
      expect(r.results.map(t => t.failures.length)).toEqual([0, 1, 1]);
      expect(r.results[1].failures[0]).toMatch(/line 9: expected wakelock released; held by view \d+ \("awake"\)$/);
      expect(r.results[2].failures[0]).toBe('nothing holds after toggling off: line 13: expected wakelock held; nothing holds it');
    } finally { server.close(); }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 180_000);

// docs/recipes/navigation-stack.md, "A screen open twice" (issues/closed/20261021-repeated-routes-share-root-state.md):
// a route drawn by its own component has `state` per stack entry (LLP 1017.000 P4c), so two entries of one route keep
// separate state, and the entry a pop removed takes its state with it: the next one opens fresh. The two entries have
// different URLs, since a push of the location already on top is no new visit (route/src/router.rs `push`).
test('a screen open twice keeps state per entry, and a popped entry\'s is gone', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-test-twice-'));
  const contract = join(dir, 'app.contract'), plan = join(dir, 'app.plan'), dist = join(dir, 'dist'), file = join(dir, 'app.test.contract');
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Twice', app: { id: 'test.exact.twice', name: 'Twice' }, host: {}, deploy: {} }));
  writeFileSync(contract, `routes nav
  home "/"
    finder "/finder/:from"

component App
  action open(from: string)
    nav = push(nav, path("finder", from))
  action back
    nav = back(nav)
  view
    main navigationKey=\`\${top(nav).id}\` navigationBack="back" width="100%" height="100%"
      for e in stack(nav) key=e.id
        if e.name == "home"
          column navigationKey=\`\${e.id}\` position="absolute" inset=0
            button press=open("home") testId="open"
              text "Find a shoe"
        if e.name == "finder"
          Finder(entry=e, open=open)

component Finder
  props
    entry: Entry
    open: action
  state pick = ""
  action choose(v: string)
    pick = v
  view
    column navigationKey=\`\${entry.id}\` position="absolute" inset=0
      header
        text "Finder" role="heading" aria-level=2
      text pick testId=\`pick-\${entry.id}\`
      button press=choose("trail") testId=\`trail-\${entry.id}\`
        text "Trail"
      button press=open(\`\${entry.id}\`) testId=\`again-\${entry.id}\`
        text "Another"
`);
  writeFileSync(file, `test "two entries keep their own state; a popped one's goes with it"
  tap "open"
  clock settle
  tap "trail-1"
  expect text "pick-1" == "trail"
  tap "again-1"
  clock settle
  expect text "pick-2" == ""
  tap "trail-2"
  expect text "pick-2" == "trail"
  back
  clock settle
  expect tree missing "pick-2"
  expect text "pick-1" == "trail"
  tap "again-1"
  clock settle
  expect text "pick-3" == ""
`);
  const root = fileURLToPath(new URL('..', import.meta.url));
  const compiler = join(process.env.CARGO_TARGET_DIR ?? join(root, 'target'), 'debug', 'contract');
  try {
    const compile = spawnSync(compiler, ['build', contract, '-o', plan], { cwd: root, encoding: 'utf8' });
    expect(compile.status, compile.stderr).toBe(0);
    const build = spawnSync(process.execPath, ['host/web-js/build.mjs', 'twice', '--plan', plan, '--out', dist, '--render', 'none'], { cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, encoding: 'utf8' });
    expect(build.status, build.stderr).toBe(0);
    const server = createServer((request, response) => serveBuildTree(dist, request, response));
    await new Promise(resolve => server.listen(0, '127.0.0.1', resolve));
    try {
      const r = await runTests({ host: 'web', file, app: 'caltrain', url: `http://127.0.0.1:${server.address().port}/` });
      expect(r.results.map(t => t.failures)).toEqual([[]]);
    } finally { server.close(); }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 180_000);

// LLP 1146 D4: what failed that the test did not arrange.
test('a failure is unarranged unless a fetch fault fired or an expect refusal names it', () => {
  const now = { ...NO_FAILURES, failures: { gateway: 2 }, failed: { gateway: 'turn 868 ms over 100 ms' }, mutations: { vote: { code: 'refused', message: 'no' }, send: { code: 'network', message: 'down' } } };
  expect(unarranged(NO_FAILURES, now, { refusals: ['vote'] })).toEqual(['resource gateway was refused 2 times (turn 868 ms over 100 ms)', 'send send failed (network: down)']);
  expect(unarranged(now, { ...now, failures: { gateway: 3 } }, { refusals: ['vote', 'send'] })).toEqual(['resource gateway was refused once (turn 868 ms over 100 ms)']);
  expect(unarranged(NO_FAILURES, { ...now, faults: { '/api': 1 } }, { refusals: ['vote', 'send'] })).toEqual([]);
  expect(unarranged(NO_FAILURES, { ...NO_FAILURES, failed: { list: 'shape' } })).toEqual(['resource list failed (shape)']);
  const churn = churnLines(['t=12 query r: src', 't=13 `r` was asked 50 times in a second, its arguments new each time (argument 1 last): …']);
  expect(churn).toEqual(['`r` was asked 50 times in a second, its arguments new each time (argument 1 last): …']);
  expect(unarranged(NO_FAILURES, NO_FAILURES, { churn })).toEqual(churn);
});

// LLP 1146 D3: an `expect perf` line against a block's counts.
test('expect perf reads a resource\'s asks and a component\'s or a view\'s work', () => {
  const site = (component, created, evaluated, line) => ({ site: line, created, rowConstructed: 0, evaluated, source: { status: 'compatible', component, file: '/a/app.contract', line } });
  const measured = { line: 3, components: new Set(['MemberRow', 'Rail', 'Sheet']), whole: { resources: [{ name: 'channelList', asked: 1 }], sites: [site('Rail', 2, 9, 10), site('Rail', 1, 4, 12)] }, under: new Map([['member-sheet', { sites: [site('Sheet', 5, 0, 40)] }]]) };
  const at = (subject, target, compare, value) => perfExpectation(measured, { subject, target, compare, value }, ['channelList', 'serverList']);
  expect(at('query', 'channelList', '<=', 1)).toBeNull();
  expect(at('query', 'serverList', '==', 0)).toBeNull();
  expect(at('query', 'nope', '==', 0)).toContain('no resource is named nope');
  expect(at('built', 'Rail', '==', 3)).toBeNull();
  expect(at('evaluated', 'Rail', '<=', 12)).toBe('evaluated "Rail" counted 13 in the `perf` block at line 3, expected <= 12 (app.contract:10 9, app.contract:12 4)');
  expect(at('built', 'MemberRow', '==', 0)).toBeNull(); // declared, never built
  expect(at('built', 'member-sheet', '==', 5)).toBeNull(); // a view by testId
  expect(at('built', 'Ghost', '==', 0)).toContain('names no component');
  expect(perfExpectation(null, { subject: 'query', target: 'a', compare: '==', value: 0 })).toContain('no `perf` block');
  expect(perfExpectation({ ...measured, whole: { sites: [] } }, { subject: 'query', target: 'a', compare: '==', value: 0 })).toContain('counts no resource asks');
  measured.whole.resources = [1, 5].map((asked, i) => ({ name: 'channelList', member: `channelList[${i + 1}]`, asked }));
  expect(at('query', 'channelList', '==', 6)).toBeNull();
});

test('a test scoped on macos runs under the mac alias and the sample host too', () => {
  expect(testScope('mac')).toBe('macos');
  expect(testScope('macos')).toBe('macos');
  expect(testScope('host')).toBe('macos');
  expect(testScope('host-ios')).toBe('ios');
  expect(testScope('web')).toBe('web');
});

test('always observes each mutation-clock configuration, including the sixteenth', async () => {
  let requests = 1, ticks = 0;
  const seen = [], clock = testMutationClock();
  const session = {
    state: async () => ({ mutationRequests: requests, slots: { ticks } }),
    clock: async () => { ticks++; if (ticks < 16) requests++; },
  };
  expect(await clock.advance(session, async state => { seen.push(state.slots.ticks); })).toBe(16);
  expect(seen).toEqual(Array.from({ length: 17 }, (_, i) => i));
});

// LLP 1153 D4: a native run has no web build's grants to read. The site pattern's API is a setting, so its `/planets`
// fault resolves against the grant app.ts names as the build evaluates it, `.env.local`'s port over `.env`'s.
test('a native run resolves a fetch fault under the API a setting names', async () => {
  const { createApp, applyPattern } = await import('../game/new.mjs');
  const parent = mkdtempSync(join(tmpdir(), 'exact-native-origins-'));
  try {
    const dir = join(parent, 'site'), env = { EXACT_BUN_LOADED: '' };
    createApp(dir);
    applyPattern(dir, 'site');
    mkdirSync(join(dir,'.exact'),{recursive:true});
    writeFileSync(join(dir,'.exact/typescript-inputs.json'),JSON.stringify({modules:[['app.ts',join(dir,'app.ts')]],types:[]}));
    expect(nativeFetchOrigins(dir, env)).toEqual(['http://127.0.0.1:8787']);
    writeFileSync(join(dir, '.env.local'), 'API_BASE_URL=http://127.0.0.1:9123\n');
    const faults = () => [{ steps: [{ op: 'fail-fetch', prefix: '/planets' }, { op: 'pass-fetch', prefix: '/planets' }] }];
    expect(backendFetchPrefixes(faults(), nativeFetchOrigins(dir, env))[0].steps.map((s) => s.prefix)).toEqual(['http://127.0.0.1:9123/planets', 'http://127.0.0.1:9123/planets']);
    expect(nativeFetchOrigins(dir, { ...env, API_BASE_URL: 'http://127.0.0.1:7000' })).toEqual(['http://127.0.0.1:7000']);
    // The backend a build selected, and a scaffold's literal origin, still come first.
    expect(nativeFetchOrigins(dir, { ...env, EXACT_BACKEND_ORIGIN: 'http://127.0.0.1:4411' })).toEqual(['http://127.0.0.1:4411']);
    writeFileSync(join(dir, 'app.ts'), "// const origin = 'http://comment.invalid';\nexport const origin = ('http://127.0.0.1:4410' satisfies string);\n");
    expect(nativeFetchOrigins(dir, env)).toEqual(['http://127.0.0.1:4410']);
  } finally { rmSync(parent, { recursive: true, force: true }); }
});

test('a module setting leaves native literal fetch grants available to faults', () => {
  const dir = mkdtempSync(join(import.meta.dir, '../target/issue-859-origins-'));
  try {
    mkdirSync(join(dir, 'src'));
    writeFileSync(join(dir, 'app.json'), JSON.stringify({ env: ['API_BASE_URL'] }));
    writeFileSync(join(dir, 'src/api.ts'), "export const API = import.meta.env.API_BASE_URL ?? 'https://built.example';\n");
    writeFileSync(join(dir, 'app.ts'), "import { API } from './src/api';\nexport const grants = 'net.fetch https:' + '\\nnet.fetch http://127.0.0.1:9123';\nexport const answer = () => API;\n");
    mkdirSync(join(dir,'.exact'),{recursive:true});
    writeFileSync(join(dir,'.exact/typescript-inputs.json'),JSON.stringify({modules:['app.ts','src/api.ts'].map(file=>[file,join(dir,file)]),types:[]}));
    const origins = nativeFetchOrigins(dir, {});
    expect(origins).toEqual(['http://127.0.0.1:9123']);
    expect(backendFetchPrefixes([{ steps: [{ op: 'fail-fetch', prefix: '/items' }] }], origins)[0].steps[0].prefix).toBe('http://127.0.0.1:9123/items');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// LLP 1103: a fault on a path is the API's. The driver arms it under the one origin the grants name, and the host
// matches that as a prefix of the whole URL, so `fail fetch "/planets"` fails the site's absolute fetch of a planet.
test('a fault on a path is armed under the API, and fails that API\'s absolute URL', async () => {
  const { faultOp, takeFault } = await import('../host/web/faults.js');
  const [planets] = backendFetchPrefixes([{ steps: [{ op: 'fail-fetch', prefix: '/planets' }] }], ['http://127.0.0.1:4790']);
  expect(planets.steps[0].prefix).toBe('http://127.0.0.1:4790/planets');
  try {
    faultOp({ fail: planets.steps[0].prefix });
    expect(takeFault('http://127.0.0.1:4790/planets/5')).toBe(true);
    expect(takeFault('http://127.0.0.1:4791/planets/5')).toBe(false);
  } finally { delete globalThis.__exactFaults; }
});

// Run with a private module registry: only the carrier is stubbed, not compilation or backend resets.
test('fresh resets Snapback beside an owned app server, with file and test seeds', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-mixed-fresh-'));
  const script = join(dir, 'check.mjs'), module = fileURLToPath(new URL('./agent-test.mjs', import.meta.url));
  writeFileSync(script, `
    import assert from 'node:assert/strict';
    import { mkdirSync, writeFileSync } from 'node:fs';
    import { mock } from 'bun:test';
    const dir = ${JSON.stringify(dir)};
    writeFileSync(dir + '/app.contract', 'component App\\n  view\\n    text \"Test\"\\n');
    const resets = [], probes = [];
    const rest = Bun.serve({port: 0, fetch: req => { probes.push(new URL(req.url).pathname); return Response.json({items: []}); }});
    const snapback = Bun.serve({port: 0, fetch: async req => {
      if (new URL(req.url).pathname === '/dev/status') return Response.json({storeId: 'fixture'});
      resets.push(await req.text()); return Response.json({reset: true, seeded: 1, generation: resets.length});
    }});
    mkdirSync(dir + '/dist/.gen', {recursive: true});
    writeFileSync(dir + '/dist/.gen/admission-data.js', JSON.stringify(['net.fetch http://127.0.0.1:' + rest.port, 'net.fetch http://127.0.0.1:' + snapback.port]));
    writeFileSync(dir + '/seed.json', '[{"table":"items","row":{"title":"Seed"}}]');
    const file = dir + '/app.test.contract';
    writeFileSync(file, 'fresh\\ntest "file reset"\\n  expect state ready == true\\ntest "seeded reset"\\n  fresh "seed.json"\\n  expect state ready == true\\n');
    let starts = 0, opens = 0;
    mock.module(${JSON.stringify(fileURLToPath(new URL('./agent.mjs', import.meta.url)))}, () => ({open: async () => {
      assert.equal(resets.length, ++opens, 'reset precedes opening');
      return {host:'linux', now:0, op:async()=>({clock:0, lines:[]}), clock:async()=>({}), state:async()=>({mutationRequests:0,slots:{ready:true}}), logs:async()=>({lines:[]}), close:async()=>{}};
    }}));
    const {runTests} = await import(${JSON.stringify(module)});
    try {
      const r = await runTests({host:'linux', app:dir, file, webDist:dir + '/dist', running:{beforeTest:async()=>{starts++}, check(){}}});
      assert.equal(r.failed, 0, JSON.stringify(r));
      assert.equal(r.passed, 2);
      assert.equal(starts, 2);
      assert.deepEqual(resets, ['', '[{"table":"items","row":{"title":"Seed"}}]']);
      assert.deepEqual(probes, ['/dev/status', '/dev/status'], 'the REST origin is probed and left alone');
      console.log('completed');
    } finally {rest.stop(true); snapback.stop(true);}
  `);
  try {
    const r = spawnSync(process.execPath, [script], {encoding:'utf8', timeout:30000});
    expect(r.status, r.stdout + r.stderr).toBe(0);
    expect(r.stdout).toContain('completed');
  } finally {rmSync(dir, {recursive:true, force:true});}
}, 30000);

// LLP 1156.006.000 D8: with no build, every test of a run would fail alike, so the run stops at the first; another
// launch failure of an app with a server is still that test's.
test('a missing build stops a test run at its first test', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-test-nobuild-')), script = join(dir, 'check.mjs');
  writeFileSync(join(dir, 'app.contract'), 'component App\n  view\n    text "Test"\n'); writeFileSync(join(dir, 'app.test.contract'), 'test "one"\n  expect state ready == true\ntest "two"\n  expect state ready == true\n');
  writeFileSync(script, `
    import assert from 'node:assert/strict'; import {mock} from 'bun:test';
    let opens = 0, missing = true;
    mock.module(${JSON.stringify(fileURLToPath(new URL('./agent.mjs', import.meta.url)))}, () => ({open: async (o) => { opens++; assert.equal(o.ax, true, 'the launch asks for the accessibility runtime (D15)'); throw Object.assign(new Error('run bun exact.mjs android first'), {missingBuild: missing}); }}));
    const {runTests} = await import(${JSON.stringify(fileURLToPath(new URL('./agent-test.mjs', import.meta.url)))});
    const run = () => runTests({host:'linux', app:${JSON.stringify(dir)}, file:${JSON.stringify(join(dir, 'app.test.contract'))}, running:{beforeTest:async()=>{}, check(){}}, ax:true});
    await assert.rejects(run(), /run bun exact.mjs android first/); assert.equal(opens, 1);
    missing = false; opens = 0;
    const r = await run(); assert.equal(r.failed, 2); assert.equal(opens, 2); console.log('completed');
  `);
  try { const r = spawnSync(process.execPath, [script], {encoding:'utf8', timeout:30000}); expect(r.status, r.stdout + r.stderr).toBe(0); expect(r.stdout).toContain('completed'); }
  finally { rmSync(dir, {recursive:true, force:true}); }
}, 30000);

test('replay selects the original ordinal among duplicate names and host variants', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-test-ordinal-')), script = join(dir, 'check.mjs');
  writeFileSync(join(dir, 'app.contract'), 'component App\n  view\n    text "Test"\n');
  writeFileSync(join(dir, 'app.test.contract'), 'test "same" on web\n  expect state ready == false\ntest "same" on linux\n  expect state ready == true\ntest "same"\n  expect state ready == false\n');
  writeFileSync(script, `
    import assert from 'node:assert/strict';
    import {mock} from 'bun:test';
    let opened = 0;
    mock.module(${JSON.stringify(fileURLToPath(new URL('./agent.mjs', import.meta.url)))}, () => ({open: async () => {
      opened++;
      return {host:'linux', now:0, op:async()=>({clock:0,lines:[]}), clock:async()=>({}), state:async()=>({mutationRequests:0,slots:{ready:true}}), logs:async()=>({lines:[]}), close:async()=>{}};
    }}));
    const {runTests} = await import(${JSON.stringify(fileURLToPath(new URL('./agent-test.mjs', import.meta.url)))});
    const r = await runTests({host:'linux', app:${JSON.stringify(dir)}, file:${JSON.stringify(join(dir, 'app.test.contract'))}, testIndex:1,
      atEnd:async(s,t)=>{assert.equal(t.index,1);return {keep:true};}});
    assert.equal(r.failed,0,JSON.stringify(r));
    assert.equal(r.passed,1); assert.equal(r.results.length,1); assert.equal(opened,1);
    assert.equal(r.results[0].index,1);
    console.log('completed');
  `);
  try {
    const r = spawnSync(process.execPath, [script], {encoding:'utf8', timeout:30000});
    expect(r.status, r.stdout + r.stderr).toBe(0);
    expect(r.stdout).toContain('completed');
  } finally {rmSync(dir, {recursive:true, force:true});}
}, 30000);

test('at keeps the origin of a remote web test', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-remote-at-')), script = join(dir, 'check.mjs');
  writeFileSync(join(dir, 'app.contract'), 'component App\n  view\n    text "Test"\n');
  writeFileSync(join(dir, 'app.test.contract'), 'test "remote route"\n  at "/settings?panel=account#name"\n  expect state ready == true\n');
  writeFileSync(script, `
    import assert from 'node:assert/strict';
    import { mock } from 'bun:test';
    const urls = [];
    mock.module(${JSON.stringify(fileURLToPath(new URL('./agent.mjs', import.meta.url)))}, () => ({ open: async options => {
      urls.push(options.url);
      return {host:'web',now:0,op:async()=>({clock:0,lines:[]}),clock:async()=>({}),state:async()=>({mutationRequests:0,slots:{ready:true}}),logs:async()=>({lines:[]}),close:async()=>{}};
    }}));
    const {runTests} = await import(${JSON.stringify(fileURLToPath(new URL('./agent-test.mjs', import.meta.url)))});
    const result = await runTests({host:'web',app:${JSON.stringify(dir)},file:${JSON.stringify(join(dir, 'app.test.contract'))},url:'http://fixture.invalid:4321/'});
    assert.equal(result.failed, 0, JSON.stringify(result));
    assert.deepEqual(urls, ['http://fixture.invalid:4321/settings?panel=account#name']);
  `);
  try {
    const result = spawnSync(process.execPath, [script], { encoding: 'utf8', timeout: 30000 });
    expect(result.status, result.stdout + result.stderr).toBe(0);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 30000);
test('the APK carrier skips a test that names a size, saying why, and refuses a drive\'s --size (#547)', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-test-apk-size-')), script = join(dir, 'check.mjs');
  writeFileSync(join(dir, 'app.contract'), 'component App\n  view\n    text "Test"\n');
  writeFileSync(join(dir, 'app.test.contract'), 'test "wide"\n  size 1280x800\n  expect state ready == true\ntest "phone"\n  expect state ready == true\n');
  writeFileSync(script, `
    import assert from 'node:assert/strict';
    import {mock} from 'bun:test';
    const sizes = [];
    mock.module(${JSON.stringify(fileURLToPath(new URL('./agent.mjs', import.meta.url)))}, () => ({open: async (o) => {
      sizes.push(o.size);
      return {host:'android', now:0, op:async()=>({clock:0,lines:[]}), clock:async()=>({}), state:async()=>({mutationRequests:0,slots:{ready:true}}), logs:async()=>({lines:[]}), close:async()=>{}};
    }}));
    const {runTests} = await import(${JSON.stringify(fileURLToPath(new URL('./agent-test.mjs', import.meta.url)))});
    const file = ${JSON.stringify(join(dir, 'app.test.contract'))};
    const r = await runTests({host:'android', reader:true, app:${JSON.stringify(dir)}, file});
    assert.equal(r.failed, 0, JSON.stringify(r));
    assert.equal(r.passed, 1);
    assert.equal(r.results[0].skipped, "size 1280x800: the APK runs at the phone's size; --headless takes it");
    assert.deepEqual(sizes, [undefined], 'only the phone-sized test opens the app');
    await assert.rejects(runTests({host:'android', reader:true, app:${JSON.stringify(dir)}, file, size:[1280, 800]}), /--size does not reach the APK/);
    const headless = await runTests({host:'android', reader:false, app:${JSON.stringify(dir)}, file});
    assert.equal(headless.passed, 2, '--headless takes the size');
    console.log('completed');
  `);
  try {
    const r = spawnSync(process.execPath, [script], {encoding:'utf8', timeout:60000});
    expect(r.status, r.stdout + r.stderr).toBe(0);
    expect(r.stdout).toContain('completed');
  } finally {rmSync(dir, {recursive:true, force:true});}
}, 60000);
test('single-value page facts replay by canonical name and drive-only forms are refused', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-test-page-')), script = join(dir, 'check.mjs');
  writeFileSync(join(dir, 'app.contract'), 'component App\n  view\n    text "Test"\n');
  const pageFacts = {frozen:'false', 'visibility-state':'hidden', online:'false', 'can-share':'false', 'can-share-files':'false', 'can-share-pictures':'false', 'can-open-files':'false', 'iframe-partition':'false', 'has-focus':'false', 'keyboard-navigation':'true', 'bold-text':'true', 'root-font-size':'24px', 'display-scale':'2', 'gpu-tier':'low'};
  const driveOnly = {posture:'folded', segments:'2x1', gap:'40', mode:'density=compact', extension:'nfc.field=clear', Segments:'2x1', Mode:'density=compact', Extension:'nfc.field=clear'};
  writeFileSync(join(dir, 'app.test.contract'), `test "page facts"\n${Object.entries(pageFacts).map(([name,value]) => `  prefer ${name} ${JSON.stringify(value)}\n`).join('')}  expect state ready == true\n  prefer Online true\n  prefer rootFontSize "28px"\n  reload\n  expect state ready == true\n${Object.entries(driveOnly).map(([name,value]) => `test "refuse ${name}"\n  prefer ${name} ${JSON.stringify(value)}\n`).join('')}`);
  writeFileSync(script, `
    import assert from 'node:assert/strict';
    import {mock} from 'bun:test';
    import {preferGroups} from ${JSON.stringify(fileURLToPath(new URL('./agent-prefer.mjs', import.meta.url)))};
    let page, opens = 0, calls = [];
    mock.module(${JSON.stringify(fileURLToPath(new URL('./agent.mjs', import.meta.url)))}, () => ({open: async () => {
      opens++; page = {};
      return {host:'linux', now:0, prefer:async facts=>{calls.push(facts); Object.assign(page,preferGroups(facts).page); return {};},
        op:async()=>({clock:0,lines:[]}), clock:async()=>({}), state:async()=>({mutationRequests:0,slots:{ready:true}}),
        logs:async()=>({lines:[]}), close:async()=>{}};
    }}));
    const {runTests} = await import(${JSON.stringify(fileURLToPath(new URL('./agent-test.mjs', import.meta.url)))});
    const expected = {...preferGroups(${JSON.stringify(pageFacts)}).page, online:true, 'root-font-size':28};
    const r = await runTests({host:'linux', app:${JSON.stringify(dir)}, file:${JSON.stringify(join(dir, 'app.test.contract'))},
      atEnd:async(s,t)=>{if(t.name === 'page facts') {assert.deepEqual(page,expected); assert.deepEqual(t.launch.media,expected);} return {};}});
    assert.equal(r.passed,1,JSON.stringify(r));
    assert.equal(r.failed,${Object.keys(driveOnly).length},JSON.stringify(r));
    assert.equal(r.results[0].end?.error,undefined);
    for (const t of r.results.slice(1)) assert.match(t.failures[0],new RegExp(t.name.slice(7)+' is drive-only for now'));
    assert.equal(opens,${Object.keys(driveOnly).length+2});
    assert.equal(calls.length,${Object.keys(pageFacts).length*2+2});
    assert.ok(calls.every(facts=>!Object.hasOwn(facts,'Online')&&!Object.hasOwn(facts,'rootFontSize')));
  `);
  try {
    const r = spawnSync(process.execPath, [script], {encoding:'utf8', timeout:30000});
    expect(r.status, r.stdout + r.stderr).toBe(0);
  } finally {rmSync(dir, {recursive:true, force:true});}
}, 30000);
