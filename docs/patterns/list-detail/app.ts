import type { Answer, Result, Sources, Storage } from './app.contract.d.ts';

const FILE = 'app:/data/items.json';
export const grants = [`fs.read ${FILE}`, `fs.write ${FILE}`].join('\n');

type Item = Result<'loadItems'>[number];
let items: Item[] | null = null; // memory; the file is what survives a launch
let last: Promise<unknown> = Promise.resolve(); // the mutation before this one

async function load(storage: Storage): Promise<Item[]> {
  if (!items) items = await storage.fs.readFile(FILE).then(
    (bytes) => JSON.parse(new TextDecoder().decode(bytes)) as Item[],
    (e) => { if (e.code === 'ENOENT') return []; throw e; }); // no file yet; 'bake' (the build) passes through
  return items;
}
async function save(storage: Storage, next: Item[]): Promise<Result<'addItem'>> {
  await storage.fs.atomicWriteFile(FILE, new TextEncoder().encode(JSON.stringify(next)));
  items = next; // only once stored: a failed write leaves memory as the file is
  return { ok: true, error: '' };
}
// One mutation at a time, each reading what the one before it saved.
function mutate<T>(work: () => Promise<T>): Promise<T> {
  const run = last.then(work, work);
  last = run.catch(() => {});
  return run;
}

const sources: Sources = {
  loadItems: async (_args, _store, storage) => [...(await load(storage))].reverse(), // newest first
  addItem: ([title, notes], _store, storage) => mutate(async () => {
    const all = await load(storage);
    const same = all.find((i) => i.title.toLowerCase() === title.toLowerCase());
    if (same) return { ok: false, error: `There’s already an item called ${same.title}. Choose another title.` }; // a refusal is a reply too
    const id = String(Math.max(0, ...all.map((i) => Number(i.id))) + 1);
    return save(storage, [...all, { id, title, notes, locked: false }]);
  }),
  lockItem: ([id, locked], _store, storage) => mutate(async () => save(storage, (await load(storage)).map((i) => i.id === id ? { ...i, locked } : i))),
  removeItem: ([id], _store, storage) => mutate(async () => save(storage, (await load(storage)).filter((i) => i.id !== id))),
};
export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);
