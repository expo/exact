import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { engineTests } from '../../../scripts/coverage.mjs';
import { roster } from '../web/index.js';

for (const [longitude, zoom, dx, dy, expected] of [
  [-122.404, 15, 0, 0, -122.404],
  [-122.404, 15, 80, 0, -122.40056677246094],
  [122.404, 15, -80, 0, 122.40056677246094],
  [-122.404, 15.8, 0, 80, -122.404],
  [179, 1, 8, 0, -175.375],
  [-179, 1, -8, 0, 175.375],
  [0, 1, 3 * 512, 0, 0],
  [0, 1, -3 * 512, 0, 0],
  [0, 1, 256, 0, -180],
]) {
  test(`pan from longitude ${longitude} at zoom ${zoom} by (${dx}, ${dy}) pixels reaches ${expected}`, () => {
    const map = Object.assign(Object.create(roster['exact-map'].view.prototype), {
      centre: [37.7946, longitude], zoom, draw() {},
    });
    map.panBy(dx, dy);
    expect(map.centre[1]).toBeCloseTo(expected, 9);
    if (dy === 0) expect(map.centre[0]).toBeCloseTo(37.7946, 9);
    expect(map.zoom).toBe(zoom);
  });
}

async function mapPage(browser, options = {}) {
  const page = await browser.newPage({ viewport: { width: 600, height: 600 }, ...options });
  await page.setContent('<div id="map" style="position:absolute;left:100px;top:100px;width:400px;height:400px"></div>');
  await page.addStyleTag({ content: readFileSync(new URL('../../../host/web/index.html', import.meta.url), 'utf8').match(/<style>([\s\S]*?)<\/style>/)[1] });
  await page.evaluate(async source => {
    const { roster, implementation } = await import(`data:text/javascript,${encodeURIComponent(source)}`);
    window.moves = [];
    window.changes = [];
    window.map = new roster['exact-map'].view({
      element: document.getElementById('map'),
      implementation: implementation({ agent: true }),
      view: 1,
      props: {
        latitude: '0', longitude: '0', zoomLevel: '15',
        markers: JSON.stringify([{ id: 'pin', latitude: 0, longitude: 0 }]),
      },
      events: {
        load() {},
        change: id => changes.push(id),
        emit: (name, camera) => moves.push({ name, camera }),
      },
    });
  }, readFileSync(new URL('../web/index.js', import.meta.url), 'utf8'));
  await page.waitForFunction(() => map.size.w === 400);
  await page.clock.setFixedTime(1000);
  await page.clock.pauseAt(1000);
  return page;
}

test('a marker press released outside the map does not keep the next mouse drag active', async () => {
  const browser = await suiteBrowser();
  try {
    const page = await mapPage(browser);

    await page.mouse.move(300, 280);
    await page.mouse.down();
    await page.mouse.move(300, 50);
    await page.mouse.up();
    expect(await page.evaluate(() => changes)).toEqual([]);

    const before = await page.evaluate(() => map.camera());
    await page.mouse.move(350, 400);
    await page.mouse.down();
    await page.mouse.move(310, 400, { steps: 3 });
    expect(await page.evaluate(() => map.map.classList.contains('dragging'))).toBe(true);
    await page.mouse.up();
    const camera = await page.evaluate(() => map.camera());
    expect(camera).not.toEqual(before);
    await page.clock.runFor(249);
    expect(await page.evaluate(() => moves)).toEqual([]);
    await page.clock.runFor(1);
    expect(await page.evaluate(() => ({
      dragging: map.map.classList.contains('dragging'), moves, changes,
    }))).toEqual({ dragging: false, moves: [{ name: 'moveend', camera }], changes: [] });

    await page.mouse.move(200, 350);
    expect(await page.evaluate(() => map.camera())).toEqual(camera);
    await page.clock.runFor(250);
    expect(await page.evaluate(() => moves)).toEqual([{ name: 'moveend', camera }]);

    const marker = await page.locator('.marker').boundingBox();
    await page.mouse.click(marker.x + marker.width / 2, marker.y + marker.height / 2);
    expect(await page.evaluate(() => changes)).toEqual(['pin']);
  } finally {
    await browser.close();
  }
});

for (const [target, releaseFirst] of [['marker', 'drag'], ['marker', 'second'], ['map', 'second']]) {
  test(`a touch drag ends after a second touch on the ${target}, releasing ${releaseFirst} first`, async () => {
    const browser = await suiteBrowser();
    try {
      const page = await mapPage(browser, { hasTouch: true });
      const cdp = await page.context().newCDPSession(page);
      const touch = (type, touchPoints) => cdp.send('Input.dispatchTouchEvent', { type, touchPoints });
      let drag = { id: 1, x: 350, y: 400 };
      const before = await page.evaluate(() => map.camera());
      await touch('touchStart', [drag]);
      drag = { ...drag, x: 310 };
      await touch('touchMove', [drag]);
      const camera = await page.evaluate(() => map.camera());
      expect(camera).not.toEqual(before);
      const marker = await page.locator('.marker').boundingBox();
      const second = target === 'marker'
        ? { id: 2, x: marker.x + marker.width / 2, y: marker.y + marker.height / 2 }
        : { id: 2, x: 400, y: 400 };
      await touch('touchStart', [drag, second]);
      await touch('touchEnd', releaseFirst === 'drag' ? [drag] : [second]);
      expect(await page.evaluate(() => map.map.classList.contains('dragging'))).toBe(releaseFirst === 'second');
      await touch('touchEnd', []);
      expect(await page.evaluate(() => map.map.classList.contains('dragging'))).toBe(false);
      await page.clock.runFor(249);
      expect(await page.evaluate(() => moves)).toEqual([]);
      await page.clock.runFor(1);
      expect(await page.evaluate(() => moves)).toEqual([{ name: 'moveend', camera }]);
      await page.mouse.move(200, 350);
      expect(await page.evaluate(() => map.camera())).toEqual(camera);
    } finally {
      await browser.close();
    }
  });
}

for (const end of ['touchEnd', 'touchCancel']) {
  test(`a marker touch selects only on release (${end})`, async () => {
    const browser = await suiteBrowser();
    try {
      const page = await mapPage(browser, { hasTouch: true });
      const cdp = await page.context().newCDPSession(page);
      await cdp.send('Input.dispatchTouchEvent', {
        type: 'touchStart', touchPoints: [{ id: 1, x: 300, y: 280 }],
      });
      expect(await page.evaluate(() => changes)).toEqual([]);
      await cdp.send('Input.dispatchTouchEvent', { type: end, touchPoints: [] });
      expect(await page.evaluate(() => changes)).toEqual(end === 'touchEnd' ? ['pin'] : []);
    } finally {
      await browser.close();
    }
  });
}

for (const key of ['Enter', 'Space']) {
  test(`a focused marker selects with ${key}`, async () => {
    const browser = await suiteBrowser();
    try {
      const page = await mapPage(browser);
      await page.locator('.marker').focus();
      await page.keyboard.press(key);
      expect(await page.evaluate(() => changes)).toEqual(['pin']);
      expect(await page.locator('.marker').getAttribute('aria-pressed')).toBe('true');
    } finally {
      await browser.close();
    }
  });
}

const engineTest = await engineTests();
for (const engine of ['chromium', 'firefox', 'webkit']) {
  engineTest[engine]('map zoom glyphs stay centred under the host reset at 390 px', async () => {
    const browser = await suiteBrowser(engine);
    try {
      const page = await mapPage(browser, { viewport: { width: 390, height: 600 } });
      const buttons = page.locator('.zoom button');
      const offsets = await buttons.evaluateAll(buttons => buttons.map(button => {
        const range = document.createRange();
        range.selectNodeContents(button);
        const text = range.getBoundingClientRect(), box = button.getBoundingClientRect();
        return { x: text.x + text.width / 2 - box.x - box.width / 2,
          y: text.y + text.height / 2 - box.y - button.clientTop - button.clientHeight / 2 };
      }));
      for (const offset of offsets) {
        expect(Math.abs(offset.x)).toBeLessThan(1);
        expect(Math.abs(offset.y)).toBeLessThan(1);
      }
      for (const [index, key, zoom] of [[0, 'Enter', 16], [1, 'Space', 15]]) {
        await buttons.nth(index).focus();
        await page.keyboard.press(key);
        await page.clock.runFor(250);
        expect(await page.evaluate(() => moves.at(-1).camera.zoom)).toBe(zoom);
      }
    } finally {
      await browser.close();
    }
  });
}
