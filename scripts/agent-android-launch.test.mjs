// The Android launch window in the manifest's first-frame background (LLP 1115.001 D3.6, agent-android.mjs
// `androidLaunch`): the theme and colours written, light and dark, and none without a colour.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { existsSync, mkdtempSync, readFileSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { androidLaunch } from './agent-android.mjs';

const inDir = (body) => { const dir = mkdtempSync(resolve(tmpdir(), 'exact-android-launch-')); try { return body(dir); } finally { rmSync(dir, { recursive: true, force: true }); } };
const read = (res, dir) => readFileSync(resolve(res, dir, 'exact_launch.xml'), 'utf8');
const colour = (xml) => /<color name="exact_launch">([^<]+)</.exec(xml)?.[1];
const light = (xml) => /<bool name="exact_launch_light">([^<]+)</.exec(xml)?.[1];

test('background_color colours the launch window, its dark member in night mode, with the bars\' icons read against it', () => inDir((dir) => {
  const res = resolve(dir, 'res');
  assert.equal(androidLaunch({ background_color: '#f4f5f2', color_scheme_dark: { background_color: '#102839' } }, res), '@style/ExactAppLaunch');
  assert.equal(colour(read(res, 'values')), '#FFF4F5F2');
  assert.equal(light(read(res, 'values')), 'true');
  assert.equal(colour(read(res, 'values-night')), '#FF102839');
  assert.equal(light(read(res, 'values-night')), 'false');
  // The window behind the first frame always; Android 12's starting window from values-v31.
  assert.match(read(res, 'values'), /<style name="ExactAppLaunch" parent="ExactLaunch">[\s\S]*android:windowBackground">@color\/exact_launch/);
  assert.doesNotMatch(read(res, 'values'), /windowSplashScreenBackground/);
  assert.match(read(res, 'values-v31'), /android:windowSplashScreenBackground">@color\/exact_launch/);
  assert.match(read(res, 'values-v31'), /android:windowLightStatusBar">@bool\/exact_launch_light/);
}));

test('one colour serves both schemes, a fixed userInterfaceStyle takes its side, and #RGB and #RRGGBBAA are Android\'s #AARRGGBB', () => inDir((dir) => {
  const res = resolve(dir, 'res');
  androidLaunch({ background_color: '#1e3a5f' }, res);
  assert.equal(colour(read(res, 'values-night')), '#FF1E3A5F');
  assert.equal(light(read(res, 'values')), 'false');
  androidLaunch({ background_color: '#fff', color_scheme_dark: { background_color: '#00000080' }, userInterfaceStyle: 'dark' }, res);
  assert.equal(colour(read(res, 'values')), '#80000000');
  assert.equal(colour(read(res, 'values-night')), '#80000000');
  androidLaunch({ background_color: '#abc', color_scheme_dark: { background_color: '#000' }, userInterfaceStyle: 'light' }, res);
  assert.equal(colour(read(res, 'values-night')), '#FFAABBCC');
}));

test('without background_color nothing is written and the reader\'s own launch theme stays', () => inDir((dir) => {
  const res = resolve(dir, 'res');
  androidLaunch({ background_color: '#fff' }, res);
  assert.equal(androidLaunch({ color_scheme_dark: { background_color: '#000' } }, res), null);
  assert.equal(existsSync(res), false);
}));
