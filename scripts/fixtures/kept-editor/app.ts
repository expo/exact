import type { Answer } from './app.contract.d.ts';

export const appId = 'com.exact.kepteditor';
export const grants = 'fs.read doc:/\nfs.write doc:/\ndoc.keep';

const code = (e: unknown) => String((e as { code?: string })?.code ?? e);

export const answer: Answer = async (source, args, _store, storage) => {
  if (source === 'openDocument') {
    const [path] = args as [string];
    if (!path) return { text: '', modifiedMs: 0, size: 0, error: '' };
    storage.fs.watch(path);
    try {
      const stat = await storage.fs.stat(path);
      const text = new TextDecoder().decode(await storage.fs.readFile(path));
      return { text, modifiedMs: stat.modifiedMs, size: stat.size, error: '' };
    } catch (e) {
      if (code(e) === 'bake') throw e;
      return { text: '', modifiedMs: 0, size: 0, error: code(e) };
    }
  }
  if (source === 'keptDocuments') {
    storage.fs.watch('doc:/');
    try {
      return (await storage.fs.recentDocuments()).map((d) => ({ path: d.path, name: d.name }));
    } catch (e) {
      if (code(e) === 'bake') throw e;
      return [];
    }
  }
  if (source === 'saveDocument') {
    const [path, text, modifiedMs, size] = args as [string, string, number, number];
    try {
      // The app's own atomic replace, of the file it last read (D2a): the
      // host carries a kept entry to the file it writes (D4).
      await storage.fs.atomicWriteFile(path, new TextEncoder().encode(text), { expect: { modifiedMs, size } });
      return { error: '' };
    } catch (e) {
      return { error: code(e) };
    }
  }
  throw { kind: 'UnknownSource', message: source };
};
