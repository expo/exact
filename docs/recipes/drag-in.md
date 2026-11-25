# Recipe: things dragged into the app

A `drop` node's `DragEvent` has `files` (`doc:` handles), `text` and
`url`; check `url` before following it.

**A dropped file is lent, not given.** On the Mac, Windows, Linux and a
browser giving file handles, its handle reads the source's file, which
may change or go; on iOS, Android, from a Mac promise and in other
browsers, a copy. An unkept handle ends with the session. Keep it one of
two ways:

- **As the app's own**: copy it into `app:/data` in a mutation the drop
  sends, as below.
- **As the person's document**, in place: declare its type
  (`file_handlers`) and grant `doc.keep` and `fs.write doc:/`;
  [the reference](../reference.md#documents-the-person-chose-doc) says which
  drops reopen. Editing a copy changes only the copy.

```contract
shape Kept
  id: string
  name: string
  size: number

shape Reply
  ok: bool

component Attachments
  resource kept = loadKept() as shape Kept[] else empty()
  mutation copied as shape Reply queue refreshes kept
  action took(e: DragEvent)
    send copied = keepDropped(e.files)
  view
    column testId="zone" drop=took flex=1
      text "Drop files here"
      for a in kept key=a.id
        text `${a.name} (${a.size} B)` testId=`file-${a.id}`
```

In `app.ts` (import `Storage` too):

```ts
const DIR = 'app:/data/attachments';
const LIST = `${DIR}/list.json`;
export const grants = `fs.read doc:/\nfs.read ${DIR}\nfs.write ${DIR}`;

const listed = (storage: Storage) =>
  storage.fs.readFile(LIST).then((b) => JSON.parse(new TextDecoder().decode(b)),
    (e) => (e.code === 'ENOENT' ? [] : Promise.reject(e)));

const sources: Sources = {
  loadKept: async (_args, _store, storage) => {
    const out = [];
    for (const a of await listed(storage)) out.push({ ...a, size: (await storage.fs.stat(`${DIR}/${a.id}/${a.name}`)).size });
    return out;
  },
  keepDropped: async ([files], _store, storage) => {
    const list = await listed(storage);
    for (const file of files) {
      if ((await storage.fs.stat(file)).isDirectory) continue; // folders are skipped
      const name = file.split('/').pop()!;
      const id = String(list.length);
      await storage.fs.mkdir(`${DIR}/${id}`);
      const path = `${DIR}/${id}/${name}`;
      list.push({ id, name });
      await storage.fs.copyFile(file, path)
        .then(() => storage.fs.atomicWriteFile(LIST, new TextEncoder().encode(JSON.stringify(list))))
        .catch((e) => storage.fs.rm(path).finally(() => { throw e; }));
    }
    return { ok: true };
  },
};
```

A copy not listed is removed if it can, and throws ([after a
write](after-a-write.md)); `copyFile` passes `readFile`'s 16 MiB limit; one
writer is assumed.

## Testing it

Beside the test, `fixtures/a/notes.txt` has 19 bytes and
`fixtures/b/notes.txt` 5:

```contract-test
test "two files of one name are kept"
  tap "zone" drop file "fixtures/a/notes.txt"
  tap "zone" drop file "fixtures/b/notes.txt"
  clock settle
  reload
  clock settle
  expect text "file-0" == "notes.txt (19 B)"
  expect text "file-1" == "notes.txt (5 B)"
```
