import { test, expect, mock } from 'bun:test';

// A tempo edit committed at a hit's time: the old tempo and the new one round that time to either side of the edit, and
// the hit still plays once (LLP 1125 D3; the runner's `a_tempo_edit_on_a_hit_plays_it_once`). Whether a row rounds
// across the edit depends on when its sequence arms, so this file's clock starts at 0 and its rows run in order.
mock.module('./navigation.js', () => import('../web/navigation.js'));
mock.module('./prompt-glue.js', () => ({}));
mock.module('./alert-message.js', () => ({}));
mock.module('./menu-rows.js', () => ({}));
mock.module('./names.js', () => ({ sourceTypes: {} }));
const { clock, advance, commit } = await import('./rt.js');
const { install } = await import('./sounds.js');
const { sequence, transport } = await import('./transport.js');
clock.agent = true;
install([['hat', 4800, 48000]]);
transport({});

test('a tempo edit on a hit plays it once', () => {
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
