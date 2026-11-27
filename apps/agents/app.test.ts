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

test('deleting a streaming conversation cancels late turns and suppresses racing titles', async () => {
  const source = await import('./app.ts?case=delete-streaming');
  const storage = { fs: { readFile: async () => { throw Object.assign(new Error('ENOENT'), { code: 'ENOENT' }); }, atomicWriteFile: async () => {} } };
  const call = (name: string, ...args: unknown[]) => (source.answer as any)(name, args, {}, storage);

  // Case 1: delete while POST /turns is in flight cancels the late job.
  let acceptPost!: (r: Response) => void, onPost!: () => void;
  let acceptPoll!: (r: Response) => void, onPoll!: () => void;
  const requests: string[] = [];
  let postJob = 'late-delete';
  let holdPost = true;
  globalThis.fetch = (async (url: any, init: any) => {
    const method = init?.method ?? 'GET';
    const path = String(url).replace(source.SERVICE, '');
    requests.push(`${method} ${path}`);
    if (method === 'POST' && path === '/turns') {
      if (holdPost) {
        onPost();
        return new Promise<Response>(resolve => { acceptPost = resolve; });
      }
      return Response.json({ id: postJob });
    }
    if (method === 'GET' && path === '/turns/j2') {
      onPoll();
      return new Promise<Response>(resolve => { acceptPoll = resolve; });
    }
    return Response.json({ connected: true });
  }) as typeof fetch;

  const posted = new Promise<void>(resolve => { onPost = resolve; });
  const { id: id1 } = await call('command', 'new', '', source.FEATURED[0]!.id, 0);
  const send1 = call('command', 'send', id1, 'Hello', 100);
  await posted;
  await call('command', 'delete', id1, '', 200);
  acceptPost(Response.json({ id: 'late-delete' }));
  await send1;
  expect(requests).toEqual(['POST /turns', 'DELETE /turns/late-delete']);

  // Case 2: delete while pulse() is awaiting GET /turns/j2 does not call POST /title.
  requests.length = 0;
  holdPost = false;
  postJob = 'j2';
  const polled = new Promise<void>(resolve => { onPoll = resolve; });
  const { id: id2 } = await call('command', 'new', '', source.FEATURED[0]!.id, 300);
  await call('command', 'send', id2, 'Second chat', 400);
  const pulsing = call('pulse', 500);
  await polled;
  await call('command', 'delete', id2, '', 600);
  acceptPoll(Response.json({ done: true, content: 'Partial', reasoning: '', thinkingMs: 0, error: '' }));
  await pulsing;
  expect(requests).not.toContain('POST /title');
  expect(requests).toContain('DELETE /turns/j2');

  // Case 3: delete while pulseOnDevice() is awaiting languageModelResponse does not title the chat.
  let createdSessions = 0, promptCalls = 0, aborted: string[] = [];
  let onDevicePoll!: () => void, resolveDevicePoll!: (r: any) => void;
  const devicePolled = new Promise<void>(resolve => { onDevicePoll = resolve; });
  device = {
    languageModelAvailability: async () => ({ tools: false }),
    createLanguageModel: async () => ({ id: `s${++createdSessions}`, overflowed: false }),
    destroyLanguageModel: async () => ({}),
    promptStreaming: async () => ({ id: 'resp-1' }),
    abortLanguageModelResponse: async ({ response }: { response: string }) => { aborted.push(response); return {}; },
    languageModelResponse: async () => {
      onDevicePoll();
      return new Promise(resolve => { resolveDevicePoll = resolve; });
    },
    prompt: async () => { promptCalls++; return { text: JSON.stringify({ title: 'Should not run' }) }; },
  };
  const { id: id3 } = await call('command', 'new', '', 'device|none|', 700);
  await call('command', 'send', id3, 'On-device prompt', 800);
  const devicePulsing = call('pulse', 900);
  await devicePolled;
  await call('command', 'delete', id3, '', 1000);
  resolveDevicePoll({ state: 'completed', text: 'Done', toolCalls: [], overflowed: false });
  await devicePulsing;
  expect(createdSessions).toBe(1);
  expect(promptCalls).toBe(0);
  expect(aborted).toEqual(['resp-1']);
});

test('concurrent initial load calls do not clobber newly created conversations or reset serial', async () => {
  const source = await import('./app.ts?case=concurrent-load');
  const initial = JSON.stringify({
    version: 1,
    serial: 5,
    conversations: [{
      id: 'c1', title: 'Earlier', titled: true, model: source.FEATURED[0]!.id,
      modelName: source.FEATURED[0]!.name, maker: source.FEATURED[0]!.maker,
      effort: 'medium', created: 1, updated: 1, messages: [], job: '', streaming: false, reply: '',
    }],
  });
  let file = initial;
  const pendingReads: Array<() => void> = [];
  const storage = { fs: {
    readFile: () => new Promise<ArrayBuffer>(resolve => {
      const snapshotBytes = new TextEncoder().encode(file).buffer;
      pendingReads.push(() => resolve(snapshotBytes));
    }),
    atomicWriteFile: async (_path: string, bytes: Uint8Array) => {
      file = new TextDecoder().decode(bytes);
    },
  } };
  const call = (name: string, ...args: unknown[]) => (source.answer as any)(name, args, {}, storage);

  const snap1 = call('snapshot', '', '', 0, 100, 0);
  const create = call('command', 'new', '', source.FEATURED[0]!.id, 200);
  const snap2 = call('snapshot', '', '', 0, 300, 0);
  expect(pendingReads.length).toBe(3);

  pendingReads.shift()!();
  await snap1;
  pendingReads.shift()!();
  const created = await create;
  expect(created.id).toBe('c6');
  pendingReads.shift()!();
  await snap2;

  const after = await call('snapshot', 'c6', '', 0, 400, 0);
  expect(after.found).toBe(true);
  expect(after.rows.map((r: any) => r.id)).toEqual(['c6', 'c1']);

  const next = await call('command', 'new', '', source.FEATURED[0]!.id, 500);
  expect(next.id).toBe('c7');
  expect(JSON.parse(file).serial).toBe(7);
});
