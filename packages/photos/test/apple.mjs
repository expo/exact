// @exact/photos's conformance suite on Apple (LLP 1133.001 D7, LLP 1133.019
// stage 2): the fixture app built for an iOS simulator or macOS and driven
// by the agent with the implementation bound (`--bind photos`), so PhotoKit
// answers and not the agent's library. It never reads this Mac's own
// library:
//
// - iOS: the simulator's library, which `--fill` fills with the package's
//   six fixture photos (`xcrun simctl addmedia`; `--fill <n>` adds n, the
//   six over and over, each copy's bytes made distinct after the JPEG's end,
//   so many share a creation date), and its permissions set by `xcrun
//   simctl privacy`, the host's own means (§11). The suite reads, draws,
//   pages through every photo, copies and adds there, and watches the change
//   observer.
// - macOS: only what holds with no library grant, since only a person can
//   grant one here and the library would be this Mac's: the status, its
//   Apple words, and managePhotoSelection's refusal. Nothing that reads or
//   adds is pressed. Then, under the agent, the `screenshot` filter over the
//   agent's library, whose one screenshot is agent-2.
//
// The limited picker and the system's prompt are a person's (checklist.md):
// `simctl privacy` grants full access only, and the agent cannot answer
// the system's alert. A photo added from outside while the app runs is not
// driven: `simctl addmedia` does not return while the driven app is
// running, so the change observer is shown by the app's own add.
//
// usage: bun packages/photos/test/apple.mjs [ios] [macos] [--sim <udid>] [--fill [<n>]]
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, readdirSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { loadedDylibs } from '../../../host/apple/link.mjs';
import { checker, makeFixture, pkg, root, run, texts, until } from './fixture.mjs';
import { photoKitChanges } from './photokit-changes.mjs';

const argv = process.argv.slice(2), sim = argv.includes('--sim') ? argv[argv.indexOf('--sim') + 1] : process.env.EXACT_SIM;
const hosts = argv.filter((a, i) => ['macos', 'ios'].includes(a) && argv[i - 1] !== '--sim');
const fill = argv.includes('--fill') ? Number(/^\d+$/.test(argv[argv.indexOf('--fill') + 1] ?? '') ? argv[argv.indexOf('--fill') + 1] : 6) : 0;
const { app, id, dir } = makeFixture(), name = app.split('/').pop();
Object.assign(process.env, { EXACT_APP_DIR: app, EXACT: root, HERMES_LEAN_SYS_OFFLINE: '1', EXACT_IDENTITY: process.env.EXACT_IDENTITY ?? '-' });
if (sim) process.env.EXACT_SIM = sim;
const { open } = await import(resolve(root, 'scripts/agent.mjs'));
const { check, failed } = checker();
const sha = (file) => createHash('sha256').update(readFileSync(file)).digest('hex');
const fixtures = [1, 2, 3, 4, 5, 6].map((n) => join(pkg, 'fixture', `agent-${n}.jpg`));
const privacy = (verb, service) => run('xcrun', ['simctl', 'privacy', sim, verb, service, id], root);
/** The newest file `app:/tmp/<path>` names in the simulator's container: the agent's session keeps `app:/tmp` as `temporary/`. */
const containerFile = (path) => {
  const data = run('xcrun', ['simctl', 'get_app_container', sim, id, 'data'], root).trim();
  return readdirSync(data, { recursive: true }).map((f) => join(data, f)).filter((f) => f.endsWith(`/temporary/${path}`) && existsSync(f))
    .sort((a, b) => statSync(b).mtimeMs - statSync(a).mtimeMs)[0];
};

for (const host of hosts.length ? hosts : ['ios', 'macos']) {
  const where = host === 'ios' ? 'ios-simulator' : 'macos';
  if (host === 'ios' && !sim) throw new Error('ios: name a simulator you booted (--sim <udid>); the suite changes its library and its privacy settings');
  if (host === 'ios' && fill) {
    const copies = join(dir, 'fill');
    mkdirSync(copies, { recursive: true });
    const files = Array.from({ length: fill }, (_, i) => {
      if (i < fixtures.length) return fixtures[i];
      const file = join(copies, `copy-${i}.jpg`);
      writeFileSync(file, Buffer.concat([readFileSync(fixtures[i % fixtures.length]), Buffer.from(`copy ${i}`)]));
      return file;
    });
    for (let i = 0; i < files.length; i += 100) run('xcrun', ['simctl', 'addmedia', sim, ...files.slice(i, i + 100)], root);
  }
  run('bun', [resolve(root, 'host/apple/build.mjs'), ...(host === 'ios' ? ['--ios', '--sim', sim] : ['--bundle']), `${name}-apple`], root);
  // On macOS the drive runs the binary inside the `.app`, so PhotoKit's state is the fixture's own bundle's.
  if (host === 'macos') {
    const bundle = readdirSync(join(app, 'target'), { recursive: true }).find((f) => /macos/.test(f) && f.endsWith('.app/Contents/Info.plist'));
    if (!bundle) throw new Error('no macOS .app under target/ (build.mjs --bundle)');
    const contents = join(app, 'target', bundle, '..'), executable = JSON.parse(run('plutil', ['-convert', 'json', '-o', '-', join(contents, 'Info.plist')])).CFBundleExecutable;
    process.env.EXACT_MAC_BIN = join(contents, 'MacOS', executable);
  }
  const plistAt = readdirSync(join(app, 'target'), { recursive: true }).find((f) => host === 'ios' ? /ios-sim/.test(f) && f.endsWith('.app/Info.plist') : /macos/.test(f) && f.endsWith('-Info.plist'));
  const plist = plistAt ? JSON.parse(run('plutil', ['-convert', 'json', '-o', '-', join(app, 'target', plistAt)])) : {};
  check(where, 'Info.plist carries the read key with its purpose', plist.NSPhotoLibraryUsageDescription === 'The conformance suite reads the library.', JSON.stringify(plist.NSPhotoLibraryUsageDescription));
  if (host === 'ios') {
    check(where, 'and the add key, and suppresses the limited-access alert', plist.NSPhotoLibraryAddUsageDescription === 'The conformance suite adds a photo.' && plist.PHPhotoLibraryPreventAutomaticLimitedAccessAlert === true, JSON.stringify(plist));
    // A package's Swift is a library of its own, loaded at the package's first use (LLP 1067.000.001).
    const library = join(dirname(join(app, 'target', plistAt)), 'Frameworks/libExactPkg_photos.dylib');
    check(where, 'the package\'s library links PhotosUI, which supplies the limited-picker category',
      existsSync(library) && loadedDylibs(library).some(name => name.endsWith('/PhotosUI.framework/PhotosUI')), library);
  }

  if (host === 'macos') {
    const s = await open({ host, app: name, bind: 'photos' });
    try {
      check(where, 'the extension is bound, not substituted', (await s.state()).extensions?.photos?.bound === true, JSON.stringify((await s.state()).extensions?.photos));
      const status = await until(s, 'status', (t) => /^true (prompt|granted|denied)/.test(t ?? ''));
      check(where, 'photoLibraryStatus answers from PhotoKit, before any grant', status === 'true prompt none prompt', status);
      check(where, 'in Apple\'s words, both levels undetermined', (await texts(s)).apple === 'notDetermined notDetermined', (await texts(s)).apple);
      await s.tap('manage');
      const managed = await until(s, 'manage-error', (t) => !!t);
      check(where, 'managePhotoSelection is iOS\'s: NotSupportedError', managed === 'NotSupportedError', managed);
      check(where, 'nothing was asked', !JSON.stringify(await s.logs()).includes('asks '), 'the journal says the module asked');
    } finally {
      await s.close();
    }
    const ops = ['tap read-shots', 'clock settle', 'tree', 'tap read-shots-2', 'clock settle', 'tree', 'tap read-all', 'clock settle', 'tree'];
    const r = spawnSync('bun', [resolve(root, 'scripts/agent.mjs'), 'macos', '--json', '--app', name, '--reset-storage', '--permission', 'photos=granted', ...ops], { cwd: root, encoding: 'utf8', maxBuffer: 1 << 26 });
    const trees = (r.stdout ?? '').split('\n').filter((l) => l.startsWith('{')).map((l) => JSON.parse(l)).filter((t) => Array.isArray(t.nodes));
    const [shots, shots2, all] = await Promise.all(trees.map((t) => texts({ tree: async () => t })));
    check('macos agent', 'the screenshot filter answers agent-2 alone, marked photoScreenshot', shots?.shots === '1 1 1 true true agent-2', shots?.shots ?? `${r.stdout.slice(-1500)}\n${r.stderr.slice(-1500)}`);
    check('macos agent', 'and at 2 a page, still one page', shots2?.shots === '1 1 1 true true agent-2', shots2?.shots);
    check('macos agent', 'unfiltered, all six', all?.all === '1 6 6 true', all?.all);
    continue;
  }

  // Undetermined: the status, and Manage asks nothing.
  privacy('reset', 'photos');
  let s = await open({ host, app: name, bind: 'photos' });
  try {
    const status = await until(s, 'status', (t) => /^true /.test(t ?? ''));
    check(where, 'undetermined: prompt, no access, add prompt', status === 'true prompt none prompt', status);
    check(where, 'in Apple\'s words', (await texts(s)).apple === 'notDetermined notDetermined', (await texts(s)).apple);
    await s.tap('manage');
    const managed = await until(s, 'managed', (t) => t !== 'none');
    check(where, 'managePhotoSelection without limited access answers the status, asking nothing', managed === 'prompt none', managed);
  } finally {
    await s.close();
  }

  // Add-only granted: the add state alone.
  privacy('grant', 'photos-add');
  s = await open({ host, app: name, bind: 'photos' });
  try {
    const status = await until(s, 'status', (t) => /^true /.test(t ?? ''));
    check(where, 'add-only: the read prompt, the add granted', status === 'true prompt none granted', status);
    check(where, 'in Apple\'s words', (await texts(s)).apple === 'notDetermined authorized', (await texts(s)).apple);
  } finally {
    await s.close();
  }

  // Full access: read, draw, copy, add, and the change observer.
  privacy('grant', 'photos');
  let copy;
  s = await open({ host, app: name, bind: 'photos' });
  try {
    const status = await until(s, 'status', (t) => t === 'true granted full granted');
    check(where, 'granted: full access, adding included', status === 'true granted full granted', status);
    await s.tap('read');
    const count = Number(await until(s, 'count', (t) => Number(t) >= 6));
    check(where, 'photoAssets lists the library, the fixtures among it', count >= 6, count);
    const first = await texts(s);
    // The newest, by creation date: a fixture, since `--fill` added them dated 2026.
    const [size, isPicture] = (first['first-size'] ?? '').split(' ');
    check(where, 'newest first, sized, its picture photos:<id>', /^\d+x\d+$/.test(size) && isPicture === 'true', `${first.first} ${first['first-size']}`);
    // Every page, 500 and then 2 at a time: each photo once, newest first.
    for (const [button, size] of [['read-all', 500], ['read-all-2', 2]]) {
      await s.tap(button);
      const all = await until(s, 'all', (t) => { const [p, n] = (t ?? '').split(' ').map(Number); return p > 0 && n >= count && p === Math.ceil(n / size); }, 120000);
      const [pages, total, distinct, ordered] = (all ?? '').split(' ');
      check(where, `photoAssets pages through the library ${size} at a time, each photo once, newest first`,
        Number(total) >= count && Number(distinct) === Number(total) && Number(pages) === Math.max(1, Math.ceil(Number(total) / size)) && ordered === 'true', all);
      if (fill > 500 && size === 500) check(where, `more than one page of 500 with ${fill} added`, Number(pages) > 1 && Number(total) >= fill, all);
    }
    // `simctl addmedia` never marks a file a screenshot (LLP 1133.019.000 §2) and the simulator's own screenshot needs
    // Simulator's window, so the filtered list holds no imported photo, and each photo it holds is marked.
    const [, total] = (await texts(s)).all.split(' ').map(Number);
    await s.tap('read-shots');
    const shots = await until(s, 'shots', (t) => Number(t?.split(' ')[0]) > 0, 60000);
    const [shotPages, shotCount, shotDistinct, shotsOrdered, marked] = (shots ?? '').split(' ');
    check(where, 'photoAssets filtered to screenshots leaves out every imported photo', Number(shotPages) >= 1 && Number(shotCount) <= total - fill
      && Number(shotDistinct) === Number(shotCount) && shotsOrdered === 'true' && marked === 'true', `${shots} of ${total}, ${fill} imported`);
    const shown = Math.min(6, count);
    const pictures = await until(s, 'pictures', (t) => Number(t?.split(' ')[0]) >= shown, 20000);
    check(where, `the first ${shown} pictures load, none fails`, Number(pictures?.split(' ')[0]) >= shown && pictures?.split(' ')[1] === '0', pictures);
    const counted = (await s.state()).extensions?.photos?.pictures;
    check(where, 'each admitted by the runner, none refused', counted?.admitted >= shown && !counted?.refused, JSON.stringify(counted));
    await s.screenshot(join(dir, 'pictures.png'));
    await s.tap('copy');
    const copied = await until(s, 'copied', (t) => t !== 'none', 20000);
    // Each copy has a name of its own, so two never share a file.
    copy = /^app:\/tmp\/(photos\/[0-9A-F-]{36}\.jpg) /.exec(copied ?? '')?.[1];
    check(where, 'copyPhotoAsset writes the newest to app:/tmp/photos/<uuid>.jpg, its size the listed one', !!copy && copied === `app:/tmp/${copy} image/jpeg true ${size}`, `${copied} ${(await texts(s))['copy-error']}`);
    await s.tap('add');
    const added = await until(s, 'added', (t) => t !== 'none', 20000);
    check(where, 'addToPhotoLibrary adds it and answers its id', !!added && added !== 'none' && added !== 'no id', `${added} ${(await texts(s))['add-error']}`);
    const grown = Number(await until(s, 'count', (t) => Number(t) > count, 20000));
    check(where, 'the change observer announces the list: one more', grown === count + 1, `${count} then ${grown}`);
    await s.tap('manage');
    const managed = await until(s, 'managed', (t) => t !== 'none');
    check(where, 'managePhotoSelection under full access answers the status at once', managed === 'granted full', managed);
  } finally {
    await s.close();
  }
  // The container is read with the app closed: `simctl` waits on a driven app.
  const file = copy && containerFile(copy), known = [...fixtures, join(root, 'scripts/fixtures/picker/oriented-gps.jpg')].map(sha);
  check(where, "the copy is the photo's own file, byte for byte", !!file && known.includes(sha(file)), file ?? `no tmp/${copy ?? 'photos/<uuid>.jpg'} in the container`);
  await photoKitChanges({ sim, id, name, open, check, dir, fixtures });

  // Denied: nothing is read, and no picture is asked.
  privacy('revoke', 'photos');
  privacy('revoke', 'photos-add');
  s = await open({ host, app: name, bind: 'photos' });
  try {
    const status = await until(s, 'status', (t) => /^true /.test(t ?? ''));
    check(where, 'revoked: denied, add denied', status === 'true denied none denied', status);
    await s.tap('read');
    await s.clock('+1000 real');
    const t = await texts(s);
    check(where, 'the read fails and no picture is drawn', t.count === '0' && t.pictures === '0 0 ', `${t.count} ${t.pictures}`);
    await s.tap('copy');
    const refused = await until(s, 'copy-error', (e) => !!e);
    check(where, 'a copy fails NotAllowedError, asking nothing', refused === 'NotAllowedError', refused);
    await s.tap('watch');
    const unread = await until(s, 'changes-error', (e) => !!e);
    check(where, 'photoChanges fails NotAllowedError', unread === 'NotAllowedError', unread);
  } finally {
    await s.close();
  }
  privacy('reset', 'photos');
  run('xcrun', ['simctl', 'uninstall', sim, id], root);
}
rmSync(dir, { recursive: true, force: true });
console.log(failed() ? `${failed()} failed` : 'all passed');
process.exit(failed() ? 1 : 0);
