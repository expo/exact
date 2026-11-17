// @ref LLP 1012 §1 `tap` — a tap addressed by id presses what it names (scripts/agent-aim.mjs): a row with a press of
// its own is clicked beside the link card at its middle, a box without one never presses the control inside it, and
// `tap <target> at <x> <y>` keeps a point's semantics (the Bluesky clone's likes on real people's posts, 2026-10-09).
import { afterAll, test, expect } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync, existsSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { createServer } from 'node:http';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { open } from '../../scripts/agent.mjs';
import { namedRefusal, pageAim, pageOutside } from '../../scripts/agent-aim.mjs';
import { tapWords } from '../../scripts/agent-keys.mjs';
import { chromium } from '../../scripts/agent-launch.mjs';
import { serveBuildTree } from './serve.mjs';
import { browserMissing } from '../../scripts/agent-playwright.mjs';
import { need } from '../../scripts/coverage.mjs';
import { fileURLToPath } from 'node:url';

const ROOT = resolve(fileURLToPath(new URL('../..', import.meta.url)));
const compiler = resolve(process.env.CARGO_TARGET_DIR ?? resolve(ROOT, 'target'), 'debug', process.platform === 'win32' ? 'contract.exe' : 'contract');

const CONTRACT = `component AimFixture
  state post = 0
  state card = 0
  state like = 0
  action openPost
    post = post + 1
  action openCard
    card = card + 1
  action liked
    like = like + 1
  view
    column testId="root" gap=8 width=300
      column testId="post-0" press=openPost padding-left=70 padding-right=70 padding-top=30 padding-bottom=30 background-color="#eeeeee"
        column testId="card-0" press=openCard width=160 height=60 background-color="#cccccc"
      column testId="wrap-0" padding-left=100 padding-right=100 padding-top=20 padding-bottom=20
        button testId="like-0" press=liked width=100 height=40
          text "Like"
      column testId="post-1" press=openPost
        column testId="cover-1" press=openCard width=300 height=60 background-color="#dddddd"
      column testId="post-2" press=openPost padding-left=100 padding-right=100 padding-top=10 padding-bottom=10
        button testId="like-2" press=liked disabled=true width=100 height=40
          text "Like"
`;

test('a refusal names nodes by testId, and `tap <target> at <x> <y>` is a press at a point', () => {
  const nodes = [{ id: 4, props: { testId: 'wrap-0' } }, { id: 5, props: { testId: 'like 0' } }];
  expect(namedRefusal('tap #4 would press #5 inside it; tap #5, or tap #4 at <x> <y>', nodes)).toBe('tap wrap-0 would press "like 0" inside it; tap "like 0", or tap wrap-0 at <x> <y>');
  expect(namedRefusal('tap #4 at (1, 2): node #5 covers its middle', nodes)).toBe('tap #4 at (1, 2): node #5 covers its middle');
  expect(tapWords(['post-0', 'at', '10', '20'])).toEqual(['post-0', { at: [10, 20] }]);
  expect(tapWords(['post-0', 'at', '10', '20', 'modifiers', 'Shift'])).toEqual(['post-0', { at: [10, 20], modifiers: 'Shift' }]);
  expect(() => tapWords(['post-0', 'at', '10'])).toThrow(/takes a number/);
});

// A sheet a button presents: under the agent its slide up runs on the drive's clock, so until the clock moves a control
// in it is below the viewport, where no scroll reaches it.
const SHEET = `component SheetFixture
  state stack = ["home"]
  state gentler = false
  derive top = match stack.at(-1) { case some(r) => r, case none => "" }
  action present
    stack = ["home", "sheet"]
  action gentle
    gentler = true
  view
    main navigationKey=top navigationBack="" width="100%" height="100%"
      for r in stack key=r
        column navigationKey=r navigationPresentation=(r == "sheet" ? "modal" : "") position="absolute" inset=0
          if r == "home"
            button "Settings" press=present testId="settings"
            column height=100 overflow="clip"
              box height=2000 flex-shrink=0
              progress testId="far"
          if r == "sheet"
            progress testId="busy"
            button "Gentler" press=gentle testId="gentler"
`;

const fixtures = new Map(), dirs = [];
afterAll(() => { for (const dir of dirs) rmSync(dir, { recursive: true, force: true }); });
const built = (source = CONTRACT) => {
  if (!fixtures.has(source)) fixtures.set(source, (async () => {
    const dir = mkdtempSync(join(tmpdir(), 'exact-aim-'));
    dirs.push(dir);
    const contract = join(dir, 'app.contract'), plan = join(dir, 'app.plan'), dist = join(dir, 'dist');
    writeFileSync(contract, source);
    const compile = spawnSync(compiler, ['build', contract, '-o', plan], { encoding: 'utf8' });
    if (compile.status !== 0) throw new Error(compile.stderr);
    const build = spawnSync(process.execPath, ['host/web-js/build.mjs', 'caltrain', '--plan', plan, '--out', dist, '--render', 'none'], { cwd: ROOT, encoding: 'utf8' });
    if (build.status !== 0) throw new Error(build.stderr);
    const server = createServer((request, response) => serveBuildTree(dist, request, response));
    await new Promise((resolve) => server.listen(0, '127.0.0.1', resolve));
    server.unref();
    return `http://127.0.0.1:${server.address().port}/`;
  })());
  return fixtures.get(source);
};

const drive = async (session) => {
  const slots = async () => (await session.state()).slots;
  const row = await session.tap('post-0');
  expect(await slots()).toMatchObject({ post: 1, card: 0 });
  expect(row.avoided?.pressing).toBeGreaterThan(0);
  await expect(session.tap('wrap-0')).rejects.toThrow('tap wrap-0 would press like-0 inside it');
  await expect(session.tap('post-1')).rejects.toThrow(/tap post-1 would press cover-1 inside it, at its middle; no point of post-1/);
  await session.tap('post-2'); // a disabled Like at its middle presses nothing: the row, beside it
  expect(await slots()).toMatchObject({ post: 2, card: 0, like: 0 });
  await session.tap('post-0', { at: [150, 60] }); // a point: the card a finger there reaches
  await session.tap('wrap-0', { at: [150, 40] });
  await session.tap('card-0');
  expect(await slots()).toMatchObject({ post: 2, card: 2, like: 1 });
};

need('chrome', chromium().unavailable)('a plain web tap presses what it names: beside the card, never Like in a box without a press, refused when covered', async () => {
  const session = await open({ host: 'web', url: await built() });
  try {
    await drive(session);
    // In the page: a surface's action button over its canvas is a recipient of its own (gpu-glue.js), and a reachable
    // perimeter 2 px wide around a child that covers the rest is found (Astra, round 1).
    const probe = await session.carrier.evaluate(`(() => {
      const box = (css, attrs = {}) => { const e = document.createElement(attrs.tag ?? 'div'); e.style.cssText = css; for (const [k, v] of Object.entries(attrs)) if (k !== 'tag') e.setAttribute(k, v); return e; };
      const canvas = box('position:fixed;left:0;top:400px;width:200px;height:100px;z-index:9', { 'data-gpu-input': '' });
      canvas.append(box('position:absolute;left:50px;top:25px;width:100px;height:50px', { tag: 'button', 'data-action': 'fire' }));
      const ring = box('position:fixed;left:220px;top:400px;width:100px;height:100px;z-index:9', { 'data-exact-on': 'press' });
      ring.append(box('position:absolute;left:2px;top:2px;width:96px;height:96px', { 'data-exact-on': 'press' }));
      document.body.append(canvas, ring);
      exact.views.set(990001, canvas); exact.views.set(990002, ring);
      const aim = (${pageAim})({ id: 990001, x: 100, y: 450 }), edge = (${pageAim})({ id: 990002, x: 270, y: 450 });
      exact.views.delete(990001); exact.views.delete(990002); canvas.remove(); ring.remove();
      return { aim, edge };
    })()`);
    expect(probe.aim?.at).toBeDefined();
    expect(probe.aim.at[0] < 50 || probe.aim.at[0] >= 150 || probe.aim.at[1] < 425 || probe.aim.at[1] >= 475).toBe(true);
    expect(probe.edge?.at).toBeDefined();
    const [ex, ey] = probe.edge.at;
    expect(ex < 222 || ex >= 318 || ey < 402 || ey >= 498).toBe(true);
  } finally { await session.close(); }
}, 900000);

need('firefox', await browserMissing('firefox'))('the same on Firefox (Playwright)', async () => {
  const session = await open({ host: 'web', browser: 'firefox', url: await built() });
  try { await drive(session); } finally { await session.close(); }
}, 900000);

// An indeterminate progress spins forever (its `::before`), which moves no box: below the viewport it keeps the scroll
// advice, and in a sheet still sliding up the refusal names the sheet's slide.
const sheetDrive = async (session) => {
  const refusal = (target) => session.tap(target).then(() => null, (error) => error.message);
  expect(await refusal('far')).toMatch(/its middle is outside the viewport; scroll it into view first$/);
  await session.tap('settings');
  const slide = /its middle is outside the viewport while a translate transition on node #\d+, which holds it, has \d+ ms to run, and the agent's clock runs it: advance the clock first \(`clock \+\d+` or `clock settle`\), not a scroll$/;
  expect(await refusal('busy')).toMatch(slide);
  const refused = await refusal('gentler');
  expect(refused).toMatch(slide);
  await session.clock(`+${/`clock \+(\d+)`/.exec(refused)[1]}`);
  await session.tap('gentler');
  expect((await session.state()).slots.gentler).toBe(true);
};

// In the page, boxes below the viewport, each animated as no fixture app is. The scroll advice stays for one that spins
// forever about its middle, one that slides forever, one turned a full turn by a finite `transform` about its middle, and
// one whose slide into view its author paused (the clock holds it); one whose slide into view the clock runs, and a
// forever spinner inside it, are told the time left, after which the box is in view.
const SCROLL = 'its middle is outside the viewport; scroll it into view first';
async function probe(session) {
  const outside = (ids) => session.carrier.evaluate(`JSON.stringify(${JSON.stringify(ids)}.map((id) => (${pageOutside})({ id })))`).then(JSON.parse);
  await session.carrier.evaluate(`(() => {
    const box = (id, css = 'position:fixed;left:0;top:2000px;width:100px;height:100px', parent = document.body) => {
      const e = document.createElement('div'); e.style.cssText = css; parent.append(e); exact.views.set(id, e); return e;
    };
    // As a transition slides a box in: from below the viewport to where its style puts it, at y 500.
    const into = { translate: ['0 1500px', '0 0'] }, above = 'position:fixed;left:0;top:500px;width:100px;height:100px';
    box(990010).animate({ rotate: ['0turn', '1turn'] }, { duration: 1000, iterations: Infinity });
    box(990011).animate({ translate: ['0 0', '0 10px'] }, { duration: 1000, iterations: Infinity });
    box(990012).animate({ transform: ['rotate(0deg)', 'rotate(360deg)'] }, 400);
    box(990013, above).animate(into, 400).pause();
    const sheet = box(990014, above);
    sheet.animate(into, 400);
    box(990015, 'width:50px;height:50px', sheet).animate({ rotate: ['0turn', '1turn'] }, { duration: 1000, iterations: Infinity });
  })()`);
  // The clock takes the running ones as its own, paused where they started; the one paused already stays its author's.
  await session.clock('+1');
  const said = await outside([990010, 990011, 990012, 990013, 990014, 990015]);
  expect(said.slice(0, 4)).toEqual(Array(4).fill(SCROLL));
  expect(said[4]).toMatch(/^its middle is outside the viewport while the animation on it has 399 ms to run, .*`clock \+399`/);
  expect(said[5]).toMatch(/^its middle is outside the viewport while the animation on node #990014, which holds it, has 399 ms to run, .*`clock \+399`/);
  await session.clock('+399');
  const y = await session.carrier.evaluate('exact.views.get(990014).getBoundingClientRect().y');
  expect(y).toBe(500);
  expect(await outside([990013])).toEqual([SCROLL]);
  await session.carrier.evaluate('for (const id of [990010, 990011, 990012, 990013, 990014, 990015]) { exact.views.get(id).remove(); exact.views.delete(id); }');
}

need('chrome', chromium().unavailable)('a tap on a control a presentation has not yet brought into view says to advance the clock, not to scroll', async () => {
  const session = await open({ host: 'web', url: await built(SHEET) });
  try {
    await sheetDrive(session);
    await probe(session);
  } finally { await session.close(); }
}, 900000);

need('firefox', await browserMissing('firefox'))('the same refusals on Firefox (Playwright)', async () => {
  const session = await open({ host: 'web', browser: 'firefox', url: await built(SHEET) });
  try { await sheetDrive(session); await probe(session); } finally { await session.close(); }
}, 900000);
