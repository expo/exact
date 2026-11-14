# Recipe: files, SQLite and grants

Where an app keeps things, and its `grants` line:

| What | Where | Grant |
| --- | --- | --- |
| A setting | `state … persist` ([a remembered setting](persisted-setting.md)) | none |
| A small collection | a JSON file, `storage.fs` (start-here's list) | `fs.read app:/data/items.json`, `fs.write …` |
| A collection that grows or is queried | SQLite, `storage.sqlite` | `sqlite.open app:/data/journal.db` |
| A secret: a token | `store.get`, `store.set` | `secret.keep token` |
| Data the app ships | a JSON file `app.ts` imports ([shipped data](shipped-data.md)) | none |
| A file read by path | app.json `host.ios.resources` (and `.macos`) | `fs.read bundle:/Resources/x` (its `to`) |
| A server | `fetch` ([a REST server](rest-backend.md)) | `net.fetch https://api.example.com` |
| A server named at run time | `fetch`, a socket | `net.fetch https:`, `net.websocket wss:` (any host and port); plain `http:` on Apple also needs `appTransportSecurity.allowsArbitraryLoads` |

A journal in SQLite (start-here's field and Add):

```contract
shape Entry
  id: string
  text: string

shape Reply
  ok: bool

component Journal
  resource entries = loadEntries() as shape Entry[] else empty()
  mutation added as shape Reply refreshes entries
  action add(text: string)
    send added = addEntry(text)
  view
    column
      for e in entries key=e.id
        text e.text
```

```ts
import type { Answer, Database, Sources, Storage } from './app.contract.d.ts';

const DB = 'app:/data/journal.db';
export const grants = `sqlite.open ${DB}`;

async function use<T>(storage: Storage, work: (db: Database) => Promise<T>): Promise<T> {
  const db = await storage.sqlite.open(DB);
  try {
    await db.execute('CREATE TABLE IF NOT EXISTS entries (id INTEGER PRIMARY KEY, text TEXT NOT NULL)');
    return await work(db);
  } finally { await db.close(); }
}
const sources: Sources = {
  loadEntries: (_args, _store, storage) => use(storage, async (db) =>
    (await db.query('SELECT id, text FROM entries ORDER BY id DESC')).rows
      .map(([id, text]) => ({ id: String(id), text: String(text) }))), // an INTEGER is a bigint
  addEntry: ([text], _store, storage) => use(storage, async (db) => {
    await db.execute('INSERT INTO entries (text) VALUES (?)', [text]);
    return { ok: true };
  }),
};
export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);
```

- A grant covers its path and its tree (`fs.write app:/data`); anything else is refused,
  code `denied`. A native line names a directory (`fs.read /etc`). `app:/data` is kept,
  `app:/cache` refetchable, `app:/tmp` scratch ([a native module's](native-module.md)
  `context.data`, `.cache`, `.temporary`).
- A file is bytes (`readFile`, `atomicWriteFile`); a missing one is code `ENOENT`: answer
  the empty collection, and branch on `e.code`, never the message.
- SQLite: `query` reads, `execute` writes, `transaction([...])` writes several or none.
- At build there is no storage (code `bake`): let it through.
