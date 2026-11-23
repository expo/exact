// `photoChanges` over PhotoKit's persistent history (LLP 1133.019.000 D2, piece 4), on an iOS simulator with full
// access granted, the fixture bound (`--bind photos`). `apple.mjs ios` runs it. Each library change is made with the
// fixture closed, through PhotoKit in a UI test's runner (PhotoChangesUITests.swift): imports, favourites, hiding
// and deletion, the system's confirmation answered. (`simctl addmedia` waits on a driven app, and once a UI test
// has run on the simulator it can wait without end.) Tokens are carried from one launch to the next, so each walk
// resumes from an archive unarchived in another process.
//
// The library updates photos of its own accord after an import, so a walk can hold updates the suite did not make:
// each check names the photos the suite imported, and edits only those. A hidden photo leaves every fetch a
// third-party app makes, `includeHiddenAssets` or not, so nothing can show it again here.
import { mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { pkg, root, run, texts, until } from './fixture.mjs';

const decode = (t) => JSON.parse(Buffer.from(t, 'base64').toString());
/** A token as the module writes one: keys sorted, as `decode` keeps them, and `/` escaped, as JSONSerialization writes it. */
const encode = (o) => Buffer.from(JSON.stringify(o).replaceAll('/', '\\/')).toString('base64');
const RUNNER = 'dev.exact.reach.uitests.xctrunner';

/** The panel's answer: its three lists, its flags, its error and its token. */
const reading = (t) => {
  const [added = '', updated = '', deleted = '', flags = ''] = (t.changes ?? '').split('|');
  const list = (l) => l.split(',').filter(Boolean);
  const [more, expired] = flags.split(' ').map((f) => f === 'true');
  return { added: list(added), updated: list(updated), deleted: list(deleted), more, expired, error: t['changes-error'], token: t.token, text: t.changes };
};
const answered = (r) => !!r.error || !!r.token;
/** Every candidate of a page, in the order the walk pages them. */
const ids = (r) => [...r.added, ...r.updated, ...r.deleted].sort();
/** One id a page, in order; a last page can be empty, where what was left of the window was deleted. */
const ascending = (pages) => {
  const all = pages.flatMap(ids), empty = pages.filter((p) => !ids(p).length);
  return pages.every((p) => ids(p).length <= 1) && all.every((id, i) => i === 0 || all[i - 1] < id) && (!empty.length || (empty.length === 1 && empty[0] === pages.at(-1)));
};
/** Each of `owned` once across `pages`' `list`, and in no other list. */
const once = (pages, owned, list) => owned.every((o) => pages.flatMap((p) => p[list]).filter((i) => i === o).length === 1
  && !pages.some((p) => ['added', 'updated', 'deleted'].some((l) => l !== list && p[l].includes(o))));
const done = (r) => !r.stale && !r.more && !r.expired && !r.error;

export async function photoKitChanges({ sim, id, name, open, check, dir, fixtures }) {
  const where = 'ios-simulator photokit';
  const say = (what, ok, detail) => check(where, what, ok, typeof detail === 'string' ? detail : JSON.stringify(detail));
  let copies = 0;
  /** `n` photo files, each with bytes of its own after the JPEG's end. */
  const copy = (n) => Array.from({ length: n }, () => {
    const file = join(dir, `change-${copies}.jpg`);
    writeFileSync(file, Buffer.concat([readFileSync(fixtures[copies % fixtures.length]), Buffer.from(`change ${copies++}`)]));
    return file;
  });
  /** PhotoKit edits made by the UI test's runner, which fails an edit that reaches fewer photos than it names: the
   * ids each reached or made. */
  const edit = (edits) => {
    const out = join(dir, 'ui');
    rmSync(out, { recursive: true, force: true });
    mkdirSync(out, { recursive: true });
    writeFileSync(join(out, 'edits.json'), JSON.stringify(edits));
    run('bun', [resolve(root, 'packages/quick-actions/test/apple-ui.mjs'), sim, out, 'testEdit'], root,
      { ...process.env, EXACT_UI_TEST_SOURCE: join(pkg, 'test/PhotoChangesUITests.swift'), EXACT_UI_TEST_APP: id });
    return JSON.parse(readFileSync(join(out, 'edited.json'), 'utf8'));
  };
  const owned = [];
  /** `n` photos imported: their ids, in the order the walk pages them. */
  const imports = (n) => {
    const [made] = edit([{ edit: 'import', files: copy(n) }]);
    owned.push(...made);
    say(`the runner imported ${n}`, made.length === n, made.length);
    return made.sort();
  };
  /** The panel once `want` holds; past the wait, the last seen, marked `stale`. */
  const panel = async (s, want = () => true, ms = 30000) => {
    const end = Date.now() + ms;
    let r;
    do {
      await s.clock('+250 real');
      r = reading(await texts(s));
      if (answered(r) && want(r)) return r;
    } while (Date.now() < end);
    return { ...r, stale: true };
  };
  /** A launch of the fixture, the Changes panel shown (one a page when `one`) and its checkpoint answered, so a
   * later answer is told from it. */
  const launch = async (fn, { one = false } = {}) => {
    const s = await open({ host: 'ios', app: name, bind: 'photos' });
    try {
      await s.tap('watch');
      if (one) await s.tap('page-of-one');
      await panel(s);
      return await fn(s);
    } finally {
      await s.close();
    }
  };
  /** The answer for `token` typed into Since, told from the answer before it. */
  const since = async (s, token, want = () => true) => {
    const before = reading(await texts(s));
    await s.type('since', token);
    return panel(s, (r) => (r.token !== before.token || r.text !== before.text) && want(r));
  };
  const advance = async (s, from, want = () => true) => {
    await s.tap('advance');
    return panel(s, (r) => r.token !== from.token && want(r));
  };
  /** From `r`, every page of its walk, Advance taking each next; it stops at a page that repeats a token or never
   * came, which `done` then refuses. */
  const drain = async (s, r) => {
    const pages = [r], seen = new Set([r.token]);
    while (r.more && !r.error && !r.stale) {
      pages.push(r = await advance(s, r));
      if (seen.has(r.token)) { r.stale = true; break; }
      seen.add(r.token);
    }
    return pages;
  };
  /** `at`, PhotoKit's archive, with `edit` made to its XML. */
  const forge = (at, edit) => {
    const file = join(dir, 'token.plist');
    writeFileSync(file, Buffer.from(at, 'base64'));
    run('plutil', ['-convert', 'xml1', file]);
    writeFileSync(file, edit(readFileSync(file, 'utf8')));
    run('plutil', ['-convert', 'binary1', file]);
    return readFileSync(file).toString('base64');
  };
  run('xcrun', ['simctl', 'privacy', sim, 'grant', 'photos', RUNNER], root);

  // A checkpoint, and nothing of the suite's since it.
  const t0 = await launch(async (s) => {
    const p = await panel(s);
    const t = p.token && decode(p.token);
    say('the token is the text the module writes', !!p.token && encode(t) === p.token, p.token);
    say("no token: a checkpoint of PhotoKit's, nothing changed", p.text === '|||false false' && t?.backend === 'photokit' && t.access === 'full' && !!t.at && !t.end,
      `${p.text} ${p.error} ${JSON.stringify(t)}`);
    return p.token;
  });

  // Three imports, read in one page, then one a page across a relaunch.
  const inserted = imports(3);
  const { t1, first } = await launch(async (s) => {
    const p = await since(s, t0, (r) => inserted.every((i) => r.added.includes(i)) || !!r.error);
    say('three imports since the checkpoint: three inserts, each once', once([p], inserted, 'added') && done(p), p);
    say('and a checkpoint past the first', !!p.token && decode(p.token).at !== decode(t0).at, p.token);
    await s.tap('page-of-one');
    const one = await panel(s, (r) => r.more || !!r.error);
    const two = await advance(s, one);
    const c = decode(two.token);
    say('one a page: a continuation holding the start, the end and the last id', c.at === decode(t0).at && !!c.end && c.after === ids(two)[0] && two.more, c);
    return { t1: p.token, first: [one, two] };
  });
  const t2 = await launch(async (s) => {
    const rest = await drain(s, await since(s, first[1].token));
    const pages = [...first, ...rest];
    say('after a relaunch the continuation finds its saved end: one id a page, in order, the walk complete, each import an insert once',
      ascending(pages) && done(pages.at(-1)) && once(pages, inserted, 'added'), pages.map((p) => `${p.text} ${!!p.stale}`));
    const after = await since(s, pages.at(-1).token);
    say("nothing of the suite's since the walk's end", !after.stale && !after.expired && !after.error && !inserted.some((i) => ids(after).includes(i)), after);
    const c = decode(first[1].token);
    // The end is looked for after the start: an earlier one is never found.
    const missing = await since(s, encode({ ...c, at: decode(t1).at, end: decode(t0).at }), (r) => r.expired || !!r.error);
    say('a continuation whose end is missing: expired, empty, a fresh checkpoint',
      missing.text === '|||false true' && !missing.error && !!decode(missing.token).at && !decode(missing.token).end, missing);
    for (const [what, at] of [['an archive of no token', Buffer.from('not an archive').toString('base64')],
      ['an archive of another class', forge(decode(t1).at, (x) => x.replaceAll('<string>PHPersistentChangeToken</string>', '<string>NSDate</string>'))]]) {
      const p = await since(s, encode({ ...decode(t1), at }), (r) => !!r.error || r.expired);
      say(`${what}: SyntaxError`, p.error === 'SyntaxError', p);
    }
    // A position PhotoKit never issued under limited access.
    const limited = await since(s, encode({ ...decode(t1), access: 'limited' }), (r) => !!r.error || r.expired);
    say('a position labelled limited: SyntaxError', limited.error === 'SyntaxError', limited);
    // History PhotoKit cannot supply, by its own error (PHPhotosErrorPersistentChangeTokenExpired): another
    // library's store, and a position past the newest.
    for (const [what, forged] of [['a token of another library', forge(decode(t1).at, (x) => x.replace(/[0-9A-F]{8}(-[0-9A-F]{4}){3}-[0-9A-F]{12}/, '00000000-0000-4000-8000-000000000000'))],
      ['a position past the newest', forge(decode(t1).at, (x) => x.replace(/<integer>(\d{2,})<\/integer>/, '<integer>999999999</integer>'))]]) {
      const p = await since(s, encode({ ...decode(t1), at: forged }), (r) => r.expired || !!r.error);
      const why = JSON.stringify(await s.logs()).match(/photoChanges (expired|failed): [^"\\]*/g)?.at(-1);
      say(`${what}: expired, as PhotoKit says`, p.text === '|||false true' && !p.error && why === 'photoChanges expired: PHPhotosErrorDomain 3105', `${p.text} ${p.error} ${why}`);
    }
    return pages.at(-1).token;
  }, { one: true });

  // A favourite, a hidden photo and a deletion.
  const [favorite, hidden, gone] = inserted;
  const reached = edit([{ edit: 'favorite', ids: [favorite] }, { edit: 'hide', ids: [hidden] }, { edit: 'delete', ids: [gone] }]);
  say('the runner made the three edits', reached.every((r) => r.length === 1), reached);
  const t3 = await launch(async (s) => {
    const p = await since(s, t2, (r) => (r.deleted.includes(hidden) && r.deleted.includes(gone)) || !!r.error);
    say('a favourite an update; the hidden photo and the deleted one removals',
      once([p], [favorite], 'updated') && once([p], [hidden, gone], 'deleted') && done(p), p);
    return p.token;
  });

  // An edit and deletions between the pages of a walk. A page reads the library as it is then. PhotoKit leaves a
  // photo deleted since out of the records before its deletion, so the walk loses it, the continuation's own key
  // included, and its removal comes in the next walk, as does the edit to a photo already read.
  edit([{ edit: 'unfavorite', ids: [favorite] }]);
  const [read, key, target] = imports(3);
  const next = await launch(async (s) => {
    const all = await since(s, t3, (r) => [read, key, target].every((i) => r.added.includes(i)) || !!r.error);
    say('unfavourited and three imports: an update and three inserts', once([all], [favorite], 'updated') && once([all], [read, key, target], 'added') && done(all), all);
    await s.tap('page-of-one');
    // Pages of one until the walk has read `key`, `read` before it.
    const pages = [await panel(s, (r) => r.more || !!r.error)];
    while (pages.at(-1).more && !ids(pages.at(-1)).includes(key) && !pages.at(-1).stale) pages.push(await advance(s, pages.at(-1)));
    say('pages of one up to an import, another read before it', ids(pages.at(-1))[0] === key && pages.at(-1).more && pages.some((p) => p.added.includes(read)),
      pages.map((p) => p.text));
    return pages.at(-1).token;
  });
  const between = edit([{ edit: 'favorite', ids: [read] }, { edit: 'delete', ids: [key, target] }]);
  say('the runner favourited the photo read and deleted two', between[0].length === 1 && between[1].length === 2, between);
  await launch(async (s) => {
    const rest = await drain(s, await since(s, next));
    const seen = rest.flatMap(ids);
    say('the walk goes on after its deleted key, without the photo deleted before its page, to its end',
      !seen.includes(target) && !seen.includes(key) && ascending(rest) && done(rest.at(-1)), { key, target, rest: rest.map((p) => p.text) });
    const later = await drain(s, await advance(s, rest.at(-1)));
    say('the next walk: the photo favourited after its page, and both deletions',
      once(later, [read], 'updated') && once(later, [key, target], 'deleted') && done(later.at(-1)), later.map((p) => p.text));
  }, { one: true });
  const t4 = await launch((s) => panel(s));

  // The simulator's own screenshot, imported, and the screenshot view: an import that is no screenshot is a removal
  // there, since a photo inserted during the app's baseline read may already be in its index.
  const screen = join(dir, 'screen.png');
  run('xcrun', ['simctl', 'io', sim, 'screenshot', screen], root);
  const [[shot]] = edit([{ edit: 'import', files: [screen] }]);
  owned.push(shot);
  const shots = await launch(async (s) => {
    const p = await since(s, t4.token, (r) => r.added.includes(shot) || !!r.error);
    say('the screenshot imported: an insert', once([p], [shot], 'added') && done(p), p);
    await s.tap('read-shots');
    const listed = ((await until(s, 'shots', (t) => Number(t?.split(' ')[0]) > 0, 60000)) ?? '').split(' ');
    console.log(`NOTE ${where}: a simulator screenshot imported through PhotoKit is ${listed[5] === shot ? '' : 'not '}the newest photoScreenshot (shots: ${listed.join(' ')})`);
    await s.tap('only-screenshots');
    const view = await panel(s, (r) => r.expired || !!r.error);
    const t = view.token && decode(view.token);
    say("the screenshots alone: the full view's token expires, a checkpoint of this view", view.text === '|||false true' && JSON.stringify(t?.subtypes) === '["screenshot"]' && !!t.at, view);
    return view.token;
  });
  const [made] = imports(1);
  await launch(async (s) => {
    await s.tap('only-screenshots');
    const p = await since(s, shots, (r) => r.deleted.includes(made) || !!r.error);
    // Every candidate here is no screenshot, the library's own updates too.
    say('screenshots: an import that is none, a removal', once([p], [made], 'deleted') && !p.added.length && !p.updated.length && done(p), p);
  });

  // A change larger than the page: 501 imports, in pages of at most 500.
  const many = imports(501);
  await launch(async (s) => {
    const one = await since(s, t4.token, (r) => r.more || !!r.error);
    say('501 imports and more: a first page of 500, with more', ids(one).length === 500 && one.more && !one.stale, `${ids(one).length} ${one.more} ${one.error}`);
    const pages = await drain(s, one), all = pages.flatMap(ids);
    say('the rest after it, each candidate once, each import an insert once', done(pages.at(-1)) && new Set(all).size === all.length && once(pages, many, 'added'),
      `${pages.length} pages, ${all.length} candidates`);
  });

  // The library as the suite found it, but for the photo it hid, which no third-party app can reach again.
  const left = owned.filter((o) => ![hidden, gone, key, target].includes(o));
  say('the suite deletes the photos it imported', edit([{ edit: 'delete', ids: left }])[0].length === left.length, left.length);
}
