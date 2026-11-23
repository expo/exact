// LLP 1094.000 D6, D8: the browser's drag of a draggable node, through the page's own `drag-out.js`, pan and swipe
// glue and drag rules, in Chromium, Firefox and WebKit under Playwright's mouse: what the drag carries, what keeps it
// from the node, a field's down turning it off until the next down, a swipe beside it, and another script's draggable,
// in any case, left its own drag.
import { afterAll, beforeAll, expect } from 'bun:test';
import { readFileSync } from 'node:fs';
import { browserMissing } from '../../../scripts/agent-playwright.mjs';
import { chromium, suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { need } from '../../../scripts/coverage.mjs';
import { glueDeclarations, wasmSource } from './glue-fixture.mjs';

// The page's own stylesheet, which decides what drags: the reset that `all: unset`s a button, the restored
// `-webkit-user-drag`, a pressable's image's, and the native fields'.
const page = readFileSync(new URL('../index.html', import.meta.url), 'utf8');
const rules = page.slice(page.indexOf('<style>') + 7, page.indexOf('</style>'));
const fixture = `<!doctype html>
<style>
${rules}
  #exact-root > * { position: absolute; left: 20px; width: 300px; height: 50px; background: #ddd; }
  img { width: 40px; height: 40px; }
</style>
<main id="exact-root">
  <button id="card" draggable="true" data-datatransferfiles="assets/a.png" style="top:10px;display:flex;align-items:center"><span id="label" style="display:inline-block;width:150px;height:40px">Card</span><img id="picture"><svg id="glyph" draggable="TRUE" data-foreign width="20" height="20"><rect width="20" height="20"></rect></svg></button>
  <div id="box" draggable="true" data-datatransfertext="box" style="top:70px"><button id="press" data-exact-on="press" style="display:inline-block;width:60px;height:40px;background:#aaa">Press<img id="icon" style="width:20px;height:20px"></button><input id="field" data-native value="some words to select" style="display:inline-block;width:110px"><input id="range" data-native type="range" min="0" max="100" value="10" style="display:inline-block;width:110px"></div>
  <div id="pan" style="top:130px;height:60px;touch-action:none"><div id="inner" draggable="true" data-datatransfertext="inner" style="width:120px;height:40px;background:#aaa;display:inline-block">inner</div><img id="panned"></div>
  <a id="link" href="/landmark/1016" style="top:200px"><img id="linked" draggable="true" data-datatransfertext="the picture"></a>
  <div id="card2" draggable="true" data-datatransfertext="card" style="top:260px;height:60px"><div id="row" style="width:300px;height:44px;touch-action:pan-y;background:#bbb"></div></div>
  <div id="swipe" draggable="true" data-datatransfertext="swipe row" style="position:absolute;left:20px;top:340px;width:300px;height:44px;touch-action:pan-y"></div>
  <div id="foreign" draggable="true" data-foreign style="top:400px;height:40px"></div>
  <button id="plain" style="top:460px;height:44px"><img id="upper" draggable="TRUE" data-foreign></button>
  <div id="outer" draggable="true" data-datatransfertext="outer" style="left:340px;top:10px;height:200px">
    <div id="pan2" style="display:flex;align-items:flex-start;height:70px;touch-action:none;background:#bbb"><button id="pan2press" data-exact-on="press" style="width:45px;height:40px">Press</button><div id="pan2tap" data-exact-on="press" style="width:45px;height:40px">Tap</div><a id="pan2link" href="/x" style="width:45px;height:40px">Link</a><div id="pan2card" draggable="true" data-datatransfertext="child" style="display:flex;width:130px;height:40px"><a id="pan2cardlink" href="/y" style="width:50px;height:40px">Link</a><svg id="pan2svg" draggable="TRUE" data-foreign width="30" height="30"><rect width="30" height="30"></rect></svg><div data-exact-on="press" style="width:40px;height:40px"><svg id="pan2presssvg" draggable="TRUE" data-foreign width="30" height="30"><rect width="30" height="30"></rect></svg></div></div><svg id="pan2bare" draggable="TRUE" data-foreign width="30" height="30"><rect width="30" height="30"></rect></svg></div>
    <div id="height" style="height:50px;touch-action:none;background:#aaa"></div>
    <div id="clip" style="height:60px;overflow:hidden"><div id="target" style="width:100%;height:100%;box-sizing:border-box"><div id="handle" style="height:100%;touch-action:none;background:#999"></div></div></div>
  </div>
  <button id="outerbutton" draggable="true" data-datatransfertext="button" style="left:340px;top:220px;height:120px;display:block">
    <div id="height2" style="height:50px;touch-action:none;background:#aaa"></div>
    <div id="clip2" style="height:60px;overflow:hidden"><div id="target2" style="width:100%;height:100%;box-sizing:border-box"><div id="handle2" style="height:100%;touch-action:none;background:#999"></div></div></div>
  </button>
</main>
<script type="module">
  import { listen, writeDraggable } from './drag-out.js';
  window.writeDraggable = writeDraggable;
  // The app's draggable nodes are those it writes through the listener; the rest are another script's.
  for (const el of document.querySelectorAll('[draggable]:not([data-foreign])')) writeDraggable(el, 'true');
  import { motionController } from './motion-glue.js';
  import { createInputHandlers } from './input-glue.js';
  window.drags = []; window.deltas = []; window.begins = [];
  const c = document.createElement('canvas'); c.width = c.height = 40; c.getContext('2d').fillRect(0, 0, 40, 40);
  await Promise.all([...document.querySelectorAll('img')].map((i) => new Promise((r) => { i.onload = r; i.src = c.toDataURL(); })));
  const byId = (id) => document.getElementById(id);
  const views = new Map([[1, byId('row')], [2, byId('pan')], [3, byId('swipe')], [4, byId('pan2')], [5, byId('height')],
    [6, byId('handle')], [7, byId('target')], [8, byId('clip')], [9, byId('height2')], [10, byId('handle2')],
    [11, byId('target2')], [12, byId('clip2')]]);
  let serial = 0;
  const motion = motionController({ views, now: () => performance.now(), generation: () => 1, inert: () => false, applyBatch() {},
    request(r) {
      if (r.op === 'gesture') return { slop: 4, knee: 64, resistance: 0.2 };
      if (r.op === 'begin') { begins.push(r.view); return { token: String(++serial), value: [0, 0] }; }
      if (r.op === 'height-begin') { begins.push(r.op); return { token: String(++serial), target: r.view, value: [r.x, 0] }; }
      if (r.op === 'transform-begin') { begins.push(r.op); return { accepted: true, runtime: r.runtime, geometrySequence: r.geometrySequence,
        translateToken: String(++serial), scaleToken: String(++serial), value: r.values.slice(0, 3) }; }
      return { accepted: true, committed: true };
    } });
  for (const id of [1, 3]) {
    const el = views.get(id);
    el.exactHandlers = ['swiperight', 'swipeleft'];
    motion.attachSwipe(el, id, (type, fn) => el.addEventListener(type, fn));
  }
  const input = createInputHandlers({ root: byId('exact-root'), views, retiredViews: new Set(), ready: () => true,
    inertAncestor: () => false, dispatch: (id, delta) => deltas.push(delta) });
  for (const [id, pan] of [[2, byId('pan')], [4, byId('pan2')]]) pan.addEventListener('pointerdown', input.pan(pan, id, (type, fn) => pan.addEventListener(type, fn)));
  for (const [id, height] of [[5, byId('height')], [9, byId('height2')]]) {
    motion.heightBinding({ id, target: id, handleKey: String(id), targetKey: String(id) });
    motion.attachHeightDrag(height, id, (type, fn) => height.addEventListener(type, fn));
  }
  for (const [id, handle] of [[6, byId('handle')], [10, byId('handle2')]]) {
    motion.transformBinding({ id, target: id + 1, clip: id + 2, runtime: '1', handleKey: String(id), targetKey: String(id + 1), clipKey: String(id + 2) });
    motion.attachTransformDrag(handle, id, (type, fn) => handle.addEventListener(type, fn));
  }
  for (const el of document.querySelectorAll('[data-foreign]')) el.addEventListener('dragstart', (e) => e.dataTransfer.setData('text/plain', el.id + "'s own"));
  if (!new URL(location.href).searchParams.has('waiting')) listen(document);
  window.addEventListener('dragstart', (e) => drags.push({ target: e.target.id, prevented: e.defaultPrevented,
    kinds: [...e.dataTransfer.items].map((i) => i.kind), text: e.dataTransfer.getData('text/plain'),
    uris: e.dataTransfer.getData('text/uri-list') }));
  window.ready = true;
</script>`;

const waitingDragFixture = `
  import { downForDrag, elementOf, lightDomDragPath, observeDragInput, pressedAt } from './drag-input.js';
  const host = {}, views = new Map(), retiredViews = new WeakSet();
  const loadAfterPaint = () => new Promise(() => {});
  ${glueDeclarations(wasmSource, ['liveView', 'dragOutWait', 'waitingInput', 'waitingView',
    'loadDragOut', 'ancestorElements', 'waits', 'waitingDrag', 'writeDraggable'])}
  export function waitForDrag(card) {
    card.dataset.view = '1';
    views.set(1, card);
    writeDraggable(card, 'true');
  }
`;

let server;
beforeAll(() => {
  server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
    const path = new URL(req.url).pathname;
    if (path === '/') return new Response(fixture, { headers: { 'content-type': 'text/html' } });
    if (path === '/waiting-drag.js') return new Response(waitingDragFixture, { headers: { 'content-type': 'text/javascript' } });
    if (path.startsWith('/assets/') || path === '/favicon.ico') return new Response('', { status: 404 });
    return new Response(Bun.file(new URL('..' + path, import.meta.url)));
  } });
});
afterAll(() => server?.stop(true));

/** A mouse drag from `selector`'s middle (moved by `from`) by `by` pixels in `steps`, and what the page saw. Under
 * Playwright, WebKit dispatches no `pointerdown` for a press after a drag on the same page, so a WebKit drag begins on
 * the page loaded again. */
async function drag(page, selector, by, { from = [0, 0], steps = 15 } = {}) {
  if (page.context().browser().browserType().name() === 'webkit') {
    await page.reload();
    await page.waitForFunction('window.ready');
  }
  await page.evaluate(() => { drags.length = 0; deltas.length = 0; begins.length = 0; });
  const box = await page.locator(selector).boundingBox();
  const [x, y] = [box.x + box.width / 2 + from[0], box.y + box.height / 2 + from[1]];
  await page.mouse.move(x, y);
  await page.mouse.down();
  await page.mouse.move(x + by[0], y + by[1], { steps });
  await page.mouse.up();
  return page.evaluate(() => {
    const field = document.getElementById('field');
    return { drags: [...drags], deltas: deltas.length, begins: [...begins], range: Number(document.getElementById('range').value),
      selection: field.selectionEnd - field.selectionStart, box: document.getElementById('box').getAttribute('draggable') };
  });
}
/** The drags the browser went on with. */
const went = (seen) => seen.drags.filter((d) => !d.prevented);

for (const [engine, missing] of [['chromium', chromium().unavailable], ['firefox', await browserMissing('firefox')], ['webkit', await browserMissing('webkit')]]) {
  need(engine, missing)(`${engine}: a draggable node's drag carries its declaration, and what keeps a contact keeps it`, async () => {
    const browser = await suiteBrowser(engine);
    try {
      const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
      await page.goto(`http://127.0.0.1:${server.port}/`);
      await page.waitForFunction('window.ready');
      // On the page as loaded: a field's down turns the node's drag off, what the app writes meanwhile is kept, and the
      // next down gives the node the app's last value.
      const fieldBox = await page.locator('#field').boundingBox(), held = () => page.evaluate(() => document.getElementById('box').getAttribute('draggable'));
      await page.mouse.move(fieldBox.x + 10, fieldBox.y + fieldBox.height / 2);
      await page.mouse.down();
      expect(await held()).toBe('false');
      await page.evaluate(() => { writeDraggable(document.getElementById('box'), 'false'); writeDraggable(document.getElementById('box'), 'true'); });
      await page.mouse.up();
      expect(await held()).toBe('false');
      // The node drags on the same page, in each engine.
      const boxBox = await page.locator('#box').boundingBox(), [x, y] = [boxBox.x + boxBox.width / 2 + 130, boxBox.y + boxBox.height / 2];
      await page.mouse.move(x, y);
      await page.mouse.down();
      expect(await held()).toBe('true');
      await page.mouse.move(x, y + 20, { steps: 15 });
      await page.mouse.up();
      expect(await page.evaluate(() => drags.filter((d) => !d.prevented).map((d) => d.text))).toEqual(['box']);
      // The card drags by its label and by its picture, carrying its file as a URL and not the picture's own bytes.
      for (const at of ['#label', '#picture']) {
        const card = went(await drag(page, at, [0, 20]));
        expect(card.map((d) => [d.kinds.every((kind) => kind === 'string'), d.uris])).toEqual([[true, `http://127.0.0.1:${server.port}/assets/a.png`]]);
      }
      // From an SVG another script marked `draggable`, Firefox, which ignores the attribute there, drags the card with
      // its data; Chromium and WebKit drag the SVG with its script's data.
      const glyph = went(await drag(page, '#glyph', [0, 20])).map((d) => [d.text, d.uris]);
      expect(glyph).toEqual([engine === 'firefox' ? ['', `http://127.0.0.1:${server.port}/assets/a.png`] : ["glyph's own", '']]);
      // A press, a picture in it, a text field and a slider inside the node keep a mouse's contact: the field selects
      // its text and the slider moves, the node stays undraggable after a field's or a slider's release, and a press on
      // the node drags it.
      expect(went(await drag(page, '#press', [0, 20], { from: [-15, 0] }))).toEqual([]);
      expect(went(await drag(page, '#icon', [0, 20]))).toEqual([]);
      const field = await drag(page, '#field', [50, 0], { from: [-40, 0] });
      expect([went(field), field.selection > 0, field.box]).toEqual([[], true, 'false']);
      const slider = await drag(page, '#range', [60, 0], { from: [-44, 0] });
      expect([went(slider), slider.box]).toEqual([[], 'false']);
      expect(slider.range).toBeGreaterThan(40);
      expect(went(await drag(page, '#box', [0, 20], { from: [130, 0] })).map((d) => d.text)).toEqual(['box']);
      // A draggable the app did not make keeps the drag its own script gave it, and so does one written `TRUE` inside
      // a button the app made.
      expect(went(await drag(page, '#foreign', [0, 20])).map((d) => d.text)).toEqual(["foreign's own"]);
      expect(went(await drag(page, '#upper', [0, 20])).map((d) => d.text)).toEqual(["upper's own"]);
      // By a mouse, the innermost draggable node inside a pan drags, and the pan does not.
      const inner = await drag(page, '#inner', [0, 20]);
      expect(went(inner).map((d) => d.text)).toEqual(['inner']);
      expect(inner.deltas).toBe(0);
      // A draggable picture inside a link carries its own declaration.
      expect(went(await drag(page, '#linked', [0, 20])).map((d) => [d.text, d.uris])).toEqual([['the picture', '']]);
      // A swipe inside a draggable card takes a sideways drag; an upright one drags the card.
      const sideways = await drag(page, '#row', [70, 0]);
      expect([went(sideways), sideways.begins]).toEqual([[], [1]]);
      expect(went(await drag(page, '#row', [0, 30])).map((d) => d.text)).toEqual(['card']);
      // A fast move whose first step passes the browser's threshold: Firefox starts the drag before its pointermove,
      // so the swipe takes the unknown movement, which swipes sideways and drags nothing upright; Chromium's and
      // WebKit's move comes first, even when it lands off the row, so an upright one drags.
      const fast = await drag(page, '#swipe', [70, 0], { steps: 3 });
      expect([went(fast), fast.begins]).toEqual([[], [3]]);
      const upright = await drag(page, '#swipe', [0, 70], { steps: 3 });
      expect([went(upright).map((d) => d.text), upright.begins]).toEqual([engine === 'firefox' ? [] : ['swipe row'], []]);
    } finally {
      await browser.close();
    }
  }, 60_000);

  // By a mouse the innermost takes the drag (LLP 1094.000 D8): a pan, a height drag or a transform drag inside a
  // draggable node, whose drag the browser dispatches at that node, and a pan over a plain image.
  // Firefox is not measured here (QUEUE.md).
  if (engine !== 'firefox') need(engine, missing)(`${engine}: a drag recognizer takes a mouse's drag from a draggable node around it`, async () => {
    const browser = await suiteBrowser(engine);
    try {
      const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
      await page.goto(`http://127.0.0.1:${server.port}/`);
      await page.waitForFunction('window.ready');
      // Each case's drags as the window heard them, cancelled or not, and what moved. A recognizer that cancels the
      // mouse's `pointerdown` leaves the browser no drag to begin. A press inside a pan leaves it one: the draggable
      // node's around the pan, which the listener cancels for the press, or a link's own in Chromium, which the pan
      // cancels. Another script's draggable inside the pan keeps its own.
      const link = (id) => (engine === 'webkit' ? [] : [`${id} cancelled`]);
      const cases = [['#pan2', [0, 25], [], 'pan'], ['#pan2press', [0, 0], ['outer cancelled'], 'pan'],
        ['#pan2tap', [0, 0], ['outer cancelled'], 'pan'],
        ['#pan2link', [0, 0], link('pan2link'), 'pan'],
        ['#pan2cardlink', [0, 0], link('pan2cardlink'), 'pan'], ['#pan2svg', [0, 0], ["pan2svg: pan2svg's own"], ''],
        ['#pan2presssvg', [0, 0], ["pan2presssvg: pan2presssvg's own"], ''], ['#pan2bare', [0, 0], [], 'pan'],
        ['#height', [0, 0], [], 'height-begin'], ['#handle', [0, 0], [], 'transform-begin'],
        ['#height2', [0, 0], [], 'height-begin'], ['#handle2', [0, 0], [], 'transform-begin'], ['#panned', [0, 0], [], 'pan']];
      const seen = [];
      for (const [selector, from] of cases) {
        const result = await drag(page, selector, [0, 30], { from });
        seen.push([selector, from, result.drags.map((d) => d.target + (d.prevented ? ' cancelled' : `: ${d.text}`)),
          result.deltas ? 'pan' : result.begins.join()]);
      }
      expect(seen).toEqual(cases);
    } finally {
      await browser.close();
    }
  }, 60_000);
}

need('chromium', chromium().unavailable).each([
  ['in the page', false],
  ['moved into a frame before capture', true],
])('chromium: a draggable form with a control named getRootNode %s', async (_, move) => {
  const browser = await suiteBrowser('chromium');
  try {
    const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
    await page.goto(`http://127.0.0.1:${server.port}/`);
    await page.waitForFunction('window.ready');
    const shadowed = await page.evaluate((move) => {
      const form = document.createElement('form'), input = document.createElement('input');
      form.id = 'drag-form';
      form.style.top = '520px';
      form.setAttribute('data-datatransfertext', 'form');
      input.type = 'hidden';
      input.name = 'getRootNode';
      form.append(input);
      document.getElementById('exact-root').append(form);
      writeDraggable(form, 'true');
      if (move) {
        const frame = document.createElement('iframe');
        frame.hidden = true;
        document.body.append(frame);
        window.addEventListener('dragstart', () => frame.contentDocument.body.append(form), { capture: true, once: true });
      }
      form.addEventListener('dragstart', (event) => {
        if (move) {
          window.formDrag = [event.defaultPrevented, event.dataTransfer.getData('text/plain')];
          event.stopPropagation();
        } else {
          event.dataTransfer.setData('text/plain', 'target listener');
        }
      });
      return form.getRootNode === input;
    }, move);
    expect(shadowed).toBe(true);
    const seen = await drag(page, '#drag-form', [0, 20]);
    if (move) expect(await page.evaluate(() => window.formDrag)).toEqual([true, '']);
    else expect(went(seen).map((drag) => drag.text)).toEqual(['form']);
  } finally {
    await browser.close();
  }
});

need('chromium', chromium().unavailable).each([
  ['only its image moves into a frame', { move: 'image' }],
  ['the entire card with a slotted image moves into a frame', { move: 'card' }],
  ['only its image moves into a frame while the wasm listener loads', { move: 'image', waiting: true }],
  ['the entire card with a slotted image moves into a frame’s shadow root', { move: 'card', to: 'shadow' }],
  ['the entire card with a slotted image moves into a frame’s shadow root while the wasm listener loads',
    { move: 'card', to: 'shadow', waiting: true }],
  ['only its image moves to the same document’s body while the wasm listener loads', { move: 'image', to: 'body', waiting: true }],
  ['a script-owned wrapper moves into a frame’s shadow root', { move: 'wrapper', to: 'shadow' }],
  ['a script-owned wrapper moves into a frame’s shadow root while the wasm listener loads',
    { move: 'wrapper', to: 'shadow', waiting: true }],
])('chromium: capture checks the image drag owner after %s', async (_, { move, to, waiting }) => {
  const browser = await suiteBrowser('chromium');
  try {
    const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
    await page.goto(`http://127.0.0.1:${server.port}/${waiting ? '?waiting' : ''}`);
    await page.waitForFunction('window.ready');
    const draggable = await page.evaluate(async ({ move, to, waiting }) => {
      const card = document.getElementById('card2'), image = document.getElementById('picture');
      card.replaceChildren(image);
      if (waiting) {
        const { waitForDrag } = await import('./waiting-drag.js');
        waitForDrag(card);
      }
      let wrapper = null;
      if (move !== 'image') {
        wrapper = document.createElement('div');
        wrapper.draggable = true;
        if (move === 'card') {
          wrapper.append(document.createElement('slot'));
          card.attachShadow({ mode: 'open' }).append(wrapper);
        } else {
          wrapper.append(image);
          card.append(wrapper);
        }
      }
      let destination = document.body;
      if (to !== 'body') {
        const frame = document.createElement('iframe');
        frame.hidden = true;
        document.body.append(frame);
        destination = frame.contentDocument.body;
        if (to === 'shadow') destination = destination.attachShadow({ mode: 'open' });
      }
      const moved = { card, image, wrapper }[move];
      window.addEventListener('dragstart', () => destination.append(moved), { capture: true, once: true });
      image.addEventListener('dragstart', (event) => {
        window.imageDrag = {
          imageTarget: event.target === image,
          wrapperOnPath: !wrapper || event.composedPath().includes(wrapper),
          moved: moved.parentNode === destination,
          connected: image.isConnected,
          cardInDocument: card.ownerDocument === document,
          cancelled: event.defaultPrevented,
          text: event.dataTransfer.getData('text/plain'),
        };
        event.stopPropagation();
      });
      return card.draggable;
    }, { move, to, waiting });
    expect(draggable).toBe(!waiting);
    await drag(page, '#picture', [0, 20]);
    expect(await page.evaluate(() => window.imageDrag)).toEqual({
      imageTarget: true, wrapperOnPath: true, moved: true, connected: true,
      cardInDocument: move !== 'card', cancelled: move !== 'wrapper', text: '',
    });
  } finally {
    await browser.close();
  }
});

// Chrome's own DragData for a drag out of the page, intercepted under the DevTools protocol (`Input.setInterceptDrags`)
// with the mouse sent through it, so no test helper reads the page's `dataTransfer` in its place: the stage 4 exit of
// LLP 1094.000 §5.
need('chromium', chromium().unavailable)("chromium: Chrome's DragData is the node's declaration, a copy", async () => {
  const browser = await suiteBrowser('chromium');
  try {
    const page = await browser.newPage({ viewport: { width: 800, height: 600 } });
    await page.goto(`http://127.0.0.1:${server.port}/`);
    await page.waitForFunction('window.ready');
    await page.evaluate(() => {
      const card = document.getElementById('card');
      card.setAttribute('data-datatransfertext', 'Mount Fuji');
      card.setAttribute('data-datatransferurl', 'https://example.com/1016');
    });
    const cdp = await page.context().newCDPSession(page);
    await cdp.send('Input.setInterceptDrags', { enabled: true });
    const intercepted = new Promise((resolve) => cdp.on('Input.dragIntercepted', resolve));
    const box = await page.locator('#label').boundingBox(), [x, y] = [box.x + 20, box.y + 20];
    const mouse = (type, dy, buttons) => cdp.send('Input.dispatchMouseEvent', { type, x, y: y + dy, button: 'left', buttons, clickCount: 1 });
    await mouse('mouseMoved', 0, 0);
    await mouse('mousePressed', 0, 1);
    for (let dy = 3; dy <= 30; dy += 3) await mouse('mouseMoved', dy, 1);
    const { data } = await intercepted;
    const file = `http://127.0.0.1:${server.port}/assets/a.png`;
    expect(data.items.map(({ mimeType, data }) => [mimeType, data])).toEqual([
      ['text/plain', 'Mount Fuji'],
      ['text/uri-list', `https://example.com/1016\r\n${file}`],
      ['text/html', `<img src="${file}">`],
      ['downloadurl', `image/png:a.png:${file}`],
    ]);
    expect(data.files ?? []).toEqual([]);
    expect(data.dragOperationsMask).toBe(1);
  } finally {
    await browser.close();
  }
}, 60_000);
