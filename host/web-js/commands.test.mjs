import { spawnSync } from 'node:child_process';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { webCompiler } from './module.mjs';
import { hostFixtureFile, glueCase, glueNode, glueDeclarations } from '../web/tests/glue-fixture.mjs';
import { test, expect } from 'bun:test';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { builtFile } from './content-names.mjs';
import { engagement } from '../web/engagement.js';

test('relaunch reloads the browser document and keeps committed storage', async () => {
  const server = Bun.serve({ port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/rt.js') return new Response('export const Hosts = {}, KeyEvent = null, Propagating = null; export const DEVELOPMENT_STORE = globalThis.testDevelopment ? \"fixture\" : null; export const Kept = () => globalThis.testKey ?? \"exact.state.\";', { headers: { 'content-type': 'text/javascript' } });
    if (path === '/commands.js') return new Response(Bun.file(new URL('./commands.js', import.meta.url)));
    if (path === '/frame-messages.js') return new Response(Bun.file(new URL('../web/frame-messages.js', import.meta.url)));
    if (path === '/navigation.js') return new Response(hostFixtureFile(new URL('../web/navigation.js', import.meta.url)), { headers: { 'content-type': 'text/javascript' } });
    if (path === '/prompt-glue.js') return new Response(Bun.file(new URL('../web/prompt-glue.js', import.meta.url)));
    if (path === '/alert-message.js') return new Response(Bun.file(new URL('../web/alert-message.js', import.meta.url)));
    if (path === '/menu-rows.js') return new Response(Bun.file(new URL('../web/menu-rows.js', import.meta.url)));
    return new Response(`<button id="restart">Restart</button><output id="state"></output>
      <script type="module">
        import { relaunch } from '/commands.js';
        const boots = Number(sessionStorage.getItem('boots') ?? 0) + 1;
        sessionStorage.setItem('boots', boots);
        document.querySelector('#state').textContent = boots + ':' + (localStorage.getItem('saved') ?? '');
        document.querySelector('#restart').onclick = () => {
          localStorage.setItem('saved', 'written before relaunch');
          relaunch(console.error)();
        };
      </script>`, { headers: { 'content-type': 'text/html' } });
  } });
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.waitForFunction(() => document.querySelector('#state').textContent === '1:');
    await Promise.all([page.waitForEvent('load'), page.locator('#restart').click()]);
    await page.waitForFunction(() => document.querySelector('#state').textContent === '2:written before relaunch');
    expect(await page.locator('#state').textContent()).toBe('2:written before relaunch');
  } finally {
    await browser?.close();
    server.stop(true);
  }
}, 30000);

// A prompt's Return commits the field's `change` before it presses the first
// action that is not destructive, so the action reads the new value and an
// action the change enables counts; while that action is disabled Return
// presses nothing, never the next one. A field with its own `submit` (the
// runtime marks its Enter `$submit`) keeps Return, and nothing is pressed.
// The page imports the glue as a plan with a dialog does (build.mjs `dialogs.flag`).
test("a prompt's Return commits its field, then presses its action", async () => {
  const server = Bun.serve({ port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/rt.js') return new Response('export const Hosts = {}, KeyEvent = null, Propagating = null; export const DEVELOPMENT_STORE = globalThis.testDevelopment ? \"fixture\" : null; export const Kept = () => globalThis.testKey ?? \"exact.state.\";', { headers: { 'content-type': 'text/javascript' } });
    if (path === '/commands.js') return new Response(Bun.file(new URL('./commands.js', import.meta.url)));
    if (path === '/frame-messages.js') return new Response(Bun.file(new URL('../web/frame-messages.js', import.meta.url)));
    if (path === '/navigation.js') return new Response(hostFixtureFile(new URL('../web/navigation.js', import.meta.url)), { headers: { 'content-type': 'text/javascript' } });
    if (path === '/prompt-glue.js') return new Response(Bun.file(new URL('../web/prompt-glue.js', import.meta.url)));
    if (path === '/alert-message.js') return new Response(Bun.file(new URL('../web/alert-message.js', import.meta.url)));
    if (path === '/menu-rows.js') return new Response(Bun.file(new URL('../web/menu-rows.js', import.meta.url)));
    return new Response(`<dialog role="alertdialog" open><input id="name">
        <button id="delete" data-exact-on="press" data-destructive="true">Delete</button>
        <button id="save" data-exact-on="press" disabled>Save</button>
        <button id="suggest" data-exact-on="press">Use Suggestion</button></dialog><output id="log"></output>
      <script type="module">
        import '/prompt-glue.js';
        let draft = '';
        const save = document.querySelector('#save'), log = document.querySelector('#log');
        document.querySelector('#name').addEventListener('change', e => { draft = e.target.value; save.disabled = draft === ''; });
        document.querySelector('#delete').onclick = () => { log.textContent = 'deleted'; };
        save.onclick = () => { log.textContent = 'saved ' + draft; };
        document.querySelector('#suggest').onclick = () => { log.textContent = 'suggested'; };
      </script>`, { headers: { 'content-type': 'text/html' } });
  } });
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    // Return queues the action on a zero-delay timer. Read after a timer
    // queued behind it, including when checking that no action ran.
    const log = () => page.evaluate(() => new Promise(resolve =>
      setTimeout(() => resolve(document.querySelector('#log').textContent))));
    await page.locator('#name').press('Enter');
    expect(await log()).toBe('');
    await page.locator('#name').fill('Kick');
    await page.locator('#name').press('Enter');
    expect(await log()).toBe('saved Kick');
    await page.evaluate(() => {
      const log = document.querySelector('#log');
      log.textContent = '';
      document.querySelector('#name').addEventListener('keydown', e => { if (e.key === 'Enter') { e.$submit = true; log.textContent = 'submitted'; } });
    });
    await page.locator('#name').press('Enter');
    expect(await log()).toBe('submitted');
    // A `change` that closes the prompt: its Save, still mounted, is not pressed.
    await page.evaluate(() => {
      const dialog = document.querySelector('dialog'), name = document.querySelector('#name');
      name.replaceWith(name.cloneNode());
      document.querySelector('#log').textContent = '';
      document.querySelector('#name').addEventListener('change', () => dialog.close());
    });
    await page.locator('#name').fill('Snare');
    await page.locator('#name').press('Enter');
    expect(await log()).toBe('');
  } finally {
    await browser?.close();
    server.stop(true);
  }
}, 30000);

// `openURL` hands the browser a scheme app.json's `open_url` declares (`exact.openSchemes`), whatever the case it is
// written in; an undeclared scheme, and one that runs or reads content even if declared, is refused; under the agent
// a declared one is journaled, not opened. `change` at a named element hears which.
test('openURL opens a declared scheme, refuses the rest, and journals under the agent', async () => {
  const server = Bun.serve({ port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/rt.js') return new Response('export const Hosts = {}, KeyEvent = null, Propagating = null; export const DEVELOPMENT_STORE = globalThis.testDevelopment ? \"fixture\" : null; export const Kept = () => globalThis.testKey ?? \"exact.state.\";', { headers: { 'content-type': 'text/javascript' } });
    for (const [name, file] of [['commands.js', './commands.js'], ['frame-messages.js', '../web/frame-messages.js'], ['navigation.js', '../web/navigation.js'], ['prompt-glue.js', '../web/prompt-glue.js'], ['alert-message.js', '../web/alert-message.js'], ['menu-rows.js', '../web/menu-rows.js']]) {
      if (path === `/${name}`) return new Response(hostFixtureFile(new URL(file, import.meta.url)), { headers: { 'content-type': 'text/javascript' } });
    }
    return new Response(`<script type="module">
        import { openURL } from '/commands.js';
        globalThis.exact = { openSchemes: ['vscode', 'javascript'] };
        const opened = [], said = [];
        window.open = (url) => { opened.push(url); };
        const page = openURL(line => said.push(line), () => false), agent = openURL(line => said.push(line), () => true);
        for (const url of ['vscode://vscode-remote/ssh-remote+devbox/p', 'VSCode://file/a', 'zed://ssh/devbox/p', 'javascript:alert(1)']) page(url);
        agent('vscode://file/b');
        const asked = document.body.appendChild(document.createElement('div')), heard = [];
        asked.id = 'asked';
        asked.addEventListener('change', e => heard.push(e.detail));
        page('https://example.com/', 'asked');
        page('zed://ssh/devbox/p', 'asked');
        page('https://example.com/', 'absent');
        await new Promise(r => setTimeout(r));
        globalThis.result = { opened, said, heard };
      </script>`, { headers: { 'content-type': 'text/html' } });
  } });
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.waitForFunction(() => globalThis.result);
    const { opened, said, heard } = await page.evaluate(() => globalThis.result);
    expect(opened).toEqual(['vscode://vscode-remote/ssh-remote+devbox/p', 'vscode://file/a', 'https://example.com/', 'https://example.com/']);
    expect(heard).toEqual(['opened', 'failed']);
    expect(said).toHaveLength(4);
    expect(said[0]).toStartWith('openURL refused: "zed://ssh/devbox/p"');
    expect(said[1]).toStartWith('openURL refused: "javascript:alert(1)"');
    expect(said[2]).toBe('openURL vscode: vscode://file/b (journaled, not opened under the agent)');
  } finally {
    await browser?.close();
    server.stop(true);
  }
}, 30000);

// A page cannot open the browser's developer tools (LLP 1020.002 D11): `openDevTools(id)` is refused in the journal.
test('openDevTools is refused on the web', async () => {
  const server = Bun.serve({ port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/rt.js') return new Response('export const Hosts = {}, KeyEvent = null, Propagating = null; export const DEVELOPMENT_STORE = globalThis.testDevelopment ? \"fixture\" : null; export const Kept = () => globalThis.testKey ?? \"exact.state.\";', { headers: { 'content-type': 'text/javascript' } });
    for (const [name, file] of [['commands.js', './commands.js'], ['frame-messages.js', '../web/frame-messages.js'], ['navigation.js', '../web/navigation.js'], ['prompt-glue.js', '../web/prompt-glue.js'], ['alert-message.js', '../web/alert-message.js'], ['menu-rows.js', '../web/menu-rows.js']]) {
      if (path === `/${name}`) return new Response(hostFixtureFile(new URL(file, import.meta.url)), { headers: { 'content-type': 'text/javascript' } });
    }
    return new Response(`<iframe id="tab"></iframe><script type="module">
        import { openDevTools } from '/commands.js';
        const said = [];
        openDevTools(line => said.push(line))('tab');
        globalThis.result = said;
      </script>`, { headers: { 'content-type': 'text/html' } });
  } });
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.waitForFunction(() => globalThis.result);
    expect(await page.evaluate(() => globalThis.result)).toEqual([`openDevTools("tab") refused: a page cannot open the browser's developer tools; open them from the browser's menu`]);
  } finally {
    await browser?.close();
    server.stop(true);
  }
}, 30000);

// The trace commands are the Apple hosts': the JS target refuses both in the journal, as the wasm host does.
test('the trace commands are refused in the journal', async () => {
  const server = Bun.serve({ port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/rt.js') return new Response('export const Hosts = {}, KeyEvent = null, Propagating = null; export const DEVELOPMENT_STORE = globalThis.testDevelopment ? \"fixture\" : null; export const Kept = () => globalThis.testKey ?? \"exact.state.\";', { headers: { 'content-type': 'text/javascript' } });
    for (const [name, file] of [['commands.js', './commands.js'], ['frame-messages.js', '../web/frame-messages.js'], ['navigation.js', '../web/navigation.js'], ['prompt-glue.js', '../web/prompt-glue.js'], ['alert-message.js', '../web/alert-message.js'], ['menu-rows.js', '../web/menu-rows.js']]) {
      if (path === `/${name}`) return new Response(hostFixtureFile(new URL(file, import.meta.url)), { headers: { 'content-type': 'text/javascript' } });
    }
    return new Response(`<script type="module">
        import { startTrace_EXPERIMENTAL, stopTrace_EXPERIMENTAL } from '/commands.js';
        const said = [], say = line => said.push(line);
        startTrace_EXPERIMENTAL(say)();
        stopTrace_EXPERIMENTAL(say)(7500);
        globalThis.result = said;
      </script>`, { headers: { 'content-type': 'text/html' } });
  } });
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.waitForFunction(() => globalThis.result);
    expect(await page.evaluate(() => globalThis.result)).toEqual([
      "startTrace_EXPERIMENTAL refused: the web host does not record traces; use the browser's performance panel",
      "stopTrace_EXPERIMENTAL refused: the web host does not record traces; use the browser's performance panel",
    ]);
  } finally {
    await browser?.close();
    server.stop(true);
  }
}, 30000);


test.each([false, true])('a saved scheme waits beneath a grid copy pin (wasm: %s)', async wasm => {
  const source = path => readFileSync(new URL(path, import.meta.url), 'utf8');
  const scheme = source('../web/scheme.js');
  const command = glueCase(source('../web/wasm/commit.js'), 'command');
  const boot = glueDeclarations(source('../web/wasm/boot.js'), ['bootNow', 'reportColors']);
  const client = glueNode(source('./dev.mjs'), n => n.type === 'VariableDeclarator' && n.id.name === 'client' && n.init);
  // The client writes its replacement boundary and the page's engagement tracker into the script (dev.mjs).
  const boundary = glueNode(source('./dev.mjs'), n => n.type === 'FunctionDeclaration' && n.id.name === 'replacementBoundary' && n);
  const script = new Function('INSPECT_EVENTS', 'replacementBoundary', 'engagement', `return (${client})(0, null)`)([], boundary, engagement)
    .replace(/^<script>|<\/script>$/g, '');
  const pin = glueNode(wasm ? source('../web/dev.js') : script, n => n.type === 'VariableDeclarator' && n.id.name === 'pin' && n.init);
  const server = Bun.serve({ port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/rt.js') return new Response('export const Hosts = {}, KeyEvent = null, Propagating = null; export const DEVELOPMENT_STORE = \"fixture\"; export const Kept = () => \"exact.state.\";', { headers: { 'content-type': 'text/javascript' } });
    if (path === '/') return new Response('<iframe name="exact-copy-1" src="/copy"></iframe>', { headers: { 'content-type': 'text/html' } });
    for (const [name, file] of [['commands.js', './commands.js'], ['frame-messages.js', '../web/frame-messages.js'], ['navigation.js', '../web/navigation.js'], ['prompt-glue.js', '../web/prompt-glue.js'], ['alert-message.js', '../web/alert-message.js'], ['menu-rows.js', '../web/menu-rows.js']]) {
      if (path === `/${name}`) return new Response(hostFixtureFile(new URL(file, import.meta.url)), { headers: { 'content-type': 'text/javascript' } });
    }
    return new Response(`<meta name="exact-dev-generation" content="fixture"><meta name="exact-scheme" content="exact.state.#scheme" data-development data-wasm><style>:root{color-scheme:light dark}body{background:light-dark(white,black)}</style><script>localStorage.setItem('exact.state.#scheme','"dark"');${scheme}</script>`, { headers: { 'content-type': 'text/html' } });
  } });
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage({ colorScheme: 'light' });
    await page.goto(server.url.href);
    const frame = page.frame({ name: 'exact-copy-1' });
    expect(await frame.evaluate(() => document.documentElement.style.colorScheme)).toBe('');
    const result = await frame.evaluate(async ({ wasm, pin, command, boot }) => {
      const x = globalThis.exact = {};
      const root = document.documentElement;
      const paint = () => [root.style.colorScheme, getComputedStyle(document.body).backgroundColor];
      let choose;
      if (wasm) {
        const noop = () => {}, services = {
          root: document.body, AGENT_ADMITTED: true, agentMode: false,
          bootAttempt: 0, incarnation: 0, devAssets: null,
          memory: { buffer: new ArrayBuffer(128) },
          wasm: { exact_in: () => 0, exact_plan_fonts: () => '[]', exact_boot_plan: () => JSON.stringify({
            ops: [{ op: 'command', name: 'setScheme', args: [JSON.parse(localStorage.getItem('exact.state.#scheme'))] }],
          }) },
          readOut: value => value, prepareFonts: async () => [], commitFonts: noop,
          encoder: new TextEncoder(), launchLocation: () => '/', preferences: () => 0, foldBits: () => 0,
          navigation: { reset: noop }, arrange: { reset: noop }, motion: { reset: noop },
          collections: { reset: noop }, lists: new Map(), views: new Map(),
          followedScrolls: new Map(), pendingScrolls: new Map(), messageFrames: new Set(), messageViews: new Set(),
          controllers: new Set(), forgettable: new Map(), inflight: new Map(), heldAsks: new Map(),
          mediaAttaching: new Map(), sizes: new Map(),
          focus: { restart: (_, apply) => apply() }, followOffset: noop, now: () => 0,
          appRootFontSize: noop, startClock: noop, afterPaint: noop, loadGpuIfNeeded: noop,
        };
        services.host = services;
        const dispatch = new Function('host', 'AGENT_ADMITTED', `return batch=>{for(const op of batch.ops){switch(op.op){${command}}}}`)(services, true);
        services.applyBatch = dispatch;
        const bootNow = new Function(...Object.keys(services), `${boot}; return bootNow;`)(...Object.values(services));
        await bootNow(new Uint8Array([1]), null, () => true, null, true);
        choose = value => dispatch({ ops: [{ op: 'command', name: 'setScheme', args: [value] }] });
      } else {
        const { setScheme } = await import('/commands.js');
        choose = setScheme(console.error, () => false, () => 'exact.state.');
      }
      const waiting = [...paint(), x.asked?.scheme?.value ?? null];
      choose('dark');
      const fresh = paint();
      root.style.colorScheme = ''; // Pin installation must reapply the retained choice.
      // Run each loop's pin function with its channel/executor dependencies.
      const channel = { before: null, held: null }, asked = {};
      const make = wasm
        ? new Function('channel', 'asked', 'modes', 'holdFacts', 'wasmCall', 'hostSize', 'copy', 'report', 'status', `return (${pin})`)
        : new Function('ch', 'ready', 'cp', 'rep', 'status', `let liveQ=Promise.resolve();const pin=${pin};return async m=>{pin(m);await liveQ}`);
      const applyPin = wasm
        ? make(channel, asked, async () => ({}), async () => ({}), async () => ({}), () => 16, true, console.error, () => {})
        : make(channel, async () => {}, true, console.error, () => {});
      await applyPin({ pin: { scheme: 'light' }, revision: 1 });
      const pinned = paint();
      await applyPin({ pin: null, revision: 2 });
      const back = paint();
      choose('light');
      const later = paint();
      document.getElementById('exact-dev-pin').remove();
      root.style.colorScheme = '';
      if (wasm) choose('dark');
      else (await import('/commands.js')).setScheme(console.error, () => false, () => 'exact.state.');
      await applyPin({ pin: null, revision: 3 });
      const unpinned = paint();
      delete x.asked;
      document.getElementById('exact-dev-pin').remove();
      root.style.colorScheme = '';
      choose('light');
      const firstExplicit = paint();
      return { firstExplicit, waiting, fresh, pinned, back, later, unpinned };
    }, { wasm, pin, command, boot });
    expect(result).toEqual({ firstExplicit: ['light', 'rgb(255, 255, 255)'], waiting: ['', 'rgb(255, 255, 255)', 'dark'], fresh: ['dark', 'rgb(0, 0, 0)'], pinned: ['dark', 'rgb(255, 255, 255)'],
      back: ['dark', 'rgb(0, 0, 0)'], later: ['light', 'rgb(255, 255, 255)'], unpinned: [wasm ? 'dark' : 'light', wasm ? 'rgb(0, 0, 0)' : 'rgb(255, 255, 255)'] });
  } finally { await browser.close(); server.stop(true); }
});


test.each([[false, false, false, false], [true, false, false, false], [true, true, false, false], [false, false, true, false], [false, true, true, false], [false, false, false, true], [true, false, false, true], [false, false, true, true]])('a page restores its scheme before paint while the runtime waits (production: %s, interaction: %s, wasm: %s, client: %s)', async (production, interaction, wasm, client) => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/scheme-paint-')), dist = resolve(dir, 'dist');
  let browser, server;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Scheme', background_color: '#f1f2f3', color_scheme_dark: { background_color: '#101112' }, app: { id: wasm ? 'com.exact.caltrain' : 'test.scheme-paint', name: 'Scheme' } }));
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  home "/" render=build activate=${interaction ? 'interaction' : 'idle'}
component App
  action choose(value: string)
    setScheme(value)
  view
    main width="100%" height="100%" background-color="light-dark(#ffffff, #000000)" testId="page"
      button "Dark" press=choose("dark") testId="dark"
      button "System" press=choose("system") testId="system"
`);
    const built = spawnSync(process.execPath, ['host/web-js/build.mjs', 'scheme', '--out', dist, '--render', client ? 'none' : 'js', ...(production ? ['--production'] : [])],
      { cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, encoding: 'utf8' });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const path = new URL(request.url).pathname;
    if (path === '/rt.js') return new Response('export const Hosts = {}, KeyEvent = null, Propagating = null; export const DEVELOPMENT_STORE = \"fixture\"; export const Kept = () => \"exact.state.\";', { headers: { 'content-type': 'text/javascript' } });
      return new Response(Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1))), { headers: { 'cache-control': 'no-store' } });
    } });
    browser = await suiteBrowser();
    const page = await browser.newPage({ colorScheme: 'light', viewport: { width: 400, height: 300 } });
    await page.goto(server.url.href);
    await page.getByTestId('dark').click();
    await page.waitForFunction(() => document.documentElement.style.colorScheme === 'dark');
    let key = await page.evaluate(() => Object.keys(localStorage).find(k => k.endsWith('#scheme')));
    expect(key).toBeDefined();
    if (wasm && client) {
      writeFileSync(resolve(dir, 'app.ts'), "export const grants = ''; export function answer(): never { throw new Error('no sources'); }\n");
      const built = spawnSync(process.execPath, ['host/web/build.mjs', 'scheme', '--wasm'],
        { cwd: root, env: { ...process.env, EXACT_APP_DIR: dir, CARGO_TARGET_DIR: resolve(root, 'target'), EXACT_WEB_DIST: resolve(dir, 'wasm') }, encoding: 'utf8' });
      expect(built.status, built.stdout + built.stderr).toBe(0);
      writeFileSync(resolve(dist, 'index.html'), readFileSync(resolve(dir, 'wasm/index.html')));
      await page.evaluate(key => localStorage.setItem('exact.state.#scheme', localStorage.getItem(key)), key);
      key = 'exact.state.#scheme';
    } else if (wasm) {
      // The retained target's document composer, with its own origin-scoped runner key, built from this checkout and run
      // as host/web/build.mjs runs it.
      const rendered = spawnSync('cargo', ['run', '-q', '-p', 'caltrain-linux', '--bin', 'caltrain-render', '--', '--plan', resolve(dist, 'app.plan'),
        '--shell', resolve(root, 'host/web/index.html'), '--build'], { cwd: root, env: { ...process.env, BUN: process.execPath, EXACT_UPDATE_TRUST: 'development' }, encoding: 'utf8', maxBuffer: 64 * 1024 * 1024 });
      expect(rendered.status, rendered.stdout + rendered.stderr).toBe(0);
      const document = JSON.parse(rendered.stdout.trim());
      expect(document.error).toBeUndefined();
      writeFileSync(resolve(dist, 'index.html'), document.page);
      await page.evaluate(key => localStorage.setItem('exact.state.#scheme', localStorage.getItem(key)), key);
      key = 'exact.state.#scheme';
    }
    let loaded = false;
    await page.route(wasm ? '**/app.wasm*' : `**/${builtFile(dist, 'app.js')}`, async () => { loaded = true; });
    if (wasm) {
      await page.route('**/*glue.js', async () => {});
      await page.route('**/navigation.js', route => route.fulfill({ status: 200, body: '' }));
    }
    const painted = async () => {
      await page.reload({ waitUntil: 'commit' });
      if (client) {
        await page.locator('#exact-root').waitFor({ state: 'attached' });
        await page.evaluate(() => new Promise(requestAnimationFrame));
        expect(await page.locator('#exact-root').innerHTML()).toBe('');
        return page.evaluate(() => getComputedStyle(document.documentElement).backgroundColor);
      }
      await page.evaluate(() => new Promise(resolve => {
        if (performance.getEntriesByType('paint').length) return resolve();
        new PerformanceObserver((_, observer) => { observer.disconnect(); resolve(); }).observe({ type: 'paint', buffered: true });
      }));
      return page.getByTestId('page').evaluate(node => getComputedStyle(node).backgroundColor);
    };
    const dark = client ? 'rgb(16, 17, 18)' : 'rgb(0, 0, 0)', light = client ? 'rgb(241, 242, 243)' : 'rgb(255, 255, 255)';
    expect(await painted()).toBe(dark);
    expect(await page.locator('meta[name=exact-scheme]').count()).toBe(1);
    expect(await page.locator('script').evaluateAll(nodes => nodes.filter(node => node.textContent.includes('meta[name=\"exact-scheme\"]')).length)).toBe(1);
    if (interaction) expect(loaded).toBe(false);
    const shot = await page.screenshot();
    expect(await page.evaluate(async bytes => {
      const image = await createImageBitmap(new Blob([new Uint8Array(bytes)], { type: 'image/png' }));
      const canvas = document.createElement('canvas'); canvas.width = image.width; canvas.height = image.height;
      const context = canvas.getContext('2d'); context.drawImage(image, 0, 0);
      return [...context.getImageData(380, 280, 1, 1).data];
    }, [...shot])).toEqual(client ? [16, 17, 18, 255] : [0, 0, 0, 255]);
    await page.evaluate(() => { window.name = 'exact-copy-1'; });
    expect(await painted()).toBe(dark);
    if (interaction) expect(loaded).toBe(false);
    await page.evaluate(() => { window.name = ''; });
    expect(await painted()).toBe(dark);
    for (const value of ['"invalid"', '{', '"system"']) {
      await page.evaluate(([key, value]) => localStorage.setItem(key, value), [key, value]);
      expect(await painted()).toBe(light);
    }
    await page.evaluate(key => { localStorage.removeItem(key); localStorage.setItem(key.replace('exact.state.', 'exact.state.other-'), '"dark"'); }, key);
    expect(await painted()).toBe(light);
  } finally { await browser?.close(); server?.stop(true); rmSync(dir, { recursive: true, force: true }); }
}, 600000);

test.each([false, true])('named windows restore normally outside a development grid (wasm: %s)', async wasm => {
  const command = glueCase(readFileSync(new URL('../web/wasm/commit.js', import.meta.url), 'utf8'), 'command');
  const scheme = readFileSync(new URL('../web/scheme.js', import.meta.url), 'utf8');
  const server = Bun.serve({ port: 0, fetch(request) {
    const url = new URL(request.url);
    if (url.pathname === '/rt.js') return new Response('export const Hosts = {}, KeyEvent = null, Propagating = null; export const DEVELOPMENT_STORE = null; export const Kept = () => "exact.state.";', { headers: { 'content-type': 'text/javascript' } });
    for (const [name, file] of [['commands.js', './commands.js'], ['frame-messages.js', '../web/frame-messages.js'], ['navigation.js', '../web/navigation.js'], ['prompt-glue.js', '../web/prompt-glue.js'], ['alert-message.js', '../web/alert-message.js'], ['menu-rows.js', '../web/menu-rows.js']]) {
      if (url.pathname === `/${name}`) return new Response(hostFixtureFile(new URL(file, import.meta.url)), { headers: { 'content-type': 'text/javascript' } });
    }
    return new Response(`<meta name="exact-scheme" content="exact.state.#scheme"><script>self.name='exact-copy-1';localStorage.setItem('exact.state.#scheme','"dark"');${scheme}</script>`, { headers: { 'content-type': 'text/html' } });
  } });
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage({ colorScheme: 'light' });
    await page.goto(server.url.href);
    expect(await page.evaluate(() => document.documentElement.style.colorScheme)).toBe('dark');
    const chosen = await page.evaluate(async ({ wasm, command }) => {
      globalThis.exact = {};
      document.documentElement.style.colorScheme = '';
      if (wasm) {
        const choose = new Function('value', 'restoring = false', `const host={restoringScheme:restoring};const AGENT_ADMITTED=false;for(const op of [{op:'command',name:'setScheme',args:[value]}]){switch(op.op){${command}}}`);
        choose('dark', true);
        const restored = document.documentElement.style.colorScheme;
        choose('light');
        return [restored, document.documentElement.style.colorScheme];
      }
      const choose = (await import('/commands.js')).setScheme(console.error);
      const restored = document.documentElement.style.colorScheme;
      choose('light');
      return [restored, document.documentElement.style.colorScheme];
    }, { wasm, command });
    expect(chosen).toEqual(['dark', 'light']);
    const [framed] = await Promise.all([page.waitForEvent('frameattached'), page.evaluate(() => {
      const frame = document.createElement('iframe');
      frame.name = 'exact-copy-2'; frame.src = '/framed'; document.body.append(frame);
    })]);
    await framed.waitForLoadState();
    expect(await framed.evaluate(() => document.documentElement.style.colorScheme)).toBe('dark');
  } finally { await browser.close(); server.stop(true); }
});

test('the early Wasm restore follows its build and agent storage policy', async () => {
  const scheme = readFileSync(new URL('../web/scheme.js', import.meta.url), 'utf8');
  const server = Bun.serve({ port: 0, fetch(request) {
    const url = new URL(request.url), development = url.pathname === '/development';
    return new Response(`<meta name="exact-scheme" content="exact.state.#scheme" data-wasm${development ? ' data-development' : ''}><script>self.name='exact-copy-1';localStorage.setItem('exact.state.#scheme','"dark"');${scheme}</script>`, { headers: { 'content-type': 'text/html' } });
  } });
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage({ colorScheme: 'light' });
    for (const development of [false, true]) for (const query of ['', '?agent', '?agent&storage=fixture']) {
      await page.goto(`${server.url}${development ? 'development' : 'production'}${query}`);
      expect(await page.evaluate(() => document.documentElement.style.colorScheme)).toBe(development && query === '?agent' ? '' : 'dark');
    }
  } finally { await browser.close(); server.stop(true); }
});

test('the development error shell restores the app scheme before its client', async () => {
  const source = readFileSync(new URL('./dev.mjs', import.meta.url), 'utf8');
  const shell = glueNode(source, n => n.type === 'TemplateLiteral' && n.quasis[0]?.value.raw.startsWith('<!doctype html><meta charset="utf-8">') && n);
  const { schemeHead } = await import('../web/look.mjs');
  const html = new Function('schemeHead', 'appDigest', 'realpathSync', 'app', 'client', 'seq', 'logicRevision', `return ${shell}`)(schemeHead, () => 'fixture', x => x, { dir: 'fixture' }, () => '', 0, null);
  const server = Bun.serve({ port: 0, fetch() { return new Response(html, { headers: { 'content-type': 'text/html' } }); } });
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage();
    await page.addInitScript(() => localStorage.setItem('exact.state.@fixture/#scheme', '"dark"'));
    await page.goto(server.url.href);
    expect(await page.evaluate(() => document.documentElement.style.colorScheme)).toBe('dark');
  } finally { await browser.close(); server.stop(true); }
});
