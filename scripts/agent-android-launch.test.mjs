// The Android launch window (LLP 1115.001 D3.6, agent-android.mjs `androidLaunch`): the manifest's launch colour in
// each scheme it names one for, and `host.android.launchImage`, with nothing written where the manifest says neither.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdtempSync, readFileSync, readdirSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { androidJavaEnv, androidLaunch, androidSdk, androidSdkPackages } from './agent-android.mjs';
import { readPng } from './app-icons.mjs';
import { encodePng } from './png.mjs';

const inDir = (body) => { const dir = mkdtempSync(resolve(tmpdir(), 'exact-android-launch-')); try { return body(dir); } finally { rmSync(dir, { recursive: true, force: true }); } };
const read = (res, dir) => readFileSync(resolve(res, dir, 'exact_launch.xml'), 'utf8');
const colour = (xml) => /<color name="exact_launch">([^<]+)</.exec(xml)?.[1];
const light = (xml) => /<bool name="exact_launch_light">([^<]+)</.exec(xml)?.[1];

test('background_color colours the launch window, its dark member in night mode, with the bars\' icons read against it', () => inDir((dir) => {
  const res = resolve(dir, 'res');
  assert.deepEqual(androidLaunch({ background_color: '#f4f5f2', color_scheme_dark: { background_color: '#102839' } }, res), { theme: '@style/ExactAppLaunch', files: [] });
  assert.equal(colour(read(res, 'values')), '#FFF4F5F2');
  assert.equal(light(read(res, 'values')), 'true');
  assert.equal(colour(read(res, 'values-night')), '#FF102839');
  assert.equal(light(read(res, 'values-night')), 'false');
  // The window behind the first frame always; Android 12's starting window from values-v31.
  assert.match(read(res, 'values'), /<style name="ExactAppLaunch" parent="ExactLaunch">[\s\S]*android:windowBackground">@color\/exact_launch/);
  assert.doesNotMatch(read(res, 'values'), /windowSplashScreenBackground/);
  assert.match(read(res, 'values-v31'), /android:windowSplashScreenBackground">@color\/exact_launch/);
  assert.match(read(res, 'values-v31'), /android:windowLightStatusBar">@bool\/exact_launch_light/);
  // Unsaid, the starting window's icon is the platform's: the launcher icon.
  assert.doesNotMatch(read(res, 'values-v31'), /windowSplashScreenAnimatedIcon/);
}));

test('one colour serves both schemes, Android follows the device whatever userInterfaceStyle says, and #RGB, #RRGGBBAA and Display P3 are Android\'s #AARRGGBB', () => inDir((dir) => {
  const res = resolve(dir, 'res');
  androidLaunch({ background_color: '#1e3a5f' }, res);
  assert.equal(colour(read(res, 'values-night')), '#FF1E3A5F');
  assert.equal(light(read(res, 'values')), 'false');
  androidLaunch({ background_color: '#fff', color_scheme_dark: { background_color: '#00000080' }, userInterfaceStyle: 'dark' }, res);
  assert.equal(colour(read(res, 'values')), '#FFFFFFFF');
  assert.equal(colour(read(res, 'values-night')), '#80000000');
  // Display P3's red is beyond sRGB, which a colour resource holds: clipped
  androidLaunch({ background_color: 'color(display-p3 1 0 0)' }, res);
  assert.equal(colour(read(res, 'values')), '#FFFF0000');
}));

test('a scheme the manifest names no colour for keeps the reader\'s launch theme there', () => inDir((dir) => {
  const res = resolve(dir, 'res');
  androidLaunch({ color_scheme_light: { background_color: '#f4efe6' } }, res);
  assert.equal(colour(read(res, 'values')), '#FFF4EFE6');
  assert.match(read(res, 'values-v31'), /windowSplashScreenBackground/);
  for (const night of ['values-night', 'values-night-v31']) {
    assert.match(read(res, night), /<style name="ExactAppLaunch" parent="ExactLaunch">\n    <\/style>/);
    assert.equal(colour(read(res, night)), undefined);
  }
  androidLaunch({ color_scheme_dark: { background_color: '#14110d' } }, res);
  // Declared in values too, which release lint asks of every resource, though the day style does not read it
  assert.equal(colour(read(res, 'values')), '#FF14110D');
  assert.doesNotMatch(read(res, 'values'), /<item/);
  assert.equal(colour(read(res, 'values-night')), '#FF14110D');
  assert.match(read(res, 'values-night-v31'), /windowSplashScreenBackground">@color\/exact_launch/);
}));

test('without a launch colour or launchImage nothing is written and the reader\'s own launch theme stays', () => inDir((dir) => {
  const res = resolve(dir, 'res');
  androidLaunch({ background_color: '#fff' }, res);
  assert.deepEqual(androidLaunch({ theme_color: '#000' }, res), { theme: null, files: [] });
  assert.equal(existsSync(res), false);
}));

/** A PNG `width` by `height` px, opaque white: a logo's stand-in. */
const png = (dir, width, height) => {
  const path = resolve(dir, 'logo.png');
  writeFileSync(path, encodePng({ width, height, data: new Uint8Array(width * height * 4).fill(255) }));
  return path;
};

test('host.android.launchImage is the starting window\'s icon from Android 12 and on the window\'s background before', () => inDir((dir) => {
  const res = resolve(dir, 'res'), path = png(dir, 1024, 512);
  const made = androidLaunch({ background_color: '#B50000', host: { android: { launchImage: { src: 'logo.png', width: 160 } } } }, res, dir);
  assert.equal(made.theme, '@style/ExactAppLaunch');
  // The picture's file and digest, for the APK's record of its sources.
  assert.deepEqual(made.files, [{ file: path, sha256: createHash('sha256').update(readFileSync(path)).digest('hex') }]);
  // At xxxhdpi's 4 px a dp, never enlarged: 160 dp is 640 px, the height following.
  const picture = readPng(resolve(res, 'drawable-nodpi/exact_launch_picture.png'));
  assert.deepEqual([picture.width, picture.height], [640, 320]);
  // Android 12's icon is laid out at 108/192 of the size it shows: 160 dp shows as 90 dp laid out.
  const icon = readFileSync(resolve(res, 'drawable/exact_launch_icon.xml'), 'utf8');
  assert.match(icon, /<item android:width="90dp" android:height="45dp" android:gravity="center" android:drawable="@drawable\/exact_launch_picture" \/>/);
  assert.match(read(res, 'values-v31'), /android:windowSplashScreenAnimatedIcon">@drawable\/exact_launch_icon/);
  assert.match(read(res, 'values-v31'), /android:windowBackground">@color\/exact_launch/);
  // Before Android 12, the window's background: the colour with the picture at its own size centred on it.
  const window = readFileSync(resolve(res, 'drawable/exact_launch_window.xml'), 'utf8');
  assert.match(window, /<item android:drawable="@color\/exact_launch" \/>\n    <item android:width="160dp" android:height="80dp" android:gravity="center"/);
  assert.match(read(res, 'values'), /android:windowBackground">@drawable\/exact_launch_window/);
}));

test('a launch picture without background_color sits on the platform\'s surface, and a small one is not enlarged', () => inDir((dir) => {
  const res = resolve(dir, 'res');
  png(dir, 100, 100);
  assert.equal(androidLaunch({ host: { android: { launchImage: { src: 'logo.png', width: 120 } } } }, res, dir).theme, '@style/ExactAppLaunch');
  assert.equal(readPng(resolve(res, 'drawable-nodpi/exact_launch_picture.png')).width, 100);
  assert.match(readFileSync(resolve(res, 'drawable/exact_launch_window.xml'), 'utf8'), /<item android:drawable="\?android:attr\/colorBackground" \/>/);
  assert.match(readFileSync(resolve(res, 'drawable-night/exact_launch_window.xml'), 'utf8'), /<item android:drawable="\?android:attr\/colorBackground" \/>/);
  for (const v31 of ['values-v31', 'values-night-v31']) assert.doesNotMatch(read(res, v31), /windowSplashScreenBackground|windowBackground|windowLightStatusBar/);
  assert.equal(colour(read(res, 'values-night')), undefined);
}));

test('the night starting window uses its PNG at the shared width and its own aspect', () => inDir((dir) => {
  const res = resolve(dir, 'res'), dark = png(dir, 300, 450);
  const light = 'day.png';
  writeFileSync(resolve(dir, light), encodePng({ width: 80, height: 40, data: new Uint8Array(80 * 40 * 4).fill(255) }));
  const image = { src: light, width: 128, color_scheme_dark: { src: 'logo.png' } };
  const { files } = androidLaunch({ host: { android: { launchImage: image } } }, res, dir);
  assert.deepEqual(files.map(({ file }) => file), [resolve(dir, light), dark]);
  const pngNight = readPng(resolve(res, 'drawable-night-nodpi/exact_launch_picture.png'));
  assert.deepEqual([pngNight.width, pngNight.height], [300, 450]);
  for (const [scheme, height, splashHeight] of [['', 64, 36], ['-night', 192, 108]]) {
    const layer = (name) => readFileSync(resolve(res, `drawable${scheme}/exact_launch_${name}.xml`), 'utf8');
    assert.ok(layer('window').includes(`android:width="128dp" android:height="${height}dp"`));
    assert.ok(layer('icon').includes(`android:width="72dp" android:height="${splashHeight}dp"`));
  }
  delete image.color_scheme_dark;
  androidLaunch({ host: { android: { launchImage: image } } }, res, dir);
  assert.equal(existsSync(resolve(res, 'drawable-night-nodpi/exact_launch_picture.png')), false);
  assert.equal(existsSync(resolve(res, 'drawable-night/exact_launch_icon.xml')), false);
  assert.match(readFileSync(resolve(res, 'drawable-night/exact_launch_window.xml'), 'utf8'), /width="128dp" android:height="64dp"[^>]*@drawable\/exact_launch_picture/);
  assert.match(read(res, 'values-night-v31'), /windowSplashScreenAnimatedIcon">@drawable\/exact_launch_icon/);
}));

test('launchImage false hides the starting window\'s icon and draws nothing', () => inDir((dir) => {
  const res = resolve(dir, 'res');
  assert.deepEqual(androidLaunch({ host: { android: { launchImage: false } } }, res, dir), { theme: '@style/ExactAppLaunch', files: [] });
  assert.match(read(res, 'values-v31'), /android:windowSplashScreenAnimatedIcon">@android:color\/transparent/);
  assert.doesNotMatch(read(res, 'values'), /<item/);
  assert.equal(existsSync(resolve(res, 'drawable')), false);
  androidLaunch({ background_color: '#B50000', host: { android: { launchImage: false } } }, res, dir);
  assert.match(read(res, 'values-v31'), /windowSplashScreenBackground">@color\/exact_launch[\s\S]*windowSplashScreenAnimatedIcon">@android:color\/transparent/);
}));

// The resources link against the reader's `ExactLaunch` on every release the APK runs on: aapt2 refuses an unknown
// attribute, a drawable it cannot resolve or a theme without its parent.
const javaEnv = androidJavaEnv(process.env), sdk = androidSdk(process.env);
for (const colour of [true, false]) test.skipIf(process.platform !== 'darwin' || !javaEnv ||
  !androidSdkPackages().every(pkg => existsSync(resolve(sdk, pkg.path))))(`Android links a launch picture ${colour ? 'on background_color' : 'on the platform\'s surface'}, the icon from Android 12 and the window's background before`, () => inDir((dir) => {
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ app: { id: 'dev.exact.ci', name: 'CI' }, host: { android: { minimumOS: '10' } } }));
  png(dir, 64, 32);
  const { theme } = androidLaunch({ ...(colour ? { background_color: '#B50000', color_scheme_dark: { background_color: '#000' } } : {}),
    host: { android: { launchImage: { src: 'logo.png', width: 120 } } } }, resolve(dir, 'app/launch/res'), dir);
  const result = spawnSync(resolve(import.meta.dir, '../host/android/gradlew'), ['--no-daemon', '--console=plain',
    ':app:processReleaseResources', `-PappTheme=${theme}`, '-PapplicationId=dev.exact.ci', '-PappName=CI', '-Papp=ci', `-PappDir=${dir}`, `-PappBuildDir=${dir}/app`,
    `-PcanvasBuildDir=${dir}/canvas`, '--project-cache-dir', `${dir}/cache`],
  { cwd: resolve(import.meta.dir, '../host/android'), encoding: 'utf8', timeout: 180000, env: { ...javaEnv, ANDROID_HOME: sdk } });
  assert.equal(result.status, 0, result.stdout + result.stderr);
  const files = readdirSync(resolve(dir, 'app/intermediates'), { recursive: true });
  const merged = readFileSync(resolve(dir, 'app/intermediates', files.find(path => /merged_manifest.*AndroidManifest.xml$/.test(path))), 'utf8');
  assert.match(merged, /android:theme="@style\/ExactAppLaunch"/);
  const tools = readdirSync(resolve(sdk, 'build-tools')).sort().at(-1);
  const dump = spawnSync(resolve(sdk, 'build-tools', tools, 'aapt2'), ['dump', 'resources', resolve(dir, 'app/intermediates', files.find(path => /linked_resources.*\.ap_$/.test(path)))], { encoding: 'utf8' });
  assert.equal(dump.status, 0, dump.stderr);
  // aapt2 names a platform attribute by its id: 0x01010054 is android:windowBackground, 0x0101062d
  // android:windowSplashScreenAnimatedIcon.
  const [base, v31] = dump.stdout.split(/\n(?=\s*resource )/).find(block => /style\/ExactAppLaunch/.test(block)).split(/\n\s*\(v31\)/);
  assert.match(base, /0x01010054=@drawable\/exact_launch_window/);
  assert.doesNotMatch(base, /0x0101062d/);
  if (colour) assert.match(v31, /0x01010054=@color\/exact_launch/);
  else assert.doesNotMatch(v31, /0x01010054/);
  assert.match(v31, /0x0101062d=@drawable\/exact_launch_icon/);
}), 200000);
