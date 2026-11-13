// `bun test apps/shelf`: the recipe stays tell-free (LLP 1115), and the data
// module keeps its store's rules. The flows are app.test.contract's, driven on
// the web and iOS.
import { expect, mock, test } from 'bun:test';
import { tells, tellsIn } from '../../scripts/no-tells.mjs';

// The build generates the package's module; these tests reach no tag.
mock.module('@exact/nfc', () => Object.fromEntries(['ndefRecords', 'write']
  .map(name => [name, () => { throw new Error(`unexpected NFC call: ${name}`); }])));

test('Shelf writes no colour, font size or weight of its own', () => {
  expect(tells(import.meta.dir)).toEqual([]);
});

test('no-tells finds literal colours and sizes, and lets roles and text styles by', () => {
  const found = tellsIn([
    'component A',
    '  view',
    '    column background-color="#f2f2f7" border="1px solid #d1d1d6"',
    '      text "a" color=(on ? "rgb(0, 122, 255)" : "red") font-size=34 font-weight=700',
    '      text "b" color="-exact-secondary-label" font="-exact-footnote" border-bottom="0.5px solid -exact-separator"',
    '      text "c" role="heading" aria-level=1 font-size="-exact-caption1" background-color="Canvas"',
    '      // color="#ffffff" in a comment is not a tell',
    '      text "#ffffff is text, not a colour"',
    '      row background-color="#B5562B" // no-tells: the brand',
  ].join('\n'));
  expect(found.map(t => `${t.line} ${t.property}`)).toEqual(['3 background-color', '3 border', '4 color', '4 font-size', '4 font-weight']);
});

// The store's rules, through the data module's own answers.
async function shelf() {
  let bytes: Uint8Array | undefined;
  let reads = 0;
  const storage = { fs: {
    readFile: async () => { reads++; if (!bytes) throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' }); return bytes.buffer; },
    mkdir: async () => {},
    atomicWriteFile: async (_path: string, next: Uint8Array) => { bytes = next.slice(); },
  } };
  const source = await import(`./app.ts?case=${Math.random()}`);
  const call = (name: string, ...args: unknown[]) => source.answer(name, args, {}, storage, {});
  const book = async (id: string) => (await call('library')).books.find((b: { id: string }) => b.id === id);
  return { call, book, reads: () => reads };
}

// Two edits that both wait on the first load (a tool's and the person's,
// LLP 1133.023 §11) each apply to the newest state, so neither is lost.
test('two edits waiting on the first load both land', async () => {
  const { call } = await shelf();
  await Promise.all([
    call('addBook', 'Dune', 'Frank Herbert', 412, 'want', '', '', 1),
    call('addBook', 'Emma', 'Jane Austen', 474, 'want', '', '', 2),
  ]);
  const titles = (await call('library')).books.map((b: { title: string }) => b.title);
  expect(titles).toContain('Dune');
  expect(titles).toContain('Emma');
});

test('progress clamps, a page read starts a book, the last page finishes it', async () => {
  const { call, book } = await shelf();
  await call('setPage', 'seed-2', -50);
  expect((await book('seed-2')).currentPage).toBe(0);
  await call('setPage', 'seed-3', 10);
  expect((await book('seed-3')).status).toBe('reading');
  await call('setPage', 'seed-3', 10000);
  expect(await book('seed-3')).toMatchObject({ currentPage: 387, status: 'done' });
  await call('setStatus', 'seed-2', 'done');
  expect((await book('seed-2')).currentPage).toBe(272);
});

test('the goal stays between 1 and 365, and the average counts rated finished books', async () => {
  const { call } = await shelf();
  await call('setGoal', 0);
  expect((await call('library')).goal).toBe(1);
  await call('setGoal', 400);
  expect((await call('library')).goal).toBe(365);
  expect((await call('library')).avgText).toBe('5.0 ★');
});


test('a fresh widget reads the same library as the app, then follows edits', async () => {
  const { call, reads } = await shelf();
  const rows = await call('widgetLibrary', 1234);
  const library = await call('library');
  expect(reads()).toBe(1);
  expect(rows).toEqual([{ date: 1234, props: { count: library.books.length, latest: library.books[0].title } }]);
  expect(rows[0].props.count).toBe(3); // The gallery's preview has seven.
  await call('removeBook', 'seed-3');
  expect(await call('widgetLibrary', 5678)).toEqual([{ date: 5678, props: { count: 2, latest: 'Piranesi' } }]);
});

// Open Library's subjects carry no page count, so a discovered work's length
// is unknown, and finishing it moves no page.
test('a discovered work is added without a length, and finishing it moves no page', async () => {
  const { call } = await shelf();
  const realFetch = globalThis.fetch;
  globalThis.fetch = (async () => Response.json({ works: [{ key: '/works/OL1W', title: 'Found', authors: [{ name: 'Someone' }] }] })) as unknown as typeof fetch;
  try {
    const [work] = (await call('subjectWorks', 'fantasy')).works;
    expect(work.pages).toBe(0);
    await call('addBook', work.title, work.author, work.pages, 'want', work.coverUrl, work.key, 1);
  } finally {
    globalThis.fetch = realFetch;
  }
  const found = (await call('library')).books.find((b: { sourceKey: string }) => b.sourceKey === '/works/OL1W');
  await call('setStatus', found.id, 'done');
  const done = (await call('library')).books.find((b: { id: string }) => b.id === found.id);
  expect(done).toMatchObject({ pages: 0, currentPage: 0, status: 'done' });
  expect((await call('library')).pagesRead).toBe(304 + 120);
});

// A write that fails leaves the library as stored: the screen never shows an
// edit a relaunch would not have.
test('a failed write leaves the library as stored', async () => {
  let bytes = new TextEncoder().encode(JSON.stringify({ books: [{ id: 'mine', title: 'My Own Book' }], goal: 12 }));
  let full = false;
  const storage = { fs: {
    readFile: async () => bytes.buffer,
    mkdir: async () => {},
    atomicWriteFile: async (_path: string, next: Uint8Array) => {
      if (full) throw Object.assign(new Error('No space left on device'), { code: 'failed' });
      bytes = next.slice();
    },
  } };
  const source = await import(`./app.ts?case=${Math.random()}`);
  const call = (name: string, ...args: unknown[]) => source.answer(name, args, {}, storage, {});
  const titles = async () => (await call('library')).books.map((b: { title: string }) => b.title);
  expect(await titles()).toEqual(['My Own Book']);
  full = true;
  expect(await call('addBook', 'Unsaved', 'Me', 100, 'want', '', '', 1)).toEqual({ ok: false });
  expect(await call('removeBook', 'mine')).toEqual({ ok: false });
  expect(await titles()).toEqual(['My Own Book']);
  full = false;
  expect(await call('addBook', 'Saved', 'Me', 100, 'want', '', '', 2)).toEqual({ ok: true });
  expect(await titles()).toEqual(['Saved', 'My Own Book']);
});

// Only a missing file is a first launch. Any other failed read leaves the
// library not ready and refuses every edit, so the seed never replaces it.
test('a failed read is not a first launch, and no edit saves over the stored library', async () => {
  const stored = new TextEncoder().encode(JSON.stringify({ books: [{ id: 'mine', title: 'My Own Book' }], goal: 12 }));
  let failing = true, writes = 0;
  const storage = { fs: {
    readFile: async () => {
      if (failing) throw Object.assign(new Error('failed filesystem: Permission denied (os error 13)'), { code: 'failed' });
      return stored.buffer;
    },
    mkdir: async () => {},
    atomicWriteFile: async () => { writes++; },
  } };
  const source = await import(`./app.ts?case=${Math.random()}`);
  const call = (name: string, ...args: unknown[]) => source.answer(name, args, {}, storage, {});
  expect((await call('library')).ready).toBe(false);
  expect(await call('addBook', 'Second Book', 'Me', 100, 'want', '', '', 1)).toEqual({ ok: false });
  expect(writes).toBe(0);
  failing = false;
  expect((await call('library')).books.map((b: { title: string }) => b.title)).toEqual(['My Own Book']);
});
