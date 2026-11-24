// expo/exact#1412 driven on macOS: the module's `native.call` and its
// `native.later` each animate the window 300 by 200 points larger and back
// (`NSWindow.setFrame(_:display:animate:)`) to the frame asked for, and every
// step of each animation is laid out at its size with no document larger than
// the viewport.
// `EXACT_WINDOW_FRAME_FILM=<dir>` draws each step there as a PNG.
//
// usage: bun scripts/fixtures/window-frame/window-frame.drives.mjs, the app built first with
//   EXACT_APP_DIR=scripts/fixtures/window-frame bun host/apple/build.mjs window-frame-apple
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const root = resolve(import.meta.dir, '../../..');
const ops = ['tap report-steps', 'clock settle'];
for (const press of ['grow', 'shrink', 'grow-later', 'shrink-later']) ops.push(`tap ${press}`, 'clock +1000 real', 'tap report-steps', 'clock settle', 'tree report');
const run = spawnSync('bun', ['scripts/agent.mjs', 'macos', '--json', ...ops], { cwd: root, encoding: 'utf8', env: { ...process.env, EXACT_APP_DIR: import.meta.dir } });
const reports = run.stdout.split('\n').filter((l) => l.startsWith('{')).map((l) => JSON.parse(l))
  .flatMap((r) => (r.nodes ?? []).filter((n) => n.props?.testId === 'report').map((n) => n.props.text));
let failures = 0;
const parsed = reports.map((r) => { const [steps, laidOut, overflowed, size] = r.split(' '); return { steps: +steps, laidOut: +laidOut, overflowed: +overflowed, size: size.split('x').map(Number) }; });
['grow', 'shrink', 'grow later', 'shrink later'].forEach((what, i) => {
  const { steps, laidOut, overflowed, size } = parsed[i] ?? {};
  // Each grow ends 300 by 200 larger than the shrink after it.
  const to = parsed[i - (i % 2)]?.size, from = parsed[i - (i % 2) + 1]?.size;
  const ok = steps > 1 && laidOut === steps && overflowed === 0 && to?.[0] - from?.[0] === 300 && to?.[1] - from?.[1] === 200;
  if (!ok) failures++;
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}: ${steps} steps, ${laidOut} laid out at their size, ${overflowed} with a document larger than the viewport, ending ${size?.join('x')}`);
});
if (reports.length < 4) console.log(run.stdout.slice(-2000), run.stderr.slice(-2000));
process.exit(failures ? 1 : 0);
