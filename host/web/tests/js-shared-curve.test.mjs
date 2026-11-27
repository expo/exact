// The JS target's shared-element curve (host/web-js/shared.js `curve`) reads the
// `--exact-layout-transition` row the web host writes (css.rs
// `layout_transition_css`): a spring there is `spring` and its curve as a
// `linear()`, whose commas must not split it.
import { afterAll, test, expect } from 'bun:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { springCurve } from './motion-scenarios.mjs';
import { stageRuntime } from './stand-ins.mjs';

const dir = mkdtempSync(resolve(tmpdir(), 'exact-js-shared-curve-'));
afterAll(() => rmSync(dir, { recursive: true, force: true }));
stageRuntime(dir, ['shared.js']);
const { curve } = await import(resolve(dir, 'shared.js'));

const row = text => ({ style: { getPropertyValue: name => (name === '--exact-layout-transition' ? text : '') } });

test('a spring row keeps its linear() curve whole', () => {
  const { ms, easing } = springCurve([300, 30, 1]);
  expect(curve(row(`${ms} 100 spring ${easing}`))).toEqual({ duration: ms, delay: 100, timing: easing });
});

test('an eased row keeps its duration, delay and timing function', () => {
  expect(curve(row('300 50 cubic-bezier(0.4, 0, 0.2, 1)'))).toEqual({ duration: 300, delay: 50, timing: 'cubic-bezier(0.4, 0, 0.2, 1)' });
  expect(curve(row('none'))).toBe(null);
});
