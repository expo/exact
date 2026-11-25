// `prefer`'s facts by their names (agent-prefer.mjs `preferGroups`): a fact answers to its near spellings
// (LLP 1116.007 D3.5: `prefer color-scheme dark`), and an unknown fact or value is still refused.
import { test, expect } from 'bun:test';
import { PREFERENCES, mediaHeard, preferGroups, preferWeb } from './agent-prefer.mjs';
import { MalformedOperation } from './agent-keys.mjs';
import { displayMode, pageReporter, preferDisplayMode, preferences } from '../host/web/navigation/environment.js';

test('screen-reader is a page substitute with three values', () => {
  for (const value of ['on', 'off', 'unknown']) {
    expect(preferGroups({ 'screen-reader': value }).page).toEqual({ 'screen-reader': value });
  }
  expect(() => preferGroups({ 'screen-reader': 'false' })).toThrow('screen-reader on|off|unknown');
  const real = pageReporter(false, { document: { documentElement: {}, getElementById: () => null }, navigator: {}, getComputedStyle: () => ({ fontSize: "16px" }) });
  expect(real.read()['screen-reader']).toBe('unknown');
  real.prefer({ 'screen-reader': 'off' });
  expect(real.read()['screen-reader']).toBe('unknown');
});

test('a near spelling of a fact is that fact', () => {
  for (const name of ['prefers-color-scheme', 'color-scheme', 'colorScheme', 'prefersColorScheme', 'Color_Scheme']) {
    expect(preferGroups({ [name]: 'dark' }).media).toEqual({ 'prefers-color-scheme': 'dark' });
  }
  expect(preferGroups({ 'reduced-motion': 'reduce', contrast: 'more' }).media).toEqual({ 'prefers-reduced-motion': 'reduce', 'prefers-contrast': 'more' });
  expect(preferGroups({ canShare: 'false', rootFontSize: '20' }).page).toEqual({ 'can-share': false, 'root-font-size': 20 });
  expect(preferGroups({ askSiri: 'available' }).page).toEqual({ 'ask-siri': 'available' });
  // As its help writes it, `<px>`: `53px` is 53 (LLP 1156.002 D5).
  expect(preferGroups({ 'root-font-size': '53px' }).page).toEqual({ 'root-font-size': 53 });
  expect(preferGroups({ frozen: 'true' }).page).toEqual({ frozen: true });
  expect(() => preferGroups({ frozen: 'maybe' })).toThrow(MalformedOperation);
  expect(preferGroups({ displayMode: 'fullscreen' }).media).toEqual({ 'display-mode': 'fullscreen' });
  expect(preferGroups({ segments: '2x1', gap: '40' }).fold).toEqual({ cols: 2, rows: 1, gap: 40 });
});

test('the midi group names what a request finds and the ports by name (LLP 1125 §4)', () => {
  expect(preferGroups({ 'midi.state': 'granted', 'midi.outputs': 'OP-XY, IAC', 'midi.inputs': 'keys' }).midi).toEqual({ state: 'granted', outputs: ['OP-XY', 'IAC'], inputs: ['keys'] });
  expect(preferGroups({ 'midi.outputs': '' }).midi).toEqual({ outputs: [] });
  expect(() => preferGroups({ 'midi.state': 'maybe' })).toThrow(MalformedOperation);
});

test('an unknown fact or a value it does not take is refused naming what is expected', () => {
  expect(preferGroups({ safeArea: '47 0 34 0' }).page).toEqual({ 'safe-area': '47 0 34 0' });
  for (const bad of ['34', '1 2 3 -1', '1 2 3 4.5', '0 0 2147483648 0'])
    expect(() => preferGroups({ 'safe-area': bad })).toThrow('four nonnegative integers');
  for (const facts of [{ 'prefers-color-scheme': 'banana' }, { theme: 'dark' }, { 'has-focus': undefined }]) expect(() => preferGroups(facts)).toThrow(MalformedOperation);
  expect(() => preferGroups({ 'color-scheme': 'blue' })).toThrow(/^prefer: prefers-color-scheme blue: expected .*prefers-color-scheme dark\|light/);
  expect(() => preferGroups({ banana: 'split' })).toThrow(/^prefer: banana split: expected display-mode /);
  // A value written as the fact names the fact that takes it (`prefer dark` listed every fact).
  expect(() => preferGroups({ theme: 'dark' })).toThrow(/^prefer: theme dark: did you mean `prefer prefers-color-scheme dark`\? expected /);
  expect(() => preferGroups({ dark: undefined })).toThrow(/^prefer: dark: did you mean `prefer prefers-color-scheme dark`\? expected /);
  expect(() => preferGroups({ reduce: undefined })).toThrow(/^prefer: reduce: did you mean `prefer prefers-reduced-motion reduce` or `prefer prefers-reduced-transparency reduce`\? /);
  expect(() => preferGroups({ true: undefined })).toThrow(/^prefer: true: expected /); // too many facts take it to name
  expect(() => preferGroups({ gap: '4' })).toThrow('prefer: gap needs segments');
});

// LLP 1039 D7: Chrome takes CDP's display-mode emulation and changes nothing, so the glue substitutes it. On a stand-in
// page whose own query says `browser`: no CDP call, the glue asked, and the reply the substitute's.
test('Chrome\'s display-mode is the glue\'s substitute, not its emulation', async () => {
  const saved = globalThis.matchMedia, calls = [], asked = [];
  globalThis.matchMedia = (query) => ({ matches: query === '(display-mode: browser)' });
  try {
    const reply = await preferWeb({ media: { 'display-mode': 'standalone' }, page: {}, emulated: {}, frame: async () => {},
      call: async (method, params) => calls.push([method, params]), evaluate: async (expression) => (0, eval)(expression),
      ask: async (request) => { asked.push(request); preferDisplayMode(request.media['display-mode']); return { page: {}, fold: {} }; } });
    expect(calls).toEqual([]);
    expect(asked).toEqual([{ op: 'prefer', media: { 'display-mode': 'standalone' } }]);
    expect(reply.media['display-mode']).toBe('standalone');
    expect(matchMedia('(display-mode: browser)').matches).toBe(true);
  } finally { globalThis.matchMedia = saved; delete globalThis.__exactDisplayMode; }
});

// The facts word's bits 29–31 are the substitute's, 6 (`tabbed`) setting the sign bit, and no other bit moves. The
// module keeps the first `matchMedia` lists it reads, so the test compares against its own first reading.
test('the display-mode substitute replaces the page\'s reading and nothing else', () => {
  const saved = globalThis.matchMedia;
  globalThis.matchMedia = (query) => ({ matches: ['(display-mode: browser)', '(prefers-color-scheme: dark)', '(pointer: coarse)'].includes(query), addEventListener() {} });
  try {
    const read = preferences(), own = displayMode(), others = (bits) => bits & ~(7 << 29);
    for (const mode of ['standalone', 'tabbed']) {
      preferDisplayMode(mode);
      expect([displayMode(), others(preferences())]).toEqual([mode, others(read)]);
    }
    const refused = (() => { try { preferDisplayMode('kiosk'); } catch (e) { return e; } })();
    expect([refused?.message, refused?.malformed, displayMode()]).toEqual(['prefer: display-mode: kiosk is not a display mode', true, 'tabbed']);
    preferDisplayMode(own);
    expect(preferences()).toBe(read);
  } finally { globalThis.matchMedia = saved; delete globalThis.__exactDisplayMode; }
});

test('segments and gap are malformed', async () => {
  const invalid = [
    ...['0x1', '1x0', '-1x1', '1.5x1', '1x1.5', `${'9'.repeat(400)}x1`].map(segments => ({ segments })),
    ...[-1, NaN, Infinity, 'banana'].map(gap => ({ segments: '2x1', gap })),
  ];
  for (const facts of invalid) {
    const error = await (async () => preferWeb(preferGroups(facts)))().catch(e => e);
    expect(error).toBeInstanceOf(MalformedOperation);
    expect(error.malformed).toBe(true);
  }
  expect(preferGroups({ segments: '1x1', gap: 0 }).fold).toEqual({ cols: 1, rows: 1, gap: 0 });
  expect(preferGroups({ segments: '2x3', gap: 0.5 }).fold).toEqual({ cols: 2, rows: 3, gap: 0.5 });
});

test('prefer mode is the app modes group (LLP 1121 D13): each pin, the scheme left alone', () => {
  const { media, mode } = preferGroups({ mode: 'dark-style=dim, density=compact' });
  expect(mode).toEqual({ 'dark-style': 'dim', density: 'compact' });
  expect(media).toEqual({});
  expect(() => preferGroups({ mode: 'dim' })).toThrow(/mode <axis>=<value>/);
  expect(() => preferGroups({ mode: 'dark-style:dim' })).toThrow(/mode <axis>=<value>/);
});

test('page preference refusals omit the malformed property', () => {
  const { prefer } = pageReporter(true);
  // Another host's real facts: Tab reaching every control on the Mac, Ask Siri in the iOS simulator's menus.
  for (const facts of [{ 'keyboard-navigation': true }, { 'ask-siri': 'available' }]) {
    try { prefer(facts); throw new Error('expected a refusal'); }
    catch (error) { expect(error.message).toContain('not a page fact'); expect(Object.hasOwn(error, 'malformed')).toBe(false); }
  }
  try { prefer({ banana: true }); throw new Error('expected malformed input'); }
  catch (error) { expect(error.malformed).toBe(true); }
});

test('pointer and hover are media facts, emulated in Chrome by its touch screen (LLP 1132 D10)', async () => {
  expect(preferGroups({ pointer: 'coarse', hover: 'none' }).media).toEqual({ pointer: 'coarse', hover: 'none' });
  expect(() => preferGroups({ pointer: 'pen' })).toThrow(/^prefer: pointer pen: expected .*pointer coarse\|fine\|none/);
  const drive = media => {
    const calls = [];
    const done = preferWeb({ media, page: {}, emulated: {}, call: async (method, params) => calls.push([method, params]), frame: async () => {}, evaluate: async () => ({}) });
    return done.then(() => calls, error => { throw Object.assign(error, { calls }); });
  };
  expect(await drive({ pointer: 'coarse' })).toEqual([['Emulation.setTouchEmulationEnabled', { enabled: true, maxTouchPoints: 5 }]]);
  expect(await drive({ pointer: 'fine', hover: 'hover' })).toEqual([['Emulation.setTouchEmulationEnabled', { enabled: false }]]);
  expect(await drive({ hover: 'none', 'prefers-color-scheme': 'dark' })).toEqual([
    ['Emulation.setTouchEmulationEnabled', { enabled: true, maxTouchPoints: 5 }],
    ['Emulation.setEmulatedMedia', { features: [{ name: 'prefers-color-scheme', value: 'dark' }] }],
  ]);
  for (const media of [{ pointer: 'none' }, { pointer: 'coarse', hover: 'hover' }, { pointer: 'fine', hover: 'none' }]) {
    const error = await drive(media).catch(e => e);
    expect(error.message).toMatch(/^Chrome prefer cannot emulate .*: its touch emulation pairs pointer coarse with hover none/);
    expect(error.calls).toEqual([]);
  }
});

test('a media prefer waits in the page for each query it flips before the next step (the authored media flake)', async () => {
  // preferWeb: arm the page's wait, emulate, await that wait, then the frames; a document gone meanwhile is said.
  const steps = [];
  const drive = (heard = true) => preferWeb({ media: { 'prefers-reduced-motion': 'reduce' }, page: {}, emulated: {},
    call: async (method) => steps.push(method), frame: async () => steps.push('frame'),
    evaluate: async (expression) => { steps.push(expression.includes('__exactMediaHeard?.[') ? 'await' : expression.includes('function mediaHeard') ? 'arm' : 'read'); return expression.includes('__exactMediaHeard?.[') ? (heard ? true : 'gone') : {}; } });
  await drive();
  expect(steps).toEqual(['arm', 'Emulation.setEmulatedMedia', 'await', 'frame', 'read']);
  await expect(drive(false)).rejects.toThrow('prefer: the page changed documents while its media changed: prefer again');

  // mediaHeard, on a stand-in page whose queries flip when told: only queries whose truth changes are waited on,
  // a range feature's lower values included (color-gamut p3 to srgb flips p3's), and none at all is no wait.
  const saved = { matchMedia: globalThis.matchMedia, heard: globalThis.__exactMediaHeard };
  const matches = new Map(), lists = new Map();
  globalThis.matchMedia = (query) => {
    if (!lists.has(query)) { const listeners = new Set(); lists.set(query, { get matches() { return matches.get(query) ?? false; }, addEventListener: (_, f) => listeners.add(f), removeEventListener: (_, f) => listeners.delete(f), listeners }); }
    return lists.get(query);
  };
  const flip = (query, value) => { matches.set(query, value); for (const f of [...lists.get(query)?.listeners ?? []]) f(); };
  try {
    matches.set('(prefers-reduced-motion: no-preference)', true);
    matches.set('(color-gamut: srgb)', true); matches.set('(color-gamut: p3)', true);
    mediaHeard({ 'prefers-reduced-motion': 'reduce', 'color-gamut': 'srgb' }, PREFERENCES, 'a');
    expect([...lists.values()].filter(l => l.listeners.size).length).toBe(3); // reduce, no-preference, p3
    let done = false;
    globalThis.__exactMediaHeard.a.then(() => { done = true; });
    flip('(prefers-reduced-motion: reduce)', true); flip('(prefers-reduced-motion: no-preference)', false);
    await Bun.sleep(1);
    expect(done).toBe(false); // p3's query has not been heard yet
    flip('(color-gamut: p3)', false);
    await Bun.sleep(1);
    expect(done).toBe(true);
    expect([...lists.values()].every(l => l.listeners.size === 0)).toBe(true);
    // Already so: nothing to hear, settled at once.
    mediaHeard({ 'prefers-reduced-motion': 'reduce', 'color-gamut': 'srgb' }, PREFERENCES, 'b');
    expect(await globalThis.__exactMediaHeard.b).toBe(true);
  } finally { globalThis.matchMedia = saved.matchMedia; globalThis.__exactMediaHeard = saved.heard; }
});

test("an implementation's own source is preferred by its owner-qualified key", () => {
  const key = 'tracking-consent.trackingAuthorizationStatus#@exact/tracking-consent/apple';
  expect(preferGroups({ extension: `${key}={"status":"restricted"}` }).extension.extension).toEqual({ [key]: { status: 'restricted' } });
});

test('freeze and resume are document facts; prefer substitutes them under the agent', () => {
  const document = new EventTarget(), platform = new EventTarget();
  Object.assign(document, { visibilityState: 'visible', documentElement: {}, hasFocus: () => true });
  Object.assign(platform, { document, navigator: {}, getComputedStyle: () => ({fontSize:'16px'}) });
  const live = pageReporter(false, platform), agent = pageReporter(true, platform), told = [];
  live.onChange(() => told.push(live.read().frozen));
  agent.onChange(() => { throw Error('the live device reached the agent'); });
  expect(live.read().frozen).toBe(false);
  document.visibilityState = 'hidden'; document.dispatchEvent(new Event('visibilitychange'));
  expect(live.read().frozen).toBe(false);
  document.dispatchEvent(new Event('freeze'));
  expect(live.read().frozen).toBe(true);
  expect(live.bits() & 128).toBe(128);
  expect(agent.read().frozen).toBe(false);
  agent.prefer({ frozen: true }); expect(agent.bits() & 128).toBe(128);
  expect(() => agent.prefer({ frozen: 'maybe' })).toThrow('not a page fact');
  expect(agent.read().frozen).toBe(true);
  document.dispatchEvent(new Event('resume'));
  expect(live.read().frozen).toBe(false);
  expect(agent.read().frozen).toBe(true);
  agent.prefer({ frozen: false }); expect(agent.read().frozen).toBe(false);
  expect(told).toEqual([false, true, false]);
});
