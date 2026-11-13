import type { Answer, Sources } from './app.contract.d.ts';
import { abortBackgroundFetch, backgroundFetch, backgroundFetchRegistration, retireBackgroundFetch } from '@exact/background-transfer';

// The suite writes the id `exact new` gave the app, and its server's origin.
export const appId = '__APP_ID__';
const origin = '__ORIGIN__';
export const grants = [`net.fetch ${origin}`, 'fs.read app:/data', 'fs.write app:/data'].join('\n');

const nameOf = (e: unknown) => { const { name, message } = e as { name?: string; message?: string }; return name === 'Error' ? `Error: ${message}` : name ?? String(e); };
const text = (s: string) => new TextEncoder().encode(s);
// A request's unsaid fields are null: a shape's `option<…>` field is present.
const request = (r: { url: string; method?: string; headers?: { name: string; value: string }[]; bodyFrom?: string; saveTo?: string }) =>
  ({ method: null, headers: null, bodyFrom: null, saveTo: null, ...r });

const sources: Sources = {
  // The query and the header stand in for a credential, which no record keeps.
  startFetch: async ([kind, id], _store, storage) => {
    const at = (path: string) => `${origin}${path}${path.includes('?') ? '&' : '?'}id=${id}&token=secret`;
    try {
      if (kind === 'upload') {
        await storage.fs.mkdir('app:/data/out').catch((e: { code?: string }) => { if (e?.code !== 'EEXIST') throw e; });
        await storage.fs.atomicWriteFile(`app:/data/out/${id}.txt`, text(`hello ${id}`));
        await backgroundFetch({ id: String(id), requests: [request({ url: at('/upload'), method: 'POST', headers: [{ name: 'Authorization', value: 'Bearer secret' }], bodyFrom: `app:/data/out/${id}.txt` })] });
      } else if (kind === 'download') {
        await backgroundFetch({ id: String(id), requests: [request({ url: at('/file'), saveTo: `app:/data/saved/${id}.txt` })], downloadTotal: 1_000_000 });
      } else if (kind === 'missing') {
        await backgroundFetch({ id: String(id), requests: [request({ url: at('/missing') })] });
      } else if (kind === 'folder') {
        // A `saveTo` that is a folder is refused, the folder kept.
        await storage.fs.mkdir(`app:/data/${id}`).catch((e: { code?: string }) => { if (e?.code !== 'EEXIST') throw e; });
        await storage.fs.atomicWriteFile(`app:/data/${id}/keep.txt`, text('kept'));
        await backgroundFetch({ id: String(id), requests: [request({ url: at('/file'), saveTo: `app:/data/${id}` })] });
      } else if (kind === 'appear') {
        // A folder made at `saveTo` while the body is on its way is kept, the body not placed.
        await backgroundFetch({ id: String(id), requests: [request({ url: at('/file?wait=1'), saveTo: `app:/data/${id}` })] });
        await storage.fs.mkdir(`app:/data/${id}`);
        await storage.fs.atomicWriteFile(`app:/data/${id}/keep.txt`, text('kept'));
      } else if (kind === 'replace') {
        await storage.fs.mkdir('app:/data/saved').catch((e: { code?: string }) => { if (e?.code !== 'EEXIST') throw e; });
        await storage.fs.atomicWriteFile(`app:/data/saved/${id}.txt`, text('old'));
        await backgroundFetch({ id: String(id), requests: [request({ url: at('/file'), saveTo: `app:/data/saved/${id}.txt` })] });
      } else if (kind === 'keep') {
        // Aborted before a body comes: the file in `saveTo` is not the response.
        await storage.fs.mkdir('app:/data/saved').catch((e: { code?: string }) => { if (e?.code !== 'EEXIST') throw e; });
        await storage.fs.atomicWriteFile(`app:/data/saved/${id}.txt`, text('old'));
        await backgroundFetch({ id: String(id), requests: [request({ url: at('/file?hold=1'), saveTo: `app:/data/saved/${id}.txt` })] });
      } else if (kind === 'userinfo') {
        await backgroundFetch({ id: String(id), requests: [request({ url: at('/file').replace('://', '://alice:secret@') })] });
      } else if (kind === 'hold') {
        await backgroundFetch({ id: String(id), requests: [request({ url: at('/file?hold=1') })] });
      } else {
        await backgroundFetch({ id: String(id), requests: [request({ url: `http://127.0.0.2:9/upload?id=${id}`, bodyFrom: `app:/data/out/${id}.txt` })] });
      }
      return { text: 'started' };
    } catch (e) {
      return { text: nameOf(e) };
    }
  },
  abortFetch: async ([id]) => {
    return { text: String(await abortBackgroundFetch({ id: String(id) })) };
  },
  // The registration as the handler's data code reads it: each record's body from its file, then retired.
  note: async ([id, heard], _store, storage) => {
    const r = await backgroundFetchRegistration({ id: String(id) });
    if (!r) return { text: `${id} gone` };
    const first = r.records[0];
    let body = '';
    if (first?.path) body = new TextDecoder().decode(await storage.fs.readFile(first.path));
    const kept = await storage.fs.readFile(`app:/data/${id}/keep.txt`).then(() => 'kept', () => 'none');
    const q = new URLSearchParams({ kept, id: String(id), heard: String(heard), result: r.result, reason: r.failureReason, status: String(first?.status ?? 0), body, path: first?.path ?? '', url: first?.url ?? '', lost: String(first?.bodyLost ?? false), uploaded: String(r.uploaded), uploadTotal: String(r.uploadTotal), downloaded: String(r.downloaded) });
    await fetch(`${origin}/heard?${q}`).catch(() => null);
    await retireBackgroundFetch({ id: String(id) });
    return { text: `${id} ${heard} ${r.result} ${r.failureReason} ${first?.status ?? 0} ${body}` };
  },
};
export const answer: Answer = (source, args, store, storage, native) => sources[source](args, store, storage, native);
