// The list-detail starter's data module against a storage whose writes can fail: memory follows what is stored.
import { expect, test } from 'bun:test';

test('the list-detail starter keeps no write that failed, and its mutations do not overwrite each other', async () => {
  const { answer } = await import('../docs/patterns/list-detail/app.ts');
  let stored = null, fail = false;
  const storage = { fs: {
    readFile: async () => {
      if (stored === null) throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' });
      return new TextEncoder().encode(stored);
    },
    atomicWriteFile: async (_path, bytes) => {
      await Promise.resolve();
      if (fail) { fail = false; throw Object.assign(new Error('ENOSPC'), { code: 'ENOSPC' }); }
      stored = new TextDecoder().decode(bytes);
    },
  } };
  const call = (source, ...args) => answer(source, args, null, storage);
  const titles = async () => (await call('loadItems')).map((i) => i.title);

  fail = true;
  await expect(call('addItem', 'Alpha', 'notes')).rejects.toMatchObject({ code: 'ENOSPC' });
  expect(await titles()).toEqual([]);
  expect(await call('addItem', 'Alpha', 'notes')).toEqual({ ok: true, error: '' });
  fail = true;
  await expect(call('removeItem', '1')).rejects.toMatchObject({ code: 'ENOSPC' });
  expect(await titles()).toEqual(['Alpha']);
  const both = await Promise.all([call('addItem', 'Beta', ''), call('addItem', 'Gamma', '')]);
  expect(both).toEqual([{ ok: true, error: '' }, { ok: true, error: '' }]);
  expect(JSON.parse(stored).map((i) => [i.id, i.title])).toEqual([['1', 'Alpha'], ['2', 'Beta'], ['3', 'Gamma']]);
});
