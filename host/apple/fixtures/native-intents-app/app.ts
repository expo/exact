import type { Answer, Sources } from './app.contract.d.ts';
export const grants = 'fs.read app:/tmp/picked';
const sources: Sources = {
  intentWidget: ([date]) => [{date, props: {title: "Screenshot"}}],
  readAttachment: async ([path], _store, storage) => { const bytes = new Uint8Array(await storage.fs.readFile(path)); return { text: `${path}:${bytes.length}:${Array.from(bytes.slice(0, 8)).join(",")}` }; },
};
export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);
