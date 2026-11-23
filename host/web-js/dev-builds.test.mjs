// The JS dev loop's builds (host/web-js/dev.mjs): a build that an input
// moved during is set aside, never published (not dist, which a fresh page
// loads, nor the design or a reload); an edit stops a young build; a value
// batch during a verifying build applies at once, and the build after
// carries it. EXACT_DEV_BUILD_HOLD_MS holds each build's end, as a slow one.
import { afterEach, expect, test } from 'bun:test';
import { spawnOwned, stopAll } from '../../scripts/test-children.mjs';
import { spawn, spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { EventEmitter } from 'node:events';
import { existsSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, realpathSync, renameSync, rmSync, statSync, symlinkSync, utimesSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { dirname, join, posix, relative, resolve, win32 } from 'node:path';
import { fileURLToPath } from 'node:url';
import { INPUT_TREE, OUTPUT, suiteBrowser } from '../../scripts/agent-launch.mjs';
import { shaderWatchRoots } from '../../scripts/app.mjs';
import { within } from '../../scripts/paths.mjs';
import { runInNewContext } from 'node:vm';
import { installLock } from '../../scripts/filesystem.mjs';
import { besidePattern } from '../web/beside.mjs';
import { appSources } from '../web/serve.mjs';
import { loopWebDist } from '../../scripts/web-build.mjs';
import { digest, lockClosure, workspaceManifest } from '../../scripts/kept.mjs';
import { freshCompiler, hostTriple } from '../../scripts/tools.mjs';
import { engagement } from '../web/engagement.js';

const ROOT = resolve(fileURLToPath(new URL('../..', import.meta.url)));
const sha = text => createHash('sha256').update(text).digest('hex');

// A dev loop a test started ends with the test, a timeout included.
afterEach(() => stopAll());

/** The JS dev loop on the app at `dir`, its dist in a temporary directory,
 * `env` beside the process's own. */
async function devLoop(dir, env = {}) {
  const listener = createServer();
  await new Promise(ok => listener.listen(0, '127.0.0.1', ok));
  const port = listener.address().port;
  await new Promise(ok => listener.close(ok));
  // The build's lock and kept generations go beside its dist (serve.mjs `replaceBuild`): the dist sits in a directory of its own.
  const dist = env.EXACT_WEB_DIST === '' ? loopWebDist({ dir, target: resolve(dir, 'target') }, 'js') : join(mkdtempSync(join(tmpdir(), 'exact-js-builds-')), 'dist');
  // Owned: ended with what it starts when the test ends, a timeout included (scripts/test-children.mjs).
  const dev = spawnOwned(process.execPath, [resolve(ROOT, 'host/web/dev.mjs'), '--app', dir, '--port', String(port)], { cwd: ROOT, env: { ...process.env, EXACT_WEB_DIST: dist, ...env }, stdio: ['ignore', 'pipe', 'pipe'] });
  const stop = async () => {
    await dev.stop(30_000);
    rmSync(dirname(dist), { recursive: true, force: true });
  };
  let out = '', err = '';
  dev.stderr.on('data', d => { err = (err + d).slice(-4000); });
  try {
    await new Promise((ok, fail) => {
      dev.stdout.on('data', d => { out += d; if (out.includes(`127.0.0.1:${port}/`)) ok(); });
      dev.on('error', fail);
      dev.on('exit', code => fail(new Error(`the dev loop exited ${code}: ${out}${err}`)));
    });
  } catch (e) { await stop(); throw e; }
  return { base: `http://127.0.0.1:${port}`, stop, said: () => out, dist };
}


test('the JS loop carries a checkpoint literally across document probes and retries', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-checkpoint-dollars-'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Checkpoint', app: { id: 'test.checkpoint', name: 'Checkpoint' } }));
  writeFileSync(join(dir, 'app.contract'), 'component App\n  state body = "initial"\n  view\n    text body testId="body"\n');
  const body = "$` $' $& $$ $5 $HOME </script><script>globalThis.__injected=true</script>";
  let loop, browser;
  try {
    loop = await devLoop(dir);
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(loop.base);
    await page.waitForFunction(() => globalThis.exact?.ready);
    const checkpoint = await page.evaluate(() => ({ time: 0, carryAnswers: false, answers: [],
      slots: globalThis.exact.state[0].map(s => [s.n.devName, s.n.devType, s()]),
    }));
    checkpoint.slots.find(([name]) => name === 'body')[2] = body;
    const post = () => fetch(`${loop.base}/__dev/checkpoint?id=1-2-3-4&seq=1`, {
      method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(checkpoint),
    });
    expect((await post()).status).toBe(204);
    expect((await fetch(`${loop.base}/?__exactDev=1-2-3-4`, { method: 'HEAD' })).status).toBe(200);
    const abandoned = await fetch(`${loop.base}/?__exactDev=1-2-3-4`);
    await abandoned.body.cancel();
    const html = await (await fetch(`${loop.base}/?__exactDev=1-2-3-4`)).text();
    const data = html.match(/<script type="application\/vnd\.exact\.dev-checkpoint"[^>]*>(.*?)<\/script>/s)?.[1];
    expect(data).toBeDefined();
    expect(JSON.parse(data)).toEqual(checkpoint);
    expect(html.match(/<!doctype html>/gi)).toHaveLength(1);
    await page.goto(`${loop.base}/?__exactDev=1-2-3-4`);
    await page.waitForFunction(() => globalThis.exact?.ready);
    expect(await page.getByTestId('body').textContent()).toBe(body);
    expect(await page.evaluate(() => globalThis.__injected)).toBeUndefined();
  } finally {
    await browser?.close();
    await loop?.stop();
    rmSync(dir, { recursive: true, force: true });
  }
}, 180000);

test('replacement generations never repeat through A, B, A and a carried reboot', async () => {
  const dir = mkdtempSync(resolve(ROOT, 'target/replacement-js-'));
  const source = join(dir, 'app.contract');
  const contract = (label, second = false) => `component App
  state kept = 1
  state retyped = ${second ? '"new"' : '1'}
${second ? '' : '  state removed = 1\n'}  action bump
    kept = kept + 1
  view
    column
      button press=bump testId="bump"
        text "Bump"
      text "${label}" testId="label"
${second ? '      Other()\n' : ''}      Editor()
component Editor
  state note = ""
  action write(value: string)
    note = value
  view
    input value=note input=write testId="draft"
component Other
  state note = "Unrelated"
  view
    text note testId="unrelated"
`;
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Replacement', app: { id: 'test.replacement', name: 'Replacement' } }));
  writeFileSync(source, contract('A'));
  let loop, browser;
  try {
    loop = await devLoop(dir);
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(loop.base + '?agent=1');
    await page.waitForFunction(() => globalThis.exact?.agentSettled);
    const read = () => page.evaluate(() => exact.agentSettled({ op: 'state' }));
    let before = await read();
    expect(before.generation).toBe(1);
    expect(before.replacement).toBeNull();
    await page.getByTestId('draft').fill('Piranesi');
    for (const [label, generation] of [['B', 2], ['A', 3], ['reboot', 4]]) {
      await page.getByTestId('bump').click();
      before = await read();
      if (label === 'reboot') {
        // The same plan, with the same carry handoff as the loop's document replacement.
        const checkpoint = await page.evaluate(() => ({ generation: exact.clock.generation, epoch: exact.clock.epoch, time: exact.clock.now,
          slots: exact.state[0].map(s => [s.n.devName, s.n.devType, s()]), answers: [], carryAnswers: false }));
        expect((await fetch(`${loop.base}/__dev/checkpoint?id=4-5-6-7&seq=3`, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(checkpoint) })).status).toBe(204);
        await page.goto(loop.base + '?agent=1&__exactDev=4-5-6-7');
      } else {
        writeFileSync(source, contract(label, label === 'B'));
        await page.waitForFunction(label => document.querySelector('[data-testid="label"]')?.textContent === label, label);
      }
      await page.waitForFunction(() => globalThis.exact?.agentSettled);
      const after = await read();
      expect(after.generation).toBe(generation);
      expect(after.epoch).toBeGreaterThanOrEqual(before.epoch);
      expect(after.incarnation).toBe(1);
      expect(after.slots.kept).toBe(before.slots.kept);
      expect(await page.getByTestId('draft').inputValue()).toBe('Piranesi');
      if (label === 'B') expect(await page.getByTestId('unrelated').textContent()).toBe('Unrelated');
      if (generation === 2) expect(after.replacement.dropped).toEqual([
        { what: 'state', name: 'retyped', why: 'type changed' },
        { what: 'state', name: 'removed', why: 'no place' },
      ]);
      expect((await page.evaluate(() => exact.agentSettled({ op: 'logs' }))).generation).toBe(generation);
    }
  } finally { await browser?.close(); await loop?.stop(); rmSync(dir, { recursive: true, force: true }); }
}, 180000);

test('a document replacement waits for composition and carries the completed text', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-composition-reload-'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Composition', app: { id: 'test.composition', name: 'Composition' } }));
  const contract = label => `component App
  state draft = ""
  action edit(value)
    draft = value
  view
    column
      input value=draft input=edit testId="draft"
      text "${label}" testId="label"
`;
  writeFileSync(join(dir, 'app.contract'), contract('before'));
  let loop, browser;
  try {
    loop = await devLoop(dir);
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(loop.base);
    await page.getByTestId('draft').focus();
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Input.imeSetComposition', { text: 'にほん', selectionStart: 3, selectionEnd: 3 });
    const built = new Promise(ok => page.on('console', message => {
      if (message.text().includes('exact dev reload: held')) ok();
    }));
    writeFileSync(join(dir, 'app.contract'), contract('after'));
    await Promise.race([built, page.waitForFunction(() => document.querySelector('[data-testid="label"]')?.textContent === 'after')]);
    expect(await page.getByTestId('label').textContent()).toBe('before');
    await cdp.send('Input.insertText', { text: '日本' });
    await page.waitForFunction(() => document.querySelector('[data-testid="label"]')?.textContent === 'after');
    expect(await page.getByTestId('draft').inputValue()).toBe('日本');
  } finally {
    await browser?.close();
    await loop?.stop();
    rmSync(dir, { recursive: true, force: true });
  }
}, 180000);

/** An expression for each target's boundary over the one `engagement` both ask (host/web/engagement.js). */
function replacementSource(target) {
  const source = readFileSync(join(ROOT, target === 'js' ? 'host/web-js/dev.mjs' : 'host/web/wasm/boot.js'), 'utf8');
  const start = target === 'js' ? source.indexOf('function replacementBoundary(engagement)') : source.indexOf('export const replacements = (() => {');
  return target === 'js' ? `(${source.slice(start)})(${engagement})`
    : `(engagement => ${source.slice(start + 'export const replacements = '.length, source.indexOf('})();', start) + 4)})(${engagement})`;
}

test.each(['js', 'wasm'])('a %s replacement keeps a held pointer across field focus changes', async target => {
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage();
    await page.setContent('<input id="a"><input id="b">');
    await page.addScriptTag({ content: `globalThis.boundary = ${replacementSource(target)}` });
    await page.locator('#a').focus();
    const box = await page.locator('#b').boundingBox();
    await page.mouse.move(box.x + 5, box.y + 5);
    await page.mouse.down();
    expect(await page.evaluate(async () => {
      globalThis.cleared = false;
      boundary.wait().then(() => globalThis.cleared = true);
      const channel = new MessageChannel();
      await new Promise(ok => { channel.port1.onmessage = ok; channel.port2.postMessage(null); });
      channel.port1.close(); channel.port2.close();
      return { active: document.activeElement.id, pending: boundary.pending, cleared };
    })).toEqual({ active: 'b', pending: 'pointer', cleared: false });
    await page.mouse.up();
    await page.waitForFunction(() => globalThis.cleared);
  } finally { await browser.close(); }
});

test.each(['js', 'wasm'])('a %s replacement waits for a scrolling touch to lift', async target => {
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage({ hasTouch: true, isMobile: true, viewport: { width: 400, height: 600 } });
    await page.setContent('<meta name="viewport" content="width=device-width"><div style="height:3000px">Scroll</div>');
    await page.addScriptTag({ content: `globalThis.boundary = ${replacementSource(target)};
      globalThis.cancelled = false;
      addEventListener('pointercancel', () => cancelled = true);` });
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x: 100, y: 400 }] });
    await page.evaluate(() => { globalThis.cleared = false; boundary.wait().then(() => cleared = true); });
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchMove', touchPoints: [{ x: 100, y: 100 }] });
    await page.waitForFunction(() => cancelled);
    expect(await page.evaluate(() => ({ cleared, pending: boundary.pending }))).toEqual({ cleared: false, pending: 'pointer' });
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await page.waitForFunction(() => cleared);
  } finally { await browser.close(); }
});

test.each(['js', 'wasm'])('a %s replacement is not held by a drag its handler cancelled', async target => {
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage();
    await page.setContent('<div id="grip" draggable="true" style="width:120px;height:120px;background:#888"></div>');
    // As Exact's transform handler does: the browser then sends no dragend and no drop.
    await page.addScriptTag({ content: `globalThis.boundary = ${replacementSource(target)};
      globalThis.cancelled = 0;
      document.getElementById('grip').addEventListener('dragstart', e => { cancelled++; e.preventDefault(); });` });
    const box = await page.locator('#grip').boundingBox();
    await page.mouse.move(box.x + 20, box.y + 20);
    await page.mouse.down();
    await page.mouse.move(box.x + 70, box.y + 70, { steps: 8 });
    await page.waitForFunction(() => cancelled > 0);
    await page.evaluate(() => { globalThis.cleared = false; boundary.wait().then(() => cleared = true); });
    expect(await page.evaluate(() => ({ cleared, pending: boundary.pending }))).toEqual({ cleared: false, pending: 'pointer' });
    await page.mouse.up();
    await page.waitForFunction(() => cleared, null, { timeout: 3000 });
    expect(await page.evaluate(() => boundary.pending)).toBeNull();
  } finally { await browser.close(); }
});

// The window's blur ends no hold: the button, the finger or Safari's pinch is still down, and its release, a
// buttonless move or `pagehide` ends it.
test.each(['js', 'wasm'])('a %s replacement stays held when the window blurs mid-interaction', async target => {
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage({ hasTouch: true });
    await page.setContent('<div id="photo" style="width:200px;height:200px"></div>');
    await page.addScriptTag({ content: `globalThis.boundary = ${replacementSource(target)};
      globalThis.pinch = type => document.getElementById('photo').dispatchEvent(new Event(type, { bubbles: true }));` });
    const cdp = await page.context().newCDPSession(page);
    const blurred = () => page.evaluate(async () => {
      globalThis.cleared = false;
      boundary.wait().then(() => cleared = true);
      dispatchEvent(new FocusEvent('blur'));
      const channel = new MessageChannel();
      await new Promise(ok => { channel.port1.onmessage = ok; channel.port2.postMessage(null); });
      channel.port1.close(); channel.port2.close();
      return { cleared, pending: boundary.pending };
    });
    const ended = () => page.waitForFunction(() => cleared && boundary.pending === null, null, { timeout: 3000 });
    await page.mouse.move(50, 50);
    await page.mouse.down();
    expect(await blurred()).toEqual({ cleared: false, pending: 'pointer' });
    await page.mouse.up();
    await ended();
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x: 50, y: 50 }] });
    expect(await blurred()).toEqual({ cleared: false, pending: 'pointer' });
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await ended();
    await page.evaluate(() => pinch('gesturestart'));
    expect(await blurred()).toEqual({ cleared: false, pending: 'gesture' });
    await page.evaluate(() => pinch('gestureend'));
    await ended();
  } finally { await browser.close(); }
});

// Safari brackets a trackpad pinch with its own events, which both targets' zoom handlers hear (zoom.js,
// motion-glue.js); no pointer is down. Dispatched here as Safari does, on the element under the gesture.
test.each(['js', 'wasm'])("a %s replacement waits for Safari's trackpad pinch to end", async target => {
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage();
    await page.setContent('<div id="photo" style="width:200px;height:200px"></div>');
    await page.addScriptTag({ content: `globalThis.boundary = ${replacementSource(target)};
      globalThis.pinch = type => document.getElementById('photo').dispatchEvent(new Event(type, { bubbles: true }));` });
    const state = () => page.evaluate(async () => {
      const channel = new MessageChannel();
      await new Promise(ok => { channel.port1.onmessage = ok; channel.port2.postMessage(null); });
      channel.port1.close(); channel.port2.close();
      return { cleared, pending: boundary.pending };
    });
    await page.evaluate(() => { pinch('gesturestart'); globalThis.cleared = false; boundary.wait().then(() => cleared = true); });
    expect(await state()).toEqual({ cleared: false, pending: 'gesture' });
    await page.evaluate(() => pinch('gesturechange'));
    expect(await state()).toEqual({ cleared: false, pending: 'gesture' });
    await page.evaluate(() => pinch('gestureend'));
    await page.waitForFunction(() => cleared, null, { timeout: 3000 });
    expect(await page.evaluate(() => boundary.pending)).toBeNull();
  } finally { await browser.close(); }
});

// The event that ends an interaction goes to the element it began on, and to no ancestor once that element has left
// the document (a touch's, https://w3c.github.io/touch-events/#event-touchend; a drag's, at its source node,
// https://html.spec.whatwg.org/multipage/dnd.html#dndevents).
test.each(['js', 'wasm'])('a %s replacement resumes when a touched element is removed before the finger lifts', async target => {
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage({ hasTouch: true, isMobile: true, viewport: { width: 400, height: 600 } });
    await page.setContent('<meta name="viewport" content="width=device-width"><div id="row" style="height:120px;background:#888"></div>');
    await page.addScriptTag({ content: `globalThis.boundary = ${replacementSource(target)}` });
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchStart', touchPoints: [{ x: 100, y: 60 }] });
    await page.evaluate(() => { globalThis.cleared = false; boundary.wait().then(() => cleared = true); document.getElementById('row').remove(); });
    expect(await page.evaluate(() => ({ cleared, pending: boundary.pending }))).toEqual({ cleared: false, pending: 'pointer' });
    await cdp.send('Input.dispatchTouchEvent', { type: 'touchEnd', touchPoints: [] });
    await page.waitForFunction(() => cleared, null, { timeout: 3000 });
    expect(await page.evaluate(() => boundary.pending)).toBeNull();
  } finally { await browser.close(); }
});

test.each(['js', 'wasm'])("a %s replacement resumes when a drag's source is removed before the drop", async target => {
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage();
    await page.setContent('<div id="grip" draggable="true" style="width:120px;height:120px;background:#888"></div><div style="height:300px"></div>');
    await page.addScriptTag({ content: `globalThis.boundary = ${replacementSource(target)};
      globalThis.ended = false;
      const grip = document.getElementById('grip');
      grip.addEventListener('dragend', () => ended = true);` });
    await page.mouse.move(20, 20);
    await page.mouse.down();
    await page.mouse.move(80, 200, { steps: 8 });
    await page.evaluate(() => { globalThis.cleared = false; boundary.wait().then(() => cleared = true); grip.remove(); });
    expect(await page.evaluate(() => ({ cleared, pending: boundary.pending }))).toEqual({ cleared: false, pending: 'gesture' });
    await page.mouse.up();
    await page.waitForFunction(() => ended);
    await page.waitForFunction(() => cleared, null, { timeout: 3000 });
    expect(await page.evaluate(() => boundary.pending)).toBeNull();
  } finally { await browser.close(); }
});

// As Safari sends a pinch's end: to the element the gesture began on.
test.each(['js', 'wasm'])('a %s replacement resumes when a pinched element is removed before the pinch ends', async target => {
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage();
    await page.setContent('<div id="photo" style="width:200px;height:200px"></div>');
    await page.addScriptTag({ content: `globalThis.boundary = ${replacementSource(target)};
      globalThis.photo = document.getElementById('photo');` });
    await page.evaluate(() => {
      photo.dispatchEvent(new Event('gesturestart', { bubbles: true }));
      globalThis.cleared = false; boundary.wait().then(() => cleared = true);
      photo.remove();
    });
    expect(await page.evaluate(() => ({ cleared, pending: boundary.pending }))).toEqual({ cleared: false, pending: 'gesture' });
    await page.evaluate(() => photo.dispatchEvent(new Event('gestureend', { bubbles: true })));
    await page.waitForFunction(() => cleared, null, { timeout: 3000 });
    expect(await page.evaluate(() => boundary.pending)).toBeNull();
  } finally { await browser.close(); }
});

test.each(['js', 'wasm'])('a %s replacement resumes when its composing field is removed', async target => {
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage();
    await page.setContent('<input id="draft">');
    await page.addScriptTag({ content: `globalThis.boundary = ${replacementSource(target)}` });
    await page.locator('#draft').focus();
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Input.imeSetComposition', { text: 'にほん', selectionStart: 3, selectionEnd: 3 });
    await page.evaluate(() => { globalThis.cleared = false; boundary.wait().then(() => cleared = true); });
    expect(await page.evaluate(() => boundary.pending)).toBe('composition');
    await page.locator('#draft').evaluate(el => el.remove());
    await page.waitForFunction(() => cleared, null, { timeout: 2000 });
    expect(await page.evaluate(() => boundary.pending)).toBeNull();
  } finally { await browser.close(); }
});

test.each(['input', 'composition', 'background', 'candidate', 'failure'])('a checkpoint handoff handles late %s', async interaction => {
  const composing = interaction === 'composition', background = interaction === 'background';
  const dir = mkdtempSync(join(tmpdir(), 'exact-late-composition-'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Handoff', app: { id: 'test.handoff', name: 'Handoff' } }));
  const contract = label => `component App
  state draft = ""
  state count = 0
  action edit(value)
    draft = value
  action tick
    count = count + 1
  task pulse mount
    every(100000, tick)
  view
    column
      input value=draft input=edit testId="draft"
      text count.toString() testId="count"
      text "${label}" testId="label"
`;
  writeFileSync(join(dir, 'app.contract'), contract('before'));
  let loop, browser;
  try {
    loop = await devLoop(dir);
    browser = await suiteBrowser();
    const page = await browser.newPage();
    // Deliver a new generation through the page's stream without depending on file watching.
    await page.addInitScript(() => {
      globalThis.documentToken = crypto.randomUUID();
      const Stream = EventSource;
      globalThis.EventSource = class extends Stream {
        constructor(...args) { super(...args); if (args[0] === '/__dev/page') globalThis.devStream = this; }
      };
    });
    await page.goto(loop.base);
    await page.getByTestId('draft').focus();
    let reached, release;
    const intercepted = new Promise(ok => reached = ok), gate = new Promise(ok => release = ok);
    let posts = 0;
    await page.route('**/__dev/checkpoint?**', async route => {
      if (background) {
        // Fire the app's timer on every POST. Background writes must not
        // turn a handoff into an endless sequence of newer checkpoints.
        await page.evaluate(() => globalThis.exact.clock.timers[0].action());
        if (++posts === 6) { reached(); await gate; }
        await route.continue();
        return;
      }
      if (++posts === 1) {
        await page.evaluate(() => globalThis.exact.clock.timers[0].action());
        reached(); await gate;
      }
      if (interaction === 'failure') { await route.fulfill({ status: 500, body: 'handoff refused' }); return; }
      await route.continue();
    });
    const documentToken = await page.evaluate(() => globalThis.documentToken);
    const retained = interaction === 'failure' ? page.waitForEvent('console', { predicate: m => m.text().includes('checkpoint handoff failed') }) : null;
    await page.evaluate(() => devStream.dispatchEvent(new MessageEvent('message', { data: JSON.stringify({ reload: 2 }) })));
    if (background) {
      await Promise.race([intercepted, page.waitForFunction(token => globalThis.documentToken !== token, documentToken)]);
      release();
      expect(posts).toBe(1);
    } else await intercepted;
    const cdp = await page.context().newCDPSession(page);
    if (composing) await cdp.send('Input.imeSetComposition', { text: 'にほん', selectionStart: 3, selectionEnd: 3 });
    else if (interaction === 'input') await page.getByTestId('draft').fill('late input');
    else if (interaction === 'candidate') await page.evaluate(() => devStream.dispatchEvent(new MessageEvent('message', { data: JSON.stringify({ reload: 3 }) })));
    release();
    if (retained) {
      await retained;
      expect(await page.evaluate(() => globalThis.documentToken)).toBe(documentToken);
      expect(await page.getByTestId('count').textContent()).toBe('1');
      expect(posts).toBe(1);
      return;
    }
    if (composing) {
      await page.waitForFunction(token => globalThis.__exactDevReplacement?.pending === 'composition' || globalThis.documentToken !== token, documentToken);
      expect(await page.evaluate(() => globalThis.documentToken)).toBe(documentToken);
      await cdp.send('Input.insertText', { text: '日本' });
    }
    await page.waitForFunction(token => globalThis.documentToken !== token, documentToken);
    await page.waitForFunction(() => globalThis.exact?.ready);
    await page.evaluate(() => globalThis.exact.ready);
    expect(await page.getByTestId('draft').inputValue()).toBe(composing ? '日本' : interaction === 'input' ? 'late input' : '');
    expect(await page.getByTestId('count').textContent()).toBe(background ? '0' : '1');
    expect(posts).toBe(background ? 1 : 2);
  } finally {
    await browser?.close(); await loop?.stop();
    rmSync(dir, { recursive: true, force: true });
  }
}, 180000);

test('a held reload abandoned at pagehide runs when the page is shown again', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-pagehide-reload-'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Pagehide', app: { id: 'test.pagehide', name: 'Pagehide' } }));
  writeFileSync(join(dir, 'app.contract'), 'component App\n  view\n    text "label" testId="label"\n');
  let loop, browser;
  try {
    loop = await devLoop(dir);
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.addInitScript(() => {
      globalThis.documentToken = crypto.randomUUID();
      const Stream = EventSource;
      globalThis.EventSource = class extends Stream {
        constructor(...args) { super(...args); if (args[0] === '/__dev/page') globalThis.devStream = this; }
      };
    });
    await page.goto(loop.base);
    await page.waitForFunction(() => globalThis.exact?.ready);
    const documentToken = await page.evaluate(() => globalThis.documentToken);
    await page.mouse.move(5, 5);
    await page.mouse.down();
    await page.evaluate(() => devStream.dispatchEvent(new MessageEvent('message', { data: JSON.stringify({ reload: 2 }) })));
    await page.waitForFunction(() => globalThis.__exactDevReplacement?.pending === 'pointer');
    // Into the back-forward cache with the reload held, and back: the document is the same one.
    await page.evaluate(() => { dispatchEvent(new PageTransitionEvent('pagehide', { persisted: true })); });
    await page.evaluate(() => { dispatchEvent(new PageTransitionEvent('pageshow', { persisted: true })); });
    await page.waitForFunction(token => globalThis.documentToken !== token, documentToken, { timeout: 15000 });
  } finally {
    await browser?.close(); await loop?.stop();
    rmSync(dir, { recursive: true, force: true });
  }
}, 180000);

test.each([['literal', false], ['bound', true]])('a blurred autofocus field stays blurred after a carried reload (%s)', async (_mode, bound) => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-blurred-carry-'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Blurred carry', app: { id: 'test.blurred.carry', name: 'Blurred carry' } }));
  const contract = label => `component App
  state wantsFocus = true
  view
    column
      input autofocus=${bound ? 'wantsFocus' : 'true'} testId="draft"
      text "${label}" testId="label"
`;
  writeFileSync(join(dir, 'app.contract'), contract('before'));
  let loop, browser;
  try {
    loop = await devLoop(dir);
    expect(loop.said()).not.toContain('does not build');
    browser = await suiteBrowser();
    const page = await browser.newPage(), messages = [];
    page.on('console', m => messages.push(`${m.type()}: ${m.text()}`));
    page.on('pageerror', e => messages.push(`pageerror: ${e.message}`));
    await page.goto(loop.base);
    await page.waitForFunction(() => document.activeElement === document.querySelector('[data-testid="draft"]'));
    await page.getByTestId('draft').blur();
    expect(await page.evaluate(() => document.activeElement === document.body)).toBe(true);
    writeFileSync(join(dir, 'app.contract'), contract('after'));
    await page.waitForFunction(() => document.querySelector('[data-testid="label"]')?.textContent === 'after').catch(e => {
      throw new Error(`${e.message}\n${messages.join('\n')}\n${loop.said()}`);
    });
    // Let the browser flush its own autofocus candidates as well as the runtime's mount work.
    await page.evaluate(() => new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok))));
    expect(await page.evaluate(() => document.activeElement === document.body)).toBe(true);
  } finally {
    await browser?.close();
    await loop?.stop();
    rmSync(dir, { recursive: true, force: true });
  }
}, 180000);


test('the JS loop drops settled answers when a page module or its imported helper changes', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-native-carry-'));
  const modules = join(dir, 'modules/web');
  mkdirSync(modules, { recursive: true });
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Native carry', app: { id: 'test.native.carry', name: 'Native carry' } }));
  const contract = label => `shape Sample
  value: string
component App
  state draft = "initial"
  resource sample = sample() as shape Sample
  action edit(value)
    draft = value
  view
    column
      input value=draft input=edit testId="draft"
      text sample.value testId="sample"
      text "${label}" testId="label"
`;
  writeFileSync(join(dir, 'app.contract'), contract('before'));
  writeFileSync(join(dir, 'app.ts'), `import type { Answer } from './app.contract.d.ts';
export const grants = '';
export const answer: Answer = (_source, _args, _store, _storage, native) => native!.later({}) as ReturnType<Answer>;
`);
  const helper = value => `export const value = () => '${value}:' + crypto.getRandomValues(new Uint32Array(2)).join('.');\n`;
  const entry = suffix => `import { value } from './value.js';\nexport const later = () => ({ value: value() + '${suffix}' });\n`;
  writeFileSync(join(modules, 'value.js'), helper('before'));
  writeFileSync(join(modules, 'index.js'), entry('entry-before'));
  let loop, browser;
  try {
    loop = await devLoop(dir);
    expect(loop.said()).not.toContain('does not build');
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(loop.base);
    await page.waitForFunction(() => document.querySelector('[data-testid="sample"]')?.textContent.startsWith('before:'));
    const before = await page.getByTestId('sample').textContent();
    const revision = () => readFileSync(join(loop.dist, '.exact-dev-logic.json'), 'utf8');
    const originalRevision = revision();
    await page.getByTestId('draft').fill('carried');
    writeFileSync(join(dir, 'app.contract'), contract('presentation'));
    await page.waitForFunction(() => document.querySelector('[data-testid="label"]')?.textContent === 'presentation');
    expect(revision()).toBe(originalRevision);
    expect(await page.getByTestId('draft').inputValue()).toBe('carried');
    expect(await page.getByTestId('sample').textContent()).toBe(before);
    writeFileSync(join(modules, 'value.js'), helper('helper-after'));
    await page.waitForFunction(() => document.querySelector('[data-testid="sample"]')?.textContent.startsWith('helper-after:'));
    expect(revision()).not.toBe(originalRevision);
    const helperRevision = revision();
    writeFileSync(join(modules, 'index.js'), entry('entry-after'));
    await page.waitForFunction(() => document.querySelector('[data-testid="sample"]')?.textContent.endsWith('entry-after'));
    expect(revision()).not.toBe(helperRevision);
    expect(await page.getByTestId('draft').inputValue()).toBe('carried');
  } finally {
    await browser?.close();
    await loop?.stop();
    rmSync(dir, { recursive: true, force: true });
  }
}, 180000);

test('the JS loop: a build an edit lands during is set aside, never published, and the newer files build', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-superseded-'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Superseded', app: { id: 'com.exact.superseded', name: 'Superseded' } }));
  const view = (text) => `component App\n  view\n    column background-color="surface.card" min-height=40\n      text "${text}"\n`;
  writeFileSync(join(dir, 'app.contract'), view('qz-first'));
  writeFileSync(join(dir, 'app.forma'), 'forma superseded\n\ntheme\n  surface.card = light-dark(#f1f3f5, #16181c)\n');
  let loop = null;
  try {
    // Each build's end comes 2 s after it: an edit lands as one ends.
    loop = await devLoop(dir, { EXACT_DEV_BUILD_HOLD_MS: '2000' });
    const { said, dist } = loop;
    const until = async (pass, ms) => { const end = Date.now() + ms; while (!pass()) { if (Date.now() > end) throw new Error(`not seen in: ${said().slice(-2000)}`); await new Promise(ok => setTimeout(ok, 100)); } };
    await until(() => said().includes('plan ready') && said().includes('design reader ready'), 900_000);
    // What dist holds, whatever file carries the text.
    const holds = (text) => { const walk = (d) => readdirSync(d, { withFileTypes: true }).some(e => e.isDirectory() ? walk(join(d, e.name)) : readFileSync(join(d, e.name)).includes(text)); return walk(dist); };
    expect(holds('qz-first')).toBe(true);
    // An edit, and another while its build runs: the first's build is set
    // aside, and only the second's is published.
    // Sampled throughout: a fresh page never loads the set-aside build.
    let stale = false;
    const sampler = setInterval(() => { try { stale ||= holds('qz-second'); } catch { /* replaced meanwhile */ } }, 10);
    writeFileSync(join(dir, 'app.contract'), view('qz-second'));
    // Its build (a fraction of a second here) has read the files, and is
    // held 2 s at its end.
    await new Promise(ok => setTimeout(ok, 1200));
    writeFileSync(join(dir, 'app.contract'), view('qz-third'));
    await until(() => said().includes('edit → plan ready') && holds('qz-third'), 300_000);
    await new Promise(ok => setTimeout(ok, 500));
    clearInterval(sampler);
    expect(said()).toContain('edit → a build superseded');
    // The input that moved, named (stage 15 D1.5).
    expect(said()).toMatch(/set aside: app\.contract moved while it built/);
    expect(stale).toBe(false);
  } finally {
    await loop?.stop().catch(() => {});
    rmSync(dir, { recursive: true, force: true });
  }
}, 1_200_000);

test('the JS loop: an edit stops a young build, leaving dist as it was; a value batch during a verifying build applies, and the next build carries it', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-stopped-'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Stopped', app: { id: 'com.exact.stopped', name: 'Stopped' } }));
  const view = (text) => `component App\n  view\n    column background-color="surface.card" min-height=40\n      text "${text}"\n`;
  const design = (dark) => `forma stopped\n\ntheme\n  surface.card = light-dark(#f1f3f5, ${dark})\n`;
  writeFileSync(join(dir, 'app.contract'), view('qz-one'));
  writeFileSync(join(dir, 'app.forma'), design('#16181c'));
  let loop = null;
  try {
    loop = await devLoop(dir, { EXACT_DEV_BUILD_HOLD_MS: '4000' });
    const { said, dist, base } = loop;
    const until = async (pass, ms) => { const end = Date.now() + ms; while (!pass()) { if (Date.now() > end) throw new Error(`not seen in: ${said().slice(-2000)}`); await new Promise(ok => setTimeout(ok, 50)); } };
    const count = (re) => (said().match(re) ?? []).length;
    await until(() => said().includes('plan ready') && said().includes('design reader ready'), 900_000);
    const bytes = () => { const out = []; const walk = (d) => { for (const e of readdirSync(d, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) e.isDirectory() ? walk(join(d, e.name)) : out.push(e.name, sha(readFileSync(join(d, e.name)))); }; walk(dist); return out.join(','); };
    let first = bytes(), seen = new Set();
    // A sample is one dist (the same directory before and after it is read;
    // a swap is two renames).
    const sample = () => { const ino = statSync(dist).ino, b = bytes(); return statSync(dist).ino === ino ? b : null; };
    const sampler = setInterval(() => { try { const b = sample(); if (b) seen.add(b); } catch { /* replaced meanwhile */ } }, 10);
    // Three edits at once: the first's build is stopped young, and only
    // that one (one stop in a burst); the last edit's build publishes.
    // The second edit must find the first's build begun: a build begins
    // after the edit's debounce and its own preparation (the compiler's
    // copy, the reader's build started), past 30 ms on a loaded machine,
    // and then both edits are one build, published, and the burst is
    // made again.
    let aside, plans;
    for (let tries = 0; ; tries++) {
      if (tries === 5) throw new Error(`no build of a burst's first edit was begun by its second, in 5 tries: ${said().slice(-2000)}`);
      first = bytes(); seen = new Set(); aside = count(/edit → a build superseded/g); plans = count(/edit → plan ready/g);
      const readers = count(/design reader ready/g);
      writeFileSync(join(dir, 'app.contract'), view(`qz-two-${tries}`));
      await new Promise(ok => setTimeout(ok, 30));
      writeFileSync(join(dir, 'app.contract'), view(`qz-three-${tries}`));
      await until(() => said().includes('(aborted)') || count(/edit → plan ready/g) > plans, 300_000);
      if (said().includes('(aborted)')) break;
      await until(() => count(/design reader ready/g) > readers, 300_000);
    }
    // The stopped build ends (set aside) and the next begins; an edit in
    // that one sets it aside too, but stops it not.
    await until(() => count(/edit → a build superseded/g) >= aside + 1, 60_000);
    await new Promise(ok => setTimeout(ok, 300));
    writeFileSync(join(dir, 'app.contract'), view('qz-four'));
    await until(() => count(/edit → plan ready/g) > plans, 300_000);
    expect(count(/\(aborted\)/g)).toBe(1);
    expect(count(/edit → a build superseded/g)).toBe(aside + 2);
    clearInterval(sampler);
    // dist was the first build's until the last one published: never the
    // stopped build's, nor a half-written one.
    const last = bytes();
    expect([...seen].every(b => b === first || b === last)).toBe(true);
    expect(last).not.toBe(first);
    const holds = (text) => { const walk = (d) => readdirSync(d, { withFileTypes: true }).some(e => e.isDirectory() ? walk(join(d, e.name)) : readFileSync(join(d, e.name)).includes(text)); return walk(dist); };
    expect(holds('qz-four')).toBe(true);
    // A value-only edit (its batch, then its verifying build, held at its
    // end) and another while that build runs: the second's batch applies
    // at once, the build is set aside, and the next carries the value.
    // (The reader's build is a cargo build on the shared target/, queued
    // behind its lock while another build holds it: a build's bound.)
    await until(() => count(/design reader ready/g) >= 3, 300_000);
    const supersededBefore = count(/edit → a build superseded/g);
    writeFileSync(join(dir, 'app.forma'), design('#202020'));
    await until(() => count(/edit → batch ready/g) >= 1, 60_000);
    await new Promise(ok => setTimeout(ok, 500));
    const held = Date.now();
    writeFileSync(join(dir, 'app.forma'), design('#303030'));
    // Its batch from the design read, not a build's, before the held
    // build could end.
    await until(() => count(/edit → batch ready .*no build/g) >= 2, 60_000);
    expect(Date.now() - held).toBeLessThan(3000);
    // Served at once (the pages' snapshot), before the held build ends.
    expect(JSON.stringify(await (await fetch(`${base}/__dev/design`)).json())).toContain('#303030');
    await until(() => count(/edit → a build superseded/g) > supersededBefore, 60_000);
    await until(() => said().includes('edit → checked'), 120_000);
    const served = await (await fetch(`${base}/__dev/design`)).json();
    expect(JSON.stringify(served)).toContain('#303030');
    // And during a full build (app.json's, which leaves the design's shape):
    // the batch applies at once, and the build after serves it.
    // Past the age an edit stops a build at, with the design reader built
    // again after it (any edit but a design's builds it), and the build
    // still running (held at its end). The reader's build is a cargo build
    // on the shared target/, queued behind its lock while another build
    // holds it: back after the full build has ended, the step is staged again.
    let batches, settled, asideBefore;
    for (let tries = 0; ; tries++) {
      if (tries === 5) throw new Error(`the design reader was not built again while a full build ran, in 5 tries: ${said().slice(-2000)}`);
      batches = count(/edit → batch ready .*no build/g); settled = count(/edit → plan ready|edit → checked/g);
      asideBefore = count(/edit → a build superseded/g);
      const readers = count(/design reader ready/g);
      writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: `Stopped again ${tries}`, app: { id: 'com.exact.stopped', name: 'Stopped' } }));
      await new Promise(ok => setTimeout(ok, 2200));
      await until(() => count(/design reader ready/g) > readers, 300_000);
      if (count(/edit → plan ready|edit → checked/g) === settled) break;
    }
    const full = Date.now();
    writeFileSync(join(dir, 'app.forma'), design('#404040'));
    await until(() => count(/edit → batch ready .*no build/g) > batches, 60_000);
    expect(Date.now() - full).toBeLessThan(3000);
    await until(() => count(/edit → plan ready|edit → checked/g) > settled, 300_000);
    expect(count(/edit → a build superseded/g)).toBe(asideBefore + 1);
    expect(JSON.stringify(await (await fetch(`${base}/__dev/design`)).json())).toContain('#404040');
  } finally {
    await loop?.stop().catch(() => {});
    rmSync(dir, { recursive: true, force: true });
  }
}, 1_200_000);

test.skipIf(process.platform === 'win32')('the design reader observes dev flags and rechecks kept inputs by content', async () => {
  mkdirSync(join(ROOT, 'target'), { recursive: true });
  const dir = mkdtempSync(join(ROOT, 'target/exact-reader-choice-')), app = join(dir, 'app'), target = join(dir, 'cargo');
  const name = 'exact-web-js', debug = join(target, 'debug', name), input = join(dir, 'compiler source.rs');
  const fast = join(target, 'host-dev', name), bin = join(dir, 'bin'), started = join(dir, 'started'), release = join(dir, 'release');
  const invoked = join(dir, 'invoked');
  const set = join(dir, '.cache/exact/tools', hostTriple(), 'reader');
  const compiler = marker => `#!/bin/sh
    if [ "$1" = js ]; then
      printf '%s\\n' '${marker}' >> "$HOME/invoked"
      exit 1
    fi
    [ "$1" = inspect-table ] || exit 1
    case "$(/bin/cat "$2")" in
      first) printf '%s' '${JSON.stringify({ plan: sha('first'), marker })}';;
      second) printf '%s' '${JSON.stringify({ plan: sha('second'), marker })}';;
    esac > "$3"
  `;
  const stamp = 1700000000;
  const pin = Bun.TOML.parse(readFileSync(join(ROOT, 'rust-toolchain.toml'), 'utf8')).toolchain.channel;
  const rustup = join(dir, 'rustup');
  const env = { HOME: dir, USERPROFILE: dir, RUSTUP_HOME: rustup, PATH: `${bin}:${process.env.PATH}`,
    CARGO_TARGET_DIR: target, EXACT_TOOLS_ORIGIN: 'none', EXACT_WEB_COMPILER: '',
    RUSTFLAGS: undefined, CARGO_ENCODED_RUSTFLAGS: undefined };
  for (const key of Object.keys(process.env)) if (key.startsWith('CARGO_PROFILE_DEV_')) env[key] = undefined;
  const until = async pass => {
    const end = Date.now() + 5000;
    while (!pass()) {
      if (Date.now() > end) throw new Error(loop.said());
      await Bun.sleep(20);
    }
  };
  let loop;
  try {
    for (const path of [app, dirname(debug), dirname(fast), set, bin, join(rustup, 'toolchains', `${pin}-${hostTriple()}`)]) mkdirSync(path, { recursive: true });
    writeFileSync(join(app, 'app.json'), JSON.stringify({ name: 'Reader choice', app: { id: 'test.readerchoice', name: 'Reader choice' } }));
    writeFileSync(join(app, 'app.contract'), 'component App\n  view\n    text "Reader"\n');
    writeFileSync(input, 'old');
    utimesSync(input, stamp, stamp);
    const escape = path => path.replaceAll(' ', '\\ ');
    const writeCompiler = (path, marker) => {
      writeFileSync(path, compiler(marker), { mode: 0o755 });
      writeFileSync(`${path}.d`, `${escape(path)}: ${escape(input)}\n`);
    };
    writeCompiler(debug, 'debug');
    expect(freshCompiler(debug)).toBe(true);
    writeCompiler(join(dir, 'optimized'), 'host-dev');
    writeFileSync(join(bin, 'cargo'), `#!/bin/sh
      [ "$1" = build ] && [ "$4" = host-dev ] || exit 1
      : > "$HOME/started"
      while [ ! -e "$HOME/release" ]; do /bin/sleep 0.02; done
      /bin/cp "$HOME/optimized" "$CARGO_TARGET_DIR/host-dev/exact-web-js"
      /bin/cp "$HOME/optimized.d" "$CARGO_TARGET_DIR/host-dev/exact-web-js.d"
    `, { mode: 0o755 });
    writeFileSync(join(set, name), compiler('set'), { mode: 0o755 });
    writeFileSync(join(set, 'manifest.json'), JSON.stringify({ source: 'built', verifiedBuild: true, products: {
      [name]: { file: name, crate: name, inputs: [[relative(ROOT, input), digest(input)]],
        closure: lockClosure(ROOT, name), workspace: sha(JSON.stringify(workspaceManifest(ROOT))) },
    } }));
    const inspect = async plan => {
      mkdirSync(loop.dist, { recursive: true });
      writeFileSync(join(loop.dist, 'app.plan'), plan);
      const response = await fetch(`${loop.base}/dev-inspect.json`);
      expect(response.status).toBe(200);
      return (await response.json()).marker;
    };
    loop = await devLoop(app, { ...env, RUSTFLAGS: '-g' });
    expect(loop.said()).not.toContain('design reader ready');
    await until(() => existsSync(started));
    expect(await inspect('first')).toBe('debug');
    writeFileSync(release, '');
    await until(() => loop.said().includes('design reader ready'));
    expect(await inspect('second')).toBe('host-dev');
    await loop.stop();
    for (const path of [fast, release, started]) rmSync(path);
    loop = await devLoop(app, env);
    expect(loop.said()).toContain("design reader ready (a tool set's)");
    expect(await inspect('first')).toBe('set');
    expect(existsSync(started)).toBe(false);
    writeFileSync(input, 'new');
    utimesSync(input, stamp, stamp);
    await Bun.sleep(1000);
    expect(await inspect('second')).toBe('debug');
    await until(() => existsSync(started));
    await loop.stop();
    rmSync(started, { force: true });

    const copy = join(target, 'web-js-dev-compiler-app');
    writeCompiler(copy, 'copy');
    writeFileSync(invoked, '');
    loop = await devLoop(app, { ...env, EXACT_WEB_COMPILER: debug });
    expect(loop.said()).not.toContain('design reader ready');
    await until(() => existsSync(started));
    expect(readFileSync(invoked, 'utf8').trim()).toBe('debug');
    writeFileSync(join(app, 'app.contract'), 'component App\n  view\n    text "Reader edit"\n');
    await until(() => readFileSync(invoked, 'utf8').trim().split('\n').length >= 2);
    expect(readFileSync(invoked, 'utf8').trim().split('\n')).toEqual(['debug', 'debug']);
  } finally {
    await loop?.stop();
    rmSync(dir, { recursive: true, force: true });
  }
}, 30_000);

test('a tool set is revalidated once a second through repeated reader requests', () => {
  const source = readFileSync(new URL('./dev.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('  const readerCompiler = ');
  const end = source.indexOf('  const toolExits = ', start);
  let now = 1, calls = 0, selected = 'set';
  const current = runInNewContext(source.slice(start, end) + '\ntoolCurrent', {
    process: { env: {} }, ownTool: 'own', tool: { path: 'set' },
    performance: { now: () => now },
    webCompilerFrom(_env, options) {
      expect(options?.rehash).toBe(true);
      calls++;
      return { path: selected };
    },
  });
  expect(current()).toBe(true);
  selected = 'debug';
  for (now = 17; now < 1001; now += 16) expect(current()).toBe(true);
  expect(calls).toBe(1);
  now = 1001;
  expect(current()).toBe(false);
  expect(calls).toBe(2);
});

test.skipIf(process.platform === 'win32')('an inspect request retries when the selected reader was evicted', async () => {
  const source = readFileSync(new URL('./dev.mjs', import.meta.url), 'utf8');
  const current = source.slice(source.indexOf('  const readerCompiler = '), source.indexOf('  const patcher = '));
  const reader = source.slice(source.indexOf('  const toolFresh = '), source.indexOf('  // The design\'s shape'));
  const dir = mkdtempSync(join(tmpdir(), 'exact-reader-eviction-'));
  const kept = join(dir, 'kept'), debug = join(dir, 'debug'), plan = join(dir, 'app.plan');
  const launch = cmd => ({ cmd, pre: [], done() {} });
  const writeReader = (path, marker) => writeFileSync(path, `#!/bin/sh
    [ "$1" = inspect-table ] || exit 1
    printf '{"plan":"%s","marker":"${marker}"}' "$(/bin/cat "$2")" > "$3"
  `, { mode: 0o755 });
  const launched = [], inspections = new Set();
  try {
    writeReader(kept, 'kept');
    writeReader(debug, 'debug');
    const inspectTable = runInNewContext(current + reader + '\ninspectTable', {
      app: { target: dir }, root: dir, stopping: false, process, resolve, readFileSync, rmSync,
      performance: { now: () => 1 }, ownTool: join(dir, 'host-dev'), tool: { path: kept, ready: true },
      buildTool() {}, inspections, launch, webCompiler: () => launch(debug),
      webCompilerFrom: () => ({ path: existsSync(kept) ? kept : debug }),
      spawn(cmd, args, options) { launched.push(cmd); return spawn(cmd, args, options); },
    });
    const inspect = async name => {
      const digest = sha(name);
      writeFileSync(plan, digest);
      const bytes = await inspectTable(digest, plan);
      return bytes && JSON.parse(bytes).marker;
    };
    expect(await inspect('first')).toBe('kept');
    rmSync(kept);
    expect(await inspect('second')).toBe('debug');
    expect(launched).toEqual([kept, kept, debug]);

    rmSync(debug);
    expect(await inspect('third')).toBeNull();
    expect(launched.slice(3)).toEqual([debug, debug]);
    expect(inspections.size).toBe(0);
    writeReader(debug, 'debug');
    expect(await inspect('third')).toBe('debug');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('a resident design reader restarts when its executable is replaced at the same path', async () => {
  const source = readFileSync(new URL('./dev.mjs', import.meta.url), 'utf8');
  const start = source.indexOf('  const patcher = ');
  const end = source.indexOf('  const toolExits = ', start);
  const askStart = source.indexOf('  const patcherAsk = ');
  const askEnd = source.indexOf('  /** A scrub', askStart);
  const dir = mkdtempSync(join(tmpdir(), 'exact-reader-replace-')), path = join(dir, 'reader');
  const spawned = [], stopped = [];
  try {
    writeFileSync(path, 'old');
    const ask = runInNewContext(source.slice(start, end) + source.slice(askStart, askEnd) + '\npatcherAsk', {
      stopping: false, toolFresh: () => true, statSync, tool: { path, built: null }, resolve,
      root: dir, app: { dir }, toolEnv: () => ({}), stopGroup: child => { if (child) stopped.push(child); },
      spawn(executable) {
        const child = new EventEmitter();
        child.stdin = new EventEmitter();
        child.stdout = new EventEmitter();
        const marker = readFileSync(executable, 'utf8');
        child.stdin.write = text => { child.answer = () => child.stdout.emit('data', JSON.stringify({ id: JSON.parse(text).id, marker }) + '\n'); };
        spawned.push(child);
        return child;
      },
    });
    const first = ask({});
    spawned[0].answer();
    expect((await first).marker).toBe('old');
    const pending = ask({});
    expect(spawned).toHaveLength(1);
    writeFileSync(join(dir, 'replacement'), 'new');
    renameSync(join(dir, 'replacement'), path);
    const next = ask({});
    expect(spawned).toHaveLength(2);
    expect(stopped).toEqual([spawned[0]]);
    spawned[0].emit('exit');
    expect(await pending).toEqual({ fallback: true });
    spawned[1].answer();
    expect((await next).marker).toBe('new');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('the JS loop: a value-only design edit while the design reader builds waits for it, then is a batch, no full build', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-reader-wait-'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Reader wait', app: { id: 'com.exact.readerwait', name: 'Reader wait' } }));
  writeFileSync(join(dir, 'app.contract'), 'component App\n  view\n    column background-color="surface.card" min-height=40\n      text "qz-wait"\n');
  const design = (light) => `forma readerwait\n\ntheme\n  surface.card = light-dark(${light}, #16181c)\n`;
  writeFileSync(join(dir, 'app.forma'), design('#f1f3f5'));
  let loop = null;
  try {
    // The reader's first build is taken 3 minutes after cargo ends (a reader building again after any other edit, on
    // a loaded machine): the app's first build is ready well before it.
    loop = await devLoop(dir, { EXACT_DEV_READER_HOLD_MS: '180000' });
    const { said } = loop;
    const count = (re) => (said().match(re) ?? []).length;
    const until = async (pass, ms) => { const end = Date.now() + ms; while (!pass()) { if (Date.now() > end) throw new Error(`not seen in: ${said().slice(-2000)}`); await new Promise(ok => setTimeout(ok, 100)); } };
    await until(() => said().includes('plan ready'), 900_000);
    if (said().includes('design reader ready')) throw new Error(`the reader was ready before the first build; the hold is too short: ${said().slice(-1500)}`);
    writeFileSync(join(dir, 'app.forma'), design('#e0f7e9'));
    await until(() => said().includes('edit → waiting for the design reader'), 60_000);
    // Nothing builds while it waits.
    expect(count(/edit → building/g)).toBe(0);
    await until(() => said().includes('design reader ready'), 400_000);
    await until(() => count(/edit → batch ready/g) >= 1, 120_000);
    // The edit's batch came after the reader, then the one build that checks it, and no other.
    expect(said().indexOf('design reader ready')).toBeLessThan(said().indexOf('edit → batch ready'));
    await until(() => count(/edit → checked/g) >= 1, 300_000);
    expect(count(/edit → building \(checking the batch\)/g)).toBe(1);
    expect(count(/edit → building/g)).toBe(1);
    expect(count(/edit → plan ready/g)).toBe(0);
    expect(JSON.stringify(await (await fetch(`${loop.base}/__dev/design`)).json())).toContain('e0f7e9');
  } finally {
    await loop?.stop().catch(() => {});
    rmSync(dir, { recursive: true, force: true });
  }
}, 1_200_000);

test("the JS loop: an edit to theme-fixture's `when` token builds with setMode and its pins, and a refused one says the design's refusal", async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-axes-'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Axes', app: { id: 'com.exact.axes', name: 'Axes' } }));
  writeFileSync(join(dir, 'app.contract'), 'component App\n  action dim\n    setMode("dark-style", "dim")\n  view\n    column background-color="surface.page" min-height=40\n      button press=dim\n        text "dim"\n      column -exact-mode="dark-style=black" background-color="surface.page"\n        text "pinned"\n');
  const design = readFileSync(resolve(ROOT, 'apps/theme-fixture/app.forma'), 'utf8');
  const page = '  surface.page = light-dark(#ffffff, #000000)\n    when dark-style=dim: light-dark(#ffffff, #15202b)\n';
  expect(design).toContain(page);
  writeFileSync(join(dir, 'app.forma'), design);
  let loop, browser;
  try {
    loop = await devLoop(dir);
    const { said, base } = loop;
    const until = async (pass, ms) => { const end = Date.now() + ms; while (!pass()) { if (Date.now() > end) throw new Error(`not seen in: ${said().slice(-2000)}`); await new Promise(ok => setTimeout(ok, 100)); } };
    const count = (re) => (said().match(re) ?? []).length;
    await until(() => said().includes('plan ready') && said().includes('design reader ready'), 900_000);
    // The `when` line's dark side: its batch, then its build, which checks
    // setMode and the pin against the design's axes.
    const checks = count(/edit → checked|edit → batch ready .*built in|edit → plan ready/g);
    writeFileSync(join(dir, 'app.forma'), design.replace(page, page.replace('#15202b', '#16212c')));
    await until(() => count(/edit → checked|edit → batch ready .*built in|edit → plan ready/g) > checks || said().includes('build failed'), 300_000);
    expect(said()).not.toContain('build failed');
    expect(JSON.stringify(await (await fetch(`${base}/__dev/design`)).json())).toContain('#16212c');
    browser = await suiteBrowser();
    const preview = await browser.newPage();
    await preview.goto(base);
    await preview.waitForFunction(() => globalThis.exact?.replacementBoundary);
    await preview.evaluate(() => exact.devError('assets/avatar.wgsl failed', 'shader'));
    // The default's light side alone, which `under scheme=dark` refuses:
    // the build says so, and not that the design declares no axes.
    writeFileSync(join(dir, 'app.forma'), design.replace(page, page.replace('#ffffff, #000000', '#fefefe, #000000')));
    await until(() => said().includes('build failed') && said().includes('[forma-'), 300_000);
    expect(said()).toContain('forma-under-light');
    expect(said()).not.toContain('declares no axes');
    const overlay = preview.locator('pre').filter({ hasText: 'forma-under-light' });
    expect(await overlay.textContent()).toContain('assets/avatar.wgsl failed');
    await overlay.click();
    expect(await overlay.count()).toBe(0);
  } finally {
    await browser?.close();
    await loop?.stop().catch(() => {});
    rmSync(dir, { recursive: true, force: true });
  }
}, 1_200_000);

test('the JS loop: two loops on two apps under one target/ build at once, each into its own stage', async () => {
  const app = (name) => {
    const dir = mkdtempSync(join(tmpdir(), `exact-stage-${name}-`));
    writeFileSync(join(dir, 'app.json'), JSON.stringify({ name, app: { id: `com.exact.${name}`, name } }));
    writeFileSync(join(dir, 'app.contract'), `component App\n  view\n    text "${name}"\n`);
    writeFileSync(join(dir, 'app.forma'), `forma ${name}\n\ntheme\n  ink = #000000\n`);
    return dir;
  };
  const dirs = [app('stagea'), app('stageb')];
  let loops = [];
  try {
    // The first's end held 8 s: the second's whole build lands inside it (one stage between them was emptied under
    // the first, which then built nothing, or the other's files).
    // One target/ for both, named (an app outside a workspace otherwise gets <its dir>/target).
    const target = process.env.CARGO_TARGET_DIR ?? resolve(ROOT, 'target');
    loops = await Promise.all([devLoop(dirs[0], { EXACT_DEV_BUILD_HOLD_MS: '8000', CARGO_TARGET_DIR: target }), devLoop(dirs[1], { CARGO_TARGET_DIR: target })]);
    for (const [i, name] of ['stagea', 'stageb'].entries()) {
      expect(loops[i].said()).not.toContain('does not build');
      const receipt = JSON.parse(readFileSync(join(loops[i].dist, '.exact-build.json'), 'utf8'));
      // Its sources as `serve` lists them, so `serve` on the loop's directory builds nothing again (expo/exact#415).
      expect([receipt.app.id, receipt.sources]).toEqual([`com.exact.${name}`, appSources(dirs[i], resolve(loops[i].dist)).digest]);
    }
  } finally {
    await Promise.all(loops.map(l => l.stop()));
    for (const dir of dirs) rmSync(dir, { recursive: true, force: true });
  }
}, 1_200_000);


test('the JS loop rebuilds when a navigation source or grant admission changes', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-navigation-watch-'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Navigation watch', app: { id: 'test.navigation-watch', name: 'Navigation watch' } }));
  writeFileSync(join(dir, 'app.contract'), 'component App\n  view\n    text "Ready"\n');
  const sources = ['host/web/navigation/routes.js', 'host/web/grant-admission.js'].map(file => resolve(ROOT, file));
  const stamps = sources.map(path => statSync(path));
  let loop;
  try {
    loop = await devLoop(dir);
    for (const source of sources) {
      const before = loop.said().length;
      const now = new Date();
      utimesSync(source, now, now);
      const deadline = performance.now() + 30_000;
      while (!loop.said().slice(before).includes('edit → plan ready')) {
        if (performance.now() > deadline) throw new Error(`no rebuild after ${source}: ${loop.said().slice(before)}`);
        await Bun.sleep(50);
      }
      expect(loop.said().slice(before)).toContain('edit → plan ready');
    }
  } finally {
    await loop?.stop();
    sources.forEach((path, i) => utimesSync(path, stamps[i].atime, stamps[i].mtime));
    rmSync(dir, { recursive: true, force: true });
  }
}, 180_000);

test('writes beside a linked Contract source leave its build running', async () => {
  const root = realpathSync(mkdtempSync(join(tmpdir(), 'exact-linked-reads-')));
  const app = join(root, 'app'), library = join(root, 'library');
  mkdirSync(join(app, 'node_modules'), { recursive: true });
  mkdirSync(library);
  symlinkSync(library, join(app, 'node_modules/ui'), 'junction');
  writeFileSync(join(library, 'package.json'), '{"name":"ui"}');
  writeFileSync(join(app, 'app.json'), '{"name":"Linked","app":{"id":"test.linked","name":"Linked"}}');
  writeFileSync(join(app, 'app.contract'), 'use Badge from "ui/views.contract"\ncomponent App\n  view\n    Badge()\n');
  const badge = text => `component Badge\n  view\n    text "${text}"\n`;
  const view = join(library, 'views.contract');
  writeFileSync(view, badge('first-badge'));
  let loop;
  try {
    loop = await devLoop(app, { EXACT_DEV_BUILD_HOLD_MS: '2000' });
    const target = process.env.CARGO_TARGET_DIR ?? join(app, 'target');
    const stage = join(target, 'web-js-dev-stage', sha(resolve(loop.dist)).slice(0, 16));
    const until = async condition => {
      const deadline = Date.now() + 60_000;
      while (!condition()) {
        if (Date.now() > deadline) throw new Error(loop.said().slice(-2000));
        await Bun.sleep(30);
      }
    };
    writeFileSync(view, badge('second-badge'));
    // The compiler has read the new view and the build has yet to publish it.
    await until(() => existsSync(join(stage, '.gen/dev-sources.json')));
    writeFileSync(join(library, 'notes.txt'), 'unread');
    mkdirSync(join(library, 'target'));
    writeFileSync(join(library, 'target/thumbnail.png'), 'unread');
    await until(() => loop.said().includes('edit → plan ready'));
    expect(readFileSync(join(loop.dist, 'app.plan')).includes('second-badge')).toBe(true);
    expect(loop.said()).not.toContain('superseded');
    expect(loop.said()).not.toContain('build in flight stopped');

    writeFileSync(view, 'use Label from "./new/label.contract"\ncomponent Badge\n  view\n    Label()\n');
    await until(() => loop.said().includes('build failed'));
    mkdirSync(join(library, 'new'));
    writeFileSync(join(library, 'new/label.contract'), 'component Label\n  view\n    text "recovered-badge"\n');
    await until(() => readFileSync(join(loop.dist, 'app.plan')).includes('recovered-badge'));
  } finally {
    await loop?.stop();
    rmSync(root, { recursive: true, force: true });
  }
}, 300_000);

// A Contract library the app depends on as `file:./packages/ui` is a copy in node_modules; the loop's rebuild runs
// host/web-js/build.mjs directly, which installs it again before it reads it (#115).
test("the JS loop: an edit to a copied file: library rebuilds with the library's new text", async () => {
  const dir = realpathSync(mkdtempSync(join(tmpdir(), 'exact-file-library-')));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Library', app: { id: 'test.file-library', name: 'Library' } }));
  writeFileSync(join(dir, 'app.contract'), 'use Badge from "@me/ui"\n\ncomponent App\n  view\n    Badge()\n');
  writeFileSync(join(dir, 'package.json'), JSON.stringify({ name: 'library', private: true, dependencies: { '@me/ui': 'file:./packages/ui' } }));
  mkdirSync(join(dir, 'packages/ui'), { recursive: true });
  writeFileSync(join(dir, 'packages/ui/package.json'), '{"name":"@me/ui","version":"1.0.0"}');
  const badge = (text) => `component Badge\n  view\n    text "${text}"\n`;
  writeFileSync(join(dir, 'packages/ui/index.contract'), badge('qz-lib-first'));
  const bun = spawnSync(process.execPath, ['install'], { cwd: dir, encoding: 'utf8' });
  expect(bun.status).toBe(0);
  let loop = null;
  try {
    loop = await devLoop(dir);
    const { said, dist } = loop;
    const until = async (pass, ms) => { const end = Date.now() + ms; while (!pass()) { if (Date.now() > end) throw new Error(`not seen in: ${said().slice(-2000)}`); await new Promise(ok => setTimeout(ok, 100)); } };
    const holds = (text) => { const walk = (d) => readdirSync(d, { withFileTypes: true }).some(e => e.isDirectory() ? walk(join(d, e.name)) : readFileSync(join(d, e.name)).includes(text)); return walk(dist); };
    await until(() => said().includes('plan ready') && said().includes('design reader ready') && holds('qz-lib-first'), 900_000);
    writeFileSync(join(dir, 'packages/ui/index.contract'), badge('qz-lib-second'));
    await until(() => holds('qz-lib-second'), 120_000);
    expect(holds('qz-lib-first')).toBe(false);
    expect(readFileSync(join(dir, 'node_modules/@me/ui/index.contract'), 'utf8')).toBe(badge('qz-lib-second'));
  } finally {
    await loop?.stop().catch(() => {});
    rmSync(dir, { recursive: true, force: true });
    // The lock the reinstall took, kept in TMPDIR by the app's path (filesystem.mjs `installLock`).
    rmSync(installLock(dir), { force: true });
  }
}, 1_200_000);


test('real loops keep their pages while test, verify and agent build the same app privately', async () => {
  const dir = mkdtempSync(resolve(ROOT, 'target/receipt-entrypoints-')), loops = [];
  try {
    const apps = ['one', 'two'].map(name => resolve(dir, name));
    for (const [n, app] of apps.entries()) {
      mkdirSync(app);
      writeFileSync(resolve(app, 'app.json'), JSON.stringify({ name: `Receipt ${n}`, app: { id: `test.receipt${n}`, name: `Receipt ${n}` } }));
      writeFileSync(resolve(app, 'app.contract'), `component App\n  view\n    text "Ready ${n}" testId="ready"\n`);
      writeFileSync(resolve(app, 'app.test.contract'), `test "ready"\n  expect text "ready" == "Ready ${n}"\n`);
      writeFileSync(resolve(app, 'exact.mjs'), `import {run} from ${JSON.stringify(resolve(ROOT, 'scripts/app-cli.mjs'))};\nprocess.exitCode = await run({dir:import.meta.dir, name:${JSON.stringify(app.split("/").at(-1))}, exact2:${JSON.stringify(ROOT)}, args:process.argv.slice(2)});\n`);
    }
    loops.push(...await Promise.all(apps.map(app => devLoop(app, { EXACT_WEB_DIST: '' }))));
    const receipts = loops.map(loop => readFileSync(resolve(loop.dist, '.exact-build.json'), 'utf8'));
    const checks = ['test', 'agent', 'verify'].map(verb => {
      const child = spawnOwned(process.execPath, [resolve(apps[0], 'exact.mjs'), verb, 'web', ...(verb === 'agent' ? ['tree'] : ['--json'])],
        { cwd: apps[0], env: { ...process.env, EXACT_WEB_DIST: '', EXACT_BAKE_OUTPUT: '' }, stdio: ['ignore', 'pipe', 'pipe'] });
      let output = ''; for (const stream of [child.stdout, child.stderr]) stream.on('data', bytes => { output += bytes; });
      return new Promise((ok, fail) => { child.once('error', fail); child.once('exit', code => ok({ verb, code, output })); });
    });
    writeFileSync(resolve(apps[0], 'unused.contract'), 'component Unused\n  text "not imported"\n');
    for (const result of await Promise.all(checks)) expect(result.code, result.verb + ': ' + result.output).toBe(0);
    for (const [n, loop] of loops.entries()) {
      const page = await (await fetch(loop.base)).text();
      expect(page).toContain('/__dev/page');
      expect(JSON.parse(readFileSync(resolve(loop.dist, '.exact-build.json'))).app).toEqual(JSON.parse(receipts[n]).app);
      expect(readFileSync(resolve(loop.dist, 'index.html'), 'utf8')).toContain(`Receipt ${n}`);
    }
    const runs = readFileSync(resolve(apps[0], '.exact/runs.jsonl'), 'utf8').trim().split('\n').map(line => JSON.parse(line));
    expect(new Set(runs.map(run => run.receipt)).size).toBeGreaterThanOrEqual(3);
    expect(runs.every(run => run.receipt && readFileSync(run.receipt, 'utf8').includes('"reads"'))).toBe(true);
  } finally {
    await Promise.all(loops.map(loop => loop.stop())); rmSync(dir, { recursive: true, force: true });
  }
}, 180_000);

test.each([posix.sep, win32.sep])('the JS watcher skips build outputs and keeps inputs with %s separators', sep => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-output-watch-'));
  const manifest = { gpu: { shaderRoots: ['shader-gen'] }, host: { macos: { resources: [{ from: 'declared.out' }, { from: 'tools/declared.err' }] } } };
  const app = { dir, manifest };
  const source = readFileSync(new URL('./dev.mjs', import.meta.url), 'utf8');
  const rules = source.slice(source.indexOf('const skipped = '), source.indexOf('/** Whether an entry'));
  const begin = source.indexOf('  const outputIsInput = (name)');
  const end = source.indexOf('  // The resident loop', begin);
  expect(begin).toBeGreaterThan(0);
  expect(end).toBeGreaterThan(begin);
  try {
    for (const name of ['local-crate/src', 'assets', 'shader-gen', 'tools']) mkdirSync(join(dir, name), { recursive: true });
    writeFileSync(join(dir, 'app.json'), JSON.stringify(manifest));
    writeFileSync(join(dir, 'local-crate/Cargo.toml'), '[package]\nname = "included"\nversion = "0.1.0"\n');
    writeFileSync(join(dir, 'local-crate/src/lib.rs'), 'pub const MESSAGE: &str = include_str!("messages.err");\n');
    let pending = null, notify, rebuilds = 0;
    const context = {
      app, OUTPUT, INPUT_TREE, shaderWatchRoots, within, existsSync, dirname, sep, besidePattern,
      fsWatch: (_path, _options, listener) => { notify = listener; },
      stopping: false, skipCargoTarget: () => false, compilerInput: path => ['node_modules/used/source.contract', 'frames/shared.contract'].some(name => path === join(dir, name)),
      backendOf: () => 'current', backendSeen: 'previous',
      temporaryOfSource: () => false, movedSince: () => true, since: Date.now() + 1000,
      resolve, statSync, timer: null, saved: 0, edited: new Set(), residentChild: null,
      starting: { failure: null }, inputs: 0, movedSinceBuild: new Set(), appeared: new Set(),
      stopYoungBuild() {}, clearTimeout() {}, setTimeout: callback => { pending = callback; return 1; },
      rebuild() { rebuilds++; },
    };
    runInNewContext(rules + source.slice(begin, end) + '\nwatch(app.dir, { recursive: true }, changed(app.dir));', context);
    for (const name of ['target/web-js-dev-stage/hash/manifest.json', 'app.contract.d.ts', 'macos.out', 'macos.err',
      'other.OUT', '.exact/evidence.err', 'node_modules/unused/source.contract', 'nested/.cache/file.js', 'frames/preview.contract']) {
      notify('change', name.replaceAll('/', sep));
      expect(context.edited.has(join(dir, name)), name).toBe(false);
      expect(pending, name).toBeNull();
    }
    expect(rebuilds).toBe(0);
    const inputs = ['app.contract', 'app.forma', 'app.ts', 'unused.ts', 'helper.js', 'local-crate/src/messages.err',
      'local-crate/src/captured.out', 'assets/logo.png', 'shader-gen/captured.out', 'declared.out', 'tools/declared.err',
      '.env.local', '.exact/backend-origin', 'node_modules/used/source.contract', 'frames/shared.contract'];
    if (sep === '/') inputs.push('nested\\target\\literal.ts');
    for (const [index, name] of inputs.entries()) {
      notify('change', name.replaceAll('/', sep));
      expect(context.edited.has(join(dir, name)), name).toBe(true);
      expect(pending, name).toBeFunction();
      pending();
      pending = null;
      expect(rebuilds, name).toBe(index + 1);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
