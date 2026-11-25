// A web tab tells the app's others of a SQLite write from the worker's `changed` (LLP 1030.004 D2), and the app gets
// only `changes` and `lastInsertRowid`.
import { afterAll, beforeAll, expect, test } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { pathToFileURL } from 'node:url';
import { runtimeSource } from '../web/tests/stand-ins.mjs';

let dir, database;
beforeAll(async () => {
  // ts-sqlite.js beside a ts-fetch.js whose `owned` is the promise itself: what an answer owns is not this test's.
  dir = mkdtempSync(resolve(tmpdir(), 'exact-ts-sqlite-'));
  writeFileSync(resolve(dir, 'ts-sqlite.js'), runtimeSource('ts-sqlite.js'));
  writeFileSync(resolve(dir, 'ts-fetch.js'), 'export const owned = promise => promise;\n');
  ({ database } = await import(pathToFileURL(resolve(dir, 'ts-sqlite.js')).href));
});
afterAll(() => rmSync(dir, { recursive: true, force: true }));

test('an execute or a transaction tells the other tabs only when it changed the database', async () => {
  const relayed = [], answers = [];
  const worker = {
    execute: async () => answers.shift(),
    transaction: async () => answers.shift(),
    prepare: async () => ({ execute: async () => answers.shift(), query: async () => ({ columns: [], rows: [] }), close: async () => {} }),
    query: async () => ({ columns: [], rows: [] }),
    close: async () => {},
  };
  const shared = { watch: () => {}, relay: topics => relayed.push(...topics) };
  const db = database(worker, 'app:/data/notes.db', null, (_, run) => run(), new Set(), shared);
  const run = async (call, ...given) => { answers.push(...given); const n = relayed.length; return [await call(), relayed.length > n]; };
  const unchanged = { changes: 0, lastInsertRowid: 1n, changed: false };
  expect(await run(() => db.execute('DROP TABLE notes'), { changes: 0, lastInsertRowid: 1n, changed: true }))
    .toEqual([{ changes: 0, lastInsertRowid: 1n }, true]);
  expect(await run(() => db.execute('CREATE TABLE IF NOT EXISTS notes (body)'), unchanged))
    .toEqual([{ changes: 0, lastInsertRowid: 1n }, false]);
  const statement = await db.prepare('DELETE FROM notes WHERE body = ?');
  expect(await run(() => statement.execute(['none']), unchanged)).toEqual([{ changes: 0, lastInsertRowid: 1n }, false]);
  expect(await run(() => db.transaction([{ sql: 'a' }, { sql: 'b' }]), [unchanged, { changes: 1, lastInsertRowid: 2n, changed: true }]))
    .toEqual([[{ changes: 0, lastInsertRowid: 1n }, { changes: 1, lastInsertRowid: 2n }], true]);
  expect(await run(() => db.transaction([{ sql: 'a' }]), [unchanged])).toEqual([[{ changes: 0, lastInsertRowid: 1n }], false]);
  expect(relayed).toEqual(['sqlite:app:/data/notes.db', 'sqlite:app:/data/notes.db']);
});
