import { hostFixtureFile as readFileSync } from './glue-fixture.mjs';
// A drag timeline's release on the web (LLP 1057.003 D3): a consumer plays a
// copy of its keyframes with the `linear()` easing `timelineEasing` gives
// for the release's samples. The first tests evaluate that easing against the
// directed progress written out by hand; the browser test holds it, between
// frames too, to Chrome seeking the paused animation to the timeline's
// progress, as the glue does while a drag is held: delays, iterations,
// directions and keyframe easing included.
import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { open } from '../../../scripts/agent.mjs';
import { cdpBrowser, chromium, suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { timelineEasing } from '../motion-glue.js';
import { need } from '../../../scripts/coverage.mjs';
import { readyPage } from './page.mjs';
import { glueDeclarations, checkedGlueCall } from './glue-fixture.mjs';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(fileURLToPath(new URL('../../..', import.meta.url)));
const compiler = resolve(process.env.CARGO_TARGET_DIR ?? resolve(ROOT, 'target'), 'debug', process.platform === 'win32' ? 'contract.exe' : 'contract');
const WEB = resolve(fileURLToPath(new URL('..', import.meta.url)));
const WEB_JS = resolve(WEB, '../web-js');
const GLUE = readFileSync(resolve(WEB, 'glue.js'), 'utf8');
const commitSource = glueDeclarations(GLUE, ['apply', 'heldLinks', 'applyBatch', 'holdWake', 'holdOrientation']);
// The wasm host's real operation switch and commit tail, with the unrelated
// host pieces inert. Keeping both functions in this one closure matters:
// the `timelines` operation sets the flag that the commit tail consumes.
const hostCommitBody = `
  let timelinesMoved = false, agentClock = 0, followOnSeek = true;
  const register = () => {}, seek = () => {};
  const loadAfterPaint = () => { throw new Error('timeline fixture: unexpected lazy module load'); };
  const exact = globalThis.exact ??= {}, retiredViews = new WeakSet(), followedScrolls = new Map(), pendingScrolls = new Map();
  const root = document.getElementById('root'), listSelection = null, textflow = null, page = null, collectionOp = null;
  const collections = {commit() {}, restyled() {}}, arrange = {commit() {}, destroy() {}, binding() {}, state() {}}, presence = {hold: () => false, live: null};
  const prepareContexts = () => {}, runFocusCommands = () => {}, inertAncestor = () => false, refreshSymbols = () => {};
  const focusAutofocus = () => {}, inputHandlers = null, positionContexts = () => {}, markScrollDocument = () => {}, syncLists = () => {};
  const followScroll = () => {}, settleFollow = () => {}, settleValue = () => {}, letGo = () => {}, flowBatch = () => {}, noteFit = () => {}, refitTexts = () => {};
  const navigation = {project() {}, apply() {}, holdLink() {}}, log = () => {}, inputReady = false, agentMode = false, agentKeepsStore = false, frameSampler = null, clocks = {sync() {}};
  const viewFor = (_, id) => views.get(id), applyProps = () => {}, attach = () => {}, listView = () => {};
  const host = { timelinesMoved, agentClock, followOnSeek, listSelection, textflow, page, inputHandlers, inputReady, frameSampler };
  ${commitSource}
  return (${checkedGlueCall})(applyBatch);
`;
const { unavailable } = chromium();
const check = need('chrome', unavailable);

test('glue fixtures name missing declarations and module bindings', () => {
  expect(() => glueDeclarations(GLUE, ['absentFixtureDeclaration'])).toThrow('glue.js fixture: missing declaration absentFixtureDeclaration');
  const source = glueDeclarations('let present = 1, omitted = 2; function commit() { return present + omitted; }', ['present', 'commit']);
  const commit = new Function(`${source}; return (${checkedGlueCall})(commit);`)();
  expect(commit).toThrow('glue.js fixture: supply the module binding used by commit: omitted is not defined');
});

test('the extracted wake-lock bindings share one lazy module and recheck holders', async () => {
  const root = {}, log = () => {}, loads = [], holds = [];
  let holders = [1];
  const load = async (...args) => { loads.push(args); return {
    holders: el => { expect(el).toBe(root); return holders; },
    hold: (held, report) => { expect(report).toBe(log); holds.push(held); },
  }; };
  const hold = new Function('loadAfterPaint', 'root', 'log', `const host = { wakeLock: null }; ${glueDeclarations(GLUE, ['holdWake'])}; return holdWake;`)(load, root, log);
  await hold(); holders = []; await hold();
  expect(loads).toEqual([['./wakelock-glue.js', 'wakeLockGlue']]);
  expect(holds).toEqual([true, false]);
});

// A `linear()` easing at input x (0–1): the last point at or before x, then
// linearly on to the next; two points at one input are a jump.
function evaluate(easing, x) {
  const points = easing.slice(7, -1).split(', ').map((s) => s.split(' ')).map(([y, at]) => [parseFloat(at) / 100, Number(y)]);
  let i = points.findLastIndex(([at]) => at <= x);
  if (i === points.length - 1) return points[i][1];
  const [x0, y0] = points[i], [x1, y1] = points[i + 1];
  return y0 + (y1 - y0) * (x - x0) / (x1 - x0);
}

// The timeline's progress at x, between the spring's evenly spaced frames.
const between = (p, x) => {
  const at = x * (p.length - 1), j = Math.min(Math.floor(at), p.length - 2);
  return p[j] + (p[j + 1] - p[j]) * (at - j);
};

function worst(timing, p, directed) {
  const easing = timelineEasing(timing, p);
  let e = 0;
  // Off every frame and every crossing: 997 is prime.
  for (let i = 0; i <= 997; i++) e = Math.max(e, Math.abs(evaluate(easing, i / 997) - directed(between(p, i / 997) * timing.endTime)));
  return e;
}

const clamp = (v) => Math.min(1, Math.max(0, v));

test('one iteration, filled both ways: the progress, clamped, with a point at each crossing', () => {
  const timing = { delay: 0, duration: 1000, iterations: 1, direction: 'normal', fill: 'both', endTime: 1000 };
  // A spring back past its start and home: it crosses 0 twice.
  const p = [0.3, 0.1, -0.05, 0.02, -0.005, 0];
  expect(worst(timing, p, (t) => clamp(t / 1000))).toBeLessThan(1e-9);
  // Six frames and three crossings; a kink is one point, a jump two.
  expect(timelineEasing(timing, p).split(', ').length).toBe(9);
});

test('a delay, and iterations that wrap or alternate', () => {
  const delayed = { delay: 500, duration: 500, iterations: 1, direction: 'normal', fill: 'both', endTime: 1000 };
  expect(worst(delayed, [0.9, 0.4, 0.2, 0.6, 1.2], (t) => clamp((t - 500) / 500))).toBeLessThan(1e-9);
  const wraps = { delay: 0, duration: 250, iterations: 4, direction: 'normal', fill: 'both', endTime: 1000 };
  const wrap = (t) => (t >= 1000 ? 1 : t <= 0 ? 0 : (t % 250) / 250);
  expect(worst(wraps, [0, 0.5, 0.26, 0.99, 0.4], wrap)).toBeLessThan(1e-6);
  const alternates = { delay: 0, duration: 500, iterations: 2, direction: 'alternate', fill: 'both', endTime: 1000 };
  const triangle = (t) => (t <= 0 ? 0 : t >= 1000 ? 0 : t < 500 ? t / 500 : 1 - (t - 500) / 500);
  expect(worst(alternates, [0, 0.3, 0.75, 1.1, 0.5], triangle)).toBeLessThan(1e-6);
});

test('an endless animation holds its start; a fill that leaves it out of effect cannot be said', () => {
  expect(timelineEasing({ delay: 0, duration: 1000, iterations: Infinity, fill: 'both', endTime: Infinity }, [0, 1])).toBeUndefined();
  const none = { delay: 0, duration: 1000, iterations: 1, direction: 'normal', fill: 'none', endTime: 1000 };
  expect(timelineEasing(none, [0.5, -0.1, 0])).toBeNull(); // before the start, out of effect
  expect(timelineEasing(none, [0.5, 1, 0.9])).toBeNull(); // at the end, out of effect
  expect(worst(none, [0.5, 0.1, 0.9], (t) => t / 1000)).toBeLessThan(1e-9); // never leaves it
  // Samples whose peak between the ends may be off by the tolerance: a peak
  // that close to the end may leave the effect; the exact ends may not.
  expect(timelineEasing(none, [0.5, 0.99999, 0.9], 2e-5)).toBeNull();
  expect(timelineEasing(none, [0.5, 0.99999, 0.9])).not.toBeNull();
  expect(timelineEasing(none, [0, 0.5, 0.9], 2e-5)).not.toBeNull();
});

check('CSS-eased translate releases follow on the compositor and on the agent clock', async () => {
  const fixture = `<style>
    @keyframes fade { from { opacity: 0.1 } to { opacity: 0.9 } }
    @keyframes pulse { 0% { opacity: 0.1 } 40% { opacity: 0.9; animation-timing-function: ease-in } 100% { opacity: 0.3 } }
    @keyframes stepped { 0% { opacity: 0.1 } 25% { opacity: 0.3; animation-timing-function: steps(2,end) } 75% { opacity: 0.7 } 100% { opacity: 0.9 } }
  </style><div id="exact-root"><div id="source"><div id="consumer"></div></div><div id="reference"></div></div>
  <script type="module">
    import { motionController, translation } from './motion-glue.js';
    import { animationClock } from './navigation.js';
    const source = document.getElementById('source'), consumer = document.getElementById('consumer'), reference = document.getElementById('reference');
    const nativeFrame = requestAnimationFrame.bind(window);
    let frames = 0, time = null;
    window.requestAnimationFrame = fn => { frames++; return nativeFrame(fn); };
    const motion = motionController({views:new Map([[1,source],[2,consumer]]),now:()=>time ?? performance.now(),generation:()=>1,
      inert:()=>false,applyBatch() {},request:r=>r.op==='begin'?{token:'1',value:[r.x,r.y]}:{accepted:true}});
    const clock = animationClock(()=>time,()=>null,()=>motion.followTimelines());
    const copies = () => consumer.getAnimations().filter(a=>a.animationName===undefined);
    const read = () => {
      const [a,b] = consumer.style.getPropertyValue('--exact-animation-range').trim().split(' ').map(Number);
      const p = (translation(source,getComputedStyle(source).translate)[0]-a)/(b-a);
      for (const animation of reference.getAnimations()) {
        const {endTime, delay} = animation.effect.getComputedTiming();
        animation.currentTime = Number.isFinite(endTime) ? p*endTime : Math.max(0,delay);
      }
      return Math.abs(Number(getComputedStyle(consumer).opacity)-Number(getComputedStyle(reference).opacity));
    };
    const prepare = (animation, style = '') => {
      source.style.cssText='--exact-drag-timeline:--drag x;translate:0px 0px;width:100px;height:100px';
      consumer.style.cssText='--exact-animation-timeline:--drag;--exact-animation-range:0 100;animation:'+animation+';animation-play-state:paused;'+style;
      reference.style.cssText='animation:'+animation+';animation-play-state:paused;'+style;
      motion.followTimelines();
    };
    const sampleRelease = driver => new Promise(ok=>{
      let worst=0, intermediate=0;
      const sample=()=>{
        worst=Math.max(worst,read());intermediate++;
        if(driver.playState==='finished')ok({worst,intermediate});else next();
      };
      // Fallback seeking can run later in the same animation-frame batch.
      const next=()=>nativeFrame(()=>setTimeout(sample,0));
      next();
    });
    window.release = async (easing, delay, animation, agent = false, range = '0 100', at = null, {effectEasing = null, to = 100, style = ''} = {}) => {
      prepare(animation, style);
      if (effectEasing) for (const el of [consumer,reference]) el.getAnimations()[0].effect.updateTiming({easing:effectEasing});
      consumer.style.setProperty('--exact-animation-range',range);
      source.style.transition='translate 400ms '+easing+' '+delay+'ms';
      getComputedStyle(source).translate;
      const ran = new Promise(ok=>source.addEventListener('transitionrun',ok,{once:true}));
      source.style.translate=to+'px 0px';
      const driver = source.getAnimations().find(a=>a.transitionProperty==='translate');
      if (agent) { time=0; clock.register(time); clock.seek(time); }
      if (!agent) await ran;
      await new Promise(nativeFrame);
      const count = copies().length, start = driver.startTime, followerStart = copies()[0]?.startTime;
      let worst=0, intermediate=0;
      const opacities=[];
      if (agent) {
        for (const t of at ?? Array.from({length:102},(_,i)=>i*(400+Math.max(0,delay))/101)) {
          time=t; clock.register(time); clock.seek(time);
          worst=Math.max(worst,read());
          opacities.push(Number(getComputedStyle(consumer).opacity));
        }
        // Chromium's CSS transition end can round above its reported currentTime.
        time=clock.settle()+1; clock.seek(time);
      } else {
        ({worst,intermediate}=await sampleRelease(driver));
      }
      if (!agent) await driver.finished;
      await Promise.resolve();
      return {count,frames,worst,intermediate,start,followerStart,opacities,remaining:copies().length};
    };
    window.interrupt = async mode => {
      prepare('fade 1s linear both');
      source.style.transition='translate 400ms ease-out 80ms';
      getComputedStyle(source).translate;
      const ran=new Promise(ok=>source.addEventListener('transitionrun',ok,{once:true}));
      source.style.translate='100px 0px';
      const first=source.getAnimations().find(a=>a.transitionProperty==='translate');
      await ran; await new Promise(nativeFrame);
      const old=copies()[0];
      first.currentTime=180; old.currentTime=180;
      const before=translation(source,getComputedStyle(source).translate)[0];
      let caught=null, error=read();
      if(mode==='grab') {
        const hold=motion.begin(1,'translate');
        caught=translation(source,getComputedStyle(source).translate)[0];
        error=Math.max(error,read());
        motion.end(hold,[0,0],true);
      } else if(mode==='cancel') first.cancel();
      else source.style.translate='0px 0px';
      motion.followTimelines();
      await Promise.resolve();
      const next=source.getAnimations().find(a=>a.transitionProperty==='translate');
      const count=copies().length;
      if(mode!=='cancel') {
        const samples=await sampleRelease(next);error=Math.max(error,samples.worst);
        await next.finished;
      } else error=Math.max(error,read());
      await Promise.resolve();
      return {before,caught,error,count,frames,oldState:old.playState,remaining:copies().length};
    };
    window.ready = true;
  </script>`;
  const server = Bun.serve({hostname:'127.0.0.1',port:0,fetch(req) {
    const path = new URL(req.url).pathname;
    return path==='/' ? new Response(fixture,{headers:{'content-type':'text/html'}})
      : new Response(readFileSync(resolve(WEB,path.slice(1))),{headers:{'content-type':'text/javascript'}});
  }});
  const browser = await suiteBrowser();
  try {
    for (const [easing, delay, animation, agent] of [
      ['ease-out', 0, 'fade 1s linear both', false],
      ['cubic-bezier(0.2, -0.4, 0.7, 1.3)', 80, 'pulse 200ms linear 100ms 2 alternate both', false],
      ['linear', -80, 'fade 1s ease-in reverse both', false],
      ['ease-in-out', 80, 'pulse 200ms linear 100ms 2 alternate both', true],
      ['steps(4, jump-start)', 0, 'fade 1s linear both', false],
      ['linear(0, 0.2 50%, 0.8 50%, 1)', 0, 'fade 1s linear both', false],
      ['cubic-bezier(0, 0, 0, 1)', 0, 'fade 1s linear both', false],
    ]) {
      const page = await browser.newPage();
      try {
        await page.goto(`http://127.0.0.1:${server.port}/`);
        await page.waitForFunction(()=>window.ready);
        const result = await page.evaluate(args=>release(...args),[easing,delay,animation,agent]);
        const fallback = /^(steps|linear)\(/.test(easing)||easing==='cubic-bezier(0, 0, 0, 1)';
        expect(result.count, `${easing}: release followers ${JSON.stringify(result)}`).toBe(fallback?0:1);
        expect(result.worst, `${easing}: consumer progress throughout the release`).toBeLessThan(1e-4);
        if (!fallback) expect(result.frames, `${easing}: no timeline frame loop`).toBe(0);
        else expect(result.frames, 'unsupported easing retains frame seeking').toBeGreaterThan(0);
        if (!agent) {
          expect(result.intermediate).toBeGreaterThan(2);
          if (!fallback) expect(result.followerStart).toBe(result.start);
        }
        expect(result.remaining, `${easing}: finished followers retire ${JSON.stringify(result)}`).toBe(0);
      } finally { await page.context().close(); }
    }
    // A small source interpolation error must not move a consumer's iteration
    // wrap. These times straddle the browser's crossing of 5px.
    for (const direction of ['normal', 'reverse', 'alternate', 'alternate-reverse']) for (const agent of [true, false]) {
      const page=await browser.newPage();
      try {
        await page.goto(`http://127.0.0.1:${server.port}/`);
        await page.waitForFunction(()=>window.ready);
        const result=await page.evaluate(args=>release(...args),[
          'cubic-bezier(.333333333333,0,.666666666667,0)', 0, `fade 1s linear 2 ${direction} both`,
          agent, '0 10', [147.36, 147.361164, 147.362],
        ]);
        const wraps=direction==='normal'||direction==='reverse';
        expect(result.worst, `${direction}: consumer matches the browser around the iteration boundary`).toBeLessThan(1e-4);
        expect(result.count, `${direction}: wrapping consumers retain seeking`).toBe(wraps?0:1);
        if (agent) {
          expect(result.opacities[0]).toBeCloseTo(direction.endsWith('reverse')?0.1:0.9,4);
          expect(result.opacities[2]).toBeCloseTo(direction==='normal'||direction==='alternate-reverse'?0.1:0.9,4);
        } else if (wraps) expect(result.frames).toBeGreaterThan(0);
        else expect(result.frames).toBe(0);
        expect(result.remaining).toBe(0);
      } finally { await page.context().close(); }
    }
    // The same source error can cross a consumer's easing jump, including a
    // jump on an individual keyframe or the effect's overall timing.
    for (const [animation, effectEasing, before, after] of [
      ['fade 1s steps(2,end) both', null, 0.1, 0.5],
      ['stepped 1s linear both', null, 0.3, 0.5],
      ['fade 1s linear(0, 0 50%, 1 50%, 1) both', null, 0.1, 0.9],
      ['fade 1s linear both', 'steps(2,end)', 0.1, 0.5],
    ]) for (const agent of [true, false]) {
      const page=await browser.newPage();
      try {
        await page.goto(`http://127.0.0.1:${server.port}/`);
        await page.waitForFunction(()=>window.ready);
        const result=await page.evaluate(args=>release(...args),[
          'cubic-bezier(.333333333333,0,.666666666667,0)', 0, animation,
          agent, '0 10', [147.36, 147.361164, 147.362], {effectEasing},
        ]);
        expect(result.worst, `${animation} / ${effectEasing}: consumer matches the browser around the easing jump`).toBeLessThan(1e-4);
        expect(result.count, 'discontinuous consumer easing retains seeking').toBe(0);
        if (agent) {
          expect(result.opacities).toEqual([before,before,after]);
        } else expect(result.frames).toBeGreaterThan(0);
        expect(result.remaining).toBe(0);
      } finally { await page.context().close(); }
    }
    // Followers sort above every CSS animation, so one standing in for an
    // animation listed before a sought or endless one would override it.
    for (const [animation, followed, opacity] of [
      ['fade 1s linear both, fade 1s steps(2,end) both', 0, 0.1],
      ['fade 1s steps(2,end) both, fade 1s linear both', 1, 0.3],
      ['fade 1s linear both, fade 1s linear infinite both', 0, 0.1],
    ]) {
      const page=await browser.newPage();
      try {
        await page.goto(`http://127.0.0.1:${server.port}/`);
        await page.waitForFunction(()=>window.ready);
        const result=await page.evaluate(args=>release(...args),['linear', 0, animation, true, '0 100', [40, 100, 160, 300]]);
        expect(result.worst, `${animation}: the consumer's animation stack keeps its order`).toBeLessThan(1e-4);
        expect(result.opacities[1]).toBeCloseTo(opacity,4);
        expect(result.count, `${animation}: only a suffix of the stack is followed`).toBe(followed);
        expect(result.remaining).toBe(0);
      } finally { await page.context().close(); }
    }
    // An overshooting curve's peak lies between samples. The browser's peak
    // passes the end of this consumer's range, which leaves `fill: none` out
    // of effect, while the samples' peak falls just short of it.
    for (const agent of [true, false]) {
      const page=await browser.newPage();
      try {
        await page.goto(`http://127.0.0.1:${server.port}/`);
        await page.waitForFunction(()=>window.ready);
        const result=await page.evaluate(args=>release(...args),[
          'cubic-bezier(.333333333333,2,.666666666667,2)', 0, 'fade 1s linear', agent, '0 170.65598',
          [234.2, 234.314575, 234.43], {to: 103, style: 'opacity:0.55'},
        ]);
        expect(result.worst, 'the consumer leaves its effect where the source passes the range').toBeLessThan(1e-4);
        expect(result.count, 'a fill boundary within the sampling error retains seeking').toBe(0);
        if (agent) expect(result.opacities[1]).toBeCloseTo(0.55,4);
        expect(result.remaining).toBe(0);
      } finally { await page.context().close(); }
    }
    for (const mode of ['reverse', 'grab', 'cancel']) {
      const page=await browser.newPage();
      try {
        await page.goto(`http://127.0.0.1:${server.port}/`);
        await page.waitForFunction(()=>window.ready);
        const result=await page.evaluate(mode=>interrupt(mode),mode);
        expect(result.oldState, `${mode}: the old follower is cancelled`).toBe('idle');
        expect(result.count, `${mode}: only a new release has a follower`).toBe(mode==='cancel'?0:1);
        expect(result.error, `${mode}: consumers follow through interruption`).toBeLessThan(1e-4);
        expect(result.frames, `${mode}: no timeline frame loop`).toBe(0);
        expect(result.remaining, `${mode}: finished followers retire`).toBe(0);
        if(mode==='grab')expect(result.caught, 'a re-grab keeps the presented translate').toBeCloseTo(result.before,4);
      } finally { await page.context().close(); }
    }
  } finally { await browser.close(); server.stop(true); }
}, 60000);

check('in Chrome, followers match timeline progress and stop when a consumer resolves elsewhere', async () => {
  const page = `<style>
@keyframes fade { from { opacity: 1 } to { opacity: 0.2 } }
@keyframes pulse { 0% { opacity: 0.1 } 40% { opacity: 0.9; animation-timing-function: ease-in } 100% { opacity: 0.3 } }
</style><div id="root"></div>
<script type="module">
  import { motionController, timelineEasing } from './motion-glue.js';
  import { engine as jsMotionEngine } from './js/motion.js';
  window.compare = (animation, p) => {
    const make = () => { const el = document.createElement('div'); el.style.animation = animation; el.style.animationPlayState = 'paused'; document.getElementById('root').append(el); return el; };
    const reference = make(), follower = make();
    const [a] = reference.getAnimations(), [b] = follower.getAnimations(), timing = b.effect.getComputedTiming();
    const easing = timelineEasing(timing, p);
    if (easing == null) return { easing: easing === null ? 'null' : 'undefined' };
    const f = follower.animate(b.effect.getKeyframes().map(({ computedOffset, ...k }) => k), { duration: 1000, easing, fill: 'both' });
    f.pause();
    let worst = 0;
    for (let i = 0; i <= 997; i++) {
      const x = i / 997, at = x * (p.length - 1), j = Math.min(Math.floor(at), p.length - 2);
      a.currentTime = (p[j] + (p[j + 1] - p[j]) * (at - j)) * timing.endTime;
      f.currentTime = x * 1000;
      worst = Math.max(worst, Math.abs(parseFloat(getComputedStyle(reference).opacity) - parseFloat(getComputedStyle(follower).opacity)));
    }
    reference.remove(); follower.remove();
    return { easing, worst };
  };
  const sourceStyle = '--exact-drag-timeline:--drag x;translate:0 0;width:10px;height:10px';
  const consumerStyle = 'opacity:.55;--exact-animation-timeline:--drag;--exact-animation-range:0 100;animation:fade 1s linear both;animation-play-state:paused';
  const makeController = async target => {
    const root = document.getElementById('root');
    root.innerHTML = '<div id="scope"><div id="old"></div><div id="next"></div><div id="consumer"></div></div>';
    const scope = document.getElementById('scope'), old = document.getElementById('old');
    const next = document.getElementById('next'), consumer = document.getElementById('consumer');
    scope.style.setProperty('--exact-timeline-scope', '--drag, --other');
    old.style.cssText = sourceStyle;
    next.style.cssText = 'translate:80px 0;width:10px;height:10px';
    consumer.style.cssText = consumerStyle;
    const views = new Map([[1, old], [2, next], [3, consumer], [4, scope]]);
    let frames = 0;
    const requestFrame = window.requestAnimationFrame;
    window.requestAnimationFrame = (...args) => { frames++; return requestFrame(...args); };
    let runtime;
    if (target === 'js') {
      const instantiate = WebAssembly.instantiate;
      const memory = new WebAssembly.Memory({initial:1});
      WebAssembly.instantiate = async () => ({instance:{exports:{memory,m_in:()=>0,m_out:()=>0,m_lower:()=>0}}});
      globalThis.__files = () => new ArrayBuffer(0);
      globalThis.exact ??= {}; globalThis.exact.After ??= {};
      try {
        runtime = await jsMotionEngine({clock:{agent:false,now:0},wall:()=>performance.now(),views,viewId:el=>Number(el.dataset.view),
          hooks:{},say:()=>{},inflight:{n:0}});
      } finally { WebAssembly.instantiate = instantiate; delete globalThis.__files; }
    }
    const motion = runtime?.api ?? motionController({ views, now: () => performance.now(), generation: () => 1,
      request: facts => facts.op === 'begin' ? {token:'7',value:[50,0]} : facts.op === 'live' ? {accepted:true} : {},
      applyBatch: () => {}, inert: () => false });
    return {root,scope,old,next,consumer,views,motion,runtime,frames:()=>frames,restore:()=>{motion.reset();window.requestAnimationFrame=requestFrame;}};
  };
  const startFollower = f => {
    f.motion.animate({ id: 1, property: 'translate', values: [[0, 0], [100, 80]], delay: 0, duration: 100000 });
    const source = f.old.getAnimations().find(a => a.animationName === undefined);
    const follower = f.consumer.getAnimations().find(a => a.animationName === undefined);
    source.currentTime = 50000; follower.currentTime = 50000;
  };
  const followerCount = f => f.consumer.getAnimations().filter(a => a.animationName === undefined).length;
  const stylesFor = (f, change) => {
    if (change === 'other-source') return [[1, 'translate:50px 0;width:10px;height:10px'], [2, '--exact-drag-timeline:--drag x;translate:80px 0;width:10px;height:10px']];
    if (change === 'renamed-axis') return [[1, '--exact-drag-timeline:--other y;translate:0 80px;width:10px;height:10px'], [3, consumerStyle.replaceAll('--drag', '--other')]];
    if (change === 'axis') return [[1, '--exact-drag-timeline:--drag y;translate:0 80px;width:10px;height:10px']];
    if (change === 'inactive') return [[1, 'translate:50px 0;width:10px;height:10px']];
    if (change === 'removed') return [[1, 'translate:50px 0;width:10px;height:10px'], [3, 'opacity:.55'], [4, '']];
    if (change === 'range') return [[3, consumerStyle.replace('0 100', '0 200')]];
    return [];
  };
  window.reconcileFollower = async (target, change) => {
    const f = await makeController(target); startFollower(f);
    const before = parseFloat(getComputedStyle(f.consumer).opacity), styles = stylesFor(f, change);
    if (target === 'wasm') {
      const commit = new Function('motion', 'views', ${JSON.stringify(hostCommitBody)})(f.motion, f.views);
      commit({ops:[...styles.map(([id,css])=>({op:'style',id,css})), {op:'timelines'}]});
    } else {
      for (const [id, css] of styles) f.views.get(id).style.cssText = css;
      f.runtime.flush();
    }
    const result = { before, after: parseFloat(getComputedStyle(f.consumer).opacity), followers:followerCount(f), frames:f.frames() };
    f.restore();
    return result;
  };
  window.cancelFollower = async operation => {
    const f = await makeController('wasm'); startFollower(f);
    let threw = null;
    try {
      if (operation === 'catch') f.motion.begin(1, 'translate');
      else if (operation === 'spring') f.motion.animate({id:1,property:'translate',values:[[50,0],[0,0]],delay:0,duration:100000});
      else if (operation === 'retire') f.motion.retire(1, 'translate');
      else if (operation === 'destroy') f.motion.destroy(1);
      else if (operation === 'finish') { f.consumer.getAnimations().find(a=>a.animationName===undefined).finish(); await new Promise(ok=>setTimeout(ok,0)); }
      else f.motion.reset();
    } catch (error) { threw = String(error); }
    const result = {threw,followers:followerCount(f)};
    f.restore(); return result;
  };
  window.ready = true;
</script>`;
  const server = createServer((req, res) => {
    if (req.url === '/') { res.writeHead(200, { 'content-type': 'text/html' }); res.end(page); return; }
    const path = decodeURIComponent(new URL(req.url, 'http://x').pathname);
    // names.js is a build's output (the plan's names), which rt.js reaches through perf.js since db825de7b; as presence.test.mjs serves it.
    if (path === '/js/names.js') { res.writeHead(200, { 'content-type': 'text/javascript' }); res.end('export const sourceTypes = {};'); return; }
    // As the JS target's build lays it out: host/web-js's modules, beside the host/web glue it copies in (build.mjs).
    const name = path.startsWith('/js/') ? path.slice(4) : path === '/motion-glue.js' ? 'motion-glue.js' : null;
    const file = name == null ? null : existsSync(resolve(WEB_JS, name)) ? resolve(WEB_JS, name) : resolve(WEB, name);
    if (!file) { res.writeHead(404); res.end(); return; }
    res.writeHead(200, { 'content-type': 'text/javascript' }); res.end(readFileSync(file));
  });
  await new Promise((ok) => server.listen(0, '127.0.0.1', ok));
  const suite = await cdpBrowser(), { cdp } = suite;
  try {
    const { sessionId } = await suite.page();
    const evaluate = async (expression) => {
      const reply = await cdp.send('Runtime.evaluate', { expression, returnByValue: true, awaitPromise: true }, sessionId);
      if (reply.exceptionDetails) throw new Error(reply.exceptionDetails.exception?.description ?? reply.exceptionDetails.text);
      return reply.result.value;
    };
    await readyPage(cdp, sessionId, `http://127.0.0.1:${server.address().port}/`);
    const compare = (animation, p) => evaluate(`compare(${JSON.stringify(animation)}, ${JSON.stringify(p)})`);
    for (const [animation, p] of [
      ['fade 1s linear both', [0.3, 0.1, -0.05, 0.02, -0.005, 0]],
      ['fade 1s linear 0.5s both', [0.9, 0.4, 0.2, 0.6, 1.2]],
      ['pulse 0.5s linear 2 alternate both', [0, 0.3, 0.75, 1.1, 0.5]],
      ['pulse 0.25s linear 4 both', [0, 0.5, 0.26, 0.99, 0.4]],
      ['pulse 0.4s linear 0.2s 1.5 reverse forwards', [0.3, 0.9, 1.3, 0.95]],
      ['fade 1s linear', [0.5, 0.1, 0.9]],
    ]) {
      const { easing, worst } = await compare(animation, p);
      expect(easing.startsWith('linear(')).toBe(true);
      // getComputedStyle serializes opacity to about six digits.
      expect(worst, animation).toBeLessThan(1e-4);
    }
    expect((await compare('fade 1s linear', [0.5, -0.1, 0])).easing).toBe('null');
    for (const operation of ['catch', 'spring', 'retire', 'destroy', 'finish', 'reset']) {
      const result = await evaluate(`cancelFollower(${JSON.stringify(operation)})`);
      expect(result.threw, `${operation}: cancelling a live follower does not throw`).toBeNull();
      expect(result.followers, `${operation}: the old follower is gone`).toBe(operation === 'spring' ? 1 : 0);
    }
    for (const target of ['wasm', 'js']) for (const [change, after, followers] of [
      ['other-source', 0.36, 0],
      ['renamed-axis', 0.68, 0],
      ['axis', 0.68, 0],
      ['inactive', 0.55, 0],
      ['removed', 0.55, 0],
      ['range', 0.8, 0],
      ['unchanged', 0.6, 1],
    ]) {
      const result = await evaluate(`reconcileFollower(${JSON.stringify(target)}, ${JSON.stringify(change)})`);
      expect(result.before, `${target} ${change}: the release follower is in flight`).toBeCloseTo(0.6, 3);
      expect(result.after, `${target} ${change}: the current timeline wins immediately`).toBeCloseTo(after, 3);
      expect(result.followers, `${target} ${change}: only a still-bound consumer keeps its follower`).toBe(followers);
      expect(result.frames, `${target} ${change}: only a changed moving resolution enters per-frame seeking`).toBe(['renamed-axis', 'axis', 'range'].includes(change) ? 1 : 0);
    }
  } finally {
    await suite.close();
    server.close();
  }
}, 60000);

const FOLLOWER_SOURCE = `keyframes fade
  from opacity=1
  to opacity=0.2

component App
  state mode = 0
  state tick = 0
  state x = 0
  state y = 0
  state otherX = 0
  state otherY = 0
  action released
    x = 260
    y = 180
    otherX = 220
    otherY = 140
  action reset
    mode = 0
    x = 0
    y = 0
    otherX = 0
    otherY = 0
  action axis
    mode = 1
  action range
    mode = 2
  action rebind
    mode = 3
  action remove
    mode = 4
  action direction
    mode = 5
  action inactive
    mode = 6
  action unchanged
    tick = tick + 1
  view
    column width=420 height=700 gap=8 padding=8
      row gap=6 flex-wrap="wrap"
        button testId="axis" press=axis
          text "axis"
        button testId="range" press=range
          text "range"
        button testId="rebind" press=rebind
          text "rebind"
        button testId="remove" press=remove
          text "remove"
        button testId="direction" press=direction
          text "direction"
        button testId="inactive" press=inactive
          text "inactive"
        button testId="unchanged" press=unchanged
          text "unchanged"
        button testId="reset" press=reset
          text "reset"
        button testId="release" press=released
          text "release"
      box testId="scope" timeline-scope="--drag, --other, --inactive" width=360 height=500 position="relative" overflow="hidden"
        box testId="consumer" position="absolute" left=0 top=0 width=40 height=40 opacity=0.55 background-color="#000000" animation=(mode == 4 ? "none" : (mode == 5 ? "fade 1s linear reverse both" : "fade 1s linear both")) animation-timeline=(mode == 4 ? "auto" : (mode == 3 ? "--other" : (mode == 6 ? "--inactive" : "--drag"))) animation-range=(mode == 2 ? "0px 600px" : "0px 300px")
        box testId="handle" swiperight=released touch-action="pan-y" position="absolute" left=0 top=50 width=300 height=80 transition="translate -exact-spring(300, 30, 1)"
        box id="other" testId="other" position="absolute" left=0 top=230 width=300 height=200 translate=\`\${otherX}px \${otherY}px\` transition="translate -exact-spring(300, 30, 1)" -exact-drag-timeline="--other x"
        box id="source" testId="source" position="absolute" left=0 top=140 width=300 height=80 translate=\`\${x}px \${y}px\` transition="translate -exact-spring(300, 30, 1)" -exact-drag-timeline=(mode == 1 ? "--drag y" : "--drag x")
`;

check('a real commit rebinds a spring follower on the wasm and JS runtimes', async () => {
  const tmp = mkdtempSync(resolve(tmpdir(), 'exact-follower-runtime-'));
  const plan = resolve(tmp, 'follower.plan'), jsDist = resolve(tmp, 'js'), wasmDist = resolve(tmp, 'wasm');
  const run = (command, args, env = {}) => {
    const result = spawnSync(command, args, { cwd: ROOT, env: { ...process.env, ...env }, encoding: 'utf8', maxBuffer: 64 << 20 });
    expect(result.status, `${command} ${args.join(' ')}\n${result.stderr}`).toBe(0);
  };
  let session;
  try {
    writeFileSync(resolve(tmp, 'follower.contract'), FOLLOWER_SOURCE);
    run(compiler, ['build', resolve(tmp, 'follower.contract'), '-o', plan]);
    // Use the lane's prepared Caltrain builds when both paths are set (the plan reads no app data), else build
    // Interaction Gallery's
    const { EXACT_MOTION_DIST: preparedWasm, EXACT_WEB_JS_DIST: preparedJs } = process.env;
    const prepared = Boolean(preparedWasm && preparedJs), app = prepared ? 'caltrain' : 'interaction-gallery';
    if (!prepared) {
      run(process.execPath, ['host/web/build.mjs', app, '--render', 'none'], { EXACT_WEB_DIST: jsDist });
      run(process.execPath, ['host/web/build.mjs', app, '--wasm'], { EXACT_WEB_DIST: wasmDist });
    }
    for (const [target, dist] of [['wasm', prepared ? resolve(preparedWasm) : wasmDist], ['js', prepared ? resolve(preparedJs) : jsDist]]) {
      session = await open({ host: 'web', plan, app, webDist: dist });
      await session.clock('settle');
      const presentation = `() => {
        const vector = id => { const v=getComputedStyle(document.querySelector('[data-testid="'+id+'"]')).translate.trim().split(/\\s+/); return v[0]==='none'?[0,0]:[parseFloat(v[0]),parseFloat(v[1]??'0')]; };
        const consumer=document.querySelector('[data-testid="consumer"]'), animations=consumer.getAnimations();
        return {source:vector('source'),other:vector('other'),opacity:parseFloat(getComputedStyle(consumer).opacity),
          followers:animations.filter(a=>a.animationName===undefined).length,
          consumerAnimations:animations.map(a=>[a.animationName,a.playState,a.currentTime]),
          sourceAnimations:document.querySelector('[data-testid="source"]').getAnimations().map(a=>[a.animationName,a.playState,a.currentTime]),
          syncs:globalThis.__followerSyncs??0,
          sameBasis:globalThis.__followerBasis===undefined||globalThis.__followerBasis===animations.find(a=>a.animationName!==undefined)};
      }`;
      const read = () => session.carrier.evaluate(`(${presentation})()`);
      const expected = (kind, sample) => {
        if (kind === 'remove' || kind === 'inactive') return 0.55;
        const value = kind === 'axis' ? sample.source[1] : kind === 'rebind' ? sample.other[0] : sample.source[0];
        const p = Math.max(0, Math.min(1, value / (kind === 'range' ? 600 : 300)));
        return kind === 'direction' ? 0.2 + 0.8 * p : 1 - 0.8 * p;
      };
      for (const kind of ['axis', 'range', 'rebind', 'remove', 'direction', 'inactive', 'unchanged']) {
        await session.tap('reset'); await session.clock('settle');
        await session.tap('handle', { down: true });
        await session.pointer('move', { dx: 90, dy: 0, ms: 80 });
        await session.pointer('up');
        await session.clock('+120');
        const before = await read();
        expect(before.followers, `${target} ${kind}: release made a compositor follower: ${JSON.stringify(before)}`).toBe(1);
        await session.carrier.evaluate(`globalThis.__followerBasis=document.querySelector('[data-testid="consumer"]').getAnimations().find(a=>a.animationName!==undefined)`);
        if (target === 'js') await session.carrier.evaluate(`globalThis.__followerSyncs=0;if(!globalThis.__followerOriginalSync){globalThis.__followerOriginalSync=exact.synced;exact.synced=()=>{globalThis.__followerSyncs++;return globalThis.__followerOriginalSync()}}`);
        await session.tap(kind);
        const samples = [await read()];
        for (const advance of ['+120', '+240', '+480']) { await session.clock(advance); samples.push(await read()); }
        await session.clock('settle'); samples.push(await read());
        for (const [i, sample] of samples.entries()) expect(sample.opacity, `${target} ${kind} sample ${i}`).toBeCloseTo(expected(kind, sample), 3);
        if (target === 'js') expect(samples[0].syncs, `${target} ${kind}: the agent does not replace post-commit reconciliation`).toBe(0);
        expect(samples[0].followers, `${target} ${kind}: only an unchanged binding retains its follower`).toBe(kind === 'unchanged' ? 1 : 0);
        if (kind === 'direction') expect(samples[0].sameBasis, `${target}: direction changed the CSS animation in place`).toBe(true);
        if (!['remove', 'inactive', 'unchanged'].includes(kind)) {
          const values = samples.map(s => kind === 'axis' ? s.source[1] : kind === 'rebind' ? s.other[0] : s.source[0]);
          expect(Math.max(...values) - Math.min(...values), `${target} ${kind}: the new source keeps moving after the commit`).toBeGreaterThan(1);
        }
      }
      // The agent's clock reconciles timelines on every advance. Open the same
      // plan without it so only the page's frame callback can keep a rebound
      // consumer following its source.
      const liveUrl = new URL(await session.carrier.evaluate('location.href'));
      liveUrl.searchParams.delete('agent');
      await session.carrier.call('Page.navigate', { url: liveUrl.href });
      for (const bound = Date.now() + 10000;;) {
        const booted = await session.carrier.evaluate(`location.href === ${JSON.stringify(liveUrl.href)} && Boolean(document.getElementById('exact-root')?.dataset.bootMs) && Boolean(globalThis.exact?.ready)`).catch(() => false);
        if (booted) break;
        if (Date.now() >= bound) throw new Error(`${target}: the live page did not boot`);
        await Bun.sleep(50);
      }
      await session.carrier.evaluate('exact.ready');
      if (target === 'wasm') await session.carrier.evaluate(`fetch('/__plan').then(r => r.arrayBuffer()).then(b => exact.reload(new Uint8Array(b), true)).then(() => exact.ready)`);
      else await session.carrier.evaluate(`(async () => {
        const bound = performance.now() + 10000;
        while (!exact.synced || exact.inflight.n) {
          if (performance.now() >= bound) throw new Error('the live motion engine did not load');
          await new Promise(requestAnimationFrame);
        }
      })()`);
      for (const kind of ['unchanged', 'range', 'rebind']) {
        const { before, samples, agent } = await session.carrier.evaluate(`(async () => {
          const read = ${presentation};
          const tap = id => document.querySelector('[data-testid="'+id+'"]').click();
          const springs = () => ['source', 'other'].flatMap(id => document.querySelector('[data-testid="'+id+'"]').getAnimations())
            .filter(a => a.animationName === undefined);
          tap('reset');
          await Promise.all(springs().map(a => a.finished));
          tap('release');
          // Start sampling in flight, with all compositor copies at the same point.
          for (const a of document.getAnimations().filter(a => a.animationName === undefined)) a.currentTime = 120;
          const before = read();
          tap('${kind}');
          const moving = springs(), samples = [read()];
          if (moving.length !== 2) throw new Error('expected two release springs');
          await new Promise((resolve, reject) => {
            let frame;
            const bound = setTimeout(() => {
              cancelAnimationFrame(frame);
              reject(new Error('release springs did not finish: ' + JSON.stringify(samples.at(-1))));
            }, 5000);
            const sample = () => {
              samples.push(read());
              if (moving.every(a => a.playState === 'finished')) {
                clearTimeout(bound); resolve();
              } else next();
            };
            // Read after all callbacks in this frame, including the host's seek.
            const next = () => { frame = requestAnimationFrame(() => setTimeout(sample, 0)); };
            next();
          });
          return { before, samples, agent: typeof exact.agentSettled === 'function' };
        })()`);
        expect(agent, `${target}: sampling runs without the agent`).toBe(false);
        expect(before.followers, `${target} ${kind}: release made a compositor follower`).toBe(1);
        expect(samples[0].followers, `${target} ${kind}: a changed binding hands off to frame seeking`).toBe(kind === 'unchanged' ? 1 : 0);
        for (const [i, sample] of samples.entries()) expect(sample.opacity, `${target} live ${kind} sample ${i}`).toBeCloseTo(expected(kind, sample), 3);
        const values = samples.map(s => kind === 'rebind' ? s.other[0] : s.source[0]);
        const intermediate = values.findIndex((v, i) => i > 0 && i < values.length - 1 && Math.abs(v - values[0]) > 1 && Math.abs(v - values.at(-1)) > 1);
        expect(intermediate, `${target} ${kind}: presentation advances through an intermediate value`).toBeGreaterThan(0);
        expect(Math.abs(samples[intermediate].opacity - samples[0].opacity), `${target} ${kind}: the consumer keeps moving without the agent`).toBeGreaterThan(0.001);
        const end = samples.at(-1);
        expect(end.source[0], `${target} ${kind}: the source finished`).toBeCloseTo(260, 2);
        expect(end.other[0], `${target} ${kind}: the rebound source finished`).toBeCloseTo(220, 2);
        expect(end.followers, `${target} ${kind}: the completed follower is retired`).toBe(0);
      }
      await session.close(); session = null;
    }
  } finally {
    await session?.close();
    rmSync(tmp, { recursive: true, force: true });
  }
}, 600000);
