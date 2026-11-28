// The content index under the agent on the JS target (LLP 1133.023 §10): the parents of an entry are an option of
// the call (LLP 1126.005 D2), checked as the runner's `modules/declared/reach.rs` checks them.
import { expect, mock, test } from 'bun:test';

mock.module('./router.js', () => ({ routeMatches: url => url.startsWith('/book/') }));
const { reach } = await import('./reach.js');

test('an entry\'s parents name an entry of the call and no empty id', () => {
  const said = [];
  const index = reach({ say: line => said.push(line), announce() {}, preferred() {} });
  const set = { key: 'search-index.setContentIndex', sub: { index: 'set' } }, add = { key: 'search-index.addContentIndexEntry', sub: { index: 'add' } };
  const entry = id => ({ id, title: id, url: `/book/${id}` });
  const place = within => index.effect(set, { kind: 'book', entries: [entry('a'), entry('b')], within }, []);
  const join = within => index.effect(add, { description: { ...entry('ursula'), kind: 'author' }, within }, []);

  expect(place(undefined)).toBeNull();
  expect(place(null)).toBeNull();
  expect(place([{ id: 'a', within: ['reading'] }, { id: 'b', within: [] }])).toBeNull();
  expect(join(['fr'])).toBeNull();
  for (const within of [[{ id: 'a', within: [''] }], [{ id: 'z', within: ['reading'] }], [{ id: '', within: ['reading'] }], [{ id: 'a', within: [7] }], [{ id: 'a' }], 'a', [7]]) {
    expect(place(within), JSON.stringify(within)).toMatchObject({ error: 'TypeError' });
  }
  for (const within of [[''], [7], 'fr']) expect(join(within), JSON.stringify(within)).toMatchObject({ error: 'TypeError' });
  const state = {};
  index.state([set], state);
  expect(state.counts).toEqual({ book: 2, author: 1 });
});
