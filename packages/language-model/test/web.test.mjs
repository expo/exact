// The browser path over a Prompt API stand-in (LLP 1133.016 D6, D9).
import { afterEach, expect, test } from 'bun:test';
import { implementation } from '../web/index.js';

const implementations = [];
afterEach(() => { for (const impl of implementations.splice(0)) impl.destroy(); });

function browser(promptStreaming, invoke = async () => ({ text: 'null' }), beforeCreate = () => {}) {
  const created = [], prompts = [], destroyed = [];
  const previous = Object.getOwnPropertyDescriptor(globalThis, 'LanguageModel');
  globalThis.LanguageModel = class {
    static async create(options) {
      const id = created.length;
      created.push(options);
      await beforeCreate(id);
      return {
        contextWindow: 4096,
        promptStreaming(input, options) { prompts.push({ id, input }); return promptStreaming(input, options); },
        destroy() { destroyed.push(id); },
      };
    }
  };
  let impl;
  try {
    impl = implementation({ agent: false, now: () => 0, foreground: () => true, changed() {}, invoke });
  } finally {
    if (previous) Object.defineProperty(globalThis, 'LanguageModel', previous);
    else delete globalThis.LanguageModel;
  }
  implementations.push(impl);
  return { impl, created, prompts, destroyed };
}

for (const partial of ['', 'A partial reply.']) {
  test(`an app abort rebuilds the browser session with its input and ${partial ? 'partial' : 'empty'} reply`, async () => {
    const started = Promise.withResolvers();
    const { impl, created, prompts, destroyed } = browser(async function* (input, { signal }) {
      if (input !== 'A question') { yield 'Next reply.'; return; }
      if (partial) yield partial;
      const stopped = new Promise((_, reject) => {
        signal.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError')), { once: true });
      });
      started.resolve();
      await stopped;
    });
    const initialPrompts = [
      { role: 'system', content: 'Be brief.' },
      { role: 'user', content: 'An earlier question' },
      { role: 'assistant', content: 'An earlier answer.' },
    ];
    const { id: session } = await impl.createLanguageModel({ initialPrompts }, { origin: 'app' });
    const { id: response } = await impl.promptStreaming({ session, input: 'A question' });
    await started.promise;
    expect(await impl.languageModelResponse({ response })).toMatchObject({ state: 'generating', text: partial });

    // The next prompt is already queued when abort frees the session.
    const next = impl.prompt({ session, input: 'Follow up' });
    await impl.abortLanguageModelResponse({ response });
    expect(await next).toMatchObject({ text: 'Next reply.' });
    expect(created).toHaveLength(2);
    expect(created[1].initialPrompts).toEqual([
      ...initialPrompts,
      { role: 'user', content: 'A question' },
      { role: 'assistant', content: partial },
    ]);
    expect(destroyed).toEqual([0]);
    expect(prompts).toEqual([{ id: 0, input: 'A question' }, { id: 1, input: 'Follow up' }]);
    expect(await impl.languageModelResponse({ response })).toMatchObject({ state: 'aborted', text: partial });

    await impl.prompt({ session, input: 'After completion' });
    expect(created).toHaveLength(2);
    expect(prompts.at(-1)).toEqual({ id: 1, input: 'After completion' });
  });
}

for (const stop of ['destroy-session', 'dispose', 'destroy-app']) {
  test(`${stop} settles queued prompts, appends and clones without rebuilding the browser session`, async () => {
    const started = Promise.withResolvers();
    const { impl, created, prompts, destroyed } = browser(async function* (_, { signal }) {
      yield 'A partial reply.';
      const stopped = new Promise((_, reject) => {
        signal.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError')), { once: true });
      });
      started.resolve();
      await stopped;
    });
    const { id: session } = await impl.createLanguageModel({}, { origin: 'app' });
    const { id: response } = await impl.promptStreaming({ session, input: 'A question' });
    await started.promise;
    const queued = impl.prompt({ session, input: 'Follow up' });
    const settled = [];
    const appended = impl.appendLanguageModel({ session, input: 'An appended question' });
    const cloned = impl.cloneLanguageModel({ session });
    appended.then(value => settled.push(['append', value]));
    cloned.then(value => settled.push(['clone', value]));

    if (stop === 'destroy-session') await impl.destroyLanguageModel({ session });
    else if (stop === 'dispose') impl.dispose([session]);
    else impl.destroy();
    expect(await queued).toEqual({ error: 'AbortError' });
    // Drain the stream's cancellation and any browser creation already started during teardown.
    await new Promise(setImmediate);

    expect(settled).toEqual([
      ['append', { error: 'AbortError' }],
      ['clone', { error: 'AbortError' }],
    ]);
    expect(created).toHaveLength(1);
    expect(destroyed).toEqual([0]);
    expect(prompts).toEqual([{ id: 0, input: 'A question' }]);
    expect(await impl.languageModelResponse({ response })).toEqual({ error: 'InvalidStateError' });
    expect(impl.state()).toMatchObject({ sessions: [], responses: [] });
  });
}

test('a superseded browser rebuild cannot replace the session that completed a newer reply', async () => {
  const started = Promise.withResolvers(), creating = Promise.withResolvers(), release = Promise.withResolvers();
  const { impl, created, prompts, destroyed } = browser(async function* (input, { signal }) {
    if (input !== 'A') { yield `${input} reply.`; return; }
    yield 'A partial reply.';
    const stopped = new Promise((_, reject) => {
      signal.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError')), { once: true });
    });
    started.resolve();
    await stopped;
  }, undefined, id => {
    if (id === 1) { creating.resolve(); return release.promise; }
  });
  const { id: session } = await impl.createLanguageModel({}, { origin: 'app' });
  const { id: a } = await impl.promptStreaming({ session, input: 'A' });
  await started.promise;
  await impl.abortLanguageModelResponse({ response: a });
  const { id: b } = await impl.promptStreaming({ session, input: 'B' });
  await creating.promise;
  await impl.abortLanguageModelResponse({ response: b });
  expect(await impl.prompt({ session, input: 'C' })).toMatchObject({ text: 'C reply.' });

  release.resolve();
  // Join the held creation and its continuations before checking which cache survived.
  await new Promise(setImmediate);
  expect(await impl.prompt({ session, input: 'D' })).toMatchObject({ text: 'D reply.' });
  expect(prompts).toEqual([{ id: 0, input: 'A' }, { id: 2, input: 'C' }, { id: 2, input: 'D' }]);
  expect(created).toHaveLength(3);
  expect(created[2].initialPrompts).toEqual([
    { role: 'user', content: 'A' },
    { role: 'assistant', content: 'A partial reply.' },
  ]);
  expect(destroyed).toEqual([0, 1]);
  expect(await impl.languageModelResponse({ response: b })).toMatchObject({ state: 'aborted' });
});

for (const stop of [false, true]) {
  test(`a clone's pending prewarm ${stop ? 'is destroyed with its owner' : 'can serve its first prompt'}`, async () => {
    const creating = Promise.withResolvers(), release = Promise.withResolvers();
    const { impl, created, prompts, destroyed } = browser(async function* () { yield 'A reply.'; }, undefined, id => {
      if (id === 1) { creating.resolve(); return release.promise; }
    });
    const initialPrompts = [{ role: 'user', content: 'Earlier input' }];
    const source = await impl.createLanguageModel({ initialPrompts }, { origin: 'app' });
    const { id: session } = await impl.cloneLanguageModel({ session: source.id });
    const prewarm = impl.prewarmLanguageModel({ session });
    await creating.promise;
    let reply;
    if (stop) await impl.destroyLanguageModel({ session });
    else reply = impl.prompt({ session, input: 'A question' });
    release.resolve();

    expect(await prewarm).toEqual(stop ? { error: 'AbortError' } : { id: session });
    if (reply) expect(await reply).toMatchObject({ text: 'A reply.' });
    expect(created).toHaveLength(2);
    expect(created[1].initialPrompts).toEqual(initialPrompts);
    expect(prompts).toEqual(stop ? [] : [{ id: 1, input: 'A question' }]);
    expect(destroyed).toEqual(stop ? [1] : []);
    expect(impl.state().sessions.map(s => s.id)).toEqual(stop ? [source.id] : [source.id, session]);
  });
}

test('an append invalidates a pending prewarm built without the appended input', async () => {
  const creating = Promise.withResolvers(), release = Promise.withResolvers();
  const { impl, created, prompts, destroyed } = browser(async function* () { yield 'A reply.'; }, undefined, id => {
    if (id === 1) { creating.resolve(); return release.promise; }
  });
  const source = await impl.createLanguageModel({}, { origin: 'app' });
  const { id: session } = await impl.cloneLanguageModel({ session: source.id });
  const prewarm = impl.prewarmLanguageModel({ session });
  await creating.promise;
  expect(await impl.appendLanguageModel({ session, input: 'Remember this' })).toMatchObject({ id: session });
  release.resolve();
  expect(await prewarm).toEqual({ error: 'AbortError' });

  expect(await impl.prompt({ session, input: 'A question' })).toMatchObject({ text: 'A reply.' });
  expect(created).toHaveLength(3);
  expect(created[2].initialPrompts).toEqual([{ role: 'user', content: 'Remember this' }]);
  expect(prompts).toEqual([{ id: 2, input: 'A question' }]);
  expect(destroyed).toEqual([1]);
});

for (const stop of ['destroy-session', 'dispose', 'destroy-app']) {
  test(`${stop} destroys a browser replacement that finishes after teardown`, async () => {
    const started = Promise.withResolvers(), creating = Promise.withResolvers(), release = Promise.withResolvers();
    const { impl, created, prompts, destroyed } = browser(async function* (_, { signal }) {
      const stopped = new Promise((_, reject) => {
        signal.addEventListener('abort', () => reject(new DOMException('aborted', 'AbortError')), { once: true });
      });
      started.resolve();
      await stopped;
    }, undefined, id => {
      if (id === 1) { creating.resolve(); return release.promise; }
    });
    const { id: session } = await impl.createLanguageModel({}, { origin: 'app' });
    const { id: response } = await impl.promptStreaming({ session, input: 'A question' });
    await started.promise;
    const pending = impl.prompt({ session, input: 'Follow up' });
    await impl.abortLanguageModelResponse({ response });
    await creating.promise;

    if (stop === 'destroy-session') await impl.destroyLanguageModel({ session });
    else if (stop === 'dispose') impl.dispose([session]);
    else impl.destroy();
    expect(await pending).toEqual({ error: 'AbortError' });
    release.resolve();
    await new Promise(setImmediate);

    expect(created).toHaveLength(2);
    expect(destroyed).toEqual([0, 1]);
    expect(prompts).toEqual([{ id: 0, input: 'A question' }]);
    expect(impl.state()).toMatchObject({ sessions: [], responses: [] });
  });
}

const tools = [{ name: 'charge', description: 'Record a charge', inputSchema: {
  type: 'object', properties: { n: { type: 'integer' } }, required: ['n'],
} }];
const calls = [1, 2].map(n => ({ type: 'tool-call', value: { callID: `c${n}`, name: 'charge', arguments: { n } } }));

for (const phase of ['buffered', 'stream-end']) {
  for (const stop of ['abort', 'cancel', 'dispose', 'destroy-session', 'destroy-app']) {
    test(`${stop} with ${phase} browser output prevents tool calls and preserves the ended response`, async () => {
      const stopped = Promise.withResolvers(), invoked = [];
      const stopResponse = async () => {
        const response = impl.state().responses[0].id;
        if (stop === 'abort') await impl.abortLanguageModelResponse({ response });
        else if (stop === 'cancel') impl.cancel(7);
        else if (stop === 'dispose') impl.dispose([response]);
        else if (stop === 'destroy-session') await impl.destroyLanguageModel({ session });
        else impl.destroy();
        stopped.resolve({ response, ended: await impl.languageModelResponse({ response }) });
      };
      const { impl, prompts } = browser(input => {
        if (typeof input !== 'string') return (async function* () { yield 'Done.'; })();
        if (phase === 'stream-end') return (async function* () {
          yield 'A partial reply.';
          yield calls;
          await stopResponse();
        })();
        const stream = new ReadableStream({ start(controller) {
          controller.enqueue('Buffered text.');
          controller.enqueue(calls);
          controller.close();
        } });
        void stopResponse();
        return stream;
      }, async (_, name, args) => {
        invoked.push({ name, args });
        return { text: 'null' };
      });
      const { id: session } = await impl.createLanguageModel({ tools }, { origin: 'app' });
      const pending = stop === 'cancel'
        ? impl.prompt({ session, input: 'Go' }, { request: 7 })
        : impl.promptStreaming({ session, input: 'Go' });
      const { response, ended } = await stopped.promise;
      expect(ended).toMatchObject(stop.startsWith('destroy') ? { error: 'InvalidStateError' } : {
        state: 'aborted', text: phase === 'buffered' ? '' : 'A partial reply.', toolCalls: [],
      });
      // Drain the closed stream and the tool loop before checking what did not run.
      await new Promise(setImmediate);
      expect(invoked).toEqual([]);
      expect(await impl.languageModelResponse({ response })).toEqual(ended);
      expect(prompts).toEqual([{ id: 0, input: 'Go' }]);
      if (stop === 'cancel') expect(await pending).toEqual({ error: 'AbortError' });
      else await pending;
    });
  }
}

for (const stop of ['abort', 'cancel', 'dispose', 'destroy-session', 'destroy-app']) {
  test(`${stop} during a tool handler prevents the remaining browser tool calls`, async () => {
    const entered = Promise.withResolvers(), handler = Promise.withResolvers(), invoked = [];
    const { impl, prompts } = browser(async function* () { yield calls; }, (handle, name, args) => {
      invoked.push({ handle, name, args });
      entered.resolve();
      return handler.promise;
    });
    const { id: session } = await impl.createLanguageModel({ tools }, { origin: 'app' });
    const pending = stop === 'cancel'
      ? impl.prompt({ session, input: 'Go' }, { request: 7 })
      : impl.promptStreaming({ session, input: 'Go' });
    await entered.promise;
    const response = impl.state().responses[0].id;
    const calling = await impl.languageModelResponse({ response });
    expect(calling.state).toBe('calling-tool');
    expect(calling.toolCalls.map(c => c.id)).toEqual(['c1']);

    if (stop === 'abort') await impl.abortLanguageModelResponse({ response });
    else if (stop === 'cancel') impl.cancel(7);
    else if (stop === 'dispose') impl.dispose([response]);
    else if (stop === 'destroy-session') await impl.destroyLanguageModel({ session });
    else impl.destroy();
    const ended = await impl.languageModelResponse({ response });
    expect(ended).toMatchObject(stop.startsWith('destroy') ? { error: 'InvalidStateError' } : { state: 'aborted' });
    handler.resolve({ text: '{"ok":true}' });
    // Let the resumed tool loop finish its promise continuations before checking what did not run.
    await new Promise(setImmediate);

    expect(invoked).toEqual([{ handle: stop === 'cancel' ? session : response, name: 'charge', args: { n: 1 } }]);
    expect(await impl.languageModelResponse({ response })).toEqual(ended);
    expect(prompts).toEqual([{ id: 0, input: 'Go' }]);
    if (stop === 'cancel') expect(await pending).toEqual({ error: 'AbortError' });
    else await pending;
  });
}

test('an uninterrupted browser reply runs every tool and prompts with their results', async () => {
  const invoked = [];
  const { impl, prompts, created } = browser(async function* (input) {
    if (typeof input === 'string') yield calls;
    else yield 'Done.';
  }, async (_, name, args) => {
    invoked.push({ name, args });
    return { text: JSON.stringify(args.n) };
  });
  const { id: session } = await impl.createLanguageModel({ tools }, { origin: 'app' });
  expect(await impl.prompt({ session, input: 'Go' })).toMatchObject({ text: 'Done.' });
  expect(invoked).toEqual([1, 2].map(n => ({ name: 'charge', args: { n } })));
  expect(prompts[1].input).toEqual([{ role: 'user', content: [1, 2].map(n => ({
    type: 'tool-response', value: { callID: `c${n}`, name: 'charge', result: [{ type: 'text', value: `${n}` }], error: undefined },
  })) }]);
  expect(created).toHaveLength(1);
});
