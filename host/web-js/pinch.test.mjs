import { expect, test } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { serveBuildTree } from '../web/serve.mjs';

test('a touch pair reports its scale and viewport focus where CSS leaves pinch to the page', async () => {
  const root = resolve(import.meta.dir, '../..');
  const dir = mkdtempSync(resolve(root, 'target/pinch-test-'));
  const dist = resolve(dir, 'dist');
  const server = createServer((req, res) => serveBuildTree(dist, req, res));
  let browser;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Pinch', app: { id: 'test.pinch', name: 'Pinch' }, host: {web: {}} }));
    writeFileSync(resolve(dir, 'app.contract'), `component App
  state scale = 0
  state x = 0
  state y = 0
  state releases = 0
  state downs = 0
  state nested = 0
  action inner(s: number, cx: number, cy: number)
    nested = nested + 1
  action sample(s, cx, cy)
    scale = s
    x = cx
    y = cy
  action release(s, cx, cy)
    sample(s, cx, cy)
    releases = releases + 1
  action down
    downs = downs + 1
  view
    column
      box testId="surface" width=320 height=240 touch-action="pan-y" pinch=sample pinchrelease=release pointerdown=down
        box testId="child" width=130 height=240 pinchrelease=inner
      box testId="other" touch-action="none" pinchrelease=inner
      text nested.toString() testId="nested"
      text \`\${scale},\${x},\${y},\${releases},\${downs}\` testId="result"
`);
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'pinch-test', '--out', dist], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
    });
    const [status, out, err] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect(status, out + err).toBe(0);
    await new Promise(ok => server.listen(0, '127.0.0.1', ok));
    browser = await suiteBrowser();
    const page = await browser.newPage({ hasTouch: true, viewport: { width: 400, height: 500 } });
    await page.goto(`http://127.0.0.1:${server.address().port}/`);
    const surface = page.getByTestId('surface'), result = page.getByTestId('result');
    const box = await surface.boundingBox();
    const x = Math.round(box.x + 130), y = Math.round(box.y + 100);
    const cdp = await page.context().newCDPSession(page);
    const touch = (type, span) => cdp.send('Input.dispatchTouchEvent', {type, touchPoints: span ? [
      {id: 1, x: x - span / 2, y}, {id: 2, x: x + span / 2, y},
    ] : []});
    const frame = () => page.evaluate(() => new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok))));
    for (const [i, end] of ['touchEnd', 'touchCancel'].entries()) {
      await touch('touchStart', 80);
      for (const span of [120, 64]) {
        await touch('touchMove', span);
        await frame();
        expect(await result.textContent()).toBe(`${span / 80},${x},${y},${i},${i + 1}`);
      }
      await touch(end);
      await frame();
      expect(await result.textContent()).toBe(`${i === 0 ? 0.8 : 1},${x},${y},${i + 1},${i + 1}`);
    }
    for (const action of ['auto', 'pinch-zoom']) {
      await surface.evaluate((e, value) => { e.style.touchAction = value; }, action);
      await touch('touchStart', 80);
      await touch('touchMove', 144);
      await touch('touchEnd');
      await frame();
      expect((await result.textContent()).split(',').slice(0, 4)).toEqual(['1', String(x), String(y), '2']);
    }
    expect(await page.getByTestId('nested').textContent()).toBe('0');
    await cdp.send('Input.dispatchMouseEvent', { type: 'mouseWheel', x, y, deltaX: 0, deltaY: -20, modifiers: 2 });
    await frame();
    expect((await result.textContent()).split(',')[3]).toBe('2');
    await surface.evaluate(e => { e.style.touchAction = 'pan-y'; });
    await surface.evaluate(e => {
      const send = (type, id, x) => {
        const event = new PointerEvent(type, {
          bubbles: true, pointerType: 'touch', pointerId: id, isPrimary: id === 1, clientX: x, clientY: 80,
        });
        e.dispatchEvent(event);
      };
      send('pointerdown', 1, 40); send('pointerdown', 2, 120);
      send('pointermove', 1, 52); send('pointermove', 2, 132);
      send('pointerup', 2, 132); send('pointerup', 1, 52);
    });
    expect((await result.textContent()).split(',')[3]).toBe('2');
    await page.getByTestId('child').evaluate(e => {
      const send = (type, id, x) => e.dispatchEvent(new PointerEvent(type, {
        bubbles: true, pointerType: 'touch', pointerId: id, isPrimary: id === 1, clientX: x, clientY: 90,
      }));
      send('pointerdown', 1, 30); send('pointerdown', 2, 50);
      send('pointermove', 2, 75); send('pointercancel', 2, 75); send('pointerup', 1, 30);
    });
    expect(await page.getByTestId('nested').textContent()).toBe('1');
    await surface.evaluate(e => {
      const send = (type, id, x) => e.dispatchEvent(new PointerEvent(type, {
        bubbles: true, pointerType: 'touch', pointerId: id, isPrimary: id === 1, clientX: x, clientY: 95,
      }));
      send('pointerdown', 1, 120); send('pointerdown', 2, 140);
      send('pointermove', 2, 160); send('pointercancel', 2, 160); send('pointerup', 1, 120);
    });
    expect((await result.textContent()).split(',').slice(0, 4)).toEqual(['1', '140', '95', '3']);
    for (const coalesced of [false, true]) {
      await surface.evaluate((e, coalesced) => {
        const event = (type, id, x, history = []) => new PointerEvent(type, {
          bubbles: true, pointerType: 'touch', pointerId: id, isPrimary: id === 1,
          clientX: x, clientY: 95, coalescedEvents: history,
        });
        e.dispatchEvent(event('pointerdown', 1, 40));
        e.dispatchEvent(event('pointerdown', 2, 120));
        const out = event('pointermove', 2, 168), back = event('pointermove', 2, 120);
        if (coalesced) e.dispatchEvent(event('pointermove', 2, 120, [out, back]));
        else { e.dispatchEvent(out); e.dispatchEvent(back); }
        e.dispatchEvent(event('pointerup', 2, 120));
        e.dispatchEvent(event('pointerup', 1, 40));
      }, coalesced);
      expect((await result.textContent()).split(',').slice(0, 4)).toEqual(['1', '80', '95', coalesced ? '5' : '4']);
    }
    await surface.evaluate(async e => {
      const other = document.querySelector('[data-testid="other"]');
      const send = (node, type, id, x) => {
        const event = new PointerEvent(type, {
          bubbles: true, pointerType: 'touch', pointerId: id, isPrimary: id === 1, clientX: x, clientY: 95,
        });
        node.dispatchEvent(event);
      };
      send(other, 'pointerdown', 1, 20);
      send(e, 'pointerdown', 2, 40);
      send(e, 'pointerdown', 3, 120);
      send(other, 'pointerup', 1, 20);
      send(e, 'pointermove', 3, 168);
      await Promise.resolve();
      send(e, 'pointermove', 2, 88);
      await Promise.resolve();
      send(e, 'pointerup', 3, 168);
      send(e, 'pointerup', 2, 88);
    });
    expect((await result.textContent()).split(',').slice(0, 4)).toEqual(['1', '128', '95', '6']);
    await surface.evaluate(e => {
      const send = (type, id, x) => {
        const event = new PointerEvent(type, {
          bubbles: true, pointerType: 'touch', pointerId: id, isPrimary: id === 1, clientX: x, clientY: 95,
        });
        Object.defineProperty(event, 'timeStamp', { value: 42 });
        e.dispatchEvent(event);
      };
      send('pointerdown', 1, 40); send('pointerdown', 2, 120);
      send('pointermove', 1, 16); send('pointermove', 2, 144);
      send('pointermove', 1, 40); send('pointermove', 2, 120);
      send('pointerup', 2, 120); send('pointerup', 1, 40);
    });
    expect((await result.textContent()).split(',')[3]).toBe('7');
  } finally {
    if (browser) await browser.close();
    if (server.listening) await new Promise(ok => server.close(ok));
    rmSync(dir, { recursive: true, force: true });
  }
}, 240000);
