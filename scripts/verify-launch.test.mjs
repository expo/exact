import { test, expect } from 'bun:test';
import { edgeGround, launchNotes } from './verify-findings.mjs';
import { builtGround, report } from './verify.mjs';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { iosSimulatorTarget } from '../host/apple/link.mjs';

test('a screen\'s ground is the one colour both of its sides show, else unread', () => {
  // A 40×40 capture of `ground` with a box of content in the middle and, where given, a sidebar down its left
  const screen = (ground, sidebar = null) => {
    const data = new Uint8Array(40 * 40 * 4);
    for (let y = 0; y < 40; y++) for (let x = 0; x < 40; x++) {
      const c = x >= 10 && x < 30 && y >= 12 && y < 28 ? [200, 30, 30] : sidebar && x < 12 ? sidebar : ground;
      data.set([...c, 255], (y * 40 + x) * 4);
    }
    return { width: 40, height: 40, data };
  };
  expect(edgeGround(screen([242, 242, 247]))).toBe('#f2f2f7');
  // A sidebar beside the page, or a ground that changes down a side (a gradient, a picture), is no answer
  expect(edgeGround(screen([255, 255, 255], [242, 242, 247]))).toBeNull();
  const graded = screen([0, 0, 0]);
  for (let y = 0; y < 40; y++) for (const x of [0, 1, 2, 37, 38, 39]) data(graded, x, y, [y * 6, y * 6, y * 6]);
  expect(edgeGround(graded)).toBeNull();
});

test('a named launch colour that is not the first screen\'s ground is a note that says it may be meant', () => {
  // UI Gallery's manifest before: white, which holds in dark mode, over a ground that is black there
  const lines = launchNotes({ background_color: '#FFF' }, { light: '#ffffff', dark: '#000000' });
  expect(lines[0]).toBe('launch: a note, not a problem, not counted: the launch screen is not the first screen\'s colour in dark mode');
  expect(lines[1]).toBe('  dark mode: the launch screen is #FFF (background_color, which holds in dark mode too unless color_scheme_dark.background_color names another) and the first screen\'s ground is #000000, so the app fades from one to the other as it opens');
  expect(lines[2]).toBe('  that is fine where it is meant; to open on the first screen\'s colour instead, put these in app.json in place of every background_color it names: "background_color": "#ffffff", "color_scheme_dark": { "background_color": "#000000" }');
  // A dark colour named apart is said as that member; an unread scheme keeps its launch colour in the remedy
  expect(launchNotes({ background_color: '#ffffff', color_scheme_dark: { background_color: '#000000' } }, { light: null, dark: '#17181b' }).slice(1)).toEqual([
    '  dark mode: the launch screen is #000000 (color_scheme_dark.background_color) and the first screen\'s ground is #17181b, so the app fades from one to the other as it opens',
    '  that is fine where it is meant; to open on the first screen\'s colour instead, put these in app.json in place of every background_color it names: "background_color": "#ffffff", "color_scheme_dark": { "background_color": "#17181b" }',
  ]);
  // A light colour named alone is said as its member, and the remedy keeps the scheme it leaves to Exact
  expect(launchNotes({ color_scheme_light: { background_color: '#f4efe6' } }, { light: '#ffffff', dark: '#000000' }).slice(1)).toEqual([
    '  light mode: the launch screen is #f4efe6 (color_scheme_light.background_color) and the first screen\'s ground is #ffffff, so the app fades from one to the other as it opens',
    '  that is fine where it is meant; to open on the first screen\'s colour instead, put these in app.json in place of every background_color it names: "color_scheme_light": { "background_color": "#ffffff" }',
  ]);
  // A Display P3 colour beyond sRGB is its clip in an sRGB capture: P3's red is #ff0000
  expect(launchNotes({ background_color: 'color(display-p3 1 0 0)' }, { light: '#ff0000', dark: '#ff0000' })).toEqual([]);
  // The remedy replaces every background_color, so a scheme's member it leaves out cannot keep winning
  expect(launchNotes({ background_color: '#888888', color_scheme_light: { background_color: '#eeeeee' } }, { light: '#ffffff', dark: '#000000' }).at(-1))
    .toBe('  that is fine where it is meant; to open on the first screen\'s colour instead, put these in app.json in place of every background_color it names: "background_color": "#ffffff", "color_scheme_dark": { "background_color": "#000000" }');
  // Android follows the device whatever userInterfaceStyle says: a light launch there is the light colour
  const fixedDark = { userInterfaceStyle: 'dark', background_color: '#ffffff', color_scheme_dark: { background_color: '#000000' } };
  expect(launchNotes(fixedDark, { light: '#ffffff', dark: '#000000' }, 'android')).toEqual([]);
  expect(launchNotes(fixedDark, { light: '#ffffff', dark: '#000000' }, 'ios')[1]).toStartWith('  light mode: the launch screen is #000000 (color_scheme_dark.background_color)');
  // A fixed scheme launches on its side in both
  expect(launchNotes({ background_color: '#ffffff', color_scheme_dark: { background_color: '#000000' }, userInterfaceStyle: 'dark' }, { light: '#000000', dark: '#000000' })).toEqual([]);
  // Nothing to say: a colour that matches (within a capture's rounding), none named, a ground unread, a translucent colour
  expect(launchNotes({ background_color: '#f2f2f7' }, { light: '#f3f2f6', dark: '#f2f2f7' })).toEqual([]);
  expect(launchNotes({}, { light: '#ffffff', dark: '#000000' })).toEqual([]);
  expect(launchNotes({ background_color: '#ffffff' }, { light: '#ffffff', dark: null })).toEqual([]);
  expect(launchNotes({ background_color: '#ffffff80' }, { light: '#000000', dark: '#000000' })).toEqual([]);
});


test('on iOS a colour the build read from the app\'s background is noted where the first screen shows another', async () => {
  const read = { light: '#ffffff', dark: '#ffffff' };
  // Nothing named: the launch is the read colour, and the first screen shows another in dark mode
  expect(launchNotes({}, { light: '#ffffff', dark: '#1c1c1e' }, 'ios', read).slice(1)).toEqual([
    '  dark mode: the launch screen is #ffffff (the app\'s own background as the build read it) and the first screen\'s ground is #1c1c1e, so the app fades from one to the other as it opens',
    '  something the build cannot see may cover it; to open on the first screen\'s colour, put these in app.json: "color_scheme_dark": { "background_color": "#1c1c1e" }',
  ]);
  // A Display P3 colour the build read is its clip in the capture, and the remedy leaves a read scheme that matches
  const p3 = { light: 'color(display-p3 1.0000 0.0000 0.0000)', dark: '#000000' };
  expect(launchNotes({}, { light: '#ff0000', dark: '#000000' }, 'ios', p3)).toEqual([]);
  expect(launchNotes({}, { light: '#ff0000', dark: '#101010' }, 'ios', p3).at(-1)).toEndWith('put these in app.json: "color_scheme_dark": { "background_color": "#101010" }');
  expect(launchNotes({}, { light: null, dark: '#101010' }, 'ios', read).at(-1)).toEndWith('put these in app.json: "color_scheme_dark": { "background_color": "#101010" }');
  // A named colour wins over the read one; a fixed style shows its side; only iOS reads one
  expect(launchNotes({ background_color: '#1c1c1e' }, { light: '#1c1c1e', dark: '#1c1c1e' }, 'ios', read)).toEqual([]);
  expect(launchNotes({ userInterfaceStyle: 'dark' }, { light: '#000000', dark: '#000000' }, 'ios', { light: '#ffffff', dark: '#000000' })).toEqual([]);
  expect(launchNotes({}, { light: '#ffffff', dark: '#1c1c1e' }, 'android', read)).toEqual([]);
  expect(launchNotes({}, { light: '#ffffff', dark: '#1c1c1e' }, 'ios', null)).toEqual([]);
  // The read colour is the one the simulator build's bake saved beside it, null with none
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-verify-ground-')), saved = process.env.EXACT_BAKE_OUTPUT;
  try {
    process.env.EXACT_BAKE_OUTPUT = dir;
    const app = { id: 'com.example.probe', dir, target: dir }, current = { missing: false, changed: [] };
    expect(await builtGround(app, current)).toBeNull();
    writeFileSync(resolve(dir, `ios-${iosSimulatorTarget().target}.build.json`), JSON.stringify({ graph: { launchBackground: 'system', launchGround: read } }));
    expect(await builtGround(app, current)).toEqual(read);
    // A build older than the plan the drive runs, or none, shows another launch screen than the plan's
    expect(await builtGround(app, { missing: false, changed: ['app.contract'] })).toBeNull();
    expect(await builtGround(app, { missing: true, changed: [] })).toBeNull();
  } finally {
    if (saved === undefined) delete process.env.EXACT_BAKE_OUTPUT; else process.env.EXACT_BAKE_OUTPUT = saved;
    rmSync(dir, { recursive: true, force: true });
  }
});

test('the note is said and counts for nothing, under --strict too', () => {
  const launch = launchNotes({ background_color: '#ffffff' }, { light: '#ffffff', dark: '#000000' });
  const said = report({ host: 'ios', built: null, tested: { passed: 1, failed: 0, results: [] }, checks: [], launch, shots: [], ms: 1000, cwd: '/app', strict: true });
  expect([said.problems, said.exit]).toEqual([0, 0]);
  expect(said.lines).toEqual(expect.arrayContaining(launch));
  expect(said.lines.at(-1)).toStartWith('verify: clean');
});

function data(image, x, y, rgb) {
  image.data.set([...rgb, 255], (y * image.width + x) * 4);
}
