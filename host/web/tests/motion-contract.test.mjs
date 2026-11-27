// The web's layout moves keep what a move promises (LLP 1063 D10, `motion-check.mjs`): seeded scenarios are played on
// the module (`presence-glue.js`) under the page's own clock and sampled frame by frame, and each frame is held to where
// the promised algorithm puts each box. Where a box is laid out is read from its twin, the same box with no row, which
// the module never touches; where it is shown, and whether the page still holds anything of a move, from the box. So a
// rewrite of the module is held to the same promises, with the browser's own rounding on both sides.
import { afterAll, expect } from 'bun:test';
import { resolve } from 'node:path';
import process from 'node:process';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { engineTests } from '../../../scripts/coverage.mjs';
import { checkMoves, reference } from '../motion-check.mjs';
import { moduleRoute, serveModules } from './module-server.mjs';
import { layouts, scenario } from './motion-scenarios.mjs';

const PRESENCE = resolve(import.meta.dirname, '../presence-glue.js');
const engineTest = await engineTests(['chromium', 'webkit', 'firefox']);

// The page: plays a scenario on the module under `animationClock`, which the agent's clock is, letting the module's
// promises settle after each step, and samples each box: where it is shown in its parent (its surface while its size
// moves, as the page reports drawing it, and for a plain box its own content too) and whether anything of a move (an
// animation, a stand-in) is left on it
const page = `<!doctype html><style>body { margin:0 } #root, #twins { position:absolute; left:0; top:0; width:800px; height:600px }
  #twins { visibility:hidden } .box { position:absolute; box-sizing:border-box; background:#0a0 }
  .column { position:absolute; left:480px; top:300px; display:flex; flex-direction:column; writing-mode:horizontal-tb }</style>
<div id="root"></div><div id="twins"></div>
<script>globalThis.exact = {};</script>
<script type="module">
  const { animationClock } = await import('./navigation.js');
  await import('./presence-glue.js');
  const root = document.getElementById('root'), twins = document.getElementById('twins');
  window.play = async ({ boxes, flow, commits, vertical }, { phase, until, ends }) => {
    root.replaceChildren(); twins.replaceChildren();
    for (const parent of [root, twins]) parent.style.writingMode = vertical ? 'vertical-rl' : '';
    const live = exact.presence(root), views = new Map(), twin = new Map(), clock = animationClock(() => 0, () => 0, () => live.sync?.());
    const row = curve => curve ? \`;--exact-layout-transition:\${curve.ms} \${curve.delay} \${curve.spring ? 'spring ' : ''}\${curve.easing}\` : '';
    const place = (b, [x, y, w, h]) => \`left:\${x}px;top:\${y}px;width:\${w}px;height:\${h}px;rotate:\${b.rotate}deg;scale:\${b.scale};translate:\${b.translate[0]}px \${b.translate[1]}px;border:\${b.border}px solid #060\`;
    const styles = { above: h => \`width:\${flow.width}px;height:\${h}px\` };
    // Each box, and its twin with no row in the twin of its parent
    const make = (id, tag, parent, style, curve, className = '') => {
      for (const [into, map, css] of [[views.get(parent) ?? root, views, style + row(curve)], [twin.get(parent) ?? twins, twin, style]]) {
        const el = document.createElement(tag);
        el.className = className; el.style.cssText = css;
        // (A view id is a number, as the module's observation reads it: the column's are named apart)
        if (map === views) el.dataset.view = { column: '101', above: '102', flow: '103' }[id] ?? id;
        if (tag === 'button') el.dataset.buttonStyle = 'bordered';
        into.append(el);
        map.set(id, el);
      }
    };
    // The first layout: each box made with its style, as the kernel's create gives it, so nothing moves
    const made = { ops: [] };
    live.before(made, views);
    for (const b of boxes) make(b.id, b.button ? 'button' : 'div', b.parent, place(b, b.box), b.curve, 'box');
    if (flow) {
      make('column', 'div', null, '', null, 'column');
      make('above', 'div', 'column', styles.above(flow.above[0]), null);
      make('flow', 'div', 'column', \`width:\${flow.width}px;height:\${flow.height}px;background:#00a\`, flow.curve);
    }
    made.ops = [...views.keys()].map(id => ({ op: 'create', id }));
    live.after(made, views);
    clock.register(0);
    const ids = [...boxes.map(b => b.id), ...(flow ? ['flow'] : [])];
    const parentOf = id => id === 'flow' ? 'column' : boxes.find(b => b.id === id).parent, isPlain = id => id === 'flow' || plain(boxes.find(b => b.id === id));
    const inParent = (box, parent) => [box.x - parent.x, box.y - parent.y, box.width, box.height];
    // Where each box is laid out: its twin, in its parent's twin
    const layout = t => ({ t, laid: Object.fromEntries(ids.map(id => [id, inParent(twin.get(id).getBoundingClientRect(), (twin.get(parentOf(id)) ?? twins).getBoundingClientRect())])) });
    const laid = [layout(0)], samples = [];
    const sample = async (k, t) => {
      clock.seek(t);
      await null;
      const drawn = new Map(live.observation().map(o => [root.querySelector(\`[data-view="\${o.id}"]\`), { x: o.x, y: o.y, width: o.w, height: o.h }]));
      const rect = el => drawn.get(el) ?? el.getBoundingClientRect();
      samples.push({ k, t, nodes: Object.fromEntries(ids.map(id => {
        const el = views.get(id), parent = rect(views.get(parentOf(id)) ?? root);
        const node = { shown: inParent(rect(el), parent), moving: el.getAnimations().length > 0 || !!el.previousElementSibling?.hasAttribute('data-exiting') };
        // A plain box's content: its own box, which keeps its laid-out size while a surface stands in for it
        if (isPlain(id)) node.content = inParent(el.getBoundingClientRect(), parent);
        return [id, node];
      })) });
    };
    const apply = async (t, i, changes) => {
      clock.seek(t);
      await null;
      const styled = Object.entries(changes).map(([id, box]) => { const b = boxes.find(b => b.id === id); return [id, place(b, box), b.curve]; });
      if (flow && flow.above[i + 1] !== flow.above[i]) styled.push(['above', styles.above(flow.above[i + 1]), null]);
      const ops = styled.map(([id, css, curve]) => ({ op: 'style', id, css: css + row(curve) }));
      live.before({ ops }, views);
      for (const [j, [id, css]] of styled.entries()) { views.get(id).style.cssText = ops[j].css; twin.get(id).style.cssText = css; }
      live.after({ ops }, views);
      laid.push(layout(t));
      clock.register(t);
      await null;
    };
    // The frames of each stretch between commits, at the scenario's phase within the frame, which puts them between a
    // spring's stops, and a millisecond either side of each move's end
    const frames = async (k, from, to) => {
      const times = ends.flatMap(e => [e - 1, e + 1]);
      for (let j = 0, t = from + phase; t < to; t = from + phase + ++j * 1000 / 60) times.push(t);
      for (const t of [...new Set(times)].filter(t => t > from && t < to).sort((a, b) => a - b)) await sample(k, t);
    };
    await sample(1, 0);
    for (const [i, c] of commits.entries()) {
      await frames(i + 1, i ? commits[i - 1].t : 0, c.t);
      await sample(i + 1, c.t);
      await apply(c.t, i, c.changes);
      await sample(i + 2, c.t);
    }
    await frames(commits.length + 1, commits.at(-1).t, until);
    await sample(commits.length + 1, until);
    return { commits: laid, samples };
  };
  // A box whose content the page shows apart from its surface: one neither a native button nor turned or scaled
  const plain = b => !b.button && !b.rotate && b.scale === 1;
  window.ready = true;
</script>`;

const server = await serveModules({ '/': [page, 'text/html'], '/presence-glue.js': moduleRoute(PRESENCE) });
afterAll(() => server.close());

/** A scenario's timeline but for its samples, its layouts the scenario's own: each node's curve and anchor. */
function planned(s) {
  // (A box's borders are the least size its surface shows)
  const nodes = Object.fromEntries(s.boxes.map(b => [b.id, { curve: b.curve, anchor: b.button ? 'centre' : 'box', floor: [2 * b.border, 2 * b.border] }]));
  if (s.flow) nodes.flow = { curve: s.flow.curve, anchor: 'box' };
  return { nodes, commits: layouts(s), samples: [] };
}

// Each engine plays the same seeded scenarios (`MOTION_SEEDS` of them); a failure names its seed
const SEEDS = Number(process.env.MOTION_SEEDS ?? 120);
if (!(Number.isInteger(SEEDS) && SEEDS > 0)) throw new Error(`MOTION_SEEDS is ${process.env.MOTION_SEEDS}, which is no count of scenarios: set it to a positive whole number, or unset it for 120`);
for (const engine of ['chromium', 'webkit', 'firefox']) engineTest[engine](`${engine}: layout moves keep what a move promises`, async () => {
  const browser = await suiteBrowser(engine), tab = await browser.newPage({ viewport: { width: 800, height: 600 } });
  try {
    await tab.goto(`${server.url}/`);
    await tab.waitForFunction(() => globalThis.ready);
    const broken = [];
    for (let seed = 1; seed <= SEEDS; seed++) {
      const s = scenario(seed), timeline = planned(s);
      // Recorded until every move the scenario makes has ended (its own layouts say when, to within the browser's
      // rounding), and a little on
      // (A move a restart ends, ends at the restart)
      const ends = Object.keys(timeline.nodes).flatMap(id => reference(timeline, id).moves.map(m => m.cut ? timeline.commits[m.cut - 1].t : m.end));
      const until = Math.max(s.commits.at(-1).t, ...ends) + 100, phase = 1 + (seed * 7919 % 15);
      try {
        Object.assign(timeline, await tab.evaluate(([s, options]) => globalThis.play(s, options), [s, { phase, until, ends }]));
      } catch (e) { broken.push({ seed, played: String(e) }); continue; }
      const errors = checkMoves(timeline);
      if (errors.length) broken.push({ seed, errors: errors.slice(0, 6), of: errors.length });
    }
    expect(broken).toEqual([]);
  } finally { await browser.close(); }
}, 180000);
