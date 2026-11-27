import { test, expect } from 'bun:test';
import { copyFileSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { chromium } from 'playwright-core';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';

// `requestPictureInPicture(id)` on the JS target (LLP 1042 §9): the image's
// picture floats as a canvas stream's video; a picture it loads next is drawn
// into the same stream; the window's close is the image's
// `leavepictureinpicture`, and a removed image closes it with no event.
test("an image floats in picture in picture and hears it enter and leave", async () => {
  const server = Bun.serve({ port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/picture-in-picture.js') return new Response(Bun.file(new URL('./picture-in-picture.js', import.meta.url)));
    return new Response(`<button id="float">Float</button><img id="page"><output id="log"></output>
      <script type="module">
        import { float, exit } from '/picture-in-picture.js';
        window.exitPip = () => exit(line => { log.textContent += line + ';'; });
        const picture = color => { const c = Object.assign(document.createElement('canvas'), { width: 320, height: 200 }); const g = c.getContext('2d'); g.fillStyle = color; g.fillRect(0, 0, 320, 200); return c.toDataURL(); };
        const img = document.querySelector('#page'), log = document.querySelector('#log');
        img.src = picture('red');
        for (const kind of ['enterpictureinpicture', 'leavepictureinpicture']) img.addEventListener(kind, () => { log.textContent += kind + ';'; });
        window.next = () => { img.src = picture('blue'); };
        document.querySelector('#float').onclick = () => float(img, 'page', line => { log.textContent += line + ';'; });
      </script>`, { headers: { 'content-type': 'text/html' } });
  } });
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.waitForFunction(() => document.querySelector('#page').naturalWidth === 320);
    await page.locator('#float').click();
    await page.waitForFunction(() => document.querySelector('#log').textContent !== '');
    const log = await page.locator('#log').textContent();
    if (log.includes('refused')) {
      // A browser without picture in picture says so, and nothing is left floating.
      expect(log).toMatch(/^requestPictureInPicture "page" refused: /);
      expect(await page.evaluate(() => document.querySelectorAll('video').length)).toBe(0);
      return;
    }
    expect(log).toBe('enterpictureinpicture;');
    const floating = await page.evaluate(() => {
      const video = document.pictureInPictureElement;
      return { video: video?.localName, track: video?.srcObject?.getVideoTracks()[0]?.readyState, size: [video?.videoWidth, video?.videoHeight] };
    });
    expect(floating).toEqual({ video: 'video', track: 'live', size: [320, 200] });
    // The window's own close (its button) is a leave.
    await page.evaluate(() => document.exitPictureInPicture());
    await page.waitForFunction(() => document.querySelector('#log').textContent.endsWith('leavepictureinpicture;'));
    expect(await page.evaluate(() => document.querySelectorAll('video').length)).toBe(0);
    // exitPictureInPicture() closes it as the window's close does; with none open it says so.
    await page.locator('#float').click();
    await page.waitForFunction(() => document.pictureInPictureElement !== null);
    await page.evaluate(() => window.exitPip());
    await page.waitForFunction(() => document.pictureInPictureElement === null);
    await page.evaluate(() => window.exitPip());
    expect(await page.locator('#log').textContent()).toBe('enterpictureinpicture;leavepictureinpicture;enterpictureinpicture;leavepictureinpicture;exitPictureInPicture ignored: nothing is in the picture-in-picture window;');
    await page.evaluate(() => { document.querySelector('#log').textContent = ''; });
    // Floated again, then the image leaves the document: the window closes, and no one hears it.
    await page.locator('#float').click();
    await page.waitForFunction(() => document.pictureInPictureElement !== null);
    await page.evaluate(() => { window.next(); document.querySelector('#page').remove(); });
    await page.waitForFunction(() => document.pictureInPictureElement === null, null, { timeout: 5000 });
    expect(await page.locator('#log').textContent()).toBe('enterpictureinpicture;');
  } finally {
    await browser?.close();
    server.stop(true);
  }
}, 30000);

// A picture from another origin without CORS would taint the canvas, whose
// stream then sends no frames: it is refused, and nothing is left floating.
test("a picture from another origin without CORS is refused", async () => {
  const other = Bun.serve({ port: 0, fetch: () => new Response(Bun.file(new URL('../../apps/canvas-gallery/assets/tile.png', import.meta.url))) });
  const server = Bun.serve({ port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/picture-in-picture.js') return new Response(Bun.file(new URL('./picture-in-picture.js', import.meta.url)));
    return new Response(`<button id="float">Float</button><img id="page" src="${other.url.href}icon.png"><output id="log"></output>
      <script type="module">
        import { float } from '/picture-in-picture.js';
        const img = document.querySelector('#page'), log = document.querySelector('#log');
        img.addEventListener('enterpictureinpicture', () => { log.textContent += 'enter;'; });
        img.addEventListener('pictureinpictureerror', e => { log.textContent += 'error event: ' + e.detail + ';'; });
        document.querySelector('#float').onclick = () => float(img, 'page', line => { log.textContent += line + ';'; });
      </script>`, { headers: { 'content-type': 'text/html' } });
  } });
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.waitForFunction(() => document.querySelector('#page').naturalWidth > 0);
    await page.locator('#float').click();
    await page.waitForFunction(() => document.querySelector('#log').textContent !== '');
    const reason = 'a picture from another origin needs CORS (Access-Control-Allow-Origin)';
    expect(await page.locator('#log').textContent()).toBe(`requestPictureInPicture "page" refused: ${reason};error event: ${reason};`);
    expect(await page.evaluate(() => [document.pictureInPictureElement, document.querySelectorAll('video').length])).toEqual([null, 0]);
  } finally {
    await browser?.close();
    server.stop(true); other.stop(true);
  }
}, 30000);

test("an element that is not an image is refused", async () => {
  const { float } = await import('./picture-in-picture.js');
  const said = [];
  float(null, 'missing', line => said.push(line));
  float({ localName: 'div' }, 'box', line => said.push(line));
  expect(said).toEqual(['requestPictureInPicture "missing" refused: no live node with that id', 'requestPictureInPicture "box" refused: not an image']);
});

// `presentation: "window"` (LLP 1042 §9.1): Document Picture-in-Picture's window
// holds an <img> showing each picture the image loads, titled as asked; its close
// is the image's `leavepictureinpicture`. A browser without it floats the default way.
test("the window presentation floats the image in a document picture-in-picture window", async () => {
  const server = Bun.serve({ port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path === '/picture-in-picture.js') return new Response(Bun.file(new URL('./picture-in-picture.js', import.meta.url)));
    return new Response(`<button id="float">Float</button><img id="page"><output id="log"></output>
      <script type="module">
        import { float, exit } from '/picture-in-picture.js';
        const picture = color => { const c = Object.assign(document.createElement('canvas'), { width: 300, height: 400 }); const g = c.getContext('2d'); g.fillStyle = color; g.fillRect(0, 0, 300, 400); return c.toDataURL(); };
        const img = document.querySelector('#page'), log = document.querySelector('#log');
        img.src = picture('red');
        for (const kind of ['enterpictureinpicture', 'leavepictureinpicture']) img.addEventListener(kind, () => { log.textContent += kind + ';'; });
        window.next = () => { img.src = picture('blue'); };
        window.exitPip = () => exit(line => { log.textContent += line + ';'; });
        document.querySelector('#float').onclick = () => float(img, 'page', line => { log.textContent += line + ';'; }, { presentation: 'window', title: 'Preview', width: 480, height: 320 });
      </script>`, { headers: { 'content-type': 'text/html' } });
  } });
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.waitForFunction(() => document.querySelector('#page').naturalWidth === 300);
    const documentWindow = await page.evaluate(() => typeof globalThis.documentPictureInPicture?.requestWindow === 'function');
    await page.locator('#float').click();
    await page.waitForFunction(() => document.querySelector('#log').textContent !== '');
    const log = await page.locator('#log').textContent();
    if (!documentWindow || log.includes('refused')) {
      // The default way: a canvas stream's video, or the browser's refusal.
      expect(await page.evaluate(() => document.querySelector('#log').textContent)).toMatch(/^(enterpictureinpicture;|requestPictureInPicture "page" refused: )/);
      return;
    }
    expect(log).toBe('enterpictureinpicture;');
    const shown = () => page.evaluate(() => { const w = documentPictureInPicture.window; return { title: w?.document.title, src: w?.document.querySelector('img')?.src === document.querySelector('#page').src }; });
    expect(await shown()).toEqual({ title: 'Preview', src: true });
    await page.evaluate(() => window.next());
    await page.waitForFunction(() => documentPictureInPicture.window?.document.querySelector('img')?.src === document.querySelector('#page').src);
    // The window's own close is a leave; exitPictureInPicture() with none open says so.
    await page.evaluate(() => documentPictureInPicture.window.close());
    await page.waitForFunction(() => document.querySelector('#log').textContent.endsWith('leavepictureinpicture;'));
    await page.evaluate(() => window.exitPip());
    expect(await page.locator('#log').textContent()).toBe('enterpictureinpicture;leavepictureinpicture;exitPictureInPicture ignored: nothing is in the picture-in-picture window;');
  } finally {
    await browser?.close();
    server.stop(true);
  }
}, 30000);

test("the window opens at the asked area in the picture's aspect", async () => {
  const { windowSize } = await import('./picture-in-picture.js');
  expect(windowSize({}, [0, 0])).toEqual([480, 320]);
  expect(windowSize({ width: 480, height: 320 }, [300, 400])).toEqual([339, 453]);
  expect(windowSize({ width: 400, height: 400 }, [1600, 900])).toEqual([533, 300]);
});

// Chromium exposes its own media menu through accessibility. Use real input so
// the browser grants activation to the control that starts the presentation.
async function pressMediaControl(page, cdp, name) {
  const end = Date.now() + 5000;
  do {
    const { nodes } = await cdp.send('Accessibility.getFullAXTree');
    for (const node of nodes.filter(n => n.name?.value?.toLowerCase() === name && n.backendDOMNodeId)) {
      const box = await cdp.send('DOM.getContentQuads', { backendNodeId: node.backendDOMNodeId }).catch(() => null);
      const q = box?.quads[0];
      if (q && q[2] > q[0] && q[5] > q[1]) {
        return page.mouse.click((q[0] + q[4]) / 2, (q[1] + q[5]) / 2);
      }
    }
    await page.evaluate(() => new Promise(requestAnimationFrame));
  } while (Date.now() < end);
  throw new Error(`Chromium did not expose ${name}`);
}

test("a URL video reports each presentation to its Contract handlers", async () => {
  const root = resolve(import.meta.dir, '../..');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const app = mkdtempSync(resolve(root, 'target/video-pip-'));
  let browser, server;
  try {
    mkdirSync(resolve(app, 'assets'));
    copyFileSync(resolve(root, 'apps/video-player/assets/motion.mp4'), resolve(app, 'assets/motion.mp4'));
    copyFileSync(resolve(root, 'scripts/fixtures/video-picture-in-picture.contract'), resolve(app, 'app.contract'));
    writeFileSync(resolve(app, 'app.json'), JSON.stringify({ name: 'PiP', app: { id: 'test.exact.pip', name: 'PiP' }, host: { web: {} } }));
    const dist = resolve(app, 'dist');
    const build = spawnSync(process.execPath, ['host/web-js/build.mjs', 'pip', '--render', 'none', '--out', dist],
      { cwd: root, env: { ...process.env, EXACT_APP_DIR: app }, encoding: 'utf8' });
    expect(build.status, build.stdout + build.stderr).toBe(0);
    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const path = new URL(request.url).pathname;
      return new Response(Bun.file(resolve(dist, path === '/' ? 'index.html' : path.slice(1))));
    } });
    browser = await suiteBrowser('chromium', { executablePath: process.env.CHROME || chromium.executablePath(), headless: process.platform !== 'darwin' && !process.env.DISPLAY });
    const page = await browser.newPage();
    await page.goto(new URL('?agent=1', server.url).href);
    await page.waitForFunction(() => window.exact?.ready && document.querySelector('video')?.videoWidth > 0);
    const press = id => page.locator(`[data-testid="${id}"]`).click();
    const counts = (ins, outs) => page.waitForFunction(([i, o]) => document.querySelector('[data-testid=events]').textContent === `Entries ${i}, exits ${o}`, [ins, outs]);
    expect(await page.locator('[data-testid=available]').textContent()).toBe('Available true');
    await press('stop');
    // Without metadata the browser rejects the request, even from a real press.
    const src = await page.locator('video').evaluate(e => { const src = e.src; e.removeAttribute('src'); e.load(); return src; });
    await press('start');
    await page.waitForFunction(() => document.querySelector('[data-testid=status]').textContent === 'InvalidStateError');
    await counts(0, 0);
    await page.locator('video').evaluate((e, src) => { e.src = src; }, src);
    await page.waitForFunction(() => document.querySelector('video').videoWidth > 0);
    await press('start');
    await counts(1, 0);
    expect(await page.evaluate(() => document.pictureInPictureElement?.id)).toBe('clip');
    expect(await page.locator('[data-testid=status]').textContent()).toBe('Started');
    const tree = await page.evaluate(() => window.exact.agentSettled({ op: 'tree', target: 'clip' }));
    expect(tree.nodes[0].pictureInPicture.presentation).toBe("system");
    await press('start');
    await press('stop');
    await counts(1, 1);
    const cdp = await page.context().newCDPSession(page);
    await page.locator('video').hover();
    await pressMediaControl(page, cdp, 'show more media controls');
    await pressMediaControl(page, cdp, 'enter picture-in-picture');
    await counts(2, 1);
    await page.evaluate(() => document.exitPictureInPicture());
    await counts(2, 2);
    await press('start');
    await counts(3, 2);
    await press('remove');
    await counts(3, 3);
    expect(await page.evaluate(() => document.pictureInPictureElement)).toBeNull();
    expect(await page.locator('[data-testid=status]').textContent()).toBe('Stopped');

    await page.addInitScript(() => Object.defineProperty(document, 'pictureInPictureEnabled', { value: false }));
    await page.reload();
    await page.waitForFunction(() => window.exact?.ready);
    expect(await page.locator('[data-testid=available]').textContent()).toBe('Available false');
    await press('start');
    expect(await page.locator('[data-testid=status]').textContent()).toContain('unavailable');
    await counts(0, 0);
  } finally {
    await browser?.close();
    server?.stop(true);
    rmSync(app, { recursive: true, force: true });
  }
}, 90000);
