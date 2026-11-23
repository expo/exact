// The fixture's photo index (LLP 1133.019.000 D1, piece 6): `apps/photos-fixture`'s Import, the importer of
// expo/exact#714, driven against the platform's own library with the implementation bound. After each sync the
// index is compared with the same view listed whole (`photoListing`). `iosImporter` is the iOS simulator's drive,
// which `apple.mjs importer` runs; android.mjs's drive 9 is Android's, with the helpers here.
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { pkg, root, run, texts, until } from './fixture.mjs';
import { photoKitEdit } from './photokit-changes.mjs';

/** What the Importer shows: the index after its last sync, the listing, and whether the two hold the same ids. */
export const reading = (t) => {
  const [count, digest, how, pages, more, syncs, error] = (t.index ?? '').split('|');
  const [listed, listedDigest, listedError] = (t.listing ?? '').split('|');
  return {
    count: Number(count), digest, how, pages: Number(pages), more: more === 'true', syncs: Number(syncs), error,
    listed: Number(listed), listedError, inStep: t['in-step'] === 'in step' && digest === listedDigest,
    answered: !!how && (!!listedDigest || !!listedError),
  };
};

/** The reading once both have answered and `want` holds; past the wait, the last seen, marked `stale`. */
export async function settled(s, want = (r) => r.inStep, ms = 180000) {
  const end = Date.now() + ms;
  let r;
  do {
    await s.clock('+250 real');
    r = reading(await texts(s));
    if (r.answered && want(r)) return r;
  } while (Date.now() < end);
  return { ...r, stale: true };
}

/** Import, its options (the screenshots alone, pages of two, a budget of two pages), then Sync. */
export async function imported(s, { shots = false, two = false, budget = false } = {}, want, ms) {
  await s.tap('import');
  if (shots) await s.tap('import-screenshots');
  if (two) await s.tap('import-pages-of-two');
  if (budget) await s.tap('import-two-pages');
  await s.tap('sync');
  return settled(s, want, ms);
}

const RUNNER = 'dev.exact.reach.uitests.xctrunner';

/** The drive on iOS simulator `sim`, erased and booted with no UI test run since (`simctl addmedia` can wait without
 * end after one). It adds 520 photos, which it leaves, and builds and installs the fixture; the photos the UI test's
 * runner imports are deleted again, and the Photos permissions it granted are reset. */
export async function iosImporter({ sim, open, check }) {
  const where = 'ios-simulator importer', id = 'com.exact.photosfixture', app = 'photos-fixture';
  const say = (what, ok, detail) => check(where, what, ok, typeof detail === 'string' ? detail : JSON.stringify(detail));
  const dir = mkdtempSync(join(tmpdir(), 'exact-photos-importer-'));
  const fixtures = [1, 2, 3, 4, 5, 6].map((n) => join(pkg, 'fixture', `agent-${n}.jpg`));
  let copies = 0;
  /** `n` photo files, each with bytes of its own after the JPEG's end. */
  const copy = (n) => Array.from({ length: n }, () => {
    const file = join(dir, `import-${copies}.jpg`);
    writeFileSync(file, Buffer.concat([readFileSync(fixtures[copies % fixtures.length]), Buffer.from(`import ${copies++}`)]));
    return file;
  });
  const files = copy(520);
  for (let i = 0; i < files.length; i += 100) run('xcrun', ['simctl', 'addmedia', sim, ...files.slice(i, i + 100)], root);
  run('bun', [resolve(root, 'host/apple/build.mjs'), '--ios', '--sim', sim, 'photos-fixture-apple'], root);
  run('xcrun', ['simctl', 'privacy', sim, 'grant', 'photos', id], root);
  run('xcrun', ['simctl', 'privacy', sim, 'grant', 'photos', RUNNER], root);
  const edit = photoKitEdit({ sim, id, dir });
  const owned = [];
  const store = `importer-${process.pid}`;
  const launch = async (fn, storage = store) => {
    const s = await open({ host: 'ios', app, bind: 'photos', storage });
    try {
      return await fn(s);
    } finally {
      await s.close();
    }
  };
  try {
    // No checkpoint: one taken, then the list and one walk from the checkpoint.
    const first = await launch((s) => imported(s));
    say('the first import lists the library into the index, in step with it',
      first.inStep && first.how === 'replaced' && first.count >= 520 && first.pages === 2 + Math.ceil(first.count / 500) && !first.error, first);
    const again = await launch((s) => imported(s));
    say('relaunched: one walk from the checkpoint, the index unchanged', again.inStep && again.how === 'walked' && again.count === first.count && !again.more, again);

    // Changes while the app is closed: nine imports, one favourited, two deleted.
    const [made] = edit([{ edit: 'import', files: copy(9) }]);
    edit([{ edit: 'favorite', ids: [made[0]] }, { edit: 'delete', ids: made.slice(1, 3) }]);
    owned.push(made[0], ...made.slice(3));
    const stopped = await launch((s) => imported(s, { two: true, budget: true }, (r) => r.how === 'walked'));
    say('two a page and stopped after two pages: a walk committed that far, more to come',
      stopped.how === 'walked' && stopped.pages === 2 && stopped.more && !stopped.inStep && !stopped.error, stopped);
    const rest = await launch((s) => imported(s, { two: true }));
    say('relaunched: the walk goes on from its last page committed, to the library',
      rest.inStep && rest.how === 'walked' && !rest.more && rest.count === first.count + 7, rest);

    // Triggers that overlap: sixty imports walked two a page, and the app's own add during the walk, whose
    // announcement asks the index again once the walk answers.
    owned.push(...edit([{ edit: 'import', files: copy(60) }])[0]);
    const overlap = await launch(async (s) => {
      await s.tap('import');
      await s.tap('import-pages-of-two');
      await until(s, 'photo-1', (t) => t !== undefined, 60000);
      await s.tap('sync');
      await s.tap('photo-1');
      await until(s, 'copied', (t) => !!t && t !== 'No copy');
      await s.tap('add');
      const r = await settled(s, (r) => r.inStep && r.count === rest.count + 61);
      const lines = JSON.stringify(await s.logs());
      const added = (await texts(s)).added;
      if (/\/L0\/\d+$/.test(added ?? '')) owned.push(added);
      return { ...r, waited: /changed photos\.photo\w+: request \d+ \([^)]*index[^)]*\) lands first/.test(lines), added };
    });
    say('an add during the walk: the index syncs again and holds it, in step',
      !overlap.stale && overlap.inStep && overlap.count === rest.count + 61 && overlap.syncs >= 2 && !!overlap.added, overlap);
    console.log(`NOTE ${where}: the add's announcement ${overlap.waited ? 'came while the walk was in flight' : 'came after the walk answered'}`);

    // A relaunch mid-list: a new store has no checkpoint, so the sync replaces; stopped after the checkpoint and
    // a page of the list, it is discarded, and the next launch lists the library whole.
    const fresh = `${store}-fresh`;
    const cut = await launch((s) => imported(s, { two: true, budget: true }, (r) => !!r.how), fresh);
    say('stopped mid-list: the replacement discarded, the index still empty', cut.how === 'interrupted' && cut.count === 0 && cut.pages === 2, cut);
    const whole = await launch((s) => imported(s), fresh);
    say('relaunched: listed whole', whole.inStep && whole.how === 'replaced' && whole.count === overlap.count, whole);

    // The screenshots alone, an index of its own; a simulator has none PhotoKit marks.
    const shots = await launch((s) => imported(s, { shots: true }));
    say('the screenshots: an index of its own, in step with their list', shots.inStep && shots.how === 'replaced' && shots.count === shots.listed, shots);

    // Denied: the read fails, and the index committed stands.
    run('xcrun', ['simctl', 'privacy', sim, 'revoke', 'photos', id], root);
    const denied = await launch((s) => imported(s, {}, (r) => !!r.error));
    say('denied: NotAllowedError, the index as it was committed', denied.how === 'failed' && denied.error === 'NotAllowedError'
      && denied.count === overlap.count && denied.listedError === 'NotAllowedError', denied);
  } finally {
    run('xcrun', ['simctl', 'privacy', sim, 'grant', 'photos', id], root);
    try {
      if (owned.length) say('the photos the drive imported are deleted', edit([{ edit: 'delete', ids: owned }])[0].length === owned.length, owned.length);
    } catch (e) {
      say('the photos the drive imported are deleted', false, e.message.slice(0, 600));
    }
    for (const app of [id, RUNNER]) run('xcrun', ['simctl', 'privacy', sim, 'reset', 'photos', app], root);
    rmSync(dir, { recursive: true, force: true });
  }
}
