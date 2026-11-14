import { test, expect, mock } from 'bun:test';

// A tempo edit committed at a hit's time: the old tempo and the new one round that time to either side of the edit, and
// the hit still plays once (LLP 1125 D3; the runner's `a_tempo_edit_on_a_hit_plays_it_once`). Whether a row rounds
// across the edit depends on when its sequence arms, so this file's clock starts at 0 and its rows run in order.
mock.module('./navigation.js', () => import('../web/navigation.js'));
mock.module('./prompt-glue.js', () => ({}));
mock.module('./alert-message.js', () => ({}));
mock.module('./menu-rows.js', () => ({}));
mock.module('./names.js', () => ({ sourceTypes: {} }));
const document = new EventTarget();
Object.assign(document, {createElement: () => ({}), querySelector: () => null, visibilityState:'visible', hasFocus: () => true, documentElement:{}});
const { clock, advance, commit, res } = await import('./rt.js');
const { page } = await import('./facts.js');
const { install } = await import('./sounds.js');
const { sequence, transport } = await import('./transport.js');
clock.agent = true;
install([['hat', 4800, 48000]]);
transport({});

test('a tempo edit on a hit plays it once', () => {
  Object.assign(globalThis, {document, addEventListener() {}, getComputedStyle: () => ({fontSize:'16px'})});
  expect(clock.now).toBe(0);
  const rows = [[0, 100, 13 / 6, 110, 1300], [-4, 60, 7 / 5, 50, 5400], [-4, 80, 2 / 3, 70, 3500], [0, 120, 2, 150, 1000]];
  for (const [from, bpm, beat, faster, edit] of rows) {
    const start = clock.now, t = start + edit;
    let on = true, tempo = bpm;
    sequence([[() => on, null, () => [[[16, 0, 0, '', [[beat, 'hat', 1, '', -1, 100, 0, 0.25, '']]]], tempo, from, [], '', ''], null, 's']]);
    advance(t, false);
    commit(() => { tempo = faster; });
    advance(t + 1000, false);
    const played = exact.sounds.state(true).voices.filter(v => v.at >= start && v.by !== 'edit');
    expect([beat, played.length]).toEqual([beat, 1]);
    expect(Math.abs(played[0].at - t)).toBeLessThan(1e-6);
    commit(() => { on = false; });
    advance(clock.now + 1000, false);
  }
});

test('freeze stops a sequence in its commit without a page reader and resume leaves it stopped', () => {
  document.visibilityState = 'hidden';
  sequence([[() => true, null, () => [[[16, 0, 0, '', [[0, 'hat', 1, '', -1, 100, 0, 0.25, '']]]], 120, 0, [], '', ''], null, 'frozen']]);
  expect(exact.transport.state().state).toBe('playing');
  const epoch = clock.epoch;
  document.dispatchEvent(new Event('freeze'));
  expect(clock.epoch).toBe(epoch + 1);
  expect(exact.transport.state()).toMatchObject({state:'stopped', reason:'background'});
  document.dispatchEvent(new Event('resume'));
  expect(exact.transport.state()).toMatchObject({state:'stopped', reason:'background'});
});

test('a sequence armed by freezing stops and cancels its scheduled output in that commit', () => {
  Object.assign(globalThis, {document, addEventListener() {}, getComputedStyle: () => ({fontSize:'16px'})});
  page({freezePage: [['frozen']]});
  const pageFact = res('freezePage', 'exactPage', () => [], undefined, undefined, '{b}', [false]);
  sequence([[() => pageFact()[0], null, () => [[[16, 0, 0, '', [[1, 'hat', 1, '', -1, 100, 0, 0.25, '']]]], 120, 0, [], '', ''], null, 'freeze-gate']]);
  expect(exact.transport.state().state).toBe('stopped');
  const before = exact.sounds.state(true).voices.length;
  document.dispatchEvent(new Event('freeze'));
  expect(exact.transport.state()).toMatchObject({state:'stopped', reason:'background', sequence:'freeze-gate'});
  const scheduled = exact.sounds.state(true).voices.slice(before);
  expect(scheduled.length).toBeGreaterThan(0);
  expect(scheduled.every(v => v.at > clock.now && v.by === 'cancelled' && v.ends === v.at)).toBe(true);
  advance(clock.now + 3000, false);
  expect(exact.sounds.state(true).voices.slice(before)).toEqual(scheduled);
  document.dispatchEvent(new Event('resume'));
});
