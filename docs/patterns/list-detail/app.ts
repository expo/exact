import type { Answer, Database, Result, Sources, Storage } from './app.contract.d.ts';

const DB = 'app:/data/items.db';
export const grants = `sqlite.open ${DB}`;
let last: Promise<unknown> = Promise.resolve();

// Close after each answer: on the web an open database holds its cross-tab lock.
function use<T>(storage: Storage, work: (db: Database) => Promise<T>): Promise<T> {
  const run = last.then(async () => {
    const db = await storage.sqlite.open(DB);
    try {
      await db.execute('CREATE TABLE IF NOT EXISTS items (id INTEGER PRIMARY KEY AUTOINCREMENT, title TEXT NOT NULL, title_key TEXT NOT NULL UNIQUE, notes TEXT NOT NULL, locked INTEGER NOT NULL DEFAULT 0)');
      return await work(db);
    } finally { await db.close(); }
  });
  last = run.catch(() => {});
  return run;
}
const saved = (): Result<'addItem'> => ({ ok: true, error: '' });
const sources: Sources = {
  loadItems: (_args, _store, storage) => use(storage, async db =>
    (await db.query('SELECT id, title, notes, locked FROM items ORDER BY id DESC')).rows
      .map(([id, title, notes, locked]) => ({ id: String(id), title: String(title), notes: String(notes), locked: Number(locked) !== 0 }))),
  addItem: ([title, notes], _store, storage) => use(storage, async db => {
    const key = title.toLowerCase(); // JavaScript's Unicode lowercasing, not SQLite's ASCII-only lower().
    const result = await db.execute('INSERT INTO items (title, title_key, notes) VALUES (?, ?, ?) ON CONFLICT(title_key) DO NOTHING', [title, key, notes]);
    if (result.changes) return saved();
    const { rows } = await db.query('SELECT title FROM items WHERE title_key = ?', [key]);
    return { ok: false, error: `There’s already an item called ${String(rows[0]?.[0] ?? title)}. Choose another title.` };
  }),
  lockItem: ([id, locked], _store, storage) => use(storage, async db => {
    await db.execute('UPDATE items SET locked = ? WHERE id = ?', [Number(locked), id]);
    return saved();
  }),
  removeItem: ([id], _store, storage) => use(storage, async db => {
    await db.execute('DELETE FROM items WHERE id = ?', [id]);
    return saved();
  }),
};
export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);
