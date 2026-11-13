// LLP 1039 D9 on the wasm target: the page's geometry (the safe-area insets,
// the segments, the keyboard's overlap) never reaches the runner before the
// size that brought it. The glue's own listeners run on real `resize` events
// of the window and the visual viewport: a rotation is one `exact_resize`
// carrying the geometry, whichever event comes first, and the geometry alone
// (the keys moving) is `exact_set_viewport_geometry` at the size already told.
import { expect, test } from 'bun:test';
import { hostFixtureFile as readFileSync, glueDeclarations } from './glue-fixture.mjs';
import { parseSync } from 'rolldown/utils';
import vm from 'node:vm';

const source = readFileSync(new URL('../glue.js', import.meta.url), 'utf8');
const declarations = glueDeclarations(source, ['pageChanged', 'resizeRunner', 'mediaChanged']);
// The glue's own registrations of the two `resize` listeners, as written.
const registrations = parseSync('glue.js', source).program.body
  .filter(s => s.type === 'ExpressionStatement')
  .map(s => source.slice(s.start, s.end))
  .filter(text => /addEventListener\("resize"/.test(text));

function page() {
  const calls = [], window = new EventTarget(), visualViewport = new EventTarget();
  const state = {
    innerWidth: 402, innerHeight: 874, JSON, document: { body: {} }, visualViewport,
    addEventListener: window.addEventListener.bind(window),
    readOut: value => value, applyBatch: () => {}, presence: { resize: x => x },
    now: () => 0, preferences: () => 0, foldBits: () => 0, requestAnimationFrame: () => {}, positionContexts: () => {},
    pageFacts: { bits: () => 0, rootFontSize: () => 16, displayScale: () => 3 },
    pageGeometry: () => JSON.stringify([[state.innerWidth > state.innerHeight ? 0 : 62, 0, 21, 0], [], 0]),
    writeIn: text => { calls.push(['write', text]); return text.length; },
    incarnation: 1, syncViewportFit: () => {},
    wasm: {
      exact_resize: (w, h, _now, _prefs, len) => { calls.push(['resize', w, h, len]); return state.refuse ? '{"ops":[],"error":"viewport: InvalidViewport"}' : '{}'; },
      exact_set_viewport_geometry: len => { calls.push(['geometry', state.innerWidth, state.innerHeight, len]); return '{}'; },
      exact_set_page: () => '{}',
    },
  };
  state.host = state;
  vm.createContext(state);
  vm.runInContext(`${declarations}\n${registrations.join('\n')}`, state);
  const rotate = () => { [state.innerWidth, state.innerHeight] = [state.innerHeight, state.innerWidth]; };
  return { calls, window, visualViewport, rotate, state };
}

const told = calls => calls.filter(c => c[0] !== 'write');

test('the window glue registers its resize listeners', () => {
  expect(registrations.length).toBeGreaterThanOrEqual(2);
});

test('a rotation is one exact_resize with its geometry, then nothing at the old size', () => {
  const p = page();
  p.window.dispatchEvent(new Event('resize'));
  p.calls.length = 0;
  p.rotate();
  p.window.dispatchEvent(new Event('resize'));
  const after = told(p.calls);
  expect(after[0]).toEqual(['resize', 874, 402, 17]);
  // The geometry told after it is at the new size, never at the size before.
  expect(after.slice(1).every(c => c[0] === 'geometry' && c[1] === 874)).toBe(true);
});

test('the visual viewport resizing first still sends the size with the geometry', () => {
  const p = page();
  p.window.dispatchEvent(new Event('resize'));
  p.calls.length = 0;
  p.rotate();
  p.visualViewport.dispatchEvent(new Event('resize'));
  expect(told(p.calls)[0]).toEqual(['resize', 874, 402, 17]);
  expect(told(p.calls).some(c => c[0] === 'geometry' && c[1] !== 874)).toBe(false);
});

test('the keys moving at the told size send the geometry alone', () => {
  const p = page();
  p.window.dispatchEvent(new Event('resize'));
  p.calls.length = 0;
  p.visualViewport.dispatchEvent(new Event('resize'));
  expect(told(p.calls)).toEqual([['geometry', 402, 874, 18]]);
});

test('a refused size is not taken as told: the geometry never follows it alone', () => {
  const p = page();
  p.window.dispatchEvent(new Event('resize'));
  p.calls.length = 0;
  p.state.refuse = true;
  p.rotate();
  p.window.dispatchEvent(new Event('resize'));
  p.visualViewport.dispatchEvent(new Event('resize'));
  const after = told(p.calls);
  expect(after.length).toBeGreaterThan(0);
  expect(after.every(c => c[0] === 'resize' && c[1] === 874)).toBe(true);
  // Taken at last: the geometry alone follows at that size.
  p.state.refuse = false;
  p.calls.length = 0;
  p.visualViewport.dispatchEvent(new Event('resize'));
  expect(told(p.calls)[0]).toEqual(['resize', 874, 402, 17]);
  p.calls.length = 0;
  p.visualViewport.dispatchEvent(new Event('resize'));
  expect(told(p.calls)).toEqual([['geometry', 874, 402, 17]]);
});
