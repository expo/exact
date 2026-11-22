import { expect, test } from 'bun:test';
import { createTransport } from './gpu-jobs.js';

const names = ['mesh', 'compare'];
const glue = 'https://example.test/gpu.js';
const module = new WebAssembly.Module(new Uint8Array([0, 97, 115, 109, 1, 0, 0, 0]));

function setup(cores = 3) {
  let now = 0;
  const workers = [], warnings = [], timers = new Map();
  let timer = 0;
  function clock(time, runTimers = false) {
    now = time;
    if (!runTimers) return;
    for (const [id, { at, callback }] of timers) {
      if (at > now) continue;
      timers.delete(id);
      callback();
    }
  }
  class Worker {
    constructor(url, options) {
      this.url = url;
      this.options = options;
      this.messages = [];
      workers.push(this);
    }
    postMessage(message, transfers) {
      if (this.throwOnPost) throw new Error('could not post');
      const inputs = message.run ? message.jobs.map(job => job.input.buffer) : [];
      const transfer = transfers ?? [];
      expect(transfer).toHaveLength(inputs.length);
      for (const [index, buffer] of transfer.entries()) expect(buffer).toBe(inputs[index]);
      this.messages.push(structuredClone(message, { transfer }));
    }
    terminate() { this.closed = true; }
    send(data, ms = 0) {
      if (!ms) return this.onmessage({ data });
      this.onmessage({ data: new Proxy(data, {
        get(target, key) {
          now += ms;
          ms = 0;
          return target[key];
        },
      }) });
    }
    ready(registered = names, version = 1) { this.send({ ready: true, version, names: registered }); }
    answer(attempt, sizes = [2, 65536]) {
      this.send({ done: true, attempt, output: new Uint8Array(sizes[0]), ms: 1, memory: sizes[1] });
    }
    error() { this.onerror({ preventDefault() {} }); }
  }
  const lane = createTransport({
    module, glue, Worker, cores, warn: value => warnings.push(value),
    now: () => now,
    setTimeout(callback, ms) { timers.set(++timer, { at: now + ms, callback }); return timer; },
    clearTimeout: id => timers.delete(id),
  });
  lane.start(names);
  return { lane, workers, warnings, clock };
}

const job = (low, high = 0) => ({ attempt: [low, high], name: 'mesh', input: new Uint8Array([low]) });
const starting = [0, []], ended = [-1, []];
const arrived = (low, size = 2, high = 0) => [0, low, high, size];
const lost = (worker, ...attempts) => [1, worker, attempts];

function take(lane, queue, ...jobs) {
  lane.events(queue);
  lane.setAsking(queue, jobs.length > 0);
  if (!jobs.length || !lane.canPost(queue)) return 0;
  const worker = lane.post(queue, jobs);
  lane.setAsking(queue, false);
  return worker;
}

test('startup clones the compiled module and uses one to four module workers', () => {
  for (const [cores, count] of [[1, 1], [3, 1], [4, 2], [64, 4]]) {
    const { lane, workers } = setup(cores);
    expect(workers).toHaveLength(count);
    expect(lane.events(1)).toEqual(starting);
    for (const worker of workers) {
      expect(worker.url.pathname).toEndWith('/gpu-jobs-worker.js');
      expect(worker.options).toEqual({ type: 'module' });
      expect(worker.messages[0]).toMatchObject({ init: true, version: 1, glue });
      expect(worker.messages[0].module).toBeInstanceOf(WebAssembly.Module);
      worker.ready();
    }
    expect(lane.events(1)).toEqual([count, []]);
    lane.close();
    expect(workers.every(worker => worker.closed)).toBe(true);
  }
});

test('a start expires at five seconds even if a late ready arrives', () => {
  const { lane, workers, clock, warnings } = setup();
  clock(4999);
  expect(lane.events(1)).toEqual(starting);
  clock(5000);
  workers[0].ready();
  expect(lane.events(1)).toEqual(ended);
  expect(workers[0].closed).toBe(true);
  expect(warnings).toHaveLength(1);
});

test('names and version must match, while another initial worker can still start', () => {
  const { lane, workers } = setup(5);
  workers[0].ready([...names].reverse());
  expect(lane.events(1)).toEqual(starting);
  workers[1].ready(names, 2);
  workers[2].ready();
  expect(lane.events(1)).toEqual([1, []]);
  expect(workers.map(worker => !!worker.closed)).toEqual([true, true, false]);
});

test('posts at most 16, transfers inputs, and announces and consumes an answer once', () => {
  const { lane, workers } = setup();
  workers[0].ready();
  lane.events(1);
  lane.setAsking(1, true);
  const jobs = Array.from({ length: 17 }, (_, i) => job(i + 1, 3));
  expect(lane.post(1, jobs)).toBe(0);
  const batch = jobs.slice(0, 16);
  const expected = structuredClone(batch);
  expect(lane.post(1, batch)).toBe(1);
  expect(workers[0].messages[1]).toEqual({ run: true, jobs: expected });
  expect(batch.map(posting => posting.input.byteLength)).toEqual(Array(16).fill(0));
  workers[0].answer([1, 3]);
  expect(lane.readResult(1, 1, 3, 2)).toBeNull();
  expect(lane.events(1)).toEqual([1, [arrived(1, 2, 3)]]);
  expect(lane.events(1)).toEqual([1, []]);
  expect(lane.readResult(1, 1, 3, 2)).toEqual(new Uint8Array(2));
  expect(lane.readResult(1, 1, 3, 2)).toBeNull();
  expect(lane.busyWorkers(1)).toBe(1);
  expect(lane.report()).toEqual([1, 65536, 0, 2, 0]);
});

test('forget drops buffers and future answers without freeing a batch early', () => {
  const { lane, workers } = setup();
  const worker = workers[0];
  worker.ready();
  take(lane, 1, job(1), job(2), job(3));
  worker.answer([1, 0]);
  worker.answer([2, 0]);
  expect(lane.events(1)).toEqual([1, [arrived(1), arrived(2)]]);
  lane.forgetAttempt(1, 1, 0);
  expect(lane.readResult(1, 1, 0, 2)).toBeNull();
  lane.forget(1);
  expect(lane.readResult(1, 2, 0, 2)).toBeNull();
  expect(lane.busyWorkers(1)).toBe(1);
  worker.answer([3, 0]);
  expect(lane.events(1)).toEqual([1, []]);
  expect(lane.canPost(1)).toBe(false);
});

test('a batch timeout runs from its last answer, and a replaced worker cannot answer a retry', () => {
  const { lane, workers, clock } = setup();
  workers[0].ready();
  take(lane, 1, job(1), job(2));
  clock(9999);
  workers[0].answer([1, 0]);
  clock(19998);
  expect(lane.events(1)).toEqual([1, [arrived(1)]]);
  clock(19999);
  expect(lane.events(1)).toEqual([0, [lost(1, [2, 0])]]);
  expect(workers[0].closed).toBe(true);
  workers[1].ready();
  expect(take(lane, 1, job(3))).toBe(2);
  workers[0].answer([2, 0]);
  workers[0].answer([3, 0]);
  expect(lane.events(1)).toEqual([1, []]);
  workers[1].answer([3, 0]);
  expect(lane.events(1)).toEqual([1, [arrived(3)]]);
});

test('three losses end the lane and drop buffers even from announced takes', () => {
  const { lane, workers, warnings, clock } = setup();
  workers[0].ready();
  take(lane, 1, job(1));
  workers[0].answer([1, 0]);
  expect(lane.events(1)).toEqual([1, [arrived(1)]]);
  take(lane, 2, job(2), job(3));
  workers[0].answer([2, 0], [3, 131072]);
  workers[0].error();
  expect(lane.events(2)).toEqual([0, [arrived(2, 3), lost(1, [3, 0])]]);
  workers[1].ready();
  lane.events(2);
  workers[1].throwOnPost = true;
  expect(take(lane, 3, job(4))).toBe(0);
  lane.events(3);
  workers[2].ready();
  take(lane, 3, job(5));
  clock(10000);
  expect(lane.events(3)).toEqual([-1, [lost(3, [5, 0])]]);
  expect(lane.events(1)).toEqual(ended);
  expect(lane.readResult(1, 1, 0, 2)).toBeNull();
  expect(lane.readResult(2, 2, 0, 3)).toBeNull();
  expect(lane.report()).toEqual([0, 131072, 0, 5, 0]);
  expect(warnings).toHaveLength(1);
});

test('traps replace their worker without counting a loss and retry only the unanswered work', () => {
  const { lane, workers } = setup();
  workers[0].ready();
  for (let i = 1; i <= 3; i++) {
    const trapped = job(i), retry = job(i, 1);
    expect(take(lane, 1, trapped, retry)).toBe(i);
    workers[i - 1].send({ trapped: true, attempt: trapped.attempt });
    expect(lane.events(1)).toEqual([0, [arrived(i, -1), lost(i, retry.attempt)]]);
    expect(workers[i - 1].closed).toBe(true);
    workers[i].ready();
  }
  expect(lane.events(1)).toEqual([1, []]);
});

test('a held queue expires after 250 ms', () => {
  const { lane, workers, clock } = setup();
  workers[0].ready();
  expect(take(lane, 1, job(1))).toBe(1);
  expect(take(lane, 2, job(2))).toBe(0);
  workers[0].answer([1, 0]);
  for (const now of [0, 16, 249]) {
    clock(now);
    expect(take(lane, 1, job(3))).toBe(0);
  }
  for (const [now, value] of [[250, 3], [266, 4], [282, 5]]) {
    clock(now);
    expect(take(lane, 1, job(value))).toBe(1);
    workers[0].answer([value, 0]);
  }
});

test('posting renews the ask without bypassing a less recently served queue', () => {
  const { lane, workers, clock } = setup();
  const worker = workers[0];
  worker.ready();
  lane.setAsking(1, true);
  expect(lane.canPost(1)).toBe(true);
  clock(250);
  expect(lane.post(1, [job(1)])).toBe(1);
  worker.answer([1, 0]);
  lane.setAsking(1, false);
  expect(take(lane, 2, job(2))).toBe(1);
  worker.answer([2, 0]);
  expect(take(lane, 1, job(3))).toBe(1);
  worker.answer([3, 0]);

  lane.setAsking(1, true);
  expect(lane.canPost(1)).toBe(true);
  clock(500);
  lane.setAsking(2, true);
  expect(lane.post(1, [job(4)])).toBe(0);
  expect(lane.post(2, [job(5)])).toBe(1);
});

test('competing queues alternate with one or two takes', () => {
  for (const takes of [1, 2]) {
    const { lane, workers, clock } = setup();
    workers[0].ready();
    for (let frame = 0; frame < 6; frame++) {
      clock(frame * 16);
      const value = 2 * frame + 1;
      const first = take(lane, 1, job(value));
      if (takes === 2) expect(take(lane, 1, ...(first ? [] : [job(value)]))).toBe(0);
      const second = take(lane, 2, job(value + 1));
      expect([first, second]).toEqual(frame % 2 ? [0, 1] : [1, 0]);
      workers[0].answer([value + frame % 2, 0]);
    }
  }
});

test('rejecting an answer stages the loss and preserves other announced bytes', () => {
  const { lane, workers } = setup();
  workers[0].ready();
  expect(take(lane, 1, job(1), job(2), job(3))).toBe(1);
  workers[0].answer([1, 0]);
  workers[0].answer([2, 0]);
  expect(lane.events(1)).toEqual([1, [arrived(1), arrived(2)]]);
  expect(lane.readResult(1, 1, 0, 3)).toBe(0);
  expect(lane.readResult(1, 1, 0, 2)).toBeNull();
  lane.reject(1);
  lane.reject(1);
  expect(workers).toHaveLength(1);
  expect(lane.readResult(1, 2, 0, 2)).toEqual(new Uint8Array(2));
  expect(lane.events(1)).toEqual([0, [lost(1, [3, 0])]]);
  expect(workers).toHaveLength(2);
});

test('only accepting an answer resets a loss streak', () => {
  for (const accept of [false, true]) {
    const { lane, workers } = setup();
    for (let i = 0; i < 2; i++) {
      workers[i].ready();
      expect(take(lane, 1, job(i + 1))).toBe(i + 1);
      workers[i].error();
      expect(lane.events(1)).toEqual([0, [lost(i + 1, [i + 1, 0])]]);
    }
    workers[2].ready();
    expect(take(lane, 1, job(3))).toBe(3);
    workers[2].answer([3, 0]);
    expect(lane.events(1)).toEqual([1, [arrived(3)]]);
    expect(lane.readResult(1, 3, 0, 2)).toEqual(new Uint8Array(2));
    if (accept) lane.acceptAnswer();
    workers[2].error();
    expect(lane.events(1)).toEqual(accept ? starting : ended);
  }
});

test('a replacement that fails to start ends the lane and preserves a known trap', () => {
  const { lane, workers, clock } = setup(4);
  for (const worker of workers) worker.ready();
  expect(take(lane, 1, job(1))).toBe(1);
  expect(take(lane, 2, job(2))).toBe(2);
  workers[0].send({ trapped: true, attempt: [1, 0] });
  expect(lane.events(2)).toEqual([1, []]);
  clock(5000, true);
  expect(workers.every(worker => worker.closed)).toBe(true);
  expect(lane.events(1)).toEqual([-1, [arrived(1, -1)]]);
});

test('a third rejected answer ends both workers during the take and drops announced bytes', () => {
  const { lane, workers } = setup(4);
  for (const worker of workers) worker.ready();
  expect(take(lane, 1, job(1))).toBe(1);
  workers[0].answer([1, 0]);
  expect(lane.events(1)).toEqual([2, [arrived(1)]]);
  expect(take(lane, 1, job(2))).toBe(1);
  for (const id of [2, 3, 4]) {
    expect(take(lane, 2, job(id + 1), job(id + 10))).toBe(id);
    const worker = workers[id - 1];
    worker.answer([id + 1, 0]);
    expect(lane.events(2)).toEqual([2, [arrived(id + 1)]]);
    expect(lane.readResult(2, id + 1, 0, 3)).toBe(0);
    lane.reject(id);
    expect(lane.busyWorkers(2)).toBe(0);
    if (id < 4) {
      expect(lane.events(2)).toEqual([1, [lost(id, [id + 10, 0])]]);
      workers[id].ready();
    }
  }
  expect(workers.every(worker => worker.closed)).toBe(true);
  expect(lane.readResult(1, 1, 0, 2)).toBeNull();
  expect(lane.events(2)).toEqual([-1, [lost(4, [14, 0])]]);
  expect(lane.events(1)).toEqual(ended);
});

test('a third loss in a message handler ends both workers and keeps traps and losses', () => {
  const { lane, workers } = setup(4);
  for (const worker of workers) worker.ready();
  take(lane, 1, job(1), job(2), job(3));
  workers[0].answer([1, 0]);
  workers[0].send({ trapped: true, attempt: [2, 0] });
  lane.events(2);
  workers[2].ready();

  expect(take(lane, 3, job(4))).toBe(2);
  for (const id of [3, 4, 5]) {
    expect(take(lane, 2, job(id + 10))).toBe(id);
    workers[id - 1].send({});
    if (id < 5) {
      lane.events(2);
      workers[id].ready();
    }
  }
  expect(workers.every(worker => worker.closed)).toBe(true);
  expect(lane.events(1)).toEqual([-1, [arrived(2, -1), lost(1, [3, 0])]]);
  expect(lane.events(2)).toEqual([-1, [lost(5, [15, 0])]]);
  expect(lane.events(3)).toEqual(ended);
});

test('close cancels a queued replacement and cannot restart the lane', () => {
  const { lane, workers, clock } = setup();
  workers[0].ready();
  workers[0].error();
  lane.close();
  expect(lane.events(1)).toEqual(ended);
  lane.start(names);
  clock(10000, true);
  expect(workers).toHaveLength(1);
});

test('a silent worker start expires without a snapshot', () => {
  const { lane, workers, clock, warnings } = setup();
  clock(4999, true);
  expect(workers[0].closed).toBeUndefined();
  clock(5000, true);
  expect(workers[0].closed).toBe(true);
  expect(warnings).toHaveLength(1);
  expect(lane.events(1)).toEqual(ended);
});

test('a hung batch expires without a snapshot and reports its attempts', () => {
  const { lane, workers, clock } = setup();
  workers[0].ready();
  take(lane, 1, job(1), job(2));
  clock(9999, true);
  expect(workers[0].closed).toBeUndefined();
  clock(10000, true);
  expect(workers[0].closed).toBe(true);
  expect(lane.events(1)).toEqual([0, [lost(1, [1, 0], [2, 0])]]);
  expect(workers).toHaveLength(2);
});

test('worker registration must finish before the startup deadline', () => {
  const { lane, workers, warnings, clock } = setup();
  const registered = [...names];
  Object.defineProperty(registered, 1, { get() { clock(5002); return names[1]; } });
  clock(4999);
  workers[0].ready(registered);
  expect(lane.events(1)).toEqual(ended);
  expect(lane.report()[4]).toBeNull();
  expect(lane.report()[2]).toBe(3);
  expect(warnings).toHaveLength(1);
});

test('reports startup and handler time, memory and idle workers', () => {
  const { lane, workers, clock } = setup(4);
  expect(lane.report()).toEqual([0, 0, 0, 0, null]);
  expect(lane.idleWorkers()).toBe(0);
  clock(5);
  workers[0].ready(['wrong']);
  expect(lane.report()[4]).toBeNull();
  clock(17);
  workers[1].send({ ready: true, version: 1, names }, 2);
  expect(lane.idleWorkers()).toBe(1);
  take(lane, 1, job(1), job(2));
  take(lane, 2, job(3));
  const answer = low => ({ done: true, attempt: [low, 0], output: new Uint8Array(2), memory: 65536 });
  workers[1].send(answer(1), 3);
  expect(lane.report()).toEqual([1, 65536, 5, 2, 19]);
  expect(lane.idleWorkers()).toBe(0);
  workers[1].send(answer(2), 4);
  expect(lane.report()).toEqual([1, 65536, 9, 4, 19]);
  expect(lane.idleWorkers()).toBe(1);
  expect(workers[1].messages).toHaveLength(2);
  expect(take(lane, 2, job(3))).toBe(2);
  expect(lane.idleWorkers()).toBe(0);
});

test('handler time includes reading event data and handling worker errors', () => {
  const { lane, workers, clock } = setup(4);
  const [first, second] = workers;
  first.onmessage({ get data() { clock(3); return { ready: true, version: 1, names }; } });
  first.onerror({ preventDefault() { clock(7); } });
  second.onmessageerror({ preventDefault() { clock(12); } });
  expect(lane.report()[2]).toBe(12);
});

test('retiring a queue forgets its place in worker assignment', () => {
  const { lane, workers } = setup();
  workers[0].ready();
  expect(take(lane, 1, job(1))).toBe(1);
  lane.retire(1);
  workers[0].answer([1, 0]);
  expect(lane.events(1)).toEqual([1, []]);
  lane.setAsking(2, true);
  expect(take(lane, 1, job(2))).toBe(1);
});
