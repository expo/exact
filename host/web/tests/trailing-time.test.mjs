// LLP 1168.000 D6.1: a time on a bubble's last line (docs/recipes/chat-header.md). The text ends with a hidden copy of
// the time, which holds room exactly as wide as it, and the time is drawn over the bubble's corner: a short last
// line keeps the time beside it, a full one pushes it to a line of its own. Checked on the JS target in Chrome, from
// the driver's layout frames and the message text's line boxes (a DOM Range's rects, grouped by line).
import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { join, resolve } from 'node:path';
import { runTests } from '../../../scripts/agent-test.mjs';
import { serveBuildTree } from '../serve.mjs';

const ROOT = resolve(import.meta.dir, '../../..');
// Bubbles 220 points wide at most: a short message, two lines whose second is short, a short right-to-left one, whose
// time is drawn at the bubble's left (its line's end), and lines that end in a word ever nearer the bubble's edge, some
// of which leave no room for the time.
// Twelve lengths, 6 to 39 letters, so a face or size other than the default still meets both outcomes.
const EDGES = Array.from({ length: 12 }, (_, i) => [`edge${i}`, `Meet at ${'abcdefghijklmnopqrstuvwxyz'.repeat(2).slice(0, 6 + 3 * i)}`]);
const MESSAGES = [
  ['short', 'See you'],
  ['two', 'Running late,\nsorry'],
  ['rtl', 'נתראה מחר'],
  // Ending in a left-to-right word: the hidden copy's leading RLM keeps it at the paragraph's (left) end, not after `Zoom`.
  ['mixed', 'נתראה ב-Zoom'],
  ...EDGES,
];
const CONTRACT = `shape Message
  id: string
  body: string
  time: string
  rtl: bool

component Chat
  resource thread = loadThread() as shape Message[] else empty()
  view
    column padding=12 gap=8 align-items="flex-start"
      for m in thread key=m.id
        if m.rtl
          box position="relative" direction="rtl" max-width=220 padding="6px 10px" border-radius=16 background-color="-exact-fill" testId=\`bubble-\${m.id}\`
            text white-space="pre-wrap"
              text m.body testId=\`words-\${m.id}\`
              text \`\u200f  \${m.time}\` opacity=0 aria-hidden="true"
            text m.time position="absolute" left=8 bottom=4 font="-exact-caption2" color="-exact-secondary-label" testId=\`time-\${m.id}\`
        else
          box position="relative" max-width=220 padding="6px 10px" border-radius=16 background-color="-exact-fill" testId=\`bubble-\${m.id}\`
            text white-space="pre-wrap"
              text m.body testId=\`words-\${m.id}\`
              text \`  \${m.time}\` opacity=0 aria-hidden="true"
            text m.time position="absolute" right=8 bottom=4 font="-exact-caption2" color="-exact-secondary-label" testId=\`time-\${m.id}\`
`;
const TS = `import type { Answer, Sources } from './app.contract.d.ts';
export const grants = '';
const sources: Sources = { loadThread: () => ${JSON.stringify(MESSAGES.map(([id, body]) => ({ id, body, time: '9:41', rtl: id === 'rtl' || id === 'mixed' })))} };
export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);
`;

test('a time sits on a short last line and drops to its own line when the last line is full', async () => {
  const dir = mkdtempSync(join(ROOT, 'target/trailing-time-'));
  const contract = join(dir, 'app.contract'), plan = join(dir, 'app.plan'), dist = join(dir, 'dist'), file = join(dir, 'app.test.contract');
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Trailing', app: { id: 'test.exact.trailing', name: 'Trailing' }, host: {}, deploy: {} }));
  writeFileSync(contract, CONTRACT);
  writeFileSync(join(dir, 'app.ts'), TS);
  writeFileSync(file, 'test "bubbles"\n  clock data\n');
  const compile = spawnSync(join(process.env.CARGO_TARGET_DIR ?? join(ROOT, 'target'), 'debug', 'contract'), ['build', contract, '-o', plan], { cwd: ROOT, encoding: 'utf8' });
  expect(compile.status, compile.stderr).toBe(0);
  const build = spawnSync(process.execPath, ['host/web-js/build.mjs', 'trailing', '--plan', plan, '--out', dist, '--render', 'none'], { cwd: ROOT, encoding: 'utf8', env: { ...process.env, EXACT_APP_DIR: dir } });
  expect(build.status, build.stderr).toBe(0);
  const server = createServer((request, response) => serveBuildTree(dist, request, response));
  await new Promise((ok) => server.listen(0, '127.0.0.1', ok));
  const seen = {};
  try {
    const r = await runTests({ host: 'web', file, app: dir, url: `http://127.0.0.1:${server.address().port}/`, atEnd: async (s) => {
      // Laid out at rest: the faces loaded and every frame drawn (one run of ten under load read a frame early).
      await s.clock('settle');
      const nodes = (await s.tree()).nodes, layout = (await s.layout()).nodes;
      const id = (testId) => nodes.find((n) => n.props?.testId === testId)?.id;
      const box = (testId) => layout.find((n) => n.id === id(testId));
      for (const [name] of MESSAGES) {
        // The message's own words' line boxes: a Range over its text, its rects grouped by their top.
        const lines = await s.carrier.evaluate(`(() => {
          const el = exact.views.get(${id(`words-${name}`)}), range = document.createRange();
          range.selectNodeContents(el);
          const byTop = new Map();
          for (const r of range.getClientRects()) if (r.width > 0) {
            const top = Math.round(r.top), line = byTop.get(top) ?? { top: r.top, bottom: r.bottom, left: r.left, right: r.right };
            byTop.set(top, { ...line, left: Math.min(line.left, r.left), right: Math.max(line.right, r.right) });
          }
          return [...byTop.values()].sort((a, b) => a.top - b.top);
        })()`);
        seen[name] = { bubble: box(`bubble-${name}`), time: box(`time-${name}`), lines };
      }
      return {};
    } });
    expect(r.results[0].failures).toEqual([]);
  } finally {
    server.close();
    rmSync(dir, { recursive: true, force: true });
  }
  const middle = (b) => b.y + b.h / 2;
  const inside = ({ bubble, time }) => time.x >= bubble.x - 0.5 && time.x + time.w <= bubble.x + bubble.w + 0.5 && time.y + time.h <= bubble.y + bubble.h + 0.5;
  // Beside the last line: its middle within that line, after its end; else on a line of its own, below the text.
  const beside = ({ time, lines }) => middle(time) > lines.at(-1).top && middle(time) < lines.at(-1).bottom;
  const holds = (bubble, rtl = false) => {
    const last = bubble.lines.at(-1);
    expect(inside(bubble)).toBe(true);
    if (!beside(bubble)) return expect(bubble.time.y).toBeGreaterThanOrEqual(last.bottom - 1), 'below';
    if (rtl) expect(bubble.time.x + bubble.time.w).toBeLessThanOrEqual(last.left);
    else expect(bubble.time.x).toBeGreaterThanOrEqual(last.right);
    return 'beside';
  };
  // A short message: one line, the time on it, after its last word.
  expect(seen.short.lines.length).toBe(1);
  expect(holds(seen.short)).toBe('beside');
  // Two lines, the second short: the time beside the second.
  expect(seen.two.lines.length).toBe(2);
  expect(holds(seen.two)).toBe('beside');
  // Right to left: the line ends at its left, and so does the time, before the last word.
  for (const name of ['rtl', 'mixed']) {
    expect(seen[name].lines.length).toBe(1);
    expect(holds(seen[name], true)).toBe('beside');
  }
  // A last line that leaves no room drops the time to a line of its own; one that does keeps it beside. Both happen.
  const edges = EDGES.map(([name]) => holds(seen[name]));
  expect(edges).toContain('beside');
  expect(edges).toContain('below');
}, 120_000);
