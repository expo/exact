import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';

for (const storage of ['available', 'absent', 'refused', 'unprepared']) {
  test(storage === 'available' ? 'picked files follow equal-size writes and deletion with a fixed storage clock'
    : storage === 'unprepared' ? 'picked files remain viewable when their storage directory cannot be created'
    : `picked files remain viewable when app storage is ${storage}`, async () => {
    const server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
      const path = new URL(request.url).pathname;
      if (path.endsWith('.js')) return new Response(readFileSync(resolve(import.meta.dirname, path.slice(1))),
        { headers: { 'content-type': 'text/javascript' } });
      return new Response('<!doctype html><input type="file">', { headers: { 'content-type': 'text/html' } });
    } });
    let browser;
    try {
      browser = await suiteBrowser();
      const page = await browser.newPage();
      await page.goto(`${server.url.href}?agent=1${storage !== 'absent' ? '&storage=picked' : ''}`);
      await page.setInputFiles('input', { name: 'notes.txt', mimeType: 'text/plain', buffer: Buffer.from('original') });
      const result = await page.evaluate(async storage => {
        Date.now = () => 1000;
        let directoryRefused = false;
        if (storage === 'unprepared') {
          const put = IDBObjectStore.prototype.put;
          IDBObjectStore.prototype.put = function (value, ...args) {
            if (value.path === 'app:/tmp/picked' && value.kind === 'directory') {
              directoryRefused = true;
              throw new DOMException('Storage quota exceeded', 'QuotaExceededError');
            }
            return put.call(this, value, ...args);
          };
        }
        if (storage === 'refused') indexedDB.open = () => {
          const request = {};
          queueMicrotask(() => {
            request.error = new DOMException('Storage is disabled', 'SecurityError');
            request.onerror();
          });
          return request;
        };
        globalThis.exact = {};
        await import('/picker-glue.js');
        const appId = 'picker-url-test', path = 'app:/tmp/picked/notes.txt';
        const sent = [];
        const picker = exact.picker({ appId, pickedPath: name => `app:/tmp/picked/${name}`,
          dispatch: (...args) => sent.push(args), log() {} });
        await picker.change(document.querySelector('input'), 7);
        async function readers() {
          const frame = document.createElement('iframe');
          let error = false;
          frame.addEventListener('error', () => { error = true; });
          const state = { path, live: true };
          await picker.appFrame(frame, state);
          const direct = error ? '' : await (await fetch(frame.src)).text();
          if (state.url) URL.revokeObjectURL(state.url);
          const given = { path: `/assets/view.html?file=${encodeURIComponent(path)}`, live: true, dispose() {} };
          await picker.appFrame(frame, given, () => {}, src => frame.setAttribute('src', src));
          const url = new URL(frame.src).searchParams.get('file');
          const page = url ? await (await fetch(url)).text() : '';
          given.dispose();
          let shared = '';
          Object.defineProperties(navigator, {
            canShare: { configurable: true, value: () => true },
            share: { configurable: true, value: async ({ files }) => { shared = await files[0].text(); } },
          });
          const share = await picker.share({}, [{ path }]);
          return { direct, page, shared, share: share.startsWith('share: refused:') ? 'refused' : share };
        }
        const original = await picker.appURL(path);
        const result = {
          sent,
          original: original ? await (await fetch(original)).text() : '',
          cached: await picker.appURL(path) === original,
          missing: await picker.appURL('app:/tmp/picked/missing.txt'),
          readers: await readers(),
        };
        if (storage !== 'available' && storage !== 'unprepared') return result;
        const { storageKey } = await import('/storage-environment.js');
        const { createFileStore } = await import('/storage-fs.js');
        const store = createFileStore(storageKey(appId));
        try {
          if (storage === 'unprepared') {
            const other = 'app:/data/kept.txt';
            await store.writeFile(other, new TextEncoder().encode('stored'));
            const url = await picker.appURL(other);
            result.directoryRefused = directoryRefused;
            result.otherFile = await (await fetch(url)).text();
            return result;
          }
          const before = await store.stat(path);
          await store.writeFile(path, new TextEncoder().encode('replaced'));
          const updated = await exact.appURL(path, appId);
          Object.assign(result, {
            sameMetadata: before.modifiedMs === (await store.stat(path)).modifiedMs && before.size === (await store.stat(path)).size,
            stored: new TextDecoder().decode(await store.readFile(path)),
            updated: await (await fetch(updated)).text(),
            cachedUpdate: await picker.appURL(path) === updated,
            originalRevoked: await fetch(original).then(() => false, () => true),
            updatedReaders: await readers(),
          });
          // Recreate without an intervening URL lookup, so deletion cannot clear the cache.
          await store.rm(path);
          await store.putBlob(path, new File(['recreate'], 'notes.txt', { type: 'text/plain' }));
          const recreated = await picker.appURL(path);
          result.recreated = await (await fetch(recreated)).text();
          result.updatedRevoked = await fetch(updated).then(() => false, () => true);
          await store.rm(path);
          result.deleted = await picker.appURL(path);
          result.recreatedRevoked = await fetch(recreated).then(() => false, () => true);
          result.deletedReaders = await readers();
          return result;
        } finally { store.close(); }
      }, storage);
      expect(result).toEqual({
        sent: [[7, 26, 'app:/tmp/picked/notes.txt\tnotes.txt\ttext/plain\t8\t\t\t']],
        original: 'original', cached: true, missing: '',
        readers: { direct: 'original', page: 'original', shared: 'original', share: 'share: shared' },
        ...(storage === 'unprepared' ? { directoryRefused: true, otherFile: 'stored' } : {}),
        ...(storage === 'available' ? { sameMetadata: true, stored: 'replaced', updated: 'replaced', cachedUpdate: true,
          originalRevoked: true, recreated: 'recreate', updatedRevoked: true, deleted: '', recreatedRevoked: true,
          updatedReaders: { direct: 'replaced', page: 'replaced', shared: 'replaced', share: 'share: shared' },
          deletedReaders: { direct: '', page: '', shared: '', share: 'refused' } } : {}),
      });
    } finally { await browser?.close(); server.stop(true); }
  }, 60_000);
}
