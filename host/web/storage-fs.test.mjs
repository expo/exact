import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';

// `fn(arg)` in a browser page that serves this directory's modules.
async function inPage(fn, arg) {
  const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const path = new URL(request.url).pathname;
    if (path.endsWith('.js')) return new Response(readFileSync(resolve(import.meta.dirname, path.slice(1))),
      { headers: { 'content-type': 'text/javascript' } });
    return new Response('<!doctype html>');
  } });
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage();
    await page.goto(`http://127.0.0.1:${server.port}`);
    return await page.evaluate(fn, arg);
  } finally { await browser.close(); server.stop(true); }
}

for (const backing of ['Blob', 'File', 'ArrayBuffer']) {
  for (const empty of [false, true]) {
    test(`${empty ? 'an empty append preserves' : 'an append extends'} a ${backing}-backed app file`, async () => {
      const result = await inPage(async ({ backing, empty }) => {
        const { createFileStore } = await import('/storage-fs.js');
        const store = createFileStore('append');
        const original = new Uint8Array([0, 128, 255, 65]);
        const source = 'app:/tmp/source', path = 'app:/data/kept', copy = 'app:/data/copy';
        try {
          if (backing === 'ArrayBuffer') await store.writeFile(source, original);
          else await store.putBlob(source, backing === 'File'
            ? new File([original], 'picked.mp3', { type: 'audio/mpeg' })
            : new Blob([original], { type: 'audio/mpeg' }));
          await store.rename(source, path);
          await store.copyFile(path, copy);
          // A view's surrounding bytes do not belong to the append, nor do later caller edits.
          const data = new Uint8Array([99, 66, 0, 254, 99]);
          const append = store.appendFile(path, empty ? data.subarray(1, 1) : data.subarray(1, 4));
          data.fill(88);
          await append;
          const typeAfterAppend = (await store.blob(path, 'application/octet-stream')).blob.type;
          await store.appendFile(path, new Uint8Array());
          const { blob } = await store.blob(path, 'application/octet-stream');
          // A copy keeps a picked file's type: one asked for with none shows the stored type itself.
          const copyType = (await store.blob(copy, '')).blob.type;
          return {
            bytes: Array.from(new Uint8Array(await store.readFile(path))),
            size: (await store.stat(path)).size,
            copy: Array.from(new Uint8Array(await store.readFile(copy))),
            blob: Array.from(new Uint8Array(await blob.arrayBuffer())),
            typeAfterAppend,
            typeAfterEmptyAppend: blob.type,
            copyType,
          };
        } finally { store.close(); }
      }, { backing, empty });
      const bytes = empty ? [0, 128, 255, 65] : [0, 128, 255, 65, 66, 0, 254];
      const type = backing === 'ArrayBuffer' ? 'application/octet-stream' : 'audio/mpeg';
      expect(result).toEqual({ bytes, size: bytes.length, copy: [0, 128, 255, 65], blob: bytes,
        typeAfterAppend: type, typeAfterEmptyAppend: type, copyType: backing === 'ArrayBuffer' ? '' : 'audio/mpeg' });
    }, 60_000);
  }
}

test('a stored Blob with no type is shown with the type asked for, as bytes are', async () => {
  // `exactBodyTo` stores the reply as such a Blob (storage-fs.js `bodyTarget`); a typed one keeps its own type.
  const types = await inPage(async () => {
    const { createFileStore } = await import('/storage-fs.js');
    const store = createFileStore('untyped');
    const pdf = new TextEncoder().encode('%PDF-1.4 tiny');
    try {
      await store.writeFile('app:/data/written.pdf', pdf);
      await store.putBlob('app:/data/fetched.pdf', new Blob([pdf]));
      await store.putBlob('app:/data/picked.pdf', new File([pdf], 'picked.pdf', { type: 'application/x-picked' }));
      const shown = async (path) => (await store.blob(path, 'application/pdf')).blob.type;
      return [await shown('app:/data/written.pdf'), await shown('app:/data/fetched.pdf'), await shown('app:/data/picked.pdf')];
    } finally { store.close(); }
  });
  expect(types).toEqual(['application/pdf', 'application/pdf', 'application/x-picked']);
}, 60_000);

test('a rename onto the other kind of entry is EISDIR or ENOTDIR, as rename(2) answers natively', async () => {
  const codes = await inPage(async () => {
    const { createFileStore } = await import('/storage-fs.js');
    const store = createFileStore('rename-kinds');
    const code = (promise) => promise.then(() => 'renamed', (error) => error.code);
    try {
      await store.mkdir('app:/data/folder');
      await store.writeFile('app:/data/file', new Uint8Array([1]));
      return [await code(store.rename('app:/data/file', 'app:/data/folder')),
        await code(store.rename('app:/data/folder', 'app:/data/file'))];
    } finally { store.close(); }
  });
  expect(codes).toEqual(['EISDIR', 'ENOTDIR']);
}, 60_000);
