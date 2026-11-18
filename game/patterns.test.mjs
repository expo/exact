// The starter's SQL runs against SQLite; failed writes leave no private copy of the list.
import { expect, test } from 'bun:test';
import { Database } from 'bun:sqlite';

test('the list-detail starter keeps no failed write, preserves other items and never reuses a deleted id', async () => {
  const { answer } = await import('../docs/patterns/list-detail/app.ts');
  const db = new Database(':memory:');
  let opened = 0, closed = 0;
  const storage = { sqlite: { open: async () => {
    opened++;
    return {
      execute: async (sql, params = []) => db.query(sql).run(...params),
      query: async (sql, params = []) => ({ rows: db.query(sql).values(...params) }),
      close: async () => { closed++; },
    };
  } } };
  const call = (source, ...args) => answer(source, args, null, storage);
  const titles = async () => (await call('loadItems')).map(i => i.title);
  try {
    expect(await titles()).toEqual([]);
    db.run("CREATE TRIGGER refuse_add BEFORE INSERT ON items BEGIN SELECT RAISE(ABORT, 'disk full'); END");
    await expect(call('addItem', 'Alpha', 'notes')).rejects.toThrow('disk full');
    expect(await titles()).toEqual([]);
    db.run('DROP TRIGGER refuse_add');
    expect(await call('addItem', 'Alpha', 'notes')).toEqual({ ok: true, error: '' });
    db.run("CREATE TRIGGER refuse_remove BEFORE DELETE ON items BEGIN SELECT RAISE(ABORT, 'disk full'); END");
    await expect(call('removeItem', '1')).rejects.toThrow('disk full');
    expect(await titles()).toEqual(['Alpha']);
    db.run('DROP TRIGGER refuse_remove');
    const both = await Promise.all([call('addItem', 'Beta', ''), call('addItem', 'Gamma', '')]);
    expect(both).toEqual([{ ok: true, error: '' }, { ok: true, error: '' }]);
    expect(await titles()).toEqual(['Gamma', 'Beta', 'Alpha']);
    await Promise.all([call('lockItem', '1', true), call('lockItem', '2', true)]);
    await Promise.all([call('removeItem', '3'), call('addItem', 'Delta', '')]);
    await call('lockItem', '3', true); // A stale detail must not change a new item that took its id.
    expect(await call('loadItems')).toEqual([
      { id: '4', title: 'Delta', notes: '', locked: false },
      { id: '2', title: 'Beta', notes: '', locked: true },
      { id: '1', title: 'Alpha', notes: 'notes', locked: true },
    ]);
    await call('addItem', 'Éclair', '');
    expect(await call('addItem', 'éclair', '')).toEqual({ ok: false, error: 'There’s already an item called Éclair. Choose another title.' });
    expect(closed).toBe(opened);
  } finally { db.close(); }
});
