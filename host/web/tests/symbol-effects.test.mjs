// LLP 1077 D12: the web's stand-ins for SF Symbol effects (navigation/symbol.js). With `symbolEffectValue`, a
// pulse or bounce plays once each time the value changes; without it, it runs while set; scale holds while set.
import { beforeEach, expect, test } from 'bun:test';

let reduced = false;
const elements = new Set();
globalThis.document = { querySelectorAll: () => [...elements] };
const motion = { get matches() { return reduced; }, addEventListener() {} };
globalThis.matchMedia = () => motion;
const { noteSymbolEffect, playSymbolEffects } = await import('../navigation/symbol.js');

/** An element with the effect in its own style, recording what it is asked to animate. */
function symbol(effect, value = null) {
  const el = {
    effect, value, played: [],
    style: { getPropertyValue: (name) => (name === '--exact-symbol-effect' ? el.effect : '') },
    getAttribute: (name) => (name === 'data-symboleffectvalue' ? el.value : null),
    animate(frames, { duration, iterations }) {
      const run = { property: Object.keys(frames[0])[0], duration, iterations, cancelled: false, cancel() { run.cancelled = true; } };
      el.played.push(run);
      return run;
    },
  };
  elements.add(el);
  return el;
}

beforeEach(() => {
  elements.clear();
  reduced = false;
  noteSymbolEffect('--exact-symbol-effect:pulse;');
  playSymbolEffects();
});

test('a pulse with a value plays once each time it changes, not for its first', () => {
  const el = symbol('pulse', '0');
  playSymbolEffects();
  expect(el.played).toEqual([]);
  el.value = '1';
  playSymbolEffects();
  expect(el.played.map(({ property, iterations }) => [property, iterations])).toEqual([['filter', 1]]);
  playSymbolEffects();
  expect(el.played.length).toBe(1);
  el.value = '2';
  playSymbolEffects();
  expect(el.played.length).toBe(2);
});

test('without a value a pulse runs while set; a value stops it', () => {
  const el = symbol('pulse');
  playSymbolEffects();
  const [running] = el.played;
  expect(running.iterations).toBe(Infinity);
  el.value = '0';
  playSymbolEffects();
  expect(running.cancelled).toBe(true);
  expect(el.played.length).toBe(1);
  elements.delete(el);
});

test('an element that leaves stops its effect', () => {
  const el = symbol('pulse');
  playSymbolEffects();
  elements.delete(el);
  playSymbolEffects();
  expect(el.played[0].cancelled).toBe(true);
});

test('a change during a play restarts it; a new effect ends it and waits for a change', () => {
  const el = symbol('pulse', '0');
  playSymbolEffects();
  el.value = '1';
  playSymbolEffects();
  el.value = '2';
  playSymbolEffects();
  expect(el.played.map(({ cancelled }) => cancelled)).toEqual([true, false]);
  el.effect = 'bounce';
  playSymbolEffects();
  expect(el.played.length).toBe(2);
  expect(el.played[1].cancelled).toBe(true);
  el.value = '3';
  playSymbolEffects();
  expect(el.played.map(({ property }) => property)).toEqual(['filter', 'filter', 'scale']);
});

test('scale holds while set, with a value too', () => {
  const el = symbol('scale', '0');
  playSymbolEffects();
  expect(el.played.map(({ property, iterations }) => [property, iterations])).toEqual([['scale', 1]]);
  el.value = '1';
  playSymbolEffects();
  expect(el.played.length).toBe(1);
});

test('a bounce plays once per change, and runs while set without a value', () => {
  const el = symbol('bounce', '0');
  playSymbolEffects();
  el.value = '1';
  playSymbolEffects();
  playSymbolEffects();
  expect(el.played.map(({ property, iterations }) => [property, iterations])).toEqual([['scale', 1]]);
  el.value = null;
  playSymbolEffects();
  expect(el.played.map(({ iterations }) => iterations)).toEqual([1, Infinity]);
});

test('reduced motion leaves out bounce and scale, not pulse, and follows a change of the setting', () => {
  reduced = true;
  const bounce = symbol('bounce'), scale = symbol('scale'), pulse = symbol('pulse');
  playSymbolEffects();
  expect([bounce.played.length, scale.played.length, pulse.played.length]).toEqual([0, 0, 1]);
  reduced = false;
  playSymbolEffects();
  expect([bounce.played.length, scale.played.length, pulse.played.length]).toEqual([1, 1, 1]);
  reduced = true;
  playSymbolEffects();
  expect([bounce.played[0].cancelled, scale.played[0].cancelled, pulse.played[0].cancelled]).toEqual([true, true, false]);
});
