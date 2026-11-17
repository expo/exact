import { test, expect, mock } from 'bun:test';

mock.module('./navigation.js', () => import('../web/navigation.js'));
mock.module('./prompt-glue.js', () => ({}));
mock.module('./alert-message.js', () => ({}));
mock.module('./menu-rows.js', () => ({}));
mock.module('./names.js', () => ({ sourceTypes: {} }));
const { clock, advance, commit, C, journal, Sounds } = await import('./rt.js');
const { install } = await import('./sounds.js');
const { sequence, transport } = await import('./transport.js');
clock.agent = true;
install([['kick', 12000, 48000], ['hat', 4800, 48000]]);
transport({});

test('a transport reader can finish a commit without a sound table', () => {
  const seq = Sounds.seq;
  try {
    Sounds.seq = undefined;
    expect(() => Sounds.stood()).not.toThrow();
  } finally {
    Sounds.seq = seq;
  }
});

for (const [edit, ends, by] of [
  ['remove', 250, 'end'], ['slower', 150, 'group'], ['tap', 200, 'group'],
  ['quiet', 25, 'stop'], ['past', 25, 'group'],
]) test(`a sequence edit restores its group: ${edit}`, () => {
  const start = clock.now, first = exact.sounds.state(true).recorded;
  let on = true, closed = true, bpm = 60;
  const hit = (beat, src) => [beat, src, 1, 'hat', -1, 100, 0, 0.25, ''];
  sequence([[() => on, null, () => [
    [[4, 0, 0, '', closed ? [hit(0, 'kick'), hit(0.1, 'hat')] : [hit(0, 'kick')]]], bpm, 0, [], '', '',
  ], null, 's']]);
  try {
    expect(exact.sounds.state(true).voices[first].ends).toBe(start + 100);
    advance(start + 25, false);
    if (edit === 'tap') commit(() => C('playSound', ['hat', start + 200, 1, 'hat']));
    if (edit === 'quiet') commit(() => C('stopSounds', []));
    if (edit === 'past') commit(() => C('playSound', ['hat', clock.now, 1, 'hat']));
    advance(start + 50, false);
    commit(() => { if (edit === 'slower') bpm = 30; else closed = false; });
    const voices = exact.sounds.state(true).voices.slice(first);
    expect([voices[0].ends, voices[0].by]).toEqual([start + ends, by]);
    expect([voices[1].ends, voices[1].by]).toEqual([start + 100, 'edit']);
    if (edit === 'slower') expect(voices[2].at).toBe(start + 150);
  } finally {
    commit(() => { on = false; });
    advance(start + 1000, false);
  }
});

for (const [at, editAt, grouped, cancelled] of [[200, 50, false, 1], [200, 50, true, 2], [200, 100, false, 1], [250, 50, false, 0]])
test(`restoring a group cut revalidates future voices: ${at}, edit=${editAt}, grouped=${grouped}`, () => {
  const start = clock.now, first = exact.sounds.state(true).recorded, firstLog = journal.length;
  let on = true, closed = true;
  const hit = (beat, src) => [beat, src, 1, 'hat', -1, 100, 0, 0.25, ''];
  sequence([[() => on, null, () => [
    [[4, 0, 0, '', closed ? [hit(0, 'kick'), hit(0.1, 'hat')] : [hit(0, 'kick')]]], 60, 0, [], '', '',
  ], null, 's']]);
  try {
    advance(start + 25, false);
    if (grouped) commit(() => C('playSound', ['kick', clock.now, 1, 'other']));
    commit(() => C('playSounds', [Array.from({ length: 32 }, (_, i) => ['hat', start + at, 1, grouped && i === 31 ? 'other' : ''])]));
    expect(exact.sounds.state(true).voices.filter(v => v.at === start + at)).toHaveLength(32);
    advance(start + editAt, false);
    commit(() => { closed = false; });
    const voices = exact.sounds.state(true).voices.slice(first);
    expect([voices[0].ends, voices[0].by]).toEqual([start + 250, 'end']);
    for (const { at } of voices) {
      expect(voices.filter(v => v.at <= at && v.ends > at).length).toBeLessThanOrEqual(32);
    }
    for (const v of voices.slice(voices.length - cancelled)) expect([v.ends, v.by]).toEqual([start + at, 'cancelled']);
    expect(journal.slice(firstLog).filter(l => l.includes('sound dropped: 32 voices'))).toHaveLength(cancelled);
    if (grouped) expect([voices[2].ends, voices[2].by]).toEqual([start + 275, 'end']);
  } finally {
    commit(() => { on = false; });
    advance(start + 1000, false);
  }
});

test('stopping a sequence restores the tapped voice it would cut', () => {
  const start = clock.now, first = exact.sounds.state(true).recorded;
  let on = true;
  sequence([[() => on, null, () => [
    [[4, 0, 0, '', [[0.1, 'hat', 1, 'hat', -1, 100, 0, 0.25, '']]]], 60, 0, [], '', '',
  ], null, 's']]);
  try {
    advance(start + 25, false);
    commit(() => C('playSound', ['kick', clock.now, 1, 'hat']));
    expect(exact.sounds.state(true).voices[first + 1].ends).toBe(start + 100);
    advance(start + 50, false);
    commit(() => { on = false; });
    const voice = exact.sounds.state(true).voices[first + 1];
    expect([voice.ends, voice.by]).toEqual([start + 275, 'end']);
  } finally {
    commit(() => { on = false; });
    advance(start + 1000, false);
  }
});

test('a gain edit preserves all future taps that fit the final schedule', () => {
  const start = clock.now, first = exact.sounds.state(true).recorded, firstLog = journal.length;
  let on = true, gain = 1;
  const hit = (beat, src, gain) => [beat, src, gain, 'hat', -1, 100, 0, 0.25, ''];
  sequence([[() => on, null, () => [
    [[4, 0, 0, '', [hit(0, 'kick', 1), hit(0.1, 'hat', gain)]]], 60, 0, [], '', '',
  ], null, 's']]);
  try {
    advance(start + 25, false);
    commit(() => C('playSounds', [Array.from({ length: 32 }, () => ['hat', start + 200, 1, ''])]));
    expect(exact.sounds.state(true).recorded - first).toBe(34);
    advance(start + 50, false);
    commit(() => { gain = 0.5; });
    const voices = exact.sounds.state(true).voices.slice(first);
    expect([voices[0].ends, voices[0].by]).toEqual([start + 100, 'group']);
    expect([voices[1].ends, voices[1].by]).toEqual([start + 100, 'edit']);
    expect([voices[34].at, voices[34].gain]).toEqual([start + 100, 0.5]);
    expect(voices.filter(v => v.at <= start + 200 && v.ends > start + 200)).toHaveLength(32);
    expect(journal.slice(firstLog).filter(l => l.includes('sound dropped'))).toHaveLength(0);
  } finally {
    commit(() => { on = false; });
    advance(start + 1000, false);
  }
});

test('stopping at a group cut does not revive the sequence voice', () => {
  const start = clock.now, first = exact.sounds.state(true).recorded;
  let on = true;
  const hit = (beat, src) => [beat, src, 1, 'hat', -1, 100, 0, 0.25, ''];
  sequence([[() => on, null, () => [
    [[4, 0, 0, '', [hit(0, 'kick'), hit(0.1, 'hat')]]], 60, 0, [], '', '',
  ], null, 's']]);
  try {
    advance(start + 100, false);
    commit(() => { on = false; });
    const voices = exact.sounds.state(true).voices.slice(first);
    expect(voices.every(v => v.state === 'ended')).toBe(true);
    expect(voices[0].ends).toBe(start + 100);
    expect([voices[1].ends, voices[1].by]).toEqual([start + 100, 'cancelled']);
  } finally {
    commit(() => { on = false; });
    advance(start + 1000, false);
  }
});

test('stopping taps restores the waiting sequence voice they cut', () => {
  const start = clock.now, first = exact.sounds.state(true).recorded;
  let on = true;
  sequence([[() => on, null, () => [
    [[4, 0, 100, '', [[0, 'kick', 1, 'hat', -1, 100, 0, 0.25, '']]]], 60, 0, [], '', '',
  ], null, 's']]);
  try {
    commit(() => C('playSound', ['hat', start + 200, 1, 'hat']));
    expect(exact.sounds.state(true).voices[first].ends).toBe(start + 200);
    advance(start + 50, false);
    commit(() => C('stopSounds', []));
    const voices = exact.sounds.state(true).voices.slice(first);
    expect([voices[0].ends, voices[0].by]).toEqual([start + 350, 'end']);
    expect([voices[1].ends, voices[1].by]).toEqual([start + 200, 'cancelled']);
  } finally {
    commit(() => { on = false; });
    advance(start + 1000, false);
  }
});

for (const replacement of ['tap', 'sequence']) test(`stopping and replacing a tap keeps voices that fit: ${replacement}`, () => {
  const start = clock.now, first = exact.sounds.state(true).recorded, firstLog = journal.length;
  let on = true, closed = false;
  const hit = (beat, src) => [beat, src, 1, 'hat', -1, 100, 0, 0.25, ''];
  sequence([[() => on, null, () => [
    [[4, 0, 100, '', closed ? [hit(0, 'kick'), hit(0.1, 'hat')] : [hit(0, 'kick')]]], 60, 0, [], '', '',
  ], null, 's']]);
  try {
    commit(() => C('playSound', ['hat', start + 200, 1, 'hat']));
    commit(() => C('playSounds', [Array.from({ length: 32 }, () => ['hat', start + 300, 1, ''])]));
    expect(exact.sounds.state(true).recorded - first).toBe(34);
    advance(start + 50, false);
    commit(() => {
      C('stopSounds', ['hat']);
      if (replacement === 'tap') C('playSound', ['hat', start + 200, 1, 'hat']);
      else closed = true;
    });
    const voices = exact.sounds.state(true).voices.slice(first);
    expect([voices[0].ends, voices[0].by]).toEqual([start + 200, 'group']);
    expect([voices[1].ends, voices[1].by]).toEqual([start + 200, 'cancelled']);
    expect(voices[34].at).toBe(start + 200);
    expect(voices.filter(v => v.at <= start + 300 && v.ends > start + 300)).toHaveLength(32);
    expect(journal.slice(firstLog).filter(l => l.includes('sound dropped'))).toHaveLength(0);
  } finally {
    commit(() => { on = false; });
    advance(start + 1000, false);
  }
});

for (const command of [false, true]) test(`commands and expansion keep the group voice awaiting restoration: command=${command}`, () => {
  const start = clock.now;
  let on = true, closed = true;
  const hit = (beat, src) => [beat, src, 1, 'hat', -1, 100, 0, 0.25, ''];
  const tail = command ? [] : [hit(0.5, 'hat')];
  sequence([[() => on, null, () => [
    [[4, 0, 0, '', closed ? [hit(0, 'kick'), hit(0.1, 'hat'), ...tail] : [hit(0, 'kick'), ...tail]]], 60, 0, [], '', '',
  ], null, 's']]);
  const kickId = exact.sounds.state(true).voices.findLast(v => v.src === 'kick').id;
  try {
    commit(() => C('playSounds', [Array.from({ length: command ? 1022 : 1021 }, () => ['hat', start + 800, 1, 'padding'])]));
    expect(exact.sounds.state(true).recorded).toBe(1024);
    advance(start + 100, false);
    commit(() => {
      closed = false;
      if (command) C('playSound', ['hat', start + 900, 1, '']);
    });
    expect(exact.sounds.state(true).recorded).toBe(1024);
    const kick = exact.sounds.state(true).voices.find(v => v.id === kickId);
    expect(kick).toBeDefined();
    expect([kick.ends, kick.by]).toEqual([start + 250, 'end']);
  } finally {
    commit(() => { on = false; });
    advance(start + 1000, false);
  }
});

test('setSounds moves a sounding voice and its end as the runner does (LLP 1096 D14)', () => {
  const start = clock.now, voice = () => exact.sounds.state(true).voices.findLast(v => v.group === 'k');
  commit(() => C('playSound', ['kick', start, 1, 'k']));
  advance(start + 100, false);
  commit(() => C('setSounds', ['k', null, 0.5, null, 0]));
  expect(voice().ends).toBe(start + 400);
  advance(start + 200, false);
  // 150 ms of the 250 ms kick played, 100 left; the ramp to 2 over 100 ms would cover 125.
  commit(() => C('setSounds', ['k', 1, 2, -0.5, 100]));
  const v = voice();
  expect(v.ends).toBeCloseTo(start + 200 + (-0.5 + Math.sqrt(3.25)) / 0.015, 9);
  expect([v.gain, v.playbackRate, v.pan]).toEqual([1, 2, -0.5]);
  commit(() => C('setSounds', ['k', 0 / 0, null, null, null]));
  const lines = journal.slice(-20).map(l => l.replace(/^t=\S+ /, ''));
  expect(lines).toContain('sounds set: 1 (group k) playbackRate 0.5 ramp 0');
  expect(lines).toContain(`sound ${v.id} ends at ${start + 400} (end)`);
  expect(lines).toContain('sounds set: 1 (group k) gain 1 playbackRate 2 pan -0.5 ramp 100');
  expect(lines).toContain('sounds not set: gain is not a number');
  advance(start + 1000, false);
});
