// `photoChanges` (LLP 1133.019.000 piece 3), driven through each fixture's Changes panel.
//
// - macOS and Android, under the agent: the journal of the agent's library from a token. A checkpoint with nothing
//   since; a deletion, an add and a restoration; a walk of one a page whose end stays fixed while the library moves;
//   the screenshots alone, where a photo that is not one is a removal; malformed, wrong-backend, wrong-view,
//   wrong-epoch, rescan-only and impossible tokens; a narrowed access, a new selection and a denial; a token from
//   another launch, whose store is new; and a denial and a grant again, which change nothing in the journal.
// - Android with the implementation bound (`--bind photos`, the read granted by `pm grant`): every view is
//   rescan-only, so a token answers `expired` with a fresh one (D3).
// - The web: absent, `NotSupportedError`.
//
// usage: bun packages/photos/test/changes.mjs <macos|android|web>   (android: ANDROID_SERIAL names the emulator)
import { spawnSync } from 'node:child_process';
import { rmSync } from 'node:fs';
import { resolve } from 'node:path';
import { checker, makeFixture, root, run, texts, until } from './fixture.mjs';
import { androidDevice, androidSdk } from '../../../scripts/agent-android.mjs';

const host = process.argv[2] ?? 'macos';
const { check, failed } = checker();
const decode = (t) => JSON.parse(Buffer.from(t, 'base64').toString());
const encode = (o) => Buffer.from(JSON.stringify(o)).toString('base64');

let app = 'photos-fixture', fixture, t0;
if (host !== 'android') {
  fixture = makeFixture();
  app = fixture.app.split('/').pop();
  Object.assign(process.env, { EXACT_APP_DIR: fixture.app, EXACT: root, HERMES_LEAN_SYS_OFFLINE: '1', EXACT_IDENTITY: process.env.EXACT_IDENTITY ?? '-' });
  if (host === 'macos') run('bun', [resolve(root, 'host/apple/build.mjs'), `${app}-apple`], root);
  else run('bun', ['exact.mjs', 'web-build', '--render', 'none'], fixture.app);
} else {
  run('bun', ['scripts/agent-android.mjs', 'build', app, '--apk'], root);
}
const { open } = await import(resolve(root, 'scripts/agent.mjs'));
const launch = (more = {}) => open({ host, app, ...(host === 'android' ? { reader: true } : {}), ...more });

/** The panel once settled, from one tree: `added|updated|deleted|more expired`, its error and its token; when `was`
 * (a reading, or a summary) is given, once the reading differs from it. */
const panel = async (s, was) => {
  await s.clock('settle');
  const end = Date.now() + 8000, read = (t) => ({ changes: t.changes, error: t['changes-error'], token: t.token });
  const same = (r) => (typeof was === 'string' ? r.changes === was : JSON.stringify(r) === JSON.stringify(was));
  let r = read(await texts(s));
  while (was !== undefined && same(r) && Date.now() < end) {
    await s.clock('+250 real');
    r = read(await texts(s));
  }
  return r;
};
const since = async (s, token) => {
  await s.type('since', token);
  return panel(s);
};
/** The read granted, and a read the launch held on its ticket let through. */
const grant = async (s) => {
  await s.prefer({ 'permission.photos': 'granted' });
  await s.tap('@recent', { choice: 'allow' }).catch(() => {});
  await s.clock('settle');
};
const deleted = (s, ids) => s.prefer({ extension: `photos.deleted=${JSON.stringify(ids)}` });
/** From panel `p`, every page of its walk, Advance taking each next: the lists joined across pages with the last
 * page's flags, and the last reading. A page that fails or repeats ends it. */
const drain = async (s, p) => {
  const lists = [[], [], []], seen = new Set();
  for (;;) {
    const parts = p.changes.split('|');
    parts.slice(0, 3).forEach((l, i) => lists[i].push(...l.split(',').filter(Boolean)));
    if (p.error || seen.has(p.token) || parts[3] !== 'true false') return { ...p, changes: `${lists.map((l) => l.join(',')).join('|')}|${p.error ? 'error' : parts[3]}` };
    seen.add(p.token);
    await s.tap('advance');
    p = await panel(s, p);
  }
};
const raw = (text) => Buffer.from(text).toString('base64');

if (host === 'web') {
  const s = await launch();
  try {
    await s.tap('watch');
    await s.clock('settle');
    const t = await texts(s);
    check('web', 'photoChanges is absent: NotSupportedError', t['changes-error'] === 'NotSupportedError', JSON.stringify(t['changes-error']));
  } finally { await s.close(); }
} else {
  let old;
  const s = await launch();
  try {
    await grant(s);
    await s.tap('watch');
    let p = await panel(s);
    t0 = p.token;
    check(host, 'no token: a checkpoint, nothing changed', p.changes === '|||false false' && decode(t0).backend === 'agent', `${p.changes} ${p.error}`);
    p = await since(s, t0);
    check(host, 'the checkpoint again: nothing since', p.changes === '|||false false' && p.token === t0, p.changes);

    await deleted(s, ['agent-4']);
    p = await panel(s, '|||false false');
    check(host, 'a deletion: its id among the removals', p.changes === '||agent-4|false false', p.changes);
    const t1 = p.token;
    // An add: the newest photo copied and added back, agent-7.
    if (host === 'android') { await s.tap('photo-1'); await s.clock('settle'); } else { await s.tap('read'); await s.clock('settle'); await s.tap('copy'); await s.clock('settle'); }
    await s.tap('add');
    await s.clock('settle');
    await deleted(s, []);
    p = await panel(s, p);
    // From the first checkpoint agent-4 was there before: deleted and back is an update.
    check(host, 'since the first checkpoint: the add an insert, agent-4 an update', p.changes === 'agent-7|agent-4||false false', p.changes);
    p = await since(s, t1);
    check(host, 'since the deletion: the add and the restoration, each an insert', p.changes === 'agent-4,agent-7|||false false', p.changes);
    const t2 = p.token;

    // A walk of one a page: its end is fixed on the first page, and each page reads the library as it is then.
    await s.type('since', t2);
    await s.tap('page-of-one');
    await deleted(s, ['agent-3', 'agent-5', 'agent-6']);
    p = await panel(s, '|||false false');
    check(host, 'one a page: the first of three', p.changes === '||agent-3|true false', p.changes);
    const continuation = p.token;
    await s.tap('advance');
    p = await panel(s, p);
    check(host, 'the second', p.changes === '||agent-5|true false', p.changes);
    await deleted(s, ['agent-3', 'agent-6']);
    p = await panel(s, p);
    check(host, 'the second again, restored after the walk began: an upsert now', p.changes === '|agent-5||true false', p.changes);
    await s.tap('advance');
    p = await panel(s, p);
    check(host, 'the third, the walk complete', p.changes === '||agent-6|false false', p.changes);
    await s.tap('advance');
    p = await panel(s, p);
    check(host, 'the restoration after the end, in the next walk', p.changes === 'agent-5|||false false', p.changes);

    const base = decode(t2);
    p = await since(s, 'not a token');
    check(host, 'a malformed token: SyntaxError', p.error === 'SyntaxError', `${p.error} ${p.changes}`);
    p = await since(s, encode({ ...base, backend: host === 'android' ? 'mediastore' : 'photokit' }));
    check(host, "another backend's token: SyntaxError", p.error === 'SyntaxError', `${p.error} ${p.changes}`);
    p = await since(s, encode({ ...base, at: String(1e6) }));
    check(host, 'a position past the journal: SyntaxError', p.error === 'SyntaxError', `${p.error} ${p.changes}`);
    const json = Buffer.from(t2, 'base64').toString();
    for (const [name, token] of [['a continuation after no photo its window changed', encode({ ...decode(continuation), after: 'zzzz' })],
      ["a continuation after its window's last photo", encode({ ...decode(continuation), after: 'agent-6' })],
      ['a rescan-only token, which the agent never answers', encode((({ at, ...rest }) => rest)(base))],
      ['a token naming no store', encode((({ epoch, ...rest }) => rest)(base))], ['a version of true', encode({ ...base, v: true })],
      ['a version of 1.0', raw(json.replace(/"v":1/, '"v":1.0'))], ['a trailing comma', raw(json.replace(/}$/, ',}'))],
      ['an unknown access', encode({ ...base, access: 'some' })], ['an unknown subtype', encode({ ...base, subtypes: ['selfie'] })],
      ['a subtype twice', encode({ ...base, subtypes: ['screenshot', 'screenshot'] })]]) {
      p = await since(s, token);
      check(host, `${name}: SyntaxError`, p.error === 'SyntaxError', `${p.error} ${p.changes}`);
    }
    for (const [name, token] of [['another view', { ...base, subtypes: ['screenshot'] }], ['another access', { ...base, access: 'limited' }],
      ['another store', { ...base, epoch: 'gone' }]]) {
      p = await since(s, encode(token));
      const fresh = p.token && decode(p.token);
      check(host, `${name}: expired, empty, a fresh checkpoint`, p.changes === '|||false true' && !p.error && fresh?.at === String(Number(fresh.at)) && fresh.epoch === base.epoch,
        `${p.changes} ${p.error} ${JSON.stringify(fresh)}`);
    }

    // The screenshots alone: agent-2 is the agent's one screenshot, so agent-4 is a removal either way.
    await s.tap('only-screenshots');
    p = await panel(s, p);
    const shots = p.token && decode(p.token);
    check(host, 'the screenshots alone: the full view expires, the word given twice kept once', p.changes === '|||false true' && JSON.stringify(shots?.subtypes) === '["screenshot"]', `${p.changes} ${JSON.stringify(shots)}`);
    await s.tap('advance');
    p = await panel(s, p);
    await deleted(s, ['agent-2', 'agent-3', 'agent-4', 'agent-6']);
    p = await drain(s, await panel(s, p));
    check(host, 'screenshots: agent-2 and agent-4 deleted, both removals', p.changes === '||agent-2,agent-4|false false', p.changes);
    await s.tap('advance');
    p = await panel(s, p);
    await deleted(s, ['agent-3', 'agent-6']);
    p = await drain(s, await panel(s, p));
    check(host, 'screenshots: both restored, agent-2 an insert and agent-4, no screenshot, a removal', p.changes === 'agent-2||agent-4|false false', p.changes);

    p = await since(s, t2);
    old = p.token;
    await s.prefer({ extension: 'photos.photoLibraryStatus={"access":"limited"}' });
    p = await panel(s, p);
    check(host, 'access narrowed to limited: the full view expires', p.changes === '|||false true' && decode(p.token).access === 'limited', `${p.changes} ${p.token && decode(p.token).access}`);
    const limited = p.token;
    await s.prefer({ extension: 'photos.selection=["agent-2"]' });
    p = await since(s, limited);
    check(host, 'a new selection: a new store, the token expires', p.changes === '|||false true' && decode(p.token).epoch !== decode(limited).epoch, p.changes);
    await s.prefer({ 'permission.photos': 'denied' });
    p = await panel(s, p);
    check(host, 'denied: NotAllowedError', p.error === 'NotAllowedError', `${p.error} ${p.changes}`);
  } finally { await s.close(); }

  // Another launch is another store.
  const again = await launch();
  try {
    await grant(again);
    await again.tap('watch');
    // The first launch's first checkpoint: the same view, a position this store has too.
    let p = await since(again, t0);
    check(host, "a token from another launch: expired", p.changes === '|||false true' && decode(p.token).epoch !== decode(t0).epoch, `${p.changes} ${p.error}`);
    await deleted(again, ['agent-4']);
    p = await panel(again, p);
    await again.tap('advance');
    p = await panel(again, p);
    const kept = p.token;
    await again.prefer({ 'permission.photos': 'denied' });
    await again.clock('settle');
    await grant(again);
    p = await since(again, kept);
    check(host, 'a denial and a grant again: nothing changed since', p.changes === '|||false false' && decode(p.token).at === decode(kept).at, `${p.changes} ${p.error}`);
  } finally { await again.close(); }

  if (host === 'android') {
    const adb = (...args) => spawnSync(resolve(androidSdk(), 'platform-tools/adb'), ['-s', androidDevice(), ...args], { encoding: 'utf8' });
    const api = Number(adb('shell', 'getprop', 'ro.build.version.sdk').stdout.trim()), where = `android ${api} mediastore`;
    adb('shell', 'pm', 'grant', 'com.exact.photosfixture', `android.permission.${api >= 33 ? 'READ_MEDIA_IMAGES' : 'READ_EXTERNAL_STORAGE'}`);
    const bound = await launch({ bind: 'photos' });
    try {
      await bound.tap('@recent', { choice: 'allow' }).catch(() => {});
      await bound.clock('settle');
      await bound.tap('watch');
      let p = await panel(bound);
      const start = p.token && decode(p.token);
      check(where, 'no token: a rescan-only starting point', p.changes === '|||false false' && start?.backend === 'mediastore' && start.access === 'full' && !('at' in start), `${p.changes} ${p.error} ${JSON.stringify(start)}`);
      p = await since(bound, p.token);
      check(where, 'any token: expired, empty, a fresh starting point', p.changes === '|||false true' && !p.error && decode(p.token).backend === 'mediastore', `${p.changes} ${p.error}`);
      p = await since(bound, old);
      check(where, "the agent's token: SyntaxError", p.error === 'SyntaxError', `${p.error} ${p.changes}`);
      p = await since(bound, encode({ ...start, at: '3' }));
      check(where, 'a position, which MediaStore never answers: SyntaxError', p.error === 'SyntaxError', `${p.error} ${p.changes}`);
    } finally { await bound.close(); }
  }
}
if (fixture) rmSync(fixture.dir, { recursive: true, force: true });
console.log(failed() ? `${failed()} failed` : 'all passed');
process.exit(failed() ? 1 : 0);
