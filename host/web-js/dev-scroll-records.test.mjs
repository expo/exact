import { expect, test } from 'bun:test';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { hostFixtureFile } from '../web/tests/glue-fixture.mjs';

function recordsServer(checkpoint = () => null) {
  return Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    const js = body => new Response(body, { headers: { 'content-type': 'text/javascript' } });
    if (path === '/rt.js') return js('export const After = [], PropHooks = {}, Resources = [], inflight = { n: 1 }, clock = {}, data = {}, journal = []; export const settled = () => { for (const f of After) f(); }; export const carryRouter = null, late = null, launchLocation = null, res = null, sig = null;');
    if (path === '/navigation.js') return js(hostFixtureFile(new URL('../web/navigation.js', import.meta.url)));
    if (['/checkpoint.js', '/focus.js'].includes(path)) return js(Bun.file(new URL('.' + path, import.meta.url)));
    const cp = checkpoint();
    const script = cp ? `<script type="application/vnd.exact.dev-checkpoint">${JSON.stringify(cp).replaceAll('<', '\\u003c')}</script>` : '';
    return new Response(`<!doctype html>${script}<main id="exact-root"></main>`, { headers: { 'content-type': 'text/html' } });
  } });
}

test.each(['answer', 'focus', 'scroll', 'expire'])('replacement records survive another reload while content is pending (%s)', async mode => {
  let checkpoint;
  const server = recordsServer(() => checkpoint);
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.clock.install();
    await page.goto(server.url.href);
    const mount = async full => page.evaluate(async full => {
      const { prepareDev } = await import('/checkpoint.js'), finish = prepareDev();
      const root = document.getElementById('exact-root');
      root.innerHTML = `<input data-carry-type="Input" id="other" value="Other">
        <div data-carry-type="Column" id="port" style="height:100px;width:200px;overflow:auto">
          <div data-carry-type="Column" id="above" style="height:${full ? 100 : 0}px"></div>
          <div ${full ? 'data-carry-type="Column"' : ''} id="body" style="height:1000px"></div>
        </div>${full ? '<input data-carry-type="Input" id="field" value="Piranesi">' : ''}`;
      finish();
    }, full);
    await mount(true);
    checkpoint = await page.evaluate(() => {
      const field = document.getElementById('field'); field.focus(); field.setSelectionRange(2, 5, 'backward');
      document.getElementById('port').scrollTop = 130;
      return __exactDevRecords();
    });
    await page.goto(server.url.href);
    await mount(false);
    expect(await page.evaluate(() => document.getElementById('port').scrollTop)).toBe(0);
    await page.clock.runFor(6000);
    if (mode === 'focus') await page.locator('#other').focus();
    if (mode === 'scroll') {
      await page.locator('#body').evaluate(el => el.setAttribute('data-carry-type', 'Column'));
      await page.locator('#port').hover(); await page.mouse.wheel(0, 55);
      await page.waitForFunction(() => document.getElementById('port').scrollTop === 55);
    }
    checkpoint = await page.evaluate(() => __exactDevRecords());
    expect(checkpoint.focus?.path).toEqual(mode === 'focus' ? [0] : [2]);
    expect(checkpoint.scrolls[0].axes.top.target).toBe(mode === 'scroll' ? 55 : 130);
    await page.goto(server.url.href);
    await mount(false);
    if (mode === 'expire') await page.clock.runFor(4001);
    const result = await page.evaluate(async () => {
      const rt = await import('/rt.js'), root = document.getElementById('exact-root');
      document.getElementById('above').style.height = '160px';
      document.getElementById('body').setAttribute('data-carry-type', 'Column');
      root.insertAdjacentHTML('beforeend', '<input data-carry-type="Input" id="field" value="Piranesi">');
      const field = document.getElementById('field');
      field.setSelectionRange(1, 3, 'forward');
      rt.inflight.n = 0; rt.settled();
      await new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      return { focus: document.activeElement.id, selection: [field.selectionStart, field.selectionEnd, field.selectionDirection],
        top: document.getElementById('port').scrollTop, restoration: history.scrollRestoration, pending: rt.After.length };
    });
    expect(result).toEqual({ focus: mode === 'expire' ? '' : mode === 'focus' ? 'other' : 'field',
      selection: ['focus', 'expire'].includes(mode) ? [1, 3, 'forward'] : [2, 5, 'backward'],
      top: mode === 'expire' ? 0 : mode === 'scroll' ? 215 : 190, restoration: 'auto', pending: 0 });
  } finally { await browser?.close(); server.stop(true); }
}, 30000);

test('replacement records match browser SVG anchoring through display contents', async () => {
  const server = recordsServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      const { prepareDev } = await import('/checkpoint.js'), rt = await import('/rt.js');
      const root = document.getElementById('exact-root'), frame = () => new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      const content = height => `<div data-carry-type="Column" id="port" style="height:100px;width:200px;overflow:auto">
        <div data-carry-type="Column" id="above" style="height:${height}px"></div>
        <div data-carry-type="Column" style="display:contents"><svg data-carry-type="Svg" width="200" height="1000" style="display:block"><rect width="200" height="1000"/></svg></div></div>`;
      root.innerHTML = content(100); prepareDev()();
      document.getElementById('port').scrollTop = 130; await frame();
      const cp = __exactDevRecords();
      document.getElementById('above').style.height = '160px'; await frame();
      const ordinary = document.getElementById('port').scrollTop;
      const script = document.createElement('script'); script.type = 'application/vnd.exact.dev-checkpoint';
      script.textContent = JSON.stringify(cp); document.head.append(script);
      const next = await import('/checkpoint.js?next'), finish = next.prepareDev();
      root.innerHTML = content(160); rt.inflight.n = 0; finish(); await frame();
      return { ordinary, restored: document.getElementById('port').scrollTop };
    });
    expect(result).toEqual({ ordinary: 190, restored: 190 });
  } finally { await browser?.close(); server.stop(true); }
}, 30000);

test.each(['button', 'select', 'input', 'page', 'clicked scroller', 'horizontal wheel'])
('replacement records cancel after scrolling takes effect (%s)', async mode => {
  const server = recordsServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
    await page.goto(server.url.href);
    await page.evaluate(async mode => {
      const { prepareDev } = await import('/checkpoint.js');
      const root = document.getElementById('exact-root');
      const controls = '<button id="button">Activate</button><select id="select"><option>One</option><option>Two</option></select><input id="input" value="Text">';
      const content = saved => `${controls}<div id="body" ${saved ? 'data-carry-type="Text"' : ''} style="height:2000px"></div>`;
      const page = mode === 'page';
      root.innerHTML = page ? content(true) : `<div id="port" data-carry-type="Column" style="height:100px;width:300px;overflow-y:auto;overflow-x:hidden">${content(true)}</div>`;
      prepareDev()();
      const port = page ? document.scrollingElement : document.getElementById('port');
      port.scrollTop = 130;
      const cp = globalThis.__exactDevRecords(), script = document.createElement('script');
      script.type = 'application/vnd.exact.dev-checkpoint'; script.textContent = JSON.stringify(cp); document.head.append(script);
      const next = await import('/checkpoint.js?next'), finish = next.prepareDev();
      (page ? root : port).innerHTML = content(false);
      port.scrollTop = 0;
      finish();
      if (['button', 'select', 'input'].includes(mode)) document.getElementById(mode).focus({ preventScroll: true });
      document.getElementById('button').onclick = () => { globalThis.activated = true; };
      globalThis.readerFinished = new Promise(ok => {
        const target = page ? document : port;
        const end = () => { if (port.scrollTop) { target.removeEventListener('scrollend', end); ok(); } };
        target.addEventListener('scrollend', end);
      });
    }, mode);
    if (mode === 'clicked scroller') {
      await page.locator('#port').click({ position: { x: 220, y: 70 } });
      await page.evaluate(() => new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(() => requestAnimationFrame(ok)))));
      expect(await page.evaluate(() => document.activeElement === document.body)).toBe(true);
      await page.keyboard.press('PageDown');
    } else if (mode === 'horizontal wheel') {
      await page.locator('#port').hover();
      await page.mouse.wheel(55, 0);
    } else if (mode === 'input') {
      await page.keyboard.press('ArrowDown');
      await page.keyboard.press('ArrowRight');
    } else await page.keyboard.press({ button: 'Space', select: 'ArrowDown', page: 'PageDown' }[mode]);
    if (mode === 'select') await page.keyboard.press('Escape');
    const scrolls = ['page', 'clicked scroller'].includes(mode);
    if (scrolls) await page.evaluate(() => readerFinished);
    await page.evaluate(() => new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok))));
    const before = await page.evaluate(mode => ({ offset: mode === 'page' ? scrollY : document.getElementById('port').scrollTop,
      activated: !!globalThis.activated, restoration: history.scrollRestoration }), mode);
    if (scrolls) expect(before.offset).toBeGreaterThan(0);
    else expect(before.offset).toBe(0);
    if (mode === 'button') expect(before.activated).toBe(true);
    if (mode !== 'page') expect(before.restoration).toBe('manual');
    const after = await page.evaluate(async mode => {
      const rt = await import('/rt.js'), port = mode === 'page' ? document.scrollingElement : document.getElementById('port');
      document.getElementById('body').setAttribute('data-carry-type', 'Text');
      rt.settled();
      await new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      const during = { offset: port.scrollTop, waiting: rt.inflight.n, restoration: history.scrollRestoration };
      rt.inflight.n = 0; rt.settled();
      return { during, offset: port.scrollTop, pending: rt.After.length };
    }, mode);
    expect(after).toEqual({ during: { offset: scrolls ? before.offset : 130, waiting: 1, restoration: 'auto' },
      offset: scrolls ? before.offset : 130, pending: 0 });
  } finally { await browser?.close(); server.stop(true); }
}, 30000);

test('replacement records match browser anchoring through nested fractional offsets', async () => {
  const server = recordsServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      const { prepareDev } = await import('/checkpoint.js'), rt = await import('/rt.js');
      const root = document.getElementById('exact-root'), frame = () => new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      const content = height => {
        let nested = '<div data-carry-type="Column" style="height:1000px">Anchor</div>';
        for (let i = 0; i < 8; i++) nested = `<div data-carry-type="Column" style="position:relative"><div class="spacer" style="height:${height}px"></div>${nested}</div>`;
        return `<div data-carry-type="Column" id="port" style="height:100px;width:200px;overflow:auto"><div style="height:100px"></div>${nested}</div>`;
      };
      root.innerHTML = content(0.4); prepareDev()();
      document.getElementById('port').scrollTop = 130; await frame();
      const cp = __exactDevRecords();
      for (const el of document.querySelectorAll('.spacer')) el.style.height = '0.6px';
      await frame();
      const ordinary = document.getElementById('port').scrollTop;
      const script = document.createElement('script'); script.type = 'application/vnd.exact.dev-checkpoint';
      script.textContent = JSON.stringify(cp); document.head.append(script);
      const next = await import('/checkpoint.js?next'), finish = next.prepareDev();
      root.innerHTML = content(0.6); rt.inflight.n = 0; finish(); await frame();
      return { ordinary, restored: document.getElementById('port').scrollTop };
    });
    expect(result).toEqual({ ordinary: 132, restored: 132 });
  } finally { await browser?.close(); server.stop(true); }
}, 30000);

test.each(['nested rotation', 'ancestor perspective'].flatMap(mode => [100, 160].map(height => [mode, height])))
('replacement records match browser anchoring through %s (%s px)', async (mode, height) => {
  const server = recordsServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async ({ mode, height }) => {
      const { prepareDev } = await import('/checkpoint.js'), rt = await import('/rt.js');
      const root = document.getElementById('exact-root'), frame = () => new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      const content = height => `<div data-carry-type="Column" style="${mode === 'nested rotation' ? 'rotate:x 30deg' : 'perspective:400px;perspective-origin:40% 60%'}">
        <div data-carry-type="Column" id="port" style="height:100px;width:200px;overflow:auto;rotate:x 30deg">
          <div data-carry-type="Column" id="above" style="height:${height}px"></div>
          <div data-carry-type="Column" style="height:100px">Anchor</div>
          <div data-carry-type="Column" style="height:1000px"></div>
        </div></div>`;
      root.innerHTML = content(100); prepareDev()();
      document.getElementById('port').scrollTop = 130; await frame();
      const cp = __exactDevRecords();
      document.getElementById('above').style.height = `${height}px`; await frame();
      const ordinary = document.getElementById('port').scrollTop;
      const script = document.createElement('script'); script.type = 'application/vnd.exact.dev-checkpoint';
      script.textContent = JSON.stringify(cp); document.head.append(script);
      const next = await import('/checkpoint.js?next'), finish = next.prepareDev();
      root.innerHTML = content(height); rt.inflight.n = 0; finish(); await frame();
      return { ordinary, restored: document.getElementById('port').scrollTop, pending: rt.After.length };
    }, { mode, height });
    expect(result).toEqual({ ordinary: 130 + height - 100, restored: 130 + height - 100, pending: 0 });
  } finally { await browser?.close(); server.stop(true); }
}, 30000);

test.each(['answer', 'expire'])('replacement records restore an enclosing anchor while an inner offset waits (%s)', async mode => {
  const server = recordsServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    if (mode === 'expire') await page.clock.install();
    await page.goto(server.url.href);
    const before = await page.evaluate(async () => {
      const { prepareDev } = await import('/checkpoint.js');
      const root = document.getElementById('exact-root');
      const content = (height, width) => `<div data-carry-type="Column" id="outer" style="height:200px;width:240px;overflow:auto">
        <div data-carry-type="Column" style="height:${height}px"></div>
        <div data-carry-type="Column" id="inner" style="height:100px;width:200px;overflow:auto"><div style="height:100px;width:${width}px"></div></div>
        <div data-carry-type="Column" style="height:1000px"></div></div>`;
      root.innerHTML = content(100, 900);
      prepareDev()();
      document.getElementById('outer').scrollTop = 150;
      document.getElementById('inner').scrollLeft = 80;
      const cp = globalThis.__exactDevRecords(), script = document.createElement('script');
      script.type = 'application/vnd.exact.dev-checkpoint'; script.textContent = JSON.stringify(cp); document.head.append(script);
      const next = await import('/checkpoint.js?next'), finish = next.prepareDev();
      root.innerHTML = content(160, 200);
      finish();
      return { outer: document.getElementById('outer').scrollTop, inner: document.getElementById('inner').scrollLeft,
        restoration: history.scrollRestoration };
    });
    expect(before).toEqual({ outer: 210, inner: 0, restoration: 'manual' });
    if (mode === 'expire') await page.clock.runFor(10000);
    const after = await page.evaluate(async mode => {
      const rt = await import('/rt.js');
      if (mode === 'answer') {
        document.getElementById('inner').firstElementChild.style.width = '900px';
        rt.settled();
        await new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      }
      const during = { outer: document.getElementById('outer').scrollTop, inner: document.getElementById('inner').scrollLeft,
        waiting: rt.inflight.n, restoration: history.scrollRestoration };
      rt.inflight.n = 0; rt.settled();
      return { during, pending: rt.After.length };
    }, mode);
    expect(after).toEqual({ during: { outer: 210, inner: mode === 'answer' ? 80 : 0, waiting: 1, restoration: 'auto' }, pending: 0 });
  } finally { await browser?.close(); server.stop(true); }
}, 30000);

test.each(['wheel', 'key', 'touch', 'unrecorded', 'clamped', 'chain', 'contain'])('replacement records cancel only the scrollers that move (%s)', async mode => {
  const server = recordsServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.evaluate(async mode => {
      const { prepareDev } = await import('/checkpoint.js');
      const root = document.getElementById('exact-root');
      document.documentElement.style.overflowAnchor = 'none';
      const content = full => `<div data-carry-type="Column" style="height:300px"></div>
        <div data-carry-type="Column" id="outer" style="width:260px;height:240px;overflow:auto;overflow-anchor:none">
          <div data-carry-type="Column" id="inner" tabindex="0" style="width:200px;height:100px;overflow:auto;overflow-anchor:none;overscroll-behavior:${mode === 'contain' ? 'contain' : 'auto'}">
            <div style="height:${['chain', 'contain'].includes(mode) ? 150 : mode === 'clamped' && !full ? 100 : 1000}px"></div>
          </div><div id="tail" style="width:${full ? 900 : 260}px;height:500px"></div>
        </div><div id="page-tail" style="height:${full ? 1600 : 0}px"></div>`;
      root.innerHTML = content(true);
      prepareDev()();
      document.getElementById('outer').scrollLeft = 130;
      document.getElementById('inner').scrollTop = ['chain', 'contain'].includes(mode) ? 50 : mode === 'unrecorded' ? 0 : 80;
      window.scrollTo(0, 160);
      const cp = globalThis.__exactDevRecords();
      const script = document.createElement('script');
      script.type = 'application/vnd.exact.dev-checkpoint'; script.textContent = JSON.stringify(cp); document.head.append(script);
      const next = await import('/checkpoint.js?next'), finish = next.prepareDev();
      root.innerHTML = content(false);
      finish();
      document.getElementById('inner').focus({ preventScroll: true });
    }, mode);
    await page.locator('#inner').hover();
    if (mode === 'key') await page.locator('#inner').evaluate(el => {
      globalThis.readerFinished = new Promise(ok => el.addEventListener('scrollend', ok, { once: true }));
    });
    if (mode === 'key') await page.keyboard.press('ArrowDown');
    else if (mode === 'touch') await page.locator('#inner').evaluate(el => {
      const touch = y => new Touch({ identifier: 1, target: el, clientX: 10, clientY: y });
      el.dispatchEvent(new TouchEvent('touchstart', { bubbles: true, touches: [touch(80)] }));
      el.dispatchEvent(new TouchEvent('touchmove', { bubbles: true, cancelable: true, touches: [touch(25)] }));
      el.scrollTop = 135;
    });
    else if (mode === 'clamped') await page.locator('#inner').evaluate(el => {
      el.dispatchEvent(new WheelEvent('wheel', { bubbles: true, deltaY: 55 }));
    });
    else await page.mouse.wheel(0, 55);
    if (mode === 'chain') await page.waitForFunction(() => document.getElementById('outer').scrollTop > 0);
    else if (mode === 'key') await page.evaluate(() => readerFinished);
    else if (!['contain', 'clamped'].includes(mode)) await page.waitForFunction(offset => document.getElementById('inner').scrollTop === offset, mode === 'unrecorded' ? 55 : 135);
    await page.evaluate(() => new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok))));
    const during = await page.evaluate(() => ({ inner: document.getElementById('inner').scrollTop,
      outer: document.getElementById('outer').scrollTop, restoration: history.scrollRestoration }));
    expect(during.restoration).toBe('manual');
    expect(during.outer).toBe(mode === 'chain' ? 55 : 0);
    const result = await page.evaluate(async mode => {
      const rt = await import('/rt.js'), outer = document.getElementById('outer');
      document.getElementById('tail').style.width = '900px';
      document.getElementById('page-tail').style.height = '1600px';
      if (mode === 'clamped') document.querySelector('#inner > div').style.height = '1000px';
      rt.settled();
      await new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      const beforeAnswer = [outer.scrollLeft, outer.scrollTop, scrollY];
      rt.inflight.n = 0; rt.settled();
      await new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      return { beforeAnswer, afterAnswer: [outer.scrollLeft, outer.scrollTop, scrollY], inner: document.getElementById('inner').scrollTop, restoration: history.scrollRestoration };
    }, mode);
    expect(result.beforeAnswer).toEqual([130, mode === 'chain' ? 55 : 0, 160]);
    expect(result.afterAnswer).toEqual(result.beforeAnswer);
    expect(result.inner).toBe(mode === 'clamped' ? 80 : during.inner);
    expect(result.restoration).toBe('auto');
  } finally { await browser?.close(); server.stop(true); }
}, 30000);

test.each(['wheel', 'key', 'touch', 'scrollbar', 'autoscroll'].flatMap(input => ['left', 'top'].map(axis => [input, axis])))
('replacement records preserve the other axis during a reader gesture (%s, %s)', async (input, axis) => {
  const server = recordsServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async ({ input, axis }) => {
      const { prepareDev } = await import('/checkpoint.js'), rt = await import('/rt.js');
      const root = document.getElementById('exact-root');
      root.innerHTML = '<div data-carry-type="Column" id="port" tabindex="0" style="height:100px;width:200px;overflow:auto;overflow-anchor:none"><div style="width:900px;height:1000px"></div></div>';
      prepareDev()();
      const port = document.getElementById('port'), frame = () => new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      port.scrollLeft = port.scrollTop = 130;
      const cp = globalThis.__exactDevRecords(), script = document.createElement('script');
      script.type = 'application/vnd.exact.dev-checkpoint'; script.textContent = JSON.stringify(cp); document.head.append(script);
      const next = await import('/checkpoint.js?next'), finish = next.prepareDev();
      port.firstElementChild.style[axis === 'left' ? 'height' : 'width'] = axis === 'left' ? '100px' : '200px';
      port.scrollLeft = port.scrollTop = 0;
      finish();
      const options = { bubbles: true, cancelable: true };
      if (input === 'touch') {
        const touch = at => new Touch({ identifier: 1, target: port, clientX: axis === 'left' ? at : 10, clientY: axis === 'top' ? at : 10 });
        port.dispatchEvent(new TouchEvent('touchstart', { ...options, touches: [touch(80)] }));
        port.dispatchEvent(new TouchEvent('touchmove', { ...options, touches: [touch(25)] }));
      } else if (input === 'autoscroll') {
        port.dispatchEvent(new PointerEvent('pointerdown', { ...options, button: 1 }));
        port.dispatchEvent(new PointerEvent('pointerup', { ...options, button: 1 }));
        await frame();
      } else port.dispatchEvent(input === 'wheel' ? new WheelEvent('wheel', { ...options, deltaX: axis === 'left' ? 55 : 0, deltaY: axis === 'top' ? 55 : 0 })
        : input === 'key' ? new KeyboardEvent('keydown', { ...options, key: axis === 'left' ? 'ArrowRight' : 'ArrowDown' })
        : new PointerEvent('pointerdown', options));
      port[axis === 'left' ? 'scrollLeft' : 'scrollTop'] = 185;
      await frame();
      const during = { offsets: [port.scrollLeft, port.scrollTop], restoration: history.scrollRestoration };
      port.dispatchEvent(new PointerEvent('pointerup', options));
      port.firstElementChild.style.width = '900px'; port.firstElementChild.style.height = '1000px'; rt.settled();
      await frame();
      const beforeAnswer = [port.scrollLeft, port.scrollTop];
      rt.inflight.n = 0; rt.settled(); await frame();
      return { during, beforeAnswer, offsets: [port.scrollLeft, port.scrollTop], restoration: history.scrollRestoration };
    }, { input, axis });
    expect(result).toEqual({ during: { offsets: axis === 'left' ? [185, 0] : [0, 185], restoration: 'manual' },
      beforeAnswer: axis === 'left' ? [185, 130] : [130, 185], offsets: axis === 'left' ? [185, 130] : [130, 185], restoration: 'auto' });
  } finally { await browser?.close(); server.stop(true); }
}, 30000);

test('replacement records fall back to a numeric offset when an SVG coordinate is non-finite', async () => {
  const server = recordsServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    const result = await page.evaluate(async () => {
      const { prepareDev } = await import('/checkpoint.js'), rt = await import('/rt.js');
      const root = document.getElementById('exact-root');
      root.innerHTML = '<div data-carry-type="Column" id="port" style="height:100px;width:200px;overflow:auto"><div style="height:100px"></div><svg data-carry-type="Svg" width="200" height="100" style="display:block"></svg><div style="height:1000px"></div></div>';
      prepareDev()();
      const port = document.getElementById('port');
      port.scrollTop = 130;
      const cp = globalThis.__exactDevRecords(), script = document.createElement('script');
      script.type = 'application/vnd.exact.dev-checkpoint'; script.textContent = JSON.stringify(cp); document.head.append(script);
      const next = await import('/checkpoint.js?next'), finish = next.prepareDev();
      port.style.transform = 'scale(0)'; port.firstElementChild.style.height = '160px'; port.scrollTop = 0;
      rt.inflight.n = 0; finish();
      await new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      const captured = globalThis.__exactDevRecords();
      return { offset: port.scrollTop, anchors: captured.scrolls.flatMap(s => Object.values(s.axes).flatMap(a => a.anchor?.nodes ?? [])), restoration: history.scrollRestoration };
    });
    expect(result).toEqual({ offset: 130, anchors: [], restoration: 'auto' });
  } finally { await browser?.close(); server.stop(true); }
}, 30000);

test.each(['plain', 'page'].flatMap(port => ['wheel-x', 'key-x', 'wheel-y', 'key-y'].map(input => [port, input])))
('replacement records keep a clamped target when input scrolls nothing (%s, %s)', async (kind, input) => {
  const server = recordsServer();
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    await page.goto(server.url.href);
    await page.evaluate(async kind => {
      const { prepareDev } = await import('/checkpoint.js');
      const root = document.getElementById('exact-root');
      root.innerHTML = kind === 'plain' ? '<div data-carry-type="Column" id="port" tabindex="0" style="height:100px;width:200px;overflow:auto;overflow-anchor:none"><div id="tail" style="height:1000px"></div></div>'
        : '<div data-carry-type="Column" id="tail" tabindex="0" style="height:2000px"></div>';
      document.documentElement.style.overflowAnchor = 'none';
      prepareDev()();
      const port = kind === 'plain' ? document.getElementById('port') : document.scrollingElement;
      port.scrollTop = 180;
      const cp = globalThis.__exactDevRecords(), script = document.createElement('script');
      script.type = 'application/vnd.exact.dev-checkpoint'; script.textContent = JSON.stringify(cp); document.head.append(script);
      const next = await import('/checkpoint.js?next'), finish = next.prepareDev();
      document.getElementById('tail').style.height = '50px';
      port.scrollTop = 0;
      finish();
      document.getElementById(kind === 'plain' ? 'port' : 'tail').focus({ preventScroll: true });
    }, kind);
    const port = page.locator(kind === 'plain' ? '#port' : '#tail');
    await port.hover();
    if (input.startsWith('key')) await page.keyboard.press(input.endsWith('x') ? 'ArrowLeft' : 'ArrowDown');
    else await port.evaluate((el, input) => el.dispatchEvent(new WheelEvent('wheel', { bubbles: true, deltaX: input.endsWith('x') ? 48 : 0, deltaY: input.endsWith('y') ? 48 : 0 })), input);
    await page.evaluate(() => new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok))));
    expect(await page.evaluate(() => history.scrollRestoration)).toBe('manual');
    const result = await page.evaluate(async kind => {
      const rt = await import('/rt.js');
      document.getElementById('tail').style.height = '2000px'; rt.settled();
      await new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      const port = kind === 'plain' ? document.getElementById('port') : document.scrollingElement;
      const during = port.scrollTop;
      rt.inflight.n = 0; rt.settled();
      await new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      return { during, after: port.scrollTop, restoration: history.scrollRestoration };
    }, kind);
    expect(result).toEqual({ during: 180, after: 180, restoration: 'auto' });
  } finally { await browser?.close(); server.stop(true); }
}, 30000);

test.each(['restore', 'focus', 'target-focus', 'scroll', 'wheel', 'boot-scroll', 'contents', 'number', 'autofocus', 'ancestor', 'stream', 'numeric', 'unanchored', 'follow', 'follow-end', 'follow-start', 'start'])('replacement records defer to mounted content and later input (%s)', async mode => {
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    const js = text => new Response(text, { headers: { 'content-type': 'text/javascript' } });
    if (path === '/rt.js') return js('export const After = [], Before = [], Hosts = {}, Resources = [{ ticket: { stream: true, messages: 0 }, pending: () => true }], inflight = { n: 0 }, PropHooks = {}, clock = {}, data = {}, journal = []; export const settled = () => { for (const f of After) f(); }; export const carryRouter = null, late = null, launchLocation = null, res = null, sig = null;');
    if (path === '/navigation.js') return js(hostFixtureFile(new URL('../web/navigation.js', import.meta.url)));
    if (['/checkpoint.js', '/focus.js', '/follow.js'].includes(path)) return js(Bun.file(new URL('.' + path, import.meta.url)));
    return new Response('<main id="exact-root"></main>', { headers: { 'content-type': 'text/html' } });
  } });
  let browser;
  try {
    browser = await suiteBrowser();
    const page = await browser.newPage();
    const errors = [];
    page.on('pageerror', e => errors.push(e.message));
    await page.goto(server.url.href);
    const reading = page.evaluate(async mode => {
      const rt = await import('/rt.js'), { prepareDev } = await import('/checkpoint.js');
      const following = mode.startsWith('follow');
      if (following) await import('/follow.js');
      const root = document.getElementById('exact-root');
      const rows = (height, child = true) => `<div data-carry-type="Column" style="height:${height}px"></div><div data-carry-type="Column" style="height:100px">${child ? '<div data-carry-type="Text" style="height:100px">Anchor</div>' : ''}</div><div data-carry-type="Column" style="height:1000px"></div>`;
      const frame = content => `<input data-carry-type="Input" id="other"><div data-carry-type="Column" id="port" style="height:100px;width:200px;overflow:auto;${mode === 'unanchored' ? 'overflow-anchor:none' : ''}">${content}</div>`;
      const text = mode === 'number' ? '12345' : mode === 'target-focus' ? 'Piranesi' : 'hello world';
      root.innerHTML = frame(rows(100)) + `<input data-carry-type="Input" id="notes" value="${text}">`;
      prepareDev()();
      if (following) {
        rt.PropHooks.scrollFollowEnd(document.getElementById('port'), 'true'); rt.settled();
        await new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      }
      document.getElementById('notes').focus();
      document.getElementById('notes').setSelectionRange(2, 5, 'backward');
      const oldPort = document.getElementById('port');
      oldPort.scrollTop = mode.endsWith('start') ? 0 : mode === 'follow-end' ? oldPort.scrollHeight - oldPort.clientHeight : 130;
      oldPort.dispatchEvent(new Event('scroll'));
      const cp = globalThis.__exactDevRecords();
      const script = document.createElement('script');
      script.type = 'application/vnd.exact.dev-checkpoint'; script.textContent = JSON.stringify(cp); document.head.append(script);
      const next = await import('/checkpoint.js?next');
      const finish = next.prepareDev();
      root.innerHTML = frame('<div style="height:1000px"></div>');
      if (following) rt.PropHooks.scrollFollowEnd(document.getElementById('port'), 'true');
      finish();
      const port = document.getElementById('port');
      if (mode === 'focus') document.getElementById('other').focus();
      if (['scroll', 'boot-scroll'].includes(mode)) {
        if (mode === 'scroll') port.dispatchEvent(new WheelEvent('wheel', { bubbles: true, deltaY: 55 }));
        port.scrollTop = 55; port.dispatchEvent(new Event('scroll'));
      }
      if (mode === 'wheel') await new Promise(ok => { globalThis.resumeRecords = ok; });
      port.innerHTML = mode === 'numeric' ? '<div style="height:1500px"></div>' : rows(160, !['ancestor', 'stream'].includes(mode));
      root.insertAdjacentHTML('beforeend', `<input data-carry-type="Input" id="notes" type="${mode === 'number' ? 'number' : 'text'}" value="${mode === 'contents' ? 'different' : text}">`);
      if (mode === 'target-focus') {
        const field = document.getElementById('notes');
        field.focus(); field.setSelectionRange(8, 8);
      }
      if (mode === 'autofocus') {
        const { autofocus, mayFocus } = await import('/focus.js');
        document.getElementById('notes').setAttribute('data-autofocus', 'true');
        mayFocus(); autofocus();
      }
      if (['stream', 'numeric'].includes(mode)) rt.Resources[0].ticket.messages = 1;
      else rt.Resources[0].ticket = null;
      for (const f of rt.After) f();
      await new Promise(ok => requestAnimationFrame(() => requestAnimationFrame(ok)));
      const field = document.getElementById('notes');
      return { focus: document.activeElement.id, scroll: port.scrollTop, selection: [field.selectionStart, field.selectionEnd, field.selectionDirection], hooks: rt.After.length };
    }, mode);
    if (mode === 'wheel') {
      await page.waitForFunction(() => typeof globalThis.resumeRecords === 'function');
      await page.locator('#port').hover();
      await page.mouse.wheel(0, 55);
      await page.waitForFunction(() => document.getElementById('port').scrollTop === 55);
      await page.evaluate(() => resumeRecords());
    }
    const result = await reading;
    expect(result.focus).toBe(mode === 'focus' ? 'other' : 'notes');
    expect(result.scroll).toBe(['scroll', 'wheel'].includes(mode) ? 55 : mode.endsWith('start') ? 0 : mode === 'follow-end' ? 1160 : ['numeric', 'unanchored'].includes(mode) ? 130 : 190);
    if (!['focus', 'target-focus', 'contents', 'number'].includes(mode)) expect(result.selection).toEqual([2, 5, 'backward']);
    if (mode === 'target-focus') {
      expect(result.selection.slice(0, 2)).toEqual([8, 8]);
      await page.keyboard.type('x');
      expect(await page.locator('#notes').inputValue()).toBe('Piranesix');
    }
    if (mode === 'number') expect(result.selection).toEqual([null, null, null]);
    if (mode === 'contents') expect(result.selection.slice(0, 2)).not.toEqual([2, 5]);
    expect(result.hooks).toBe(mode.startsWith('follow') ? 1 : 0);
    expect(errors).toEqual([]);
  } finally { await browser?.close(); server.stop(true); }
}, 30000);
