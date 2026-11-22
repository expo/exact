import { expect, test } from 'bun:test';
import { serveJobs } from './gpu-jobs-worker.js';

function setup(run, initialize = () => ({ memory: { buffer: new ArrayBuffer(65536) } })) {
  let time = 0;
  const messages = [], imports = [];
  const scope = {
    postMessage: (message, transfers) => messages.push({ message, transfers }),
    close: () => { scope.closed = true; },
    performance: { now: () => ++time },
  };
  const handle = serveJobs(scope, async glue => {
    imports.push(glue);
    return {
      default: initialize,
      exact_game_worker_names: () => ['mesh'],
      exact_game_worker_run: run,
    };
  });
  const send = data => handle({ data });
  return { scope, messages, imports, send };
}

const module = {};
const glue = 'https://example.test/gpu.js';
const init = { init: true, version: 1, module, glue };
const job = low => ({ attempt: [low, 4], name: 'mesh', input: new Uint8Array([low]) });
const run = (...jobs) => ({ run: true, jobs });

test('initializes the supplied module once and transfers each tagged answer in order', async () => {
  const initialized = [];
  const { send, messages, imports } = setup((name, input, mark) => {
    expect(name).toBe('mesh');
    mark(true);
    mark(false);
    return Uint8Array.of(0, input[0]);
  }, options => {
    initialized.push(options);
    return { memory: { buffer: new ArrayBuffer(65536) } };
  });
  await send(init);
  expect(imports).toEqual([glue]);
  expect(initialized).toEqual([{ module_or_path: module }]);
  await send(run(job(1), job(2)));
  expect(messages[0]).toEqual({ message: { ready: true, version: 1, names: ['mesh'] }, transfers: undefined });
  for (const low of [1, 2]) {
    const { message, transfers } = messages[low];
    expect(message).toEqual({ done: true, attempt: [low, 4], output: Uint8Array.of(0, low), ms: 1, memory: 65536 });
    expect(transfers).toHaveLength(1);
    expect(transfers[0]).toBe(message.output.buffer);
  }
  await expect(send(init)).rejects.toThrow();
  expect(imports).toEqual([glue]);
});

test('a trap inside run answers for that job and closes without running the rest', async () => {
  const calls = [];
  const { send, messages, scope } = setup((name, input, mark) => {
    calls.push(input[0]);
    mark(true);
    throw new WebAssembly.RuntimeError('unreachable');
  });
  await send(init);
  await send(run(job(1), job(2)));
  expect(calls).toEqual([1]);
  expect(messages[1]).toEqual({ message: { trapped: true, attempt: [1, 4] }, transfers: undefined });
  expect(scope.closed).toBe(true);
});

test.each([
  ['decoding', () => { throw new WebAssembly.RuntimeError('decode'); }, 'decode'],
  ['encoding', mark => { mark(true); mark(false); throw new WebAssembly.RuntimeError('encode'); }, 'encode'],
  ['an unexpected job error', mark => { mark(true); throw new Error('job error'); }, 'job error'],
])('the worker owner receives an error for %s', async (_stage, fail, message) => {
  const source = await Bun.file(new URL('./gpu-jobs-worker.js', import.meta.url)).text();
  const scheduled = [], errors = [], messages = [];
  class WorkerGlobalScope {
    performance = { now: () => 0 };
    postMessage(event) { messages.push(event); }
  }
  const scope = new WorkerGlobalScope();
  const owner = { onerror: event => errors.push(event.error) };
  const importGlue = async () => ({
    default: () => ({ memory: new WebAssembly.Memory({ initial: 1 }) }),
    exact_game_worker_names: () => ['mesh'],
    exact_game_worker_run: (_name, _input, mark) => fail(mark),
  });
  new Function('globalThis', 'WorkerGlobalScope', 'setTimeout', 'loadGlue',
    source.replace('export function serveJobs', 'function serveJobs').replace('url => import(url)', 'loadGlue'))(
    scope, WorkerGlobalScope, callback => scheduled.push(callback), importGlue);

  scope.onmessage({ data: init });
  await Promise.resolve();
  await Promise.resolve();
  expect(messages).toEqual([{ ready: true, version: 1, names: ['mesh'] }]);
  scope.onmessage({ data: run(job(1)) });
  await Promise.resolve();
  for (const callback of scheduled) {
    try { callback(); }
    catch (error) { owner.onerror({ error }); }
  }
  expect(errors.map(error => error.message)).toEqual([message]);
});
