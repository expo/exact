import { glueDeclarations, glueCase, glueStandIns } from './glue-fixture.mjs';
import { hostFixtureFile as readFileSync } from './glue-fixture.mjs';
// The production presence controller under Chrome's layout and animation
// clocks, including the host's resize entry and child reconciliation.
import { beforeAll, afterAll, test, expect } from 'bun:test';
import { existsSync } from 'node:fs';
import { createServer } from 'node:http';
import { resolve } from 'node:path';
import { cdpBrowser, chromium } from '../../../scripts/agent-launch.mjs';
import { comparePresence } from '../parity.mjs';
import { need } from '../../../scripts/coverage.mjs';
import { readyPage } from './page.mjs';
import { springCurve } from './motion-scenarios.mjs';
import { fileURLToPath } from 'node:url';

test('the timeline comparator rejects missing ghosts, drift, and invalid recordings', () => {
  const node = { x: 10, y: 20, w: 100, h: 40, opacity: .5, exiting: true };
  const reference = { steps: [{ advance: 100 }], samples: [{ name: 'exit-200', clock: 200, nodes: { card: node, sibling: { ...node, exiting: false } } }] };
  const changed = change => { const other = structuredClone(reference); change(other); return comparePresence(reference, other); };
  expect(comparePresence(reference, reference)).toEqual([]);
  expect(changed(r => { r.samples[0].nodes.card.x += .09; r.samples[0].nodes.card.opacity += .004; })).toEqual([]);
  for (const change of [
    r => { r.samples = []; },
    r => { r.steps[0].advance = 101; },
    r => { r.samples[0].clock++; },
    r => { r.samples[0].nodes.card = null; },
    r => { delete r.samples[0].nodes.sibling; },
    r => { r.samples[0].nodes.card.h += 1; },
    r => { r.samples[0].nodes.card.opacity += .02; },
    r => { r.samples[0].nodes.card.x = NaN; },
    r => { r.samples[0].nodes.card.exiting = false; },
  ]) expect(changed(change).length).toBeGreaterThan(0);
  expect(comparePresence({ samples: [] }, { samples: [] }).length).toBeGreaterThan(0);
});

const WEB = resolve(fileURLToPath(new URL('..', import.meta.url)));
const { unavailable } = chromium();
const check = need('chrome', unavailable);
const glue = readFileSync(resolve(WEB, 'glue.js'), 'utf8');
const children = glueCase(glue, 'children');
const resizeNames = ['resizeRunner', 'mediaChanged', 'pageChanged'];
const resize = glueDeclarations(glue, resizeNames), resizeStandIns = glueStandIns(glue, resizeNames, ['presenceLoader', 'animationClock', 'settleValue']);
const page = `<style>
  body { margin:0 } #exact-root { position:relative; padding:20px }
  #card { width:100px; height:40px; background:red; --exact-layout-transition:1000 0 linear }
  #card[data-pressed] { transform:scale(.8) }
  @keyframes pulse { from { opacity:1 } to { opacity:.2 } }
</style><div id="exact-root"></div>
<script type="module">
  import { presenceLoader, animationClock, settleValue } from './navigation.js';
  ${resizeStandIns}
  {
  window.exact = {};
  const { shownClip } = await import('./presence-glue.js');
  window.shownClip = shownClip;
  const root = document.getElementById('exact-root'), views = new Map();
  const presence = presenceLoader(async () => exact.presence, root, batch => applyBatch(batch), () => {});
  presence.hold({ ops:[{ op:'style', css:'--exact-layout-transition:1000 0 linear' }] });
  await Promise.resolve();
  const clock = animationClock(() => 0, () => 0, () => presence.live.sync?.());
  const viewFor = (_, id) => views.get(id);
  function applyBatch(batch) {
    presence.live.before(batch, views);
    for (const op of batch.ops ?? []) {
      switch (op.op) {
      case 'style': views.get(op.id).style.cssText = op.css; break;
      ${children}
      }
    }
    presence.live.after(batch, views);
  }
  const readOut = x => x, now = () => 0, preferences = () => 0;
  const positionContexts = () => {}, onPreferences = () => {}, foldBits = () => 0, onFold = () => {}; // no fold posture (LLP 1078 D6)
  const wasm = { exact_resize: () => JSON.stringify(window.resizeBatch), exact_set_screen_reader() {} };
  const host = { wasm, incarnation: 1 };
  ${resize}
  window.fixture = html => {
    for (const a of document.getAnimations()) a.cancel();
    root.innerHTML = html; views.clear();
    for (const el of root.querySelectorAll('[id]')) views.set(el.id, el);
    applyBatch({ ops:[...views].map(([id, el]) => ({ op:'style', id, css:el.style.cssText })) });
  };
  window.batch = applyBatch;
  // The clock's time, at which the animations a batch started begin (by default the time is 0 at each batch)
  window.seek = (ms, from = 0) => { clock.register(from); clock.seek(ms); };
  window.resizePresence = batch => { window.resizeBatch = batch; mediaChanged(); };
  window.surface = () => root.querySelector('[data-exiting] > div');
  window.readPresence = () => presence.live.observation();
  window.tracking = () => presence.live.tracking();
  window.leave = css => presence.live.exit(document.getElementById('card'), css);
  window.ready = true;
  }
</script>`;

// Register the shared browser's cleanup in the file, outside the beforeAll hook.
const suite = unavailable ? null : await cdpBrowser({ args: ['--window-size=500,500'] });
let server, evaluate;
beforeAll(async () => {
  if (unavailable) return;
  server = createServer((req, res) => {
    if (req.url === '/favicon.ico') { res.writeHead(204); res.end(); return; }
    res.writeHead(200, { 'content-type': req.url === '/' ? 'text/html' : 'text/javascript' });
    if (req.url === '/js/names.js') { res.end('export const sourceTypes = {};'); return; }
    // `/js/` is the JS target's directory, beside this one; its modules import ours as `../web/`.
    const path = req.url.startsWith('/js/') ? req.url.slice(4) : req.url.replace(/^\/(web\/)?/, '');
    const js = resolve(WEB, '../web-js', path);
    // names.js is a build's output (the plan's names), which rt.js reaches through perf.js since db825de7b.
    if (req.url === '/js/names.js') { res.end('export const sourceTypes = {};'); return; }
    res.end(req.url === '/' ? page : readFileSync(req.url.startsWith('/js/') && existsSync(js) ? js : resolve(WEB, path)));
  });
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  const { cdp } = suite, { sessionId } = await suite.page();
  const call = (method, params) => cdp.send(method, params, sessionId);
  evaluate = async expression => {
    const reply = await call('Runtime.evaluate', { expression, returnByValue:true, awaitPromise:true });
    if (reply.exceptionDetails) throw new Error(reply.exceptionDetails.exception?.description ?? reply.exceptionDetails.text);
    return reply.result.value;
  };
  await readyPage(cdp, sessionId, `http://127.0.0.1:${server.address().port}/`);
}, 60000);
afterAll(async () => {
  await suite?.close(); server?.close();
});

const card = 'width:100px;height:40px;background:red;--exact-layout-transition:1000 0 linear';
const fixture = html => evaluate(`fixture(${JSON.stringify(html)})`);
const style = (css, id = 'card') => evaluate(`batch({ops:[{op:'style',id:${JSON.stringify(id)},css:${JSON.stringify(css)}}]})`);
// What paints on top at a point of the holder, the ghosts made hit-testable for the read
const topmost = (x, y) => evaluate(`(() => {
  const ghosts = [...document.querySelectorAll('[data-exiting]')];
  for (const g of ghosts) { g.inert = false; g.style.pointerEvents = 'auto'; }
  const r = document.getElementById('holder').getBoundingClientRect(), el = document.elementFromPoint(r.left + ${x}, r.top + ${y});
  for (const g of ghosts) { g.inert = true; g.style.pointerEvents = 'none'; }
  return el.id || el.exactExitId;
})()`);
// A motion controller from motion-glue.js that knows only the element `id`, as `window.motion`, over an engine that
// accepts every hold
const motionFor = id => evaluate(`(async () => {
  const { motionController } = await import('./motion-glue.js');
  window.motion = motionController({ views: new Map([['${id}', document.getElementById('${id}')]]), now: () => 0, generation: () => 0,
    request: facts => facts.op === 'begin' ? { token: 1, value: [0, 0] } : { accepted: true }, applyBatch: () => {}, inert: () => false });
})()`);
// Lifts the element `id` as an Arrange drag does, and reads its `z-index`
const lift = async id => {
  await motionFor(id);
  return evaluate(`motion.raiseReorder('${id}', true); getComputedStyle(document.getElementById('${id}')).zIndex`);
};

check('presence inspection reads the moving surface and the leaving ghost', async () => {
  await fixture(`<div id="card" data-view="7" style="${card}"></div>`);
  await style(card.replace('40px', '140px'));
  await evaluate('seek(250)');
  const read = () => evaluate('readPresence()');
  const moving = (await read())[0];
  expect(moving.id).toBe(7);
  expect(moving.h).toBeCloseTo(65, 2);
  expect(moving.opacity).toBe(1);
  await evaluate(`leave('pulse 1000ms linear both'); seek(500)`);
  const leaving = (await read())[0];
  expect(leaving.exiting).toBe(true);
  expect(leaving.opacity).toBeCloseTo(.6, 3);
});

// GitHub #747: the runner's batch for a commit that changes nothing holds only its clock stamp.
check('a batch that changes no tree measures no view, and the next that resizes one plays it', async () => {
  await fixture(`<div id="card" style="${card}"></div>`);
  const grown = JSON.stringify(card.replace('40px', '140px'));
  expect(await evaluate(`(() => {
    let reads = 0;
    const rect = Element.prototype.getBoundingClientRect, computed = window.getComputedStyle;
    Element.prototype.getBoundingClientRect = function () { reads++; return rect.call(this); };
    window.getComputedStyle = function (...args) { reads++; return computed.apply(this, args); };
    try {
      batch({ops:[{op:'at',ms:5}]});
      const still = reads;
      batch({ops:[{op:'at',ms:6},{op:'style',id:'card',css:${grown}}]});
      return [still, reads > still, document.getAnimations().length > 0];
    } finally { Element.prototype.getBoundingClientRect = rect; window.getComputedStyle = computed; }
  })()`)).toEqual([0, true, true]);
});

// A removed row stays in the set of declaring views, with its subtree, until a batch drops it; the agent's clock
// steps by that set (`tracking`).
check('removed views are tracked no longer when the batches after them change no tree', async () => {
  await fixture(`<div id="list"><div id="card" style="${card}"></div><div id="next" style="${card}"></div></div>`);
  expect(await evaluate(`(() => {
    const was = tracking();
    batch({ops:[{op:'at',ms:7},{op:'children',id:'list',ids:[]}]});
    batch({ops:[{op:'at',ms:8}]});
    return [was, document.getElementById('card'), tracking()];
  })()`)).toEqual([true, null, false]);
});

check('resize snaps an active move and takes new boxes without a move', async () => {
  await fixture(`<div id="card" style="${card}"></div>`);
  await style(card.replace('40px', '140px'));
  await evaluate('seek(200)');
  expect(await evaluate('document.getAnimations().length')).toBeGreaterThan(0);
  await evaluate(`resizePresence({ops:[{op:'style',id:'card',css:${JSON.stringify(card.replace('40px', '200px'))}}]})`);
  expect(await evaluate('document.getAnimations().length')).toBe(0);
  expect(await evaluate('surface()')).toBeNull();
  expect(await evaluate('document.getElementById("card").offsetHeight')).toBe(200);
});

check('a move adds its translate onto a zero translate added under it, and a turned and scaled box moves in its parent\'s space', async () => {
  // WebKit paints a translate added to `none` farther from the box than it places it, and one added to `0px 0px` where
  // it places it
  const turned = `${card};position:absolute;top:100px;rotate:90deg;scale:2`;
  const centre = () => evaluate('(r => [Math.round(r.left + r.width / 2), Math.round(r.top + r.height / 2)])(document.getElementById("card").getBoundingClientRect())');
  await fixture(`<div id="card" style="${turned};left:0px"></div>`);
  const from = await centre();
  await style(`${turned};left:100px`);
  await evaluate('seek(0)');
  expect(await centre()).toEqual(from);
  await evaluate('seek(500)');
  expect(await centre()).toEqual([from[0] + 50, from[1]]);
  expect(await evaluate(`document.getElementById('card').getAnimations().filter(a => a.effect.getKeyframes().length).map(a => [a.effect.getKeyframes()[0].translate, a.effect.composite])`))
    .toEqual([['0px', 'add'], ['-100px', 'add']]);
});

check('the zero translate under a move is held at its first frame, and goes with the move however it ends', async () => {
  // Every animation the card has, and those that show something
  const parts = 'document.getElementById("card").getAnimations().length', showing = 'document.getElementById("card").getAnimations().filter(a => a.effect.getKeyframes().length).length';
  const pin = '(r => [r.left, r.top])(document.getElementById("pin").getBoundingClientRect())';
  await fixture(`<div id="card" style="${card}"><div id="pin" style="position:fixed;left:0;top:0;width:10px;height:10px"></div></div>`);
  await style(card + ';margin-left:100px');
  expect(await evaluate(`document.getElementById('card').getAnimations().filter(a => a.effect.getKeyframes().length).map(a => [a.id, a.effect.getKeyframes()[0].translate, a.effect.composite, a.playState === 'paused'])`))
    .toEqual([['exact-layout-base', '0px', 'add', true], ['exact-layout-move', '-100px', 'add', false]]);
  // While the move plays, the moving card contains its fixed child, as a translated box does
  expect(await evaluate(pin)).not.toEqual([0, 0]);
  // The page's clock seeks the move, not the zero, which stays at its first frame; and the zero still applies when
  // something seeks every animation the card has
  await evaluate('seek(400)');
  expect(await evaluate(`document.getAnimations().find(a => a.id === 'exact-layout-base').currentTime`)).toBe(0);
  expect(await evaluate(`(card => { for (const a of card.getAnimations()) { a.pause(); a.currentTime = 400; } return card.getAnimations().map(a => a.id); })(document.getElementById('card'))`))
    .toEqual(expect.arrayContaining(['exact-layout-base', 'exact-layout-move']));
  await evaluate(`document.getAnimations().find(a => a.id === 'exact-layout-move').play()`);
  // Paused and sought past its end, the move shows nothing more, and the next frame lets it and the zero go with no batch
  expect(await evaluate(`(async () => {
    const move = document.getAnimations().find(a => a.id === 'exact-layout-move');
    move.pause(); move.currentTime = 5000;
    return new Promise(done => requestAnimationFrame(() => done([${parts}, ${pin}])));
  })()`)).toEqual([0, [0, 0]]);
  // Cancelled on its own, it takes the zero with it
  await style(card + ';margin-left:0px');
  expect(await evaluate(showing)).toBe(2);
  // (Read once the cancellation has settled, before a frame could retire the move)
  expect(await evaluate(`(async () => { const move = document.getAnimations().find(a => a.id === 'exact-layout-move'), settled = move.finished.catch(() => {}); move.cancel(); await settled; return ${parts}; })()`)).toBe(0);
  // Paused and taken off its element, it goes with its zero at the next frame
  await style(card + ';margin-left:100px');
  expect(await evaluate(`(() => {
    const move = document.getAnimations().find(a => a.id === 'exact-layout-move');
    move.pause(); move.effect.target = null;
    return new Promise(done => requestAnimationFrame(() => done(${parts})));
  })()`)).toBe(0);
});

check('an older translate animation the box still shows stays when something plays every animation it has', async () => {
  // A finished animation that fills is replaced by later finished, filling ones on the same property; the zero under a
  // move never finishes, played or not
  await fixture(`<div id="card" style="${card}"></div>`);
  expect(await evaluate(`(async () => {
    const card = document.getElementById('card');
    const older = window.older = card.animate([{ translate: '0px 0px' }, { translate: '30px 0px' }], { duration: 1, fill: 'forwards' });
    await older.finished;
    return older.replaceState;
  })()`)).toBe('active');
  await style(card + ';margin-left:100px');
  expect(await evaluate(`(async () => {
    for (const a of document.getElementById('card').getAnimations()) a.play();
    await new Promise(requestAnimationFrame); await new Promise(requestAnimationFrame);
    return [older.replaceState, document.getAnimations().find(a => a.id === 'exact-layout-base').playState];
  })()`)).toEqual(['active', 'running']);
});

check('a move sought past its end adds nothing to the next one', async () => {
  await fixture(`<div id="card" style="${card}"></div>`);
  await style(card + ';margin-left:100px');
  // In one task, so that no frame retires the move first
  const [shown, start] = await evaluate(`(() => {
    const card = document.getElementById('card'), move = document.getAnimations().find(a => a.id === 'exact-layout-move');
    move.pause(); move.currentTime = move.effect.getComputedTiming().endTime + 100;
    const shown = card.getBoundingClientRect().x;
    batch({ops:[{op:'style',id:'card',css:${JSON.stringify(card + ';margin-left:200px')}}]});
    seek(0);
    return [shown, card.getBoundingClientRect().x];
  })()`);
  expect(start).toBeCloseTo(shown, 1);
});

check('a batch after a move shows nothing more reads no box in the box the move contained it in', async () => {
  const pin = left => `position:fixed;left:${left};top:0;width:10px;height:10px;--exact-layout-transition:1000 0 linear`;
  await fixture(`<div id="card" style="${card}"><div id="pin" style="${pin('50%')}"></div></div>`);
  await style(card + ';margin-left:100px');
  // (The fixed child, read in its parent, takes a move of its own here, which ends now)
  await evaluate(`document.getElementById('pin').getAnimations().forEach(a => a.finish())`);
  // In one task: the card's move is sought past its end, and the batch then moves its fixed child, which starts from
  // where the viewport places it, as the card no longer contains it
  expect(await evaluate(`(() => {
    const move = document.getAnimations().find(a => a.id === 'exact-layout-move' && a.effect.target.id === 'card');
    move.pause(); move.currentTime = 5000;
    batch({ops:[{op:'style',id:'pin',css:${JSON.stringify(pin('75%'))}}]});
    seek(0);
    return Math.round(document.getElementById('pin').getBoundingClientRect().left - innerWidth / 2);
  })()`)).toBe(0);
});

check('a row the browser refuses ends the moves under way, and an easing retargeted to where the box is shown plays nothing', async () => {
  const x = () => evaluate('document.getElementById("card").getBoundingClientRect().x');
  const moving = () => evaluate('document.getElementById("card").getAnimations().length');
  await fixture(`<div id="card" style="${card}"></div>`);
  const start = await x();
  await style(card + ';margin-left:100px');
  await evaluate('seek(500)');
  // Halfway, retargeted to 50 points, where it is shown
  await style(card + ';margin-left:50px');
  await evaluate('seek(500, 500)');
  expect([await x() - start, await moving()]).toEqual([50, 0]);

  await style(card + ';margin-left:150px');
  await evaluate('seek(1000, 500)');
  // (`steps(0)` is no easing, as an overflowed curve is none)
  await style(card.replace('1000 0 linear', '1000 0 steps(0)') + ';margin-left:200px');
  await evaluate('seek(1000, 1000)');
  expect([await x() - start, await moving()]).toEqual([200, 0]);
  // A refused row alone, the box laid where it was, ends its move too
  await style(card + ';margin-left:300px');
  await evaluate('seek(1500, 1000)');
  await style(card.replace('1000 0 linear', '1000 0 steps(0)') + ';margin-left:300px');
  await evaluate('seek(1500, 1500)');
  expect([await x() - start, await moving()]).toEqual([300, 0]);
  // On a curve that reaches far past its end, even a two-hundredth of a point left over is a move to see: shown at
  // 50.005 halfway to 100 and retargeted to 50, the box goes back almost to 0 at three quarters
  const far = card.replace('1000 0 linear', '1000 0 linear(0, 0.50005 50%, 10000 75%, 1)');
  await style(far + ';margin-left:0px');
  await evaluate('seek(3000, 2000)');
  await style(far + ';margin-left:100px');
  await evaluate('seek(3500, 3000)');
  await style(far + ';margin-left:50px');
  await evaluate('seek(4250, 3500)');
  expect(Math.abs(await x() - start)).toBeLessThan(1);
  // At 70 percent, where the share left is a float that leaves no exact zero, the same as halfway
  await fixture(`<div id="card" style="${card}"></div>`);
  await style(card + ';margin-left:100px');
  await evaluate('seek(700)');
  await style(card + ';margin-left:70px');
  await evaluate('seek(700, 700)');
  expect([Math.round(await x() - start), await moving()]).toEqual([70, 0]);
});

check('a stand-in goes with the last move that changes the size, and the moves left go on on their clocks', async () => {
  const { ms, easing } = springCurve([300, 30, 1]), spring = card.replace('1000 0 linear', `${ms} 0 spring ${easing}`);
  const x = () => evaluate('document.getElementById("card").getBoundingClientRect().x');
  await fixture(`<div id="card" style="${spring}"></div>`);
  await style(spring.replace('40px', '140px'));
  // A move of the place alone 100 ms before the resize ends, read 5 ms either side of that end
  const begun = ms - 100;
  await evaluate(`seek(${begun})`);
  await style(spring.replace('40px', '140px') + ';margin-left:100px');
  await evaluate(`seek(${ms - 5}, ${begun})`);
  const before = await x();
  await evaluate(`seek(${ms + 5}, ${begun})`);
  const after = await x();
  expect(await evaluate(`(card => [document.querySelectorAll('[data-exiting]').length, getComputedStyle(card).backgroundColor])(document.getElementById('card'))`))
    .toEqual([0, 'rgb(255, 0, 0)']);
  // Its move kept its clock: 10 ms on it is a few points further, not back at its start
  expect(after - before).toBeGreaterThan(0);
  expect(after - before).toBeLessThan(10);
});

check('a padding that moves a scaled box\'s content-box origin, and not its laid box, starts no move', async () => {
  // Scaled about a point of its content box, at a length, a percentage or both, a box shows its border box where the
  // padding puts that point
  for (const at of ['0 0', '50% 50%', 'calc(10px + 50%) 50%']) {
    const scaled = `${card};box-sizing:border-box;scale:2;transform-box:content-box;transform-origin:${at}`;
    await fixture(`<div id="card" style="${scaled}"></div>`);
    await style(scaled + ';padding-left:20px');
    expect([at, await evaluate('document.getElementById("card").getAnimations().filter(a => a.effect.getKeyframes().length).length')]).toEqual([at, 0]);
  }
});

check('a box inside a moving box moves with it though a box that is none lies between them', async () => {
  // A parent and its child both declare the row, with a `display: contents` wrapper between: the child, laid where it
  // was in its parent, makes no move of its own
  const child = 'position:absolute;left:10px;top:10px;width:20px;height:20px;--exact-layout-transition:1000 0 linear';
  // (A turn on the wrapper paints nothing, as it makes no box)
  await fixture(`<div id="card" style="${card};position:relative"><div id="wrap" style="display:contents;rotate:180deg"><div id="child" style="${child}"></div></div></div>`);
  const offset = () => evaluate(`(c => [Math.round(c.getBoundingClientRect().left - document.getElementById('card').getBoundingClientRect().left), c.getAnimations().filter(a => a.effect.getKeyframes().length).length])(document.getElementById('child'))`);
  await style(card + ';position:relative;margin-left:100px');
  await evaluate('seek(0)');
  expect(await offset()).toEqual([10, 0]);
  await evaluate('seek(500)');
  expect(await offset()).toEqual([10, 0]);
  // Widened where it is laid, it moves no edge it keeps
  await style(child.replace('width:20px', 'width:60px'), 'child');
  await evaluate('seek(0)');
  expect((await offset())[0]).toBe(10);
});

check('a box moved within the box it is measured in keeps its moves, and one whose measuring box ceases to be ends them', async () => {
  const box = 'position:absolute;left:0;top:0;width:20px;height:20px;background:red;--exact-layout-transition:1000 0 linear';
  const width = () => evaluate('surface()?.getBoundingClientRect().width ?? document.getElementById("box").getBoundingClientRect().width');
  // Moved halfway through its resize from one `display: contents` wrapper to another in the card it is measured in,
  // it goes on resizing, its stand-in beside it
  await fixture(`<div id="card" style="position:relative;width:200px;height:100px"><div id="one" style="display:contents"><div id="box" style="${box}"></div></div><div id="two" style="display:contents"></div></div>`);
  await style(box.replace('width:20px', 'width:100px'), 'box');
  await evaluate('seek(500)');
  await evaluate(`batch({ops:[{op:'children',id:'two',ids:['box']}]}); seek(500)`);
  expect(Math.round(await width())).toBe(60);
  // Moved into a box scaled twice about its corner within the card, it starts where it was: its move is in the scaled
  // box's points, half the card's
  const scaled = 'position:absolute;left:50px;top:0;width:100px;height:50px;scale:2;transform-origin:0 0';
  await fixture(`<div id="card" style="position:relative;width:200px;height:100px"><div id="box" style="${box}"></div><div id="wrap" style="${scaled}"></div></div>`);
  const left = () => evaluate(`Math.round(document.getElementById('box').getBoundingClientRect().left - document.getElementById('card').getBoundingClientRect().left)`);
  await evaluate(`batch({ops:[{op:'children',id:'wrap',ids:['box']}]}); seek(0)`);
  expect(await left()).toBe(0);
  await evaluate('seek(1000)');
  expect(await left()).toBe(50);
  // Its measuring box made none (`display: contents`) halfway through a move, the move ends
  await fixture(`<div id="card" style="position:relative;width:200px;height:100px"><div id="wrap" style="position:relative;width:100px;height:50px"><div id="box" style="${box}"></div></div></div>`);
  await style(box.replace('left:0', 'left:80px'), 'box');
  await evaluate('seek(500)');
  await style('display:contents', 'wrap');
  expect(await evaluate('document.getElementById("box").getAnimations().filter(a => a.effect.getKeyframes().length).length')).toBe(0);
});

check('a box a batch moves to another parent ends its moves there, and its stand-in with them', async () => {
  await fixture(`<div id="a" style="position:relative;height:100px"><div id="card" style="${card}"></div></div><div id="b" style="position:relative;margin-left:40px;height:100px"></div>`);
  await style(card.replace('40px', '140px'));
  await evaluate('seek(500)');
  await evaluate(`batch({ops:[{op:'children',id:'b',ids:['card']},{op:'style',id:'card',css:${JSON.stringify(card.replace('40px', '120px'))}}]}); seek(500, 500)`);
  // At once at its place in B: its top-left on B's, at its new height
  expect(await evaluate(`(card => { const r = card.getBoundingClientRect(), b = card.parentElement.getBoundingClientRect(); return [card.parentElement.id, card.getAnimations().length, document.querySelectorAll('[data-exiting]').length, r.left - b.left, r.top - b.top, r.height]; })(document.getElementById('card'))`))
    .toEqual(['b', 0, 0, 0, 0, 120]);
});

check('an interrupted spring goes on under a delayed next move, from where it is shown', async () => {
  const { ms, easing } = springCurve([300, 30, 1]), spring = card.replace('1000 0 linear', `${ms} 100 spring ${easing}`);
  await fixture(`<div id="card" style="${spring}"></div>`);
  await style(spring + ';margin-left:100px');
  // Retargeted 30 ms into its spring, by a move that waits 100 ms
  await evaluate('seek(130)');
  const before = await evaluate('document.getElementById("card").getBoundingClientRect().x');
  await style(spring + ';margin-left:180px');
  await evaluate('seek(130, 130)');
  expect(await evaluate('document.getElementById("card").getBoundingClientRect().x')).toBeCloseTo(before, 1);
  // The first move, going on, carries the card right while the new one waits
  await evaluate('seek(220, 130)');
  const later = await evaluate('document.getElementById("card").getBoundingClientRect().x');
  expect(later - before).toBeGreaterThan(20);
});

check('a growing surface follows live paint, opacity and pressed transforms', async () => {
  await fixture(`<div id="card" style="${card}"></div>`);
  await style(card.replace('40px', '140px'));
  await evaluate(`document.getElementById('card').animate([
    {backgroundColor:'red',borderColor:'red',boxShadow:'0px 0px 4px red',opacity:1},
    {backgroundColor:'blue',borderColor:'blue',boxShadow:'0px 0px 4px blue',opacity:.2}
  ], {duration:1000,fill:'both'}); seek(500);
  document.getElementById('card').setAttribute('data-pressed','');
  new Promise(requestAnimationFrame);`);
  expect(await evaluate('getComputedStyle(surface()).backgroundColor')).toBe('rgb(128, 0, 128)');
  expect(await evaluate('getComputedStyle(document.getElementById("card")).backgroundColor')).toBe('rgba(0, 0, 0, 0)');
  expect(await evaluate('getComputedStyle(surface()).opacity')).toBe('0.6');
  expect(await evaluate('getComputedStyle(surface()).boxShadow')).toContain('rgb(128, 0, 128)');
  expect(await evaluate('new DOMMatrix(getComputedStyle(surface()).transform).a')).toBe(0.8);
  await style(card.replace('40px', '140px') + ';border:2px solid green;border-radius:12px');
  await evaluate('seek(500)');
  expect(await evaluate('getComputedStyle(surface()).borderTopWidth')).toBe('2px');
});

check('a resizing surface carries the box\'s backdrop filter, which the box drops while the surface draws', async () => {
  const blurred = card + ';background:#0008;backdrop-filter:blur(20px)';
  await fixture(`<div id="card" style="${blurred}"></div>`);
  await style(blurred.replace('40px', '140px'));
  await evaluate('seek(500)');
  expect(await evaluate('getComputedStyle(surface()).backdropFilter')).toBe('blur(20px)');
  expect(await evaluate('getComputedStyle(document.getElementById("card")).backdropFilter')).toBe('none');
  await evaluate('document.getAnimations().forEach(a => a.finish()); Promise.resolve()');
  expect(await evaluate('getComputedStyle(document.getElementById("card")).backdropFilter')).toBe('blur(20px)');
});

check('paint transitions retain their clocks and restore their paint when size motion ends', async () => {
  const css = card + ';border:2px solid red;transition:background-color 1s linear,border-color 1s linear,opacity 1s linear';
  await fixture(`<div id="card" style="${css}"></div>`);
  await style(css.replaceAll('red', 'blue').replace('40px', '140px') + ';opacity:.2');
  await evaluate('seek(500)');
  expect(await evaluate('getComputedStyle(surface()).backgroundColor')).toBe('rgb(128, 0, 128)');
  expect(await evaluate('getComputedStyle(surface()).borderTopColor')).toBe('rgb(128, 0, 128)');
  expect(await evaluate('getComputedStyle(surface()).opacity')).toBe('0.6');
  expect(await evaluate('getComputedStyle(document.getElementById("card")).backgroundColor')).toBe('rgba(0, 0, 0, 0)');
  // A style-only update during the move must start from the paint on screen.
  await style(css.replaceAll('red', 'green').replace('40px', '140px') + ';opacity:.2');
  await evaluate('seek(0)');
  expect(await evaluate('getComputedStyle(surface()).backgroundColor')).toBe('rgb(128, 0, 128)');
  await evaluate('document.getAnimations().forEach(a => a.finish()); Promise.resolve()');
  expect(await evaluate('getComputedStyle(document.getElementById("card")).backgroundColor')).toBe('rgb(0, 128, 0)');
});

check('a shrinking clip reveals children up to the shown box, then restores overflow', async () => {
  await fixture(`<div id="card" style="${card.replace('40px', '140px')};overflow:hidden"><div id="body" style="height:140px;width:100px;background:blue"></div></div>`);
  await style(card + ';overflow:hidden');
  await evaluate('seek(500)');
  expect(await evaluate('document.elementFromPoint(40,90).id')).toBe('body');
  expect(await evaluate('document.elementFromPoint(40,120).id')).not.toBe('body');
  await evaluate('document.getAnimations().forEach(a => a.finish()); Promise.resolve()');
  expect(await evaluate('getComputedStyle(document.getElementById("card")).overflow')).toBe('hidden');
  expect(await evaluate('surface()')).toBeNull();
});

check('a shrinking clip that `hidden` makes a scroll container keeps clipping once a box in it turns sticky', async () => {
  const body = 'height:120px;width:100px;background:blue', sticky = ';position:sticky;top:0';
  // Which box turns sticky, how the card clips, and whether it goes on revealing its body: under `hidden` any shown
  // sticky box keeps the card clipping at its laid-out box, one in a scroller inside it too, since one might stick to
  // the card; `clip` makes no scroll container, and a box that is not shown sticks to nothing
  for (const [turns, overflow, reveals] of [['header', 'hidden', false], ['body', 'hidden', false], ['ghost', 'hidden', true], ['body', 'clip', true]]) {
    const css = { header: 'height:10px', body, ghost: 'display:none' };
    await fixture(`<div id="card" style="${card.replace('40px', '140px')};overflow:${overflow}"><div id="panel" style="overflow:auto;height:20px;width:100px"><div id="header" style="${css.header}"></div></div><div id="ghost" style="${css.ghost}"></div><div id="body" style="${body}"></div></div>`);
    await style(card + `;overflow:${overflow}`);
    await evaluate('seek(500)');
    // Halfway, 90 points shown, a box inside it turns sticky
    await style(css[turns] + sticky, turns);
    await evaluate('seek(500)');
    expect([turns, overflow, await evaluate('document.elementFromPoint(40, 70).id') === 'body']).toEqual([turns, overflow, reveals]);
  }
});

check('an easing restarts with what the moves under way had left, however little, and with nothing when that cancels', async () => {
  const x = () => evaluate('document.getElementById("card").getBoundingClientRect().x');
  // Halfway from 0 to 128 on a curve 2^-48 along there: retargeted to 0, 2^-41 points are left, which three quarters
  // into the restart, 2^48 along, are 128 points
  const far = card.replace('1000 0 linear', `1000 0 linear(0, ${2 ** -48} 50%, ${2 ** 48} 75%, 1)`);
  await fixture(`<div id="card" style="${far}"></div>`);
  const start = await x();
  await style(far + ';margin-left:128px');
  await evaluate('seek(500)');
  await style(far + ';margin-left:0px');
  await evaluate('seek(1250, 500)');
  expect(Math.round(await x() - start)).toBe(-128);
  // Halfway from 0 to 128 on a curve 2^-48 past halfway there, retargeted to 64: 2^-41 points are left, which at 87.5
  // percent of the restart, 2^48 along, are 128 points back
  const back = card.replace('1000 0 linear', `1000 0 linear(0 0%, ${2 ** -47} 25%, 1 75%, ${2 ** 48} 87.5%, 1 100%)`);
  await fixture(`<div id="card" style="${back}"></div>`);
  await style(back + ';margin-left:128px');
  await evaluate('seek(500)');
  await style(back + ';margin-left:64px');
  await evaluate('seek(1375, 500)');
  expect(Math.round(await x() - start)).toBe(-64);
  // At 9,999 ms of 10,000, 9,999 points along, retargeted there: nothing to play, though the share is a float
  const long = card.replace('1000 0 linear', '10000 0 linear');
  await fixture(`<div id="card" style="${long}"></div>`);
  await style(long + ';margin-left:10000px');
  await evaluate('seek(9999)');
  await style(long + ';margin-left:9999px');
  await evaluate('seek(9999, 9999)');
  expect([Math.round(await x() - start), await evaluate('document.getElementById("card").getAnimations().length')]).toEqual([9999, 0]);
});

check('a native button keeps its laid-out size and moves by its centre, which a resize around its place does not restart', async () => {
  const button = (left, width) => `position:absolute;left:${left}px;top:0;width:${width}px;height:44px;--exact-layout-transition:1000 0 linear`;
  const shown = () => evaluate(`(r => [Math.round(r.left + r.width / 2), Math.round(r.width), document.getElementById('card').getAnimations().some(a => a.id === 'exact-layout-move')])(document.getElementById('card').getBoundingClientRect())`);
  await fixture(`<button id="card" data-button-style="bordered" style="${button(0, 100)}"></button>`);
  await style(button(100, 100)); // its centre moves from 50 to 150
  await evaluate('seek(500)');
  expect(await shown()).toEqual([100, 100, true]);
  // Half way it shows from 50 to 150: what it shows there takes the press, and its destination box does not
  expect(await evaluate('[document.elementFromPoint(60, 20).id, document.elementFromPoint(190, 20).id]')).toEqual(['card', 'exact-root']);
  await style(button(125, 50)); // the same centre, a smaller button: it goes on, at its new size
  await evaluate('seek(500)');
  expect(await shown()).toEqual([100, 50, true]);
  await evaluate('seek(750)');
  expect(await shown()).toEqual([125, 50, true]);
  expect(await evaluate('surface()')).toBeNull();
});

check('a native button hidden during its move does not resume it when shown again', async () => {
  const button = (left, display = 'block') => `position:absolute;left:${left}px;top:0;width:100px;height:44px;display:${display};--exact-layout-transition:1000 0 linear`;
  const moving = () => evaluate(`document.getElementById('card').getAnimations().some(a => a.id === 'exact-layout-move')`);
  await fixture(`<button id="card" data-button-style="bordered" style="${button(0)}"></button>`);
  const none = () => evaluate(`document.getElementById('card').getAnimations().length === 0`);
  await style(button(100));
  await evaluate('seek(300)');
  expect(await moving()).toBe(true);
  await style(button(100, 'none'));
  expect(await none()).toBe(true);
  await style(button(100));
  expect(await none()).toBe(true);
});

check('a batch that changes no tree measures no moving native button, which goes on', async () => {
  const button = left => `position:absolute;left:${left}px;top:0;width:100px;height:44px;--exact-layout-transition:1000 0 linear`;
  await fixture(`<button id="card" data-button-style="bordered" style="${button(0)}"></button>`);
  await style(button(100));
  await evaluate('seek(300)');
  expect(await evaluate(`(() => {
    let reads = 0;
    const proto = Element.prototype, rect = proto.getBoundingClientRect, rects = proto.getClientRects, computed = window.getComputedStyle;
    proto.getBoundingClientRect = function () { reads++; return rect.call(this); };
    proto.getClientRects = function () { reads++; return rects.call(this); };
    window.getComputedStyle = function (...args) { reads++; return computed.apply(this, args); };
    try {
      batch({ops:[{op:'at',ms:400}]});
      return [reads, document.getElementById('card').getAnimations().some(a => a.id === 'exact-layout-move')];
    } finally { proto.getBoundingClientRect = rect; proto.getClientRects = rects; window.getComputedStyle = computed; }
  })()`)).toEqual([0, true]);
});

check('a moving native button inside an exit ghost stops when the ghost goes, with no batch after', async () => {
  const button = left => `position:absolute;left:${left}px;top:0;width:100px;height:44px;--exact-layout-transition:20000 0 linear`;
  await fixture(`<div id="card" style="${card};position:relative"><button id="tab" data-button-style="bordered" style="${button(0)}"></button></div>`);
  await style(button(100), 'tab');
  expect(await evaluate(`!!(globalThis.move = document.getElementById('tab').getAnimations().find(a => a.id === 'exact-layout-move'))`)).toBe(true);
  await evaluate(`leave('pulse 100ms linear both'); seek(200)`);
  await evaluate('new Promise(resolve => setTimeout(resolve, 50))');
  expect(await evaluate(`[document.getElementById('card'), globalThis.move.playState]`)).toEqual([null, 'idle']);
});

check('a native button whose resizing, clipping parent is hidden ends its move, though the parent held a box for it', async () => {
  const button = left => `position:absolute;left:${left}px;top:0;width:100px;height:44px;--exact-layout-transition:20000 0 linear`;
  const parent = (height, display = 'block') => `${card.replace('40px', height)};position:relative;overflow:hidden;display:${display}`;
  await fixture(`<div id="card" style="${parent('40px')}"><button id="tab" data-button-style="bordered" style="${button(0)}"></button></div>`);
  await style(button(100), 'tab');
  await style(parent('140px'));
  expect(await evaluate(`[document.getElementById('tab').getAnimations().some(a => a.id === 'exact-layout-move'), !!surface()]`)).toEqual([true, true]);
  await style(parent('140px', 'none'));
  expect(await evaluate(`document.getElementById('tab').getAnimations().length`)).toBe(0);
  await style(parent('140px'));
  expect(await evaluate(`document.getElementById('tab').getAnimations().length`)).toBe(0);
});

check('a resized box drops its surface when the paused move leaves the element', async () => {
  await fixture(`<div id="card" style="${card};overflow:hidden"><div id="body" style="height:100px;width:100px"></div></div>`);
  await style(card.replace('40px', '140px') + ';overflow:hidden');
  expect(await evaluate(`(async () => {
    seek(500);
    const card = document.getElementById('card');
    const move = card.getAnimations().find(a => a.id === 'exact-layout-move');
    const during = getComputedStyle(card).clipPath;
    move.currentTime = move.effect.getComputedTiming().endTime;
    const goneWhilePaused = move.playState === 'paused' && !card.getAnimations().includes(move);
    await new Promise(resolve => requestAnimationFrame(resolve));
    const { clipPath, overflow, backgroundColor } = getComputedStyle(card);
    return { during, goneWhilePaused, clipPath, overflow, backgroundColor,
      hit: document.elementFromPoint(40, 110).id, hasSurface: !!surface() };
  })()`)).toEqual({ during: expect.stringContaining('exact-presence-clip'), goneWhilePaused: true,
    clipPath: 'none', overflow: 'hidden', backgroundColor: 'rgb(255, 0, 0)', hit: 'body', hasSurface: false });
});

check('a same-name exit is a fresh finite play beside the unchanged infinite animation', async () => {
  for (const paused of [true, false]) {
    await fixture(`<div id="card" style="${card};animation:pulse 1s infinite linear"></div>`);
    await evaluate(`window.playing = document.getElementById('card').getAnimations()[0];
      ${paused ? 'seek(400)' : 'playing.currentTime = 400'};
      batch({ops:[{op:'exit',id:'card',css:'pulse 200ms linear both'}]});`);
    const animations = await evaluate('document.getAnimations().map(a => ({old:a === playing, time:a.currentTime, iterations:a.effect.getTiming().iterations, duration:a.effect.getTiming().duration}))');
    expect(animations.find(a => a.old)).toMatchObject({ old:true, iterations:null, duration:1000 });
    expect(animations.find(a => a.old).time).toBeGreaterThanOrEqual(400);
    expect(animations.find(a => !a.old)?.duration).toBe(200);
    await evaluate('document.getAnimations().filter(a => a !== playing).forEach(a => a.finish()); Promise.resolve()');
    expect(await evaluate('document.querySelector("[data-exiting]")')).toBeNull();
  }
});

check('reordering during a size move keeps the surface at its owner', async () => {
  await fixture(`<div id="row" style="display:flex;position:relative"><div id="card" style="${card}"></div><div id="other" style="width:50px;height:40px"></div></div>`);
  await style(card.replace('40px', '140px'));
  await evaluate('seek(300); batch({ops:[{op:"children",id:"row",ids:["other","card"]}]}); seek(0)');
  const boxes = await evaluate('(() => { const a = document.getElementById("card").getBoundingClientRect(), b = surface().getBoundingClientRect(); return [a.x,a.y,b.x,b.y]; })()');
  expect(boxes[2]).toBeCloseTo(boxes[0], 1);
  expect(boxes[3]).toBeCloseTo(boxes[1], 1);
});

check('reordering equal siblings keeps a surface anchored when its owner has the same box', async () => {
  await fixture(`<div id="row" style="position:relative">
    <div id="a" style="height:40px"></div><div id="card" style="${card}"></div><div id="b" style="height:40px"></div></div>`);
  await style(card.replace('40px', '140px'));
  await evaluate('seek(300)');
  const before = await evaluate('document.getElementById("card").getBoundingClientRect().y');
  await evaluate('batch({ops:[{op:"children",id:"row",ids:["b","card","a"]}]})');
  expect(await evaluate('document.getElementById("card").getBoundingClientRect().y')).toBe(before);
  expect(await evaluate('surface().getBoundingClientRect().y')).toBeCloseTo(before, 1);
});

check('ghosts keep their paint order among their siblings and isolate their parent until its last exit ends', async () => {
  // After the exits: `plain` and `later` (moved up over the card's box) at 0-40, the ghosts at 0-40 and 80-120.
  await fixture(`<div id="holder" style="position:relative">
    <div id="card" style="width:200px;height:40px;z-index:5"></div>
    <div id="plain" style="width:200px;height:40px"></div>
    <div id="second" style="position:relative;z-index:2;width:200px;height:40px"></div>
    <div id="later" style="position:relative;top:-40px;width:100px;height:40px"></div>
  </div>`);
  await evaluate(`batch({ops:[{op:'exit',id:'card',css:'pulse 1000ms linear both'},
    {op:'exit',id:'second',css:'pulse 2000ms linear both'}]}); seek(0)`);
  expect(await evaluate('document.getElementById("holder").style.isolation')).toBe('isolate');
  expect(await topmost(150, 20)).toBe('card'); // a positioned box over an in-flow one
  expect(await topmost(50, 20)).toBe('later'); // level 0 in tree order: the z-index it had no use for stays unused
  expect(await evaluate('[...document.querySelectorAll("[data-exiting]")].map(el => getComputedStyle(el).zIndex)')).toEqual(['auto', '2']);
  // Each ghost starts at its own last box, though the first one's exit took it out of flow before the second's
  expect(await evaluate('[...document.querySelectorAll("[data-exiting]")].map(el => el.offsetTop)')).toEqual([0, 80]);
  await evaluate(`document.getAnimations().find(a => a.effect.getTiming().duration === 1000).finish(); Promise.resolve()`);
  expect(await evaluate('document.querySelectorAll("[data-exiting]").length')).toBe(1);
  expect(await evaluate('document.getElementById("holder").style.isolation')).toBe('isolate');
  // Replacing cssText while a ghost lives cannot drop the temporary trap.
  await style('position:relative;color:blue', 'holder');
  expect(await evaluate('document.getElementById("holder").style.isolation')).toBe('isolate');
  await evaluate('document.getAnimations().forEach(a => a.finish()); Promise.resolve()');
  expect(await evaluate('document.getElementById("holder").style.isolation')).toBe('');
});

check('a ghost paints its subtree in its turn, though only its position made it a stacking context', async () => {
  // After the exit, `later` (z-index 1) lies over the card's box, where the card's child has z-index 10. The exit
  // animates a colour, which makes no stacking context of its own, as an opacity animation would.
  await fixture(`<style>@keyframes tint { to { background-color: blue } }</style><div id="holder" style="position:relative">
    <div id="card" style="position:sticky;top:0;width:200px;height:40px"><div id="inner" style="position:relative;z-index:10;height:40px"></div></div>
    <div id="plain" style="width:200px;height:40px"></div>
    <div id="later" style="position:relative;z-index:1;top:-40px;width:100px;height:40px"></div>
  </div>`);
  await evaluate(`leave('tint 1000ms linear both'); seek(0)`);
  expect(await topmost(50, 20)).toBe('later');
});

check('a view that leaves while lifted stops being lifted, as Apple lets go of it', async () => {
  // After the exit, `later` lies over the card's box; the lift had raised the card over everything
  await fixture(`<div id="holder" style="position:relative">
    <div id="card" style="width:200px;height:40px"></div>
    <div id="plain" style="width:200px;height:40px"></div>
    <div id="later" style="position:relative;top:-40px;width:100px;height:40px"></div>
  </div>`);
  expect(await lift('card')).toBe('2147483647');
  await evaluate(`leave('pulse 1000ms linear both'); seek(0)`);
  expect(await topmost(50, 20)).toBe('later');
  expect(await evaluate('getComputedStyle(document.querySelector("[data-exiting]")).zIndex')).toBe('auto');
  // The drag ends after its view left, which motion let go of: the ghost stays out of flow
  await evaluate(`motion.raiseReorder('card', false)`);
  expect(await evaluate('document.querySelector("[data-exiting]").style.position')).toBe('absolute');
  // And a lifted row inside a view that leaves
  await fixture(`<div id="holder" style="position:relative"><div id="card" style="width:200px;height:80px">
    <div id="row" style="width:200px;height:40px"></div></div></div>`);
  expect(await lift('row')).toBe('2147483647');
  await evaluate(`leave('pulse 1000ms linear both'); seek(0)`);
  expect(await evaluate('(() => { const cs = getComputedStyle(document.querySelector("[data-exiting] > div")); return [cs.zIndex, cs.position]; })()')).toEqual(['auto', 'static']);
});

check('a hold that ends after its view left leaves the ghost as it was shown', async () => {
  await fixture(`<div id="holder" style="position:relative">
    <div id="card" style="width:200px;height:40px"></div>
    <div id="plain" style="width:200px;height:40px"></div>
  </div>`);
  await motionFor('card');
  const shown = () => evaluate('(() => { const el = document.getElementById("card") ?? document.querySelector("[data-exiting]"); return [el.style.position, getComputedStyle(el).translate]; })()');
  await evaluate(`window.hold = motion.begin('card', 'translate'); motion.move(hold, [30, 0])`);
  const [, translate] = await shown();
  expect(translate).not.toBe('none');
  await evaluate(`leave('pulse 1000ms linear both'); seek(0)`);
  await evaluate('motion.end(hold)');
  expect(await shown()).toEqual(['absolute', translate]);
});

check('a lift that ends on a ghosts parent keeps the parent isolated', async () => {
  // `over` (z-index 1) lies over the card, whose z-index 5 stays inside the row while the row is isolated
  await fixture(`<div id="holder" style="position:relative">
    <div id="row" style="display:flex;flex-direction:column;width:200px">
      <div id="card" style="position:relative;z-index:5;width:200px;height:40px"></div>
      <div id="other" style="width:200px;height:40px"></div>
    </div>
    <div id="over" style="position:absolute;z-index:1;top:0;left:0;width:200px;height:40px"></div>
  </div>`);
  expect(await lift('row')).toBe('2147483647');
  await evaluate(`leave('pulse 1000ms linear both'); seek(0)`);
  await evaluate(`motion.raiseReorder('row', false)`);
  expect(await evaluate('document.getElementById("row").style.isolation')).toBe('isolate');
  expect(await topmost(50, 20)).toBe('over');
});

check('a flex or grid items ghost paints at the z-index that ranked it', async () => {
  for (const display of ['flex', 'grid;grid-auto-flow:column;grid-template-columns:50px 50px 50px']) {
    // After the exit, `low` and `high` take the card's place, one half each
    await fixture(`<div id="holder" style="position:relative;display:${display}">
      <div id="card" style="flex:none;width:100px;height:40px;z-index:5"></div>
      <div id="low" style="flex:none;position:relative;z-index:3;width:50px;height:40px"></div>
      <div id="high" style="flex:none;position:relative;z-index:7;width:50px;height:40px"></div>
    </div>`);
    await evaluate(`leave('pulse 1000ms linear both'); seek(0)`);
    expect(await topmost(25, 20)).toBe('card');
    expect(await topmost(75, 20)).toBe('high');
  }
});

check('a finished or cancelled ghost restores the parents current isolation row', async () => {
  for (const initial of ['', 'auto', 'isolate']) {
    for (const updated of ['', 'auto', 'isolate']) {
      await fixture(`<div id="holder" style="position:relative;${initial ? `isolation:${initial}` : ''}"><div id="card" style="width:100px;height:40px"></div></div>`);
      await evaluate(`leave('pulse 1000ms linear both'); seek(0)`);
      await style(`position:relative;${updated ? `isolation:${updated}!important` : ''}`, 'holder');
      expect(await evaluate('document.getElementById("holder").style.isolation')).toBe('isolate');
      await evaluate('document.getAnimations().forEach(a => a.cancel()); Promise.resolve()');
      expect(await evaluate('document.getElementById("holder").style.isolation')).toBe(updated);
      expect(await evaluate('document.getElementById("holder").style.getPropertyPriority("isolation")')).toBe(updated ? 'important' : '');
    }
  }
});

check('layout resizing uses its parents structural isolation without animating it', async () => {
  await fixture(`<div id="holder" style="position:relative;isolation:isolate"><div id="card" style="${card};isolation:isolate"></div></div>`);
  await style(card.replace('40px', '140px') + ';isolation:isolate');
  await evaluate('seek(500)');
  expect(await evaluate('surface().getBoundingClientRect().height')).toBeCloseTo(90, 2);
  expect(await evaluate('document.getElementById("holder").getAnimations().length')).toBe(0);
  expect(await evaluate('document.getElementById("holder").style.isolation')).toBe('isolate');
});

check('JS commits keep ghost containment and restore the latest paint decision after the last exit', async () => {
  await fixture('');
  await evaluate(`(async () => {
    const { h, when, sig, W, act, pr, mount, pieces, usePaint } = await import('/js/rt.js');
    const { paintUse } = await import('/js/paint.js');
    usePaint(paintUse());
    const shown = sig(true), trapped = sig(false);
    window.jsShown = act(v => W(shown, v));
    window.jsIsolation = act(v => W(trapped, v));
    mount(root => {
      const app = h(root, 'div', 0, {'data-exact-f':'133'}, 0);
      window.jsHolder = h(app, 'div', 0, {'data-exact-f':'0'}, 0);
      when(jsHolder, shown, p => {
        for (const duration of [1000, 2000]) {
          const c = h(p, 'div', 0, {'data-exact-f':'4'}, 0);
          c.style.cssText = 'width:100px;height:40px;--exact-exit-animation:pulse ' + duration + 'ms linear both';
          pr(c);
        }
      }, () => {});
      // A live z descendant requires isolation independently of the ghosts.
      when(jsHolder, trapped, p => {
        const c = h(p, 'div', 0, {'data-exact-f':'513','data-exact-zi':'2'}, 0);
        c.style.cssText = 'position:absolute;z-index:2;width:10px;height:10px';
      }, () => {});
      const sibling = h(app, 'div', 0, {'data-exact-f':'513','data-exact-zi':'1'}, 0);
      sibling.style.cssText = 'position:absolute;z-index:1;width:100px;height:40px';
    });
    await pieces();
    if (!jsHolder.$paint?.own || jsHolder.$paint.own.outside) throw Error('the production paint pass must classify the holder');
  })()`);
  expect(await evaluate('jsHolder.style.isolation')).toBe('');
  for (const required of [true, false]) {
    await evaluate('jsShown(false); seek(0)');
    expect(await evaluate('document.querySelectorAll("[data-exiting]").length')).toBe(2);
    expect(await evaluate('jsHolder.style.isolation')).toBe('isolate');
    // These actions must commit the production pass themselves: no manual flush.
    for (const trapped of [true, false, true]) {
      await evaluate(`jsIsolation(${trapped})`);
      expect(await evaluate('jsHolder.$paint.isolated')).toBe(trapped);
      expect(await evaluate('jsHolder.$ghostIsolation.value')).toBe(trapped ? 'isolate' : '');
      expect(await evaluate('jsHolder.style.isolation')).toBe('isolate');
    }
    await evaluate('document.getAnimations().find(a => a.effect.getTiming().duration === 1000).finish(); Promise.resolve()');
    expect(await evaluate('document.querySelectorAll("[data-exiting]").length')).toBe(1);
    expect(await evaluate('jsHolder.style.isolation')).toBe('isolate');
    await evaluate(`jsIsolation(${required})`);
    expect(await evaluate('jsHolder.$ghostIsolation.value')).toBe(required ? 'isolate' : '');
    await evaluate(`document.getAnimations().forEach(a => a.${required ? 'finish' : 'cancel'}()); Promise.resolve()`);
    expect(await evaluate('document.querySelectorAll("[data-exiting]").length')).toBe(0);
    expect(await evaluate('!!jsHolder.$ghostIsolation')).toBe(false);
    expect(await evaluate('jsHolder.style.isolation')).toBe(required ? 'isolate' : '');
    await evaluate('jsIsolation(false); jsShown(true)');
  }
});

check('basic clips follow the shown size on the content and stand-in', async () => {
  for (const clip of ['inset(25%)', 'inset(10% 20% round 30% / 20%)', 'polygon(0% 0%,100% 0%,50% 100%)',
    'circle()', 'ellipse()', 'circle(40% at right bottom)', 'circle(40% at center)', 'circle(40% at top)',
    'ellipse(30% 40% at bottom left)', 'ellipse(30% 40% at right)', 'circle(40% at 40% 60%)', 'ellipse(closest-side farthest-side at 30% 40%)', "path('M25 25H75V75H25Z')"]) {
    for (const [grow, border] of [[true, 0], [false, 0], [true, 8], [false, 8]]) {
      const overflow = border ? 'hidden' : 'visible';
      const from = grow ? 100 : 200, to = grow ? 200 : 100;
      const css = n => `width:${n}px;height:${n}px;background:red;box-sizing:border-box;overflow:${overflow};border:${border}px solid red;border-radius:20% / 30%;clip-path:${clip};--exact-layout-transition:1000 0 linear`;
      await fixture(`<div id="card" style="${css(from)}"><div style="width:200px;height:200px"></div></div>`);
      await style(css(to));
      for (const time of [250, 500, 750]) {
        await evaluate(`seek(${time})`);
        const shown = from + (to-from)*time/1000;
        const result = await evaluate(`(() => {
          const el = document.getElementById('card'), sf = surface(), r = el.getBoundingClientRect();
          const oracle = document.createElement('div');
          oracle.style.cssText = 'position:absolute;left:250px;top:20px;width:${shown}px;height:${shown}px;clip-path:${clip.replaceAll("'", '\"')};background:red;box-sizing:border-box;overflow:${overflow};border:${border}px solid red;border-radius:20% / 30%';
          const child = document.createElement('div');
          child.style.cssText = 'width:200px;height:200px'; oracle.append(child);
          document.body.append(oracle);
          const points = [];
          for (let y=7; y<200; y+=17) for (let x=7; x<200; x+=17) points.push([x,y]);
          const expectedContent = points.map(([x,y]) => document.elementFromPoint(250+x,20+y) === child);
          const content = points.map(([x,y]) => el.firstElementChild === document.elementFromPoint(r.left+x,r.top+y));
          el.style.visibility = 'hidden';
          sf.parentElement.inert = false; sf.parentElement.style.pointerEvents = 'auto'; sf.parentElement.style.zIndex = '1';
          child.remove();
          const expectedBackground = points.map(([x,y]) => document.elementFromPoint(250+x,20+y) === oracle);
          const background = points.map(([x,y]) => document.elementFromPoint(r.left+x,r.top+y) === sf);
          el.style.visibility = ''; sf.parentElement.inert = true; sf.parentElement.style.pointerEvents = 'none'; sf.parentElement.style.zIndex = '-1';
          const raw = shownClip(${JSON.stringify(clip)}, ${shown}, ${shown});
          const valid = CSS.supports('clip-path', raw);
          oracle.style.clipPath = 'none';
          oracle.style.clipPath = raw;
          const resolved = points.map(([x,y]) => document.elementFromPoint(250+x,20+y) === oracle);
          oracle.remove();
          return { expectedBackground, expectedContent, content, background, resolved, valid };
        })()`);
        expect(result.content, `${clip} content ${grow} ${border} ${time}`).toEqual(result.expectedContent);
        expect(result.background, `${clip} surface ${grow} ${time}`).toEqual(result.expectedBackground);
        expect(result.valid, `${clip} resolves to valid CSS`).toBe(true);
        expect(result.resolved, `${clip} raw position ${shown}`).toEqual(result.expectedBackground);
      }
    }
  }
});
