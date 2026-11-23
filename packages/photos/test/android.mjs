// @exact/photos and the image codec on Android (LLP 1133.019 stage 3), on `apps/photos-fixture`, the library's
// consumer there (the photo editor has no Android port).
//
// With no flag it builds the fixture's APK and checks what the build made: the read permissions with their API
// ranges (READ_EXTERNAL_STORAGE ends at 32), ACCESS_MEDIA_LOCATION, no permission for adding; the roster naming the
// photos extension; the package's Kotlin and the reader's codec compiled in; the agent's six photos carried, byte for
// byte the package's fixture. `--emulator` then drives the APK on an arm64 emulator (ANDROID_SERIAL, else adb's
// one device) whose adb can run as root, to read the edits back (on a Google Play image, which refuses root, drive 4
// checks only their sizes and says so): under the agent, the library through the reader's
// picture door, a copy and an add, limited access and a revocation; the image codec's conformance (LLP 1133.019 §11,
// checklist step 9) through the fixture's `editImage`; and MediaStore itself, the implementation bound and the read
// granted by `pm grant`, and paged through with Older; and, from API 30, the replacement path `photoChanges` sends an
// app to (drive 8: what another app changes while the fixture is away is listed on its return), and the fixture's
// photo index kept in step with MediaStore through it (drive 9). Both need an NDK in ANDROID_NDK_HOME and a JDK 17 or newer in JAVA_HOME. checklist.md's
// Android part is what needs a person: the platform's dialogs and what is chosen in them.
//
// usage: bun packages/photos/test/android.mjs [--emulator]
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { basename, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { checker } from '../../camera/test/fixture.mjs';
import { decodePng } from '../../../scripts/png.mjs';
import { androidDevice, androidSdk } from '../../../scripts/agent-android.mjs';
import { apkBuildDir } from '../../../scripts/agent-android.mjs';

/** What a JPEG's EXIF says of the checks (orientation, GPS, date, camera, rights, maker note), or null with none. */
function exif(path) {
  if (!path) return null;
  const b = readFileSync(path);
  for (let i = 2; i + 4 < b.length && b[i] === 0xff;) {
    const marker = b[i + 1], length = b.readUInt16BE(i + 2);
    if (marker === 0xe1 && b.toString('latin1', i + 4, i + 10) === 'Exif\0\0') {
      const t = i + 10, le = b.toString('latin1', t, t + 2) === 'II';
      const u16 = o => le ? b.readUInt16LE(t + o) : b.readUInt16BE(t + o), u32 = o => le ? b.readUInt32LE(t + o) : b.readUInt32BE(t + o);
      const ifd = o => Object.fromEntries(Array.from({ length: u16(o) }, (_, k) => o + 2 + k * 12).map(e => [u16(e), e]));
      const zero = ifd(u32(4)), sub = zero[0x8769] ? ifd(u32(zero[0x8769] + 8)) : {};
      return { orientation: zero[0x0112] ? u16(zero[0x0112] + 8) : null, gps: !!zero[0x8825], make: !!zero[0x010f], copyright: !!zero[0x8298],
        date: !!sub[0x9003], makerNote: !!sub[0x927c] };
    }
    if (marker === 0xda) break;
    i += 2 + length;
  }
  return null;
}

const here = import.meta.dir, root = resolve(here, '../../..'), app = 'photos-fixture';
const sha = bytes => createHash('sha256').update(bytes).digest('hex');
const AGENT = n => sha(readFileSync(resolve(here, `../fixture/agent-${n}.jpg`)));
const { check, failed } = checker();
const built = apkBuildDir(app);

const build = spawnSync('bun', ['scripts/agent-android.mjs', 'build', app, '--apk'], { cwd: root, encoding: 'utf8', maxBuffer: 1 << 28 });
check('android', 'the fixture builds as an APK', build.status === 0, (build.stdout + build.stderr).slice(-2000));
const merged = existsSync(built) && readdirSync(built, { recursive: true })
  .find(f => String(f).includes('merged_manifests/release/') && String(f).endsWith('AndroidManifest.xml'));
const manifest = merged ? readFileSync(resolve(built, merged), 'utf8') : '';
const permission = name => new RegExp(`<uses-permission\\s+android:name="${name.replace(/\./g, '\\.')}"([^>]*)/>`).exec(manifest);
check('android', 'READ_EXTERNAL_STORAGE ends at API 32', /android:maxSdkVersion="32"/.test(permission('android.permission.READ_EXTERNAL_STORAGE')?.[1] ?? ''), merged ?? 'no merged manifest');
check('android', 'READ_MEDIA_IMAGES, READ_MEDIA_VISUAL_USER_SELECTED and ACCESS_MEDIA_LOCATION have no end',
  ['READ_MEDIA_IMAGES', 'READ_MEDIA_VISUAL_USER_SELECTED', 'ACCESS_MEDIA_LOCATION'].every(p => permission(`android.permission.${p}`) && !/maxSdkVersion/.test(permission(`android.permission.${p}`)[1])), manifest.slice(0, 1200));
check('android', 'adding asks no storage permission', !permission('android.permission.WRITE_EXTERNAL_STORAGE'), manifest.slice(0, 1200));
const roster = resolve(built, 'extensions/exact/app/ExactExtensions.kt');
check('android', 'the roster names the photos extension', existsSync(roster) && readFileSync(roster, 'utf8').includes('"photos" to { c -> exact.photos.PhotosModule(c) }'), roster);
const apk = resolve(built, 'outputs/apk/release/app-release.apk');
const listed = spawnSync('unzip', ['-l', apk], { encoding: 'utf8' }).stdout ?? '';
for (let n = 1; n <= 6; n++) {
  const bytes = spawnSync('unzip', ['-p', apk, `assets/app/assets/photos-fixture/agent-${n}.jpg`], { maxBuffer: 1 << 24 }).stdout;
  check('android', `agent-${n}.jpg is carried as the package's fixture`, bytes?.length > 0 && sha(bytes) === AGENT(n), listed.slice(0, 400));
}

if (process.argv.includes('--emulator')) {
  const adb = (...args) => spawnSync(resolve(androidSdk(), 'platform-tools/adb'), ['-s', androidDevice(), ...args], { maxBuffer: 1 << 26 });
  const api = Number(adb('shell', 'getprop', 'ro.build.version.sdk').stdout.toString().trim());
  const where = `android ${api}`;
  /** One launch of the APK under the agent, with [more] flags: each op's JSON reply, and the journal's and the app's
   * lines. */
  const drive = (permissions, ops, more = []) => {
    const flags = [...more, ...Object.entries(permissions).flatMap(([name, state]) => ['--permission', `${name}=${state}`])];
    const r = spawnSync('bun', ['scripts/agent.mjs', 'android', '--reader', '--json', '--app', app, ...flags, ...ops], { cwd: root, encoding: 'utf8', maxBuffer: 1 << 26 });
    const replies = (r.stdout ?? '').split('\n').filter(l => /^[[{"]|^(null|true|false|-?\d)/.test(l)).map(l => JSON.parse(l));
    if (replies.length < ops.length) console.log(r.stdout.slice(-3000), r.stderr.slice(-3000));
    const logs = replies.find(x => Array.isArray(x?.lines));
    replies.lines = [...logs?.lines ?? [], ...logs?.host ?? []];
    return replies;
  };
  const tree = t => JSON.stringify(t ?? '');
  const grid = t => [...tree(t).matchAll(/"photo-(\d+)"/g)].length;
  // 1: the launch's read waits on a ticket; allowed, six pictures drawn by the door; the first copied byte for byte.
  {
    const r = drive({ photos: 'prompt' }, ['clock +100', 'state pending', 'tap @recent allow', 'clock settle', 'tree', 'state extensions',
      'tap photo-1', 'clock settle', 'state copied', 'logs']);
    const copy = r.lines.map(l => /photos: copied (\S+) to (\S+) sha256 ([0-9a-f]{64})/.exec(l)).find(Boolean);
    check(where, '1: the launch read waits on a photos ticket', r[1]?.some?.(p => p.device?.args?.capability === 'photos'), JSON.stringify(r[1]));
    check(where, '1: six pictures', grid(r[4]) === 6, tree(r[4]).slice(0, 400));
    check(where, '1: each picture admitted, asked of the extension, none refused', (r[5]?.photos?.pictures?.admitted ?? 0) >= 6 && !r[5]?.photos?.pictures?.refused
      && r.lines.filter(l => /photos: picture agent-\d at \d+x\d+ cover/.test(l)).length >= 6, JSON.stringify(r[5]?.photos?.pictures));
    // Each copy has a name of its own, so two never share a file.
    check(where, "1: photo-1 is agent-1's copy, byte for byte, at a name of its own", copy?.[1] === 'agent-1' && /^app:\/tmp\/photos\/[0-9A-F-]{36}\.jpg$/.test(copy[2])
      && r[8]?.path === copy[2] && copy?.[3] === AGENT(1), `${JSON.stringify(copy)} ${r[8]?.path}`);
  }
  // 2: a copy added back joins the library as the newest, readable as the app's own.
  {
    const r = drive({ photos: 'granted', 'photos-add': 'granted' }, ['clock settle', 'tap photo-3', 'clock settle', 'tap add', 'clock settle', 'state added', 'tree', 'logs']);
    const add = r.lines.map(l => /photos: added (\S+) from (\S+) sha256 ([0-9a-f]{64}) (\d+)x(\d+)/.exec(l)).find(Boolean);
    check(where, '2: the add is agent-7, the stored turned photo kept as it was', add?.[1] === 'agent-7' && add?.[3] === AGENT(3), JSON.stringify(add));
    check(where, '2: seven pictures, the add first', grid(r[6]) === 7, tree(r[6]).slice(0, 400));
  }
  // 3: limited access, the agent's selection of three; then a photo leaves it and its picture is withdrawn.
  {
    const r = drive({ photos: 'prompt' }, ['clock +100', 'tap @recent limited', 'clock settle', 'tree', 'prefer extension photos.selection=["agent-5"]', 'clock settle', 'tree', 'logs']);
    check(where, '3: limited shows three and Manage', grid(r[3]) === 3 && tree(r[3]).includes('"manage"'), tree(r[3]).slice(0, 400));
    check(where, '3: a smaller selection shows one, the others revoked', grid(r[6]) === 1 && r.lines.some(l => /photos: revoked photos:agent-1/.test(l)), r.lines.filter(l => /photos:/.test(l)).join(' | '));
  }
  // 3b: the screenshot filter (LLP 1133.019.000 D4) over the agent's library, whose one screenshot is agent-2.
  {
    const r = drive({ photos: 'granted' }, ['clock settle', 'tap screenshots', 'clock settle', 'tree', 'tap screenshots', 'clock settle', 'tree']);
    const sources = t => (t?.nodes ?? []).map(n => n.props?.imageSource).filter(v => /^photos:/.test(v ?? ''));
    check(where, '3b: Screenshots lists agent-2 alone', JSON.stringify(sources(r[3])) === '["photos:agent-2"]', JSON.stringify(sources(r[3])));
    check(where, '3b: off again, all six', grid(r[6]) === 6, tree(r[6]).slice(0, 400));
  }
  // 4: the codec (D16), §11's image conformance and checklist step 9, in one launch: each call's record, then its
  // file, read from the app's directory (an emulator's adb runs as root) and decoded here by `sips`.
  {
    const cases = [
      ['upright', 'fixture:oriented-gps.jpg', 'transform', {}],
      ['rotate 90', 'fixture:oriented-gps.jpg', 'transform', { rotate: 90 }],
      ['flip', 'fixture:oriented-gps.jpg', 'transform', { flip: 'horizontal' }],
      ['crop', 'fixture:oriented-gps.jpg', 'transform', { crop: { x: 0, y: 0, width: 24, height: 64 } }],
      ['free turn png', 'fixture:oriented-gps.jpg', 'transform', { rotate: 30, type: 'image/png' }],
      ['free turn jpeg', 'fixture:oriented-gps.jpg', 'transform', { rotate: 30 }],
      ['compress', 'fixture:oriented-gps.jpg', 'compress', { maxDimension: 4000, maxBytes: 1000000, rotate: 90 }],
      ['keep', 'fixture:every-tag.jpg', 'transform', { metadata: 'keep' }],
      ['no-location', 'fixture:every-tag.jpg', 'transform', { metadata: 'no-location' }],
      ['none', 'fixture:every-tag.jpg', 'transform', { metadata: 'none' }],
      ['agent-3', 'copy', 'transform', {}],
      ['agent-3 top half', 'copy', 'transform', { crop: { x: 0, y: 0, width: 360, height: 240 } }],
      ['agent-3 top half at 24', 'copy', 'transform', { crop: { x: 0, y: 0, width: 360, height: 240 }, resizeWidth: 24 }],
      ['gain map, a quarter', 'fixture:gainmap-iso.jpg', 'transform', { rotate: 90 }],
      ['gain map, 10°', 'fixture:gainmap-iso.jpg', 'transform', { rotate: 10 }],
    ];
    // `copy` is photo-3's copy, whose name the drive cannot know: Edit the copy makes it the source.
    const ops = ['clock settle', 'tap photo-3', 'clock settle'];
    for (const [, source, call, options] of cases)
      ops.push(source === 'copy' ? 'tap edit-copy' : `type edit-source ${source}`, `type edit-options ${JSON.stringify(options)}`, `tap ${call}`, 'clock settle', 'state edited');
    // Under the agent, app storage is a named scratch store's.
    const r = drive({ photos: 'granted' }, [...ops, 'logs'], ['--storage', 'photos-codec']);
    const records = cases.map((_, i) => { try { return JSON.parse(r[3 + i * 5 + 4]?.record ?? 'null'); } catch { return null; } });
    // The files are in the app's private directory, which adb reads only as root: the APK is not debuggable, so
    // `run-as` refuses, and a Google Play image's adbd refuses root. There the sizes are checked and the files are not.
    const su = adb('root'), said = `${su.stdout ?? ''}${su.stderr ?? ''}`.trim(), rooted = su.status === 0 && !/cannot run as root/.test(said);
    const onFile = rooted ? check : () => {};
    if (rooted) adb('wait-for-device');
    else console.log(`SKIP ${where} 4: the edits' pixels and metadata, since adb cannot read the app's files here (adb root: ${said || `exit ${su.status}`}); a Google APIs image without Play runs adb as root`);
    // The launch's temporary directory: the newest holding these edits.
    const dirs = rooted ? adb('shell', 'ls -dt $(find /data/data/com.exact.photosfixture -type d -name edits)').stdout.toString().trim().split(/\s+/) : [];
    const out = resolve(tmpdir(), `photos-android-${process.pid}`);
    mkdirSync(out, { recursive: true });
    const file = (record) => {
      if (!rooted || !record?.path?.startsWith('app:/tmp/edits/')) return null;
      const local = resolve(out, `${api}-${basename(record.path)}`);
      return adb('pull', `${dirs[0]}/${record.path.slice('app:/tmp/edits/'.length)}`, local).status === 0 ? local : null;
    };
    const pixels = (path) => {
      const png = path.endsWith('.png') ? path : `${path}.png`;
      if (png !== path) spawnSync('sips', ['-s', 'format', 'png', path, '--out', png]);
      return decodePng(readFileSync(png));
    };
    const at = (img, x, y) => [...img.data.slice((y * img.width + x) * 4, (y * img.width + x) * 4 + 4)];
    const blue = ([r, g, b]) => b > 150 && r < 100, red = ([r, g, b]) => r > 150 && b < 100;
    const by = Object.fromEntries(cases.map(([name], i) => [name, { record: records[i], path: file(records[i]) }]));
    const sized = (name, w, h) => check(where, `4: ${name} is ${w}x${h}`, by[name].record?.width === w && by[name].record?.height === h, JSON.stringify(by[name].record));
    const img = name => by[name].path && pixels(by[name].path);
    sized('upright', 48, 64);
    const up = img('upright');
    onFile(where, '4: upright, blue on the left', up && blue(at(up, 6, 32)) && red(at(up, 41, 32)), up && JSON.stringify([at(up, 6, 32), at(up, 41, 32)]));
    onFile(where, '4: upright, orientation 1 and the GPS kept', exif(by.upright.path)?.orientation === 1 && exif(by.upright.path)?.gps, JSON.stringify(exif(by.upright.path)));
    sized('rotate 90', 64, 48);
    const turned = img('rotate 90');
    onFile(where, '4: rotate 90, blue on top', turned && blue(at(turned, 32, 6)) && red(at(turned, 32, 41)), turned && JSON.stringify([at(turned, 32, 6), at(turned, 32, 41)]));
    const flipped = img('flip');
    onFile(where, '4: flip, blue on the right', flipped && red(at(flipped, 6, 32)) && blue(at(flipped, 41, 32)), flipped && JSON.stringify([at(flipped, 6, 32), at(flipped, 41, 32)]));
    sized('crop', 24, 64);
    const cut = img('crop');
    onFile(where, '4: crop, all blue', cut && [[2, 2], [21, 2], [2, 61], [21, 61], [12, 32]].every(([x, y]) => blue(at(cut, x, y))), cut && JSON.stringify(at(cut, 21, 32)));
    const clear = img('free turn png'), white = img('free turn jpeg');
    onFile(where, '4: rotate 30, a PNG\'s corners are clear', clear && at(clear, 0, 0)[3] === 0 && at(clear, clear.width - 1, clear.height - 1)[3] === 0, clear && JSON.stringify(at(clear, 0, 0)));
    onFile(where, '4: rotate 30, a JPEG\'s corners are white', white && at(white, 0, 0).slice(0, 3).every(v => v > 240), white && JSON.stringify(at(white, 0, 0)));
    sized('compress', 64, 48);
    onFile(where, '4: compressImage turns and keeps no metadata', by.compress.path && !exif(by.compress.path), JSON.stringify(by.compress.path && exif(by.compress.path)));
    const keep = exif(by.keep.path), bare = exif(by['no-location'].path), none = exif(by.none.path);
    onFile(where, '4: keep keeps the GPS, the date, the camera, the rights and the maker note', keep?.gps && keep.date && keep.make && keep.copyright && keep.makerNote, JSON.stringify(keep));
    onFile(where, '4: no-location keeps the date, the camera and the rights, and no GPS or maker note', bare && !bare.gps && !bare.makerNote && bare.date && bare.make && bare.copyright, JSON.stringify(bare));
    onFile(where, '4: none writes no metadata', by.none.path && !none, JSON.stringify(none));
    sized('agent-3', 360, 480);
    sized('agent-3 top half', 360, 240);
    sized('agent-3 top half at 24', 24, 16);
    const half = img('agent-3 top half'), whole = img('agent-3');
    onFile(where, '4: agent-3\'s top half is the upright photo\'s', half && whole && [[30, 30], [180, 120], [330, 200]].every(([x, y]) => at(half, x, y).slice(0, 3).every((v, i) => Math.abs(v - at(whole, x, y)[i]) < 24)),
      half && whole && JSON.stringify([at(half, 180, 120), at(whole, 180, 120)]));
    check(where, '4: a free turn of a gain-map photo says hdr false', by['gain map, 10°'].record?.hdr === false, JSON.stringify(by['gain map, 10°'].record));
    // Recorded, not checked: whether API 34 keeps the gain map through a quarter turn (D16).
    console.log(`NOTE ${where} 4: gain map through a quarter turn: hdr ${by['gain map, a quarter'].record?.hdr}`);
  }
  // 5: MediaStore for real, the implementation bound (§11's conformance on an emulator): two photos pushed and
  // scanned, the read granted by `pm grant`; the list, a copy with its location, an add whose row is no longer
  // pending and joins the list. The app's own rows and the pushed files go first, so a rerun starts alike.
  {
    const id = 'com.exact.photosfixture', images = 'content://media/external/images/media';
    adb('shell', `content delete --uri ${images} --where "owner_package_name='${id}' OR _display_name LIKE 'photos-drive-%'"`);
    adb('shell', 'rm -f /sdcard/Pictures/photos-drive-*');
    for (const [from, name] of [[resolve(here, '../fixture/agent-3.jpg'), 'photos-drive-turned.jpg'], [resolve(root, 'scripts/fixtures/picker/oriented-gps.jpg'), 'photos-drive-gps.jpg']])
      adb('push', from, `/sdcard/Pictures/${name}`);
    adb('shell', api >= 30 ? 'content call --method scan_volume --uri content://media --arg external_primary'
      : 'am broadcast -a android.intent.action.MEDIA_SCANNER_SCAN_FILE -d file:///sdcard/Pictures/photos-drive-turned.jpg && am broadcast -a android.intent.action.MEDIA_SCANNER_SCAN_FILE -d file:///sdcard/Pictures/photos-drive-gps.jpg');
    const read = api >= 33 ? 'READ_MEDIA_IMAGES' : 'READ_EXTERNAL_STORAGE';
    for (const p of [read, 'ACCESS_MEDIA_LOCATION']) adb('shell', 'pm', 'grant', id, `android.permission.${p}`);
    // The launch's read is decided before the implementation is bound, so it waits on the runner's ticket.
    const shot = resolve(tmpdir(), `photos-android-${api}-mediastore.png`);
    const r = drive({}, ['clock +100', 'tap @recent allow', 'clock settle', 'tree', 'tap photo-1', 'clock settle', 'state copied', 'tap add',
      'clock settle', 'state added', 'clock settle', 'tree', `screenshot ${shot}`, 'layout copy-picture', 'logs'], ['--bind', 'photos']);
    const status = t => /"testId":"status","text":"([^"]*)"/.exec(tree(t))?.[1];
    check(where, '5: granted, full access, the two pushed photos listed', status(r[3]) === 'granted full granted' && grid(r[3]) >= 2, `${status(r[3])}, ${grid(r[3])} photos`);
    const copied = r[6], pushed = { 'photos-drive-turned.jpg': [360, 480, AGENT(3)], 'photos-drive-gps.jpg': [48, 64] };
    check(where, '5: the newest copied upright, its location kept', pushed[copied?.name] && copied.width === pushed[copied.name][0] && copied.height === pushed[copied.name][1]
      && copied.android?.locationRedacted === false, JSON.stringify(copied));
    const added = r[9]?.asset;
    const row = added ? adb('shell', `content query --uri ${added} --projection _display_name:is_pending:relative_path:owner_package_name`).stdout.toString() : '';
    check(where, '5: the add is a MediaStore row of the app\'s, no longer pending, under Pictures', /is_pending=0/.test(row) && row.includes(`owner_package_name=${id}`) && /relative_path=Pictures/.test(row), `${added} ${row}`);
    // The reader's own loader sizes a turned JPEG by its upright size (an `img` 48 wide, its height auto).
    const drawn = r[13]?.node?.absolute;
    check(where, '5: the copy drawn by the reader\'s loader is upright', copied && drawn?.w === 48 && Math.abs(drawn.h - 48 * copied.height / copied.width) < 1, JSON.stringify(drawn));
    check(where, '5: the add joins the list', grid(r[11]) === grid(r[3]) + 1, `${grid(r[3])} then ${grid(r[11])}; ${shot}`);
  }
  // 6: MediaStore paged (D1): 30 more pushed, the six fixtures' bytes each made distinct after the JPEG's end, so
  // many share a date taken and a date added; read 12 a page through Older to the oldest, each row once; then removed.
  {
    const id = 'com.exact.photosfixture', images = 'content://media/external/images/media';
    adb('shell', `content delete --uri ${images} --where "_display_name LIKE 'photos-page-%'"`);
    adb('shell', 'rm -f /sdcard/Pictures/photos-page-*');
    const out = resolve(tmpdir(), `photos-android-pages-${process.pid}`);
    mkdirSync(out, { recursive: true });
    for (let i = 0; i < 30; i++) {
      const file = resolve(out, `photos-page-${i}.jpg`);
      writeFileSync(file, Buffer.concat([readFileSync(resolve(here, `../fixture/agent-${1 + i % 6}.jpg`)), Buffer.from(`page ${i}`)]));
      adb('push', file, '/sdcard/Pictures/');
    }
    adb('shell', api >= 30 ? 'content call --method scan_volume --uri content://media --arg external_primary'
      : `for f in /sdcard/Pictures/photos-page-*; do am broadcast -a android.intent.action.MEDIA_SCANNER_SCAN_FILE -d file://$f; done`);
    adb('shell', 'pm', 'grant', id, `android.permission.${api >= 33 ? 'READ_MEDIA_IMAGES' : 'READ_EXTERNAL_STORAGE'}`);
    const rows = (adb('shell', `content query --uri ${images} --projection _id`).stdout.toString().match(/^Row:/gm) ?? []).length;
    const pages = Math.ceil(rows / 12), ops = ['clock +100', 'tap @recent allow', 'clock settle', 'tree'];
    for (let p = 1; p < pages; p++) ops.push('tap older', 'clock settle', 'tree');
    const r = drive({}, ops, ['--bind', 'photos']);
    const trees = r.filter(x => Array.isArray(x?.nodes));
    const shown = trees.map(t => t.nodes.filter(n => /^photos:/.test(n.props?.imageSource ?? '')).map(n => n.props.imageSource));
    const all = shown.flat(), ids = trees.map(t => new Set(t.nodes.map(n => n.props?.testId)));
    check(where, `6: ${rows} rows in ${pages} pages of 12, each once`, trees.length === pages && all.length === rows && new Set(all).size === rows,
      `${trees.length} pages, ${all.length} shown, ${new Set(all).size} distinct`);
    check(where, '6: Older on every page but the last, which says the oldest', ids.slice(0, -1).every(t => t.has('older')) && ids.at(-1)?.has('oldest'),
      JSON.stringify(ids.map(t => [t.has('older'), t.has('oldest')])));
    // Gone again, so drive 5's list stays under one page.
    adb('shell', `content delete --uri ${images} --where "_display_name LIKE 'photos-page-%'"`);
    adb('shell', 'rm -f /sdcard/Pictures/photos-page-*');
  }
  // 7: MediaStore filtered to screenshots (LLP 1133.019.000 D4): files in `Pictures/Screenshots/` and
  // `DCIM/screenshots/` (another case), and one the system takes, are listed newest first, 12 a page through Older;
  // files in `Pictures/MyScreenshots/`, `Pictures/Screenshots/old/` and `Pictures/` are not. The emulator may be
  // shared, so the drive removes only what it made: its files, which carry this run's pid, the directories that were
  // not there before, and the system's screenshot when it was the one SystemUI wrote after the key.
  {
    const id = 'com.exact.photosfixture', images = 'content://media/external/images/media', tag = `photos-shot-${process.pid}-`;
    const dirs = { 'Pictures/Screenshots/': 10, 'DCIM/screenshots/': 3, 'Pictures/MyScreenshots/': 2, 'Pictures/Screenshots/old/': 1, 'Pictures/': 1 };
    const exists = dir => adb('shell', `ls -d '/sdcard/${dir}'`).status === 0;
    const made = Object.keys(dirs).filter(d => !exists(d)).reverse();
    const out = resolve(tmpdir(), `photos-android-shots-${process.pid}`);
    mkdirSync(out, { recursive: true });
    const seeded = [];
    let system = [];
    const rowsOf = () => [...adb('shell', `content query --uri ${images} --projection _id:_display_name:relative_path:owner_package_name:datetaken:date_added`).stdout.toString()
      .matchAll(/_id=(\d+), _display_name=(.*), relative_path=(.*), owner_package_name=(.*), datetaken=(.*), date_added=(.*)$/gm)]
      .map(m => ({ id: m[1], name: m[2], path: m[3].trim(), owner: m[4].trim(), taken: m[5] === 'NULL' ? null : Number(m[5]), added: Number(m[6]) }));
    const poll = (done) => { let rows = rowsOf(); for (let t = 0; t < 20 && !done(rows); t++) { spawnSync('sleep', ['0.5']); rows = rowsOf(); } return rows; };
    try {
      for (const [dir, count] of Object.entries(dirs))
        for (let i = 0; i < count; i++) {
          const name = `${tag}${seeded.length}.jpg`, file = resolve(out, name);
          writeFileSync(file, Buffer.concat([readFileSync(resolve(here, `../fixture/agent-${1 + seeded.length % 6}.jpg`)), Buffer.from(name)]));
          adb('shell', `mkdir -p '/sdcard/${dir}'`);
          adb('push', file, `/sdcard/${dir}`);
          seeded.push({ name, dir });
        }
      adb('shell', api >= 30 ? 'content call --method scan_volume --uri content://media --arg external_primary'
        : seeded.map(f => `am broadcast -a android.intent.action.MEDIA_SCANNER_SCAN_FILE -d file:///sdcard/${f.dir}${f.name}`).join(' && '));
      const at = (rows, f) => rows.some(r => r.name === f.name && r.path.toLowerCase() === f.dir.toLowerCase());
      const indexed = poll(rows => seeded.every(f => at(rows, f)));
      check(where, `7: the ${seeded.length} files are indexed where they were pushed`, seeded.every(f => at(indexed, f)), JSON.stringify(seeded.filter(f => !at(indexed, f))));
      // The system's own screenshot: SystemUI writes it under Pictures/Screenshots/ a moment after the key.
      const before = new Set(indexed.map(r => r.id)), taken = r => !before.has(r.id) && r.owner === 'com.android.systemui' && /^Screenshot_/.test(r.name);
      adb('shell', 'input keyevent KEYCODE_SYSRQ');
      const rows = poll(rows => rows.some(taken));
      system = rows.filter(taken);
      const screenshot = p => /(^|\/)screenshots\/*$/i.test(p);
      // The list's order: the date taken, else the date added, then the row, each newest first (D1).
      const newest = (a, b) => (b.taken ?? -Infinity) - (a.taken ?? -Infinity) || b.added - a.added || Number(b.id) - Number(a.id);
      const want = rows.filter(r => screenshot(r.path)).sort(newest).map(r => `photos:${images}/${r.id}`);
      const decoys = rows.filter(r => r.name.startsWith(tag) && !screenshot(r.path)).map(r => `photos:${images}/${r.id}`);
      adb('shell', 'pm', 'grant', id, `android.permission.${api >= 33 ? 'READ_MEDIA_IMAGES' : 'READ_EXTERNAL_STORAGE'}`);
      const pages = Math.ceil(want.length / 12), ops = ['clock +100', 'tap @recent allow', 'clock settle', 'tree', 'tap screenshots', 'clock settle', 'tree'];
      for (let p = 1; p < pages; p++) ops.push('tap older', 'clock settle', 'tree');
      const r = drive({}, ops, ['--bind', 'photos']);
      const trees = r.filter(x => Array.isArray(x?.nodes)).slice(1);
      const shown = trees.map(t => t.nodes.filter(n => /^photos:/.test(n.props?.imageSource ?? '')).map(n => n.props.imageSource));
      const all = shown.flat(), last = new Set(trees.at(-1)?.nodes.map(n => n.props?.testId));
      check(where, '7: the system took one screenshot, in Pictures/Screenshots/', system.length === 1 && system[0].path === 'Pictures/Screenshots/', JSON.stringify(system));
      check(where, `7: Screenshots lists the ${want.length} rows in a Screenshots directory newest first, ${pages} pages of 12, each once, the last the oldest`,
        want.length >= 14 && JSON.stringify(all) === JSON.stringify(want) && shown.slice(0, -1).every(p => p.length === 12) && last.has('oldest'),
        `${JSON.stringify(shown.map(p => p.length))} of ${want.length}`);
      check(where, '7: none of MyScreenshots/, Screenshots/old/ or Pictures/', decoys.length === 4 && !decoys.some(d => all.includes(d)), JSON.stringify(decoys));
    } finally {
      // The shell may not delete another app's row by its URI, so each file goes and the volume is scanned.
      for (const f of [...seeded.map(f => ({ path: f.dir, name: f.name })), ...(system.length === 1 ? system : [])]) adb('shell', `rm -f '/sdcard/${f.path}${f.name}'`);
      for (const dir of made) adb('shell', `rmdir '/sdcard/${dir}'`);
      adb('shell', api >= 30 ? 'content call --method scan_volume --uri content://media --arg external_primary'
        : [...seeded.map(f => `${f.dir}${f.name}`), ...system.map(s => `${s.path}${s.name}`)].map(p => `am broadcast -a android.intent.action.MEDIA_SCANNER_SCAN_FILE -d file:///sdcard/${p}`).join(' && '));
    }
  }
  // 8: the replacement path (LLP 1133.019.000 D1, D3), over MediaStore's screenshots, in one launch: a token answers
  // `expired`, and after each change another app makes while the fixture is away (a screenshot added, a photo moved
  // into `Screenshots/` and out, trashed, restored and deleted) the list on return is MediaStore's, with nothing
  // pressed and the read unchanged. Then, relaunched, a denial and limited access. The shell moves, trashes and
  // restores a row through MediaStore, as root (an emulator's adb runs as root), and deletes by the file. It touches
  // only the files it made, by their paths, and puts the fixture's read permissions and their user-fixed flags back.
  if (api >= 30) {
    const { open } = await import(resolve(root, 'scripts/agent.mjs'));
    const id = 'com.exact.photosfixture', images = 'content://media/external/images/media', tag = `exact-photos-rescan-${process.pid}-${Date.now()}-`;
    const shots = 'Pictures/Screenshots/', made = ['Pictures/', shots].filter(d => adb('shell', `ls -d '/sdcard/${d}'`).status !== 0).reverse();
    const read = `android.permission.${api >= 33 ? 'READ_MEDIA_IMAGES' : 'READ_EXTERNAL_STORAGE'}`, selected = 'android.permission.READ_MEDIA_VISUAL_USER_SELECTED';
    const held = p => {
      const line = adb('shell', `dumpsys package ${id}`).stdout.toString().split('\n').find(l => l.trim().startsWith(`${p}: granted=`));
      if (!line) throw new Error(`dumpsys package ${id} names no ${p}`);
      return { granted: /granted=true/.test(line), fixed: /USER_FIXED/.test(line), set: /USER_SET/.test(line) };
    };
    const found = Object.fromEntries([read, ...(api >= 34 ? [selected] : [])].map(p => [p, held(p)]));
    const out = resolve(tmpdir(), `photos-android-rescan-${process.pid}`);
    mkdirSync(out, { recursive: true });
    const scan = () => adb('shell', 'content call --method scan_volume --uri content://media --arg external_primary');
    const rowsOf = () => [...adb('shell', `content query --uri ${images} --projection _id:_display_name:relative_path:datetaken:date_added`).stdout.toString()
      .matchAll(/_id=(\d+), _display_name=(.*), relative_path=(.*), datetaken=(.*), date_added=(.*)$/gm)]
      .map(m => ({ id: m[1], name: m[2], path: m[3].trim(), taken: m[4] === 'NULL' ? null : Number(m[4]), added: Number(m[5]) }));
    const poll = (done) => { let rows = rowsOf(); for (let t = 0; t < 20 && !done(rows); t++) { spawnSync('sleep', ['0.5']); rows = rowsOf(); } return rows; };
    const names = [];
    const push = (name, dir) => {
      // The cleanup removes the name from both directories, so it must be free in both.
      for (const d of ['Pictures/', shots]) if (adb('shell', `ls '/sdcard/${d}${name}'`).status === 0) throw new Error(`/sdcard/${d}${name} is there already`);
      const file = resolve(out, name);
      writeFileSync(file, Buffer.concat([readFileSync(resolve(here, '../fixture/agent-1.jpg')), Buffer.from(name)]));
      adb('shell', `mkdir -p '/sdcard/${dir}'`);
      names.push(name);
      adb('push', file, `/sdcard/${dir}`);
      scan();
      const at = r => r.name === name && r.path === dir;
      return poll(rows => rows.some(at)).find(at);
    };
    const uri = row => `${images}/${row.id}`, update = (row, bind) => adb('shell', `content update --uri ${uri(row)} --bind ${bind}`);
    // What the first page of screenshots should be now: the list's order (D1) over the rows in a Screenshots directory.
    const screenshot = r => /(^|\/)screenshots\/*$/i.test(r.path);
    const newest = (a, b) => (b.taken ?? -Infinity) - (a.taken ?? -Infinity) || b.added - a.added || Number(b.id) - Number(a.id);
    const want = () => rowsOf().filter(screenshot).sort(newest).slice(0, 12).map(r => `photos:${uri(r)}`);
    const shown = t => (t?.nodes ?? []).map(n => n.props?.imageSource).filter(v => /^photos:/.test(v ?? ''));
    const text = (t, testId) => t?.nodes?.find(n => n.props?.testId === testId)?.props?.text;
    const component = adb('shell', `cmd package resolve-activity --brief -c android.intent.category.LAUNCHER ${id}`).stdout.toString().trim().split('\n').at(-1);
    const decode = t => { try { return JSON.parse(Buffer.from(t, 'base64').toString()); } catch { return null; } };
    // The drive's photos have no date taken, so they sort after every screenshot that has one.
    const dated = rowsOf().filter(r => screenshot(r) && r.taken !== null).length;
    let s;
    if (dated > 9) check(where, '8: at most 9 screenshots here with a date taken, so the drive\'s photos reach the first page', false, `${dated} have one`);
    else try {
      const s1 = push(`${tag}1.jpg`, shots), p1 = push(`${tag}2.jpg`, 'Pictures/');
      adb('shell', 'pm', 'grant', id, read);
      s = await open({ host: 'android', app, reader: true, bind: 'photos' });
      await s.tap('@recent', { choice: 'allow' }).catch(() => {});
      await s.clock('settle');
      await s.tap('screenshots');
      await s.clock('settle');
      let page = shown(await s.tree());
      check(where, '8: the first page of screenshots, the one pushed there in it and the one in Pictures/ not', JSON.stringify(page) === JSON.stringify(want())
        && page.includes(`photos:${uri(s1)}`) && !page.includes(`photos:${uri(p1)}`), `${JSON.stringify(page)} against ${JSON.stringify(want())}`);
      await s.tap('watch');
      await s.tap('only-screenshots');
      await s.clock('settle');
      const panel = async () => {
        const t = await s.tree();
        return { changes: text(t, 'changes'), error: text(t, 'changes-error'), token: text(t, 'token') };
      };
      let p = await panel();
      const full = p.token, start = decode(full);
      check(where, '8: no token: a rescan-only starting point for the screenshots', p.changes === '|||false false' && start?.backend === 'mediastore' && start.access === 'full'
        && JSON.stringify(start.subtypes) === '["screenshot"]' && !('at' in start), JSON.stringify(p));
      await s.type('since', p.token);
      await s.clock('settle');
      p = await panel();
      check(where, '8: the token: expired, empty, the same starting point', p.changes === '|||false true' && !p.error && JSON.stringify(decode(p.token)) === JSON.stringify(start), JSON.stringify(p));
      /** [change] made while the fixture is in the background: the first page after its return once it is
       * MediaStore's, else after 10 s, and what MediaStore's is. */
      const away = async (change) => {
        adb('shell', 'input keyevent KEYCODE_HOME');
        const front = () => /ResumedActivity.*com\.exact\.photosfixture\//.test(adb('shell', 'dumpsys activity activities').stdout.toString());
        for (let n = 0; n < 20 && front(); n++) spawnSync('sleep', ['0.25']);
        const left = !front();
        change();
        const target = want();
        adb('shell', `am start -a android.intent.action.MAIN -c android.intent.category.LAUNCHER -n ${component}`);
        let t = await s.tree();
        for (let n = 0; n < 40 && JSON.stringify(shown(t)) !== JSON.stringify(target); n++) {
          await s.clock('+250 real');
          t = await s.tree();
        }
        return { page: shown(t), target, status: text(t, 'status'), left };
      };
      const listed = (name, r, ok) => check(where, `8: ${name}: on return the first page is MediaStore's, the read unchanged`,
        ok && r.left && JSON.stringify(r.page) === JSON.stringify(r.target) && r.status === 'granted full granted',
        `${r.left ? '' : 'never left the foreground; '}${r.status} ${JSON.stringify(r.page)} against ${JSON.stringify(r.target)}`);
      let s2;
      let r = await away(() => { s2 = push(`${tag}3.jpg`, shots); });
      listed('a screenshot added', r, s2 && r.page.includes(`photos:${uri(s2)}`));
      r = await away(() => update(p1, `relative_path:s:${shots}`));
      listed('a photo moved into Screenshots/', r, r.page.includes(`photos:${uri(p1)}`));
      r = await away(() => update(p1, 'relative_path:s:Pictures/'));
      listed('the photo moved out', r, !r.page.includes(`photos:${uri(p1)}`));
      r = await away(() => update(s1, 'is_trashed:i:1'));
      listed('a screenshot trashed', r, !r.page.includes(`photos:${uri(s1)}`));
      r = await away(() => update(s1, 'is_trashed:i:0'));
      listed('the screenshot restored, its id kept', r, r.page.includes(`photos:${uri(s1)}`));
      r = await away(() => { adb('shell', `rm -f '/sdcard/${shots}${s1.name}'`); scan(); poll(rows => !rows.some(x => x.id === s1.id)); });
      listed('the screenshot deleted', r, !r.page.includes(`photos:${uri(s1)}`));
      await s.close();
      s = null;

      /** A launch after setting [grants], the Changes panel given the full view's token. */
      const relaunch = async (grants) => {
        for (const [p, on] of Object.entries(grants)) adb('shell', 'pm', on ? 'grant' : 'revoke', id, p);
        s = await open({ host: 'android', app, reader: true, bind: 'photos' });
        await s.tap('@recent', { choice: 'allow' }).catch(() => {});
        await s.clock('settle');
        await s.tap('watch');
        await s.tap('only-screenshots');
        await s.clock('settle');
        await s.type('since', full);
        await s.clock('settle');
        return { panel: await panel(), tree: await s.tree() };
      };
      // A denial: the read revoked and fixed by the user, so asking answers at once with no dialog.
      adb('shell', 'pm', 'revoke', id, read);
      adb('shell', 'pm', 'set-permission-flags', id, read, 'user-fixed');
      let { panel: denied, tree } = await relaunch(api >= 34 ? { [selected]: false } : {});
      check(where, '8: denied: the list and the token NotAllowedError, nothing shown', text(tree, 'recent-error') === 'NotAllowedError' && shown(tree).length === 0
        && denied.error === 'NotAllowedError' && /^denied none/.test(text(tree, 'status') ?? ''), `${text(tree, 'status')}, ${text(tree, 'recent-error')}, ${JSON.stringify(denied)}`);
      await s.close();
      s = null;
      adb('shell', 'pm', 'clear-permission-flags', id, read, 'user-fixed');

      // Limited access (API 34) with nothing selected here: the list reads the app's own photos (drive 5 leaves its
      // add) and none of the shell's, and the full view's token expires into a limited one.
      if (api >= 34) {
        let limited;
        ({ panel: limited, tree } = await relaunch({ [selected]: true }));
        const mine = shown(tree), own = (adb('shell', `content query --uri ${images} --projection _id --where "owner_package_name='${id}'"`).stdout.toString()
          .match(/_id=\d+/g) ?? []).map(m => `photos:${images}/${m.slice(4)}`);
        check(where, '8: limited: status limited, Manage, the list read with the app\'s own photo and none of the shell\'s', /^granted limited/.test(text(tree, 'status') ?? '')
          && text(tree, 'recent-error') === '' && JSON.stringify(tree).includes('"manage"') && own.some(o => mine.includes(o))
          && ![s1, s2, p1].some(x => x && mine.includes(`photos:${uri(x)}`)), `${text(tree, 'status')}, ${text(tree, 'recent-error')}, ${JSON.stringify(mine)}, own ${JSON.stringify(own)}`);
        // The drive's screenshot is on the first page of screenshots under full access (the check above): not here.
        await s.tap('screenshots');
        await s.clock('settle');
        const shots8 = shown(await s.tree());
        check(where, '8: limited: the screenshots read, without the shell\'s screenshot', text(await s.tree(), 'recent-error') === '' && s2 && !shots8.includes(`photos:${uri(s2)}`),
          JSON.stringify(shots8));
        check(where, '8: limited: the full view\'s token expires into a limited one', limited.changes === '|||false true' && decode(limited.token)?.access === 'limited'
          && JSON.stringify(decode(limited.token)?.subtypes) === '["screenshot"]', JSON.stringify(limited));
      } else console.log(`SKIP ${where} 8: limited access needs API 34`);
    } finally {
      if (s) await s.close().catch(e => console.log(`8: the session did not close: ${e.message}`));
      for (const [p, was] of Object.entries(found)) {
        adb('shell', 'pm', was.granted ? 'grant' : 'revoke', id, p);
        for (const [flag, on] of [['user-fixed', was.fixed], ['user-set', was.set]]) adb('shell', 'pm', on ? 'set-permission-flags' : 'clear-permission-flags', id, p, flag);
      }
      const now = Object.entries(found).filter(([p, was]) => JSON.stringify(held(p)) !== JSON.stringify(was));
      check(where, "8: the fixture's read permissions are as they were", now.length === 0, JSON.stringify(now));
      // This run's files, by their names, wherever the drive left them; a trashed one under its trashed name.
      for (const name of names)
        adb('shell', `rm -f '/sdcard/Pictures/${name}' '/sdcard/${shots}${name}' /sdcard/Pictures/.trashed-*-'${name}' /sdcard/${shots}.trashed-*-'${name}'`);
      for (const dir of made) adb('shell', `rmdir '/sdcard/${dir}'`);
      scan();
    }
  } else console.log(`SKIP ${where} 8: trash and restore need API 30`);
  // 9: the fixture's photo index of the screenshots (LLP 1133.019.000 D1, importer.mjs), against MediaStore, which
  // answers every token `expired`: 501 screenshots pushed, the first import, a change while the app is away, a
  // relaunch, a sync stopped mid-list and a denial. Each sync is one replacement, and the index settles in step with
  // the list of the screenshots. The read permissions and their flags are put back, and the pushed files removed.
  if (api >= 30) {
    const { open } = await import(resolve(root, 'scripts/agent.mjs'));
    const { imported, settled } = await import('./importer.mjs');
    const id = 'com.exact.photosfixture', tag = `exact-photos-index-${process.pid}-`, shots = '/sdcard/Pictures/Screenshots';
    const read = `android.permission.${api >= 33 ? 'READ_MEDIA_IMAGES' : 'READ_EXTERNAL_STORAGE'}`, selected = 'android.permission.READ_MEDIA_VISUAL_USER_SELECTED';
    const flags = p => adb('shell', `dumpsys package ${id}`).stdout.toString().split('\n').find(l => l.trim().startsWith(`${p}: granted=`)) ?? '';
    const found = Object.fromEntries([read, ...(api >= 34 ? [selected] : [])].map(p => [p, flags(p)]));
    const made = adb('shell', `ls -d ${shots}`).status !== 0;
    const scan = () => adb('shell', 'content call --method scan_volume --uri content://media --arg external_primary');
    const out = resolve(tmpdir(), `photos-android-index-${process.pid}`);
    mkdirSync(out, { recursive: true });
    let pushed = 0;
    /** `n` more screenshots, pushed as one directory and scanned once. */
    const push = (n) => {
      const batch = resolve(out, `batch-${pushed}`);
      mkdirSync(batch, { recursive: true });
      for (let i = 0; i < n; i++, pushed++)
        writeFileSync(resolve(batch, `${tag}${pushed}.jpg`), Buffer.concat([readFileSync(resolve(here, `../fixture/agent-${1 + pushed % 6}.jpg`)), Buffer.from(`${tag}${pushed}`)]));
      adb('shell', `mkdir -p ${shots}`);
      adb('push', `${batch}/.`, `${shots}/`);
      scan();
    };
    const component = adb('shell', `cmd package resolve-activity --brief -c android.intent.category.LAUNCHER ${id}`).stdout.toString().trim().split('\n').at(-1);
    const store = `importer-${process.pid}`;
    let s;
    const launch = async (grants = { [read]: true, ...(api >= 34 ? { [selected]: false } : {}) }) => {
      for (const [p, on] of Object.entries(grants)) adb('shell', 'pm', on ? 'grant' : 'revoke', id, p);
      s = await open({ host: 'android', app, reader: true, bind: 'photos', storage: store });
      await s.tap('@recent', { choice: 'allow' }).catch(() => {});
      await s.clock('settle');
    };
    const close = async () => { await s?.close(); s = null; };
    /** One replacement: the stored token's walk, which expires, then the list and the walk from its checkpoint. */
    const replacement = r => r.how === 'replaced' && r.pages === 2 + Math.ceil(r.listed / 500);
    try {
      push(501);
      await launch();
      const first = await imported(s, { shots: true });
      // The first has no checkpoint: it takes one, where a later sync's walk expires.
      check(where, '9: the first import of 501 screenshots and more: the list, in step', first.inStep && replacement(first) && first.count >= 501 && first.syncs === 1, JSON.stringify(first));
      // Away: three screenshots deleted and two pushed. On return the module announces the list and the feed.
      adb('shell', 'input keyevent KEYCODE_HOME');
      spawnSync('sleep', ['1']);
      adb('shell', `rm -f ${[0, 1, 2].map(i => `${shots}/${tag}${i}.jpg`).join(' ')}`);
      push(2);
      adb('shell', `am start -a android.intent.action.MAIN -c android.intent.category.LAUNCHER -n ${component}`);
      const back = await settled(s, r => r.inStep && r.count === first.count - 1 && r.syncs > first.syncs);
      await s.clock('+2000 real');
      const after = await settled(s);
      check(where, '9: back in the app: the index replaced, in step, one replacement a sync', back.inStep && replacement(after) && after.count === first.count - 1, JSON.stringify(after));
      check(where, '9: the return asks the index once', after.syncs === first.syncs + 1, `${first.syncs} then ${after.syncs}`);
      await close();
      await launch();
      const again = await imported(s, { shots: true });
      check(where, '9: relaunched: the token expires, one replacement, in step', again.inStep && replacement(again) && again.syncs === 1, JSON.stringify(again));
      await close();
      push(1);
      await launch();
      const cut = await imported(s, { shots: true, two: true, budget: true }, r => !!r.how);
      check(where, '9: stopped mid-list: the replacement discarded, the index as it was', cut.how === 'interrupted' && cut.pages === 2 && cut.count === again.count, JSON.stringify(cut));
      await close();
      adb('shell', 'pm', 'set-permission-flags', id, read, 'user-fixed');
      await launch({ [read]: false, ...(api >= 34 ? { [selected]: false } : {}) });
      const denied = await imported(s, { shots: true }, r => !!r.error);
      check(where, '9: denied: NotAllowedError, the index as it was committed', denied.how === 'failed' && denied.error === 'NotAllowedError' && denied.count === again.count, JSON.stringify(denied));
      await close();
      adb('shell', 'pm', 'clear-permission-flags', id, read, 'user-fixed');
      await launch();
      const whole = await imported(s, { shots: true });
      check(where, '9: granted again: one replacement, in step', whole.inStep && replacement(whole) && whole.count === again.count + 1, JSON.stringify(whole));
    } finally {
      await close().catch(e => console.log(`9: the session did not close: ${e.message}`));
      for (const [p, line] of Object.entries(found)) {
        adb('shell', 'pm', /granted=true/.test(line) ? 'grant' : 'revoke', id, p);
        for (const [flag, word] of [['user-fixed', 'USER_FIXED'], ['user-set', 'USER_SET']]) adb('shell', 'pm', line.includes(word) ? 'set-permission-flags' : 'clear-permission-flags', id, p, flag);
      }
      const now = Object.entries(found).filter(([p, line]) => flags(p).trim() !== line.trim());
      check(where, "9: the fixture's read permissions are as they were", now.length === 0, JSON.stringify(now));
      adb('shell', `rm -f ${shots}/${tag}*`);
      if (made) adb('shell', `rmdir ${shots}`);
      scan();
    }
  } else console.log(`SKIP ${where} 9: the index's drive needs API 30`);
}
console.log(failed() ? `${failed()} failed` : 'all passed');
process.exit(failed() ? 1 : 0);
