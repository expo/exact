import { afterEach, expect, mock, test } from 'bun:test';

// The device's language model, as a test supplies it; a call it supplies nothing for fails.
let device: Record<string, (...args: any[]) => unknown> = {};
mock.module('@exact/language-model', () => Object.fromEntries([
  'abortLanguageModelResponse', 'createLanguageModel', 'destroyLanguageModel', 'languageModelAvailability', 'languageModelResponse', 'prompt', 'promptStreaming',
].map(name => [name, (...args: any[]) => {
  const call = device[name];
  if (!call) throw new Error(`unexpected device call: ${name}`);
  return call(...args);
}])));
const realFetch = globalThis.fetch;
afterEach(() => { globalThis.fetch = realFetch; device = {}; });

test('stopping while the service accepts a turn drops its blank row and cancels the late job', async () => {
  const source = await import('./app.ts');
  const storage = { fs: { readFile: async () => { throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' }); }, atomicWriteFile: async () => {} } };
  const call = (name: string, ...args: unknown[]) => (source.answer as any)(name, args, {}, storage);
  let accept!: (r: Response) => void, started!: () => void;
  const pending = new Promise<Response>(resolve => { accept = resolve; });
  const posted = new Promise<void>(resolve => { started = resolve; });
  const deleted: string[] = [];
  globalThis.fetch = (async (url: any, init: any) => {
    if (init?.method === 'POST') { started(); return pending; }
    if (init?.method === 'DELETE') { deleted.push(String(url)); return Response.json({}); }
    return Response.json({ connected: true });
  }) as typeof fetch;
  const { id } = await call('command', 'new', '', source.FEATURED[0]!.id, 0);
  const send = call('command', 'send', id, 'Hello', 100);
  await posted;
  await call('command', 'stop', id, '', 200);
  accept(Response.json({ id: 'late' }));
  await send;
  const state = await call('snapshot', id, '', 0, 300, 0);
  expect(state.streaming).toBe(false);
  expect(state.messages.map((m: any) => m.body)).toEqual(['Hello']);
  expect(deleted).toEqual([`${source.SERVICE}/turns/late`]);
});

// Only a missing file is a first launch. Any other failed read saves nothing,
// so an empty list never replaces the stored conversations.
test('a failed read is not a first launch, and no command saves over the stored conversations', async () => {
  const source = await import('./app.ts?case=failed-read');
  let writes = 0;
  const storage = { fs: {
    readFile: async () => { throw Object.assign(new Error('failed filesystem: Permission denied (os error 13)'), { code: 'failed' }); },
    atomicWriteFile: async () => { writes++; },
  } };
  const call = (name: string, ...args: unknown[]) => (source.answer as any)(name, args, {}, storage);
  await expect(call('command', 'new', '', source.FEATURED[0]!.id, 0)).rejects.toThrow('Permission denied');
  await expect(call('snapshot', '', '', 0, 100, 0)).rejects.toThrow('Permission denied');
  expect(writes).toBe(0);
});

// The module keeps a session alive until the app destroys it, so the app must destroy every one it lets go.
test('changing the model, retrying and deleting release the on-device sessions', async () => {
  const source = await import('./app.ts?case=sessions');
  const storage = { fs: { readFile: async () => { throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' }); }, atomicWriteFile: async () => {} } };
  const call = (name: string, ...args: unknown[]) => (source.answer as any)(name, args, {}, storage);
  const live = new Set<string>();
  let made = 0, hold: Promise<void> | undefined;
  device = {
    languageModelAvailability: async () => ({ tools: false }),
    createLanguageModel: async () => { await hold; const id = `s${++made}`; live.add(id); return { id, overflowed: false }; },
    destroyLanguageModel: async ({ session }: { session: string }) => { live.delete(session); return {}; },
    promptStreaming: async () => { throw Object.assign(new Error('busy'), { code: 'UnknownError' }); },
  };
  const create = async () => (device.createLanguageModel!() as Promise<{ id: string }>).then(m => m.id);

  // A conversation with its download's session, moved to a hosted model, then deleted.
  for (let i = 0; i < 3; i++) {
    const { id } = await call('command', 'new', '', `device|none|${await create()}`, 0);
    await call('command', 'model', id, source.FEATURED[0]!.id, 10);
    await call('command', 'delete', id, '', 20);
  }
  expect([...live]).toEqual([]);

  // A failed reply retried: the retry makes a session from the history and the old one goes.
  const { id } = await call('command', 'new', '', `device|none|${await create()}`, 0);
  await call('command', 'send', id, 'Hello', 10);
  await call('command', 'retry', id, '', 20);
  expect(live.size).toBe(1);
  await call('command', 'delete', id, '', 30);
  expect([...live]).toEqual([]);

  // A session that finishes being made after its conversation was deleted.
  let resume!: () => void;
  hold = new Promise(resolve => { resume = resolve; });
  const late = (await call('command', 'new', '', 'device|none|', 0)).id;
  const send = call('command', 'send', late, 'Hello', 10);
  await call('command', 'delete', late, '', 20);
  resume();
  await send;
  expect([...live]).toEqual([]);
});
