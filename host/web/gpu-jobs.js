// Browser transport for one GPU module instance (LLP 1182.000 D9).
const version = 1;
// The numeric snapshot rows cross the module boundary without copying answers.
const arrived = 0, lost = 1, trapped = -1;
const startTimeout = 5000, batchTimeout = 10000, askLifetime = 250;
const keyOf = (low, high) => `${high}:${low}`;

export function createTransport({
  module, glue, Worker = globalThis.Worker, cores = globalThis.navigator?.hardwareConcurrency ?? 1,
  now = () => performance.now(), warn = message => console.warn(message),
  setTimeout = globalThis.setTimeout, clearTimeout = globalThis.clearTimeout,
}) {
  const workers = new Map(), queues = new Map();
  const workerCount = Math.min(Math.max(1, cores - 2), 4);
  let names, nextWorker = 1, served = 0, losses = 0, replacements = 0, ended = false;
  let startedAt, readyMs = null;
  let workerBytes = 0, handlerMs = 0, buffered = 0, peakBuffered = 0;

  function queueFor(id) {
    if (!queues.has(id)) queues.set(id, { served: 0, results: new Map(), events: [] });
    return queues.get(id);
  }

  function end(reason) {
    if (ended) return;
    ended = true;
    replacements = 0;
    for (const worker of workers.values()) stop(worker);
    workers.clear();
    buffered = 0;
    for (const queue of queues.values()) {
      queue.asked = undefined;
      queue.results.clear();
      queue.events = queue.events.filter(event => event[0] === lost || event[3] === trapped);
    }
    if (reason) warn(`exact: GPU jobs: ${reason}; using inline jobs`);
  }

  function stop(worker) {
    clearTimeout(worker.timer);
    worker.port?.terminate();
  }

  function remove(worker, reason) {
    workers.delete(worker.id);
    stop(worker);
    if (worker.ready) {
      if (worker.batch) {
        const { queue, attempts } = worker.batch;
        const unanswered = [...attempts.values()].filter(job => job.wanted).map(job => job.attempt);
        if (unanswered.length) queues.get(queue).events.push([lost, worker.id, unanswered]);
      }
      if (reason && ++losses >= 3) end('three consecutive worker losses');
      else replacements++;
    } else if (worker.replacement) {
      end(`worker replacement failed (${reason})`);
    } else if (nextWorker > workerCount && !workers.size && !replacements) {
      end('no workers started');
    }
  }

  function expire(worker, at) {
    if (at < worker.deadline) return false;
    remove(worker, worker.ready ? 'batch timed out' : 'start timed out');
    return true;
  }

  function setDeadline(worker, deadline) {
    clearTimeout(worker.timer);
    worker.deadline = deadline;
    worker.timer = deadline === Infinity ? undefined : setTimeout(() => {
      if (workers.has(worker.id) && !expire(worker, now())) setDeadline(worker, deadline);
    }, Math.ceil(deadline - now()));
  }

  function receive(worker, message, at) {
    if (expire(worker, at)) return;
    if (!worker.ready) {
      if (message?.ready !== true || message.version !== version || !Array.isArray(message.names)
        || message.names.length !== names.length || names.some((name, index) => message.names[index] !== name)) {
        remove(worker, 'worker registration differs');
      } else {
        const checkedAt = now();
        if (expire(worker, checkedAt)) return;
        worker.ready = true;
        setDeadline(worker, Infinity);
        readyMs ??= checkedAt - startedAt;
      }
      return;
    }

    const batch = worker.batch;
    if (!batch) return;
    if ((!message?.done && !message?.trapped) || !Array.isArray(message.attempt)
      || (message.done && !(message.output instanceof Uint8Array))) {
      remove(worker, 'invalid worker reply');
      return;
    }
    const [low, high] = message.attempt, key = keyOf(low, high);
    const job = batch.attempts.get(key);
    if (!job) return;
    batch.attempts.delete(key);
    if (job.wanted) {
      const queue = queues.get(batch.queue);
      if (message.trapped) {
        queue.events.push([arrived, low, high, trapped]);
      } else {
        const bytes = message.output;
        queue.results.set(key, { bytes, announced: false });
        queue.events.push([arrived, low, high, bytes.length]);
        buffered += bytes.byteLength;
        peakBuffered = Math.max(peakBuffered, buffered);
      }
    }
    if (message.trapped) {
      remove(worker);
    } else {
      workerBytes = Math.max(workerBytes, Number.isFinite(message.memory) ? message.memory : 0);
      if (!batch.attempts.size) worker.batch = undefined;
      setDeadline(worker, worker.batch ? at + batchTimeout : Infinity);
    }
  }

  function spawn(replacement) {
    if (ended) return;
    const worker = { id: nextWorker++, replacement, deadline: (replacement ? now() : startedAt) + startTimeout };
    workers.set(worker.id, worker);
    try {
      const port = worker.port = new Worker(new URL('./gpu-jobs-worker.js', import.meta.url), { type: 'module' });
      port.onmessage = event => {
        const at = now();
        try {
          if (workers.has(worker.id)) receive(worker, event.data, at);
        } finally {
          handlerMs += now() - at;
        }
      };
      port.onerror = port.onmessageerror = event => {
        const at = now();
        try {
          event.preventDefault?.();
          if (workers.has(worker.id)) remove(worker, 'worker error');
        } finally {
          handlerMs += now() - at;
        }
      };
      port.postMessage({ init: true, version, module, glue });
      setDeadline(worker, worker.deadline);
    } catch (error) {
      remove(worker, String(error));
    }
  }

  const readyCount = () => [...workers.values()].filter(worker => worker.ready).length;
  const idleWorkers = () => [...workers.values()].filter(worker => worker.ready && !worker.batch).length;
  const activeAsk = (queue, at) => queue.asked !== undefined && at - queue.asked < askLifetime;
  function idleFor(id, at = now()) {
    if (ended) return;
    let winner, leastServed = Infinity;
    for (const [other, queue] of queues) {
      if (!activeAsk(queue, at)) continue;
      if (queue.served < leastServed || (queue.served === leastServed && other < winner)) {
        winner = other;
        leastServed = queue.served;
      }
    }
    if (winner === id) return [...workers.values()].find(worker => worker.ready && !worker.batch);
  }

  function removeResult(queue, key) {
    const result = queue.results.get(key);
    if (result) {
      queue.results.delete(key);
      buffered -= result.bytes.byteLength;
    }
    return result;
  }

  function forget(id) {
    const queue = queues.get(id);
    if (!queue) return;
    for (const result of queue.results.values()) buffered -= result.bytes.byteLength;
    queue.results.clear();
    queue.events = [];
    queue.asked = undefined;
    for (const { batch } of workers.values()) {
      if (batch?.queue === id) for (const job of batch.attempts.values()) job.wanted = false;
    }
  }

  return {
    start(registered, requestedAt = now()) {
      if (ended || names) return;
      names = registered;
      startedAt = requestedAt;
      for (let index = 0; index < workerCount; index++) spawn(false);
    },
    events(id) {
      const at = now();
      for (const worker of workers.values()) expire(worker, at);
      while (replacements > 0) {
        replacements--;
        spawn(true);
      }

      const queue = queues.get(id), events = queue?.events ?? [];
      if (queue) queue.events = [];
      for (const [kind, low, high, size] of events) {
        if (kind === arrived && size !== trapped) queue.results.get(keyOf(low, high)).announced = true;
      }
      return [ended ? -1 : readyCount(), events];
    },
    canPost: id => !!idleFor(id),
    idleWorkers,
    busyWorkers: id => [...workers.values()].filter(worker => worker.batch?.queue === id).length,
    setAsking(id, asking) {
      if (asking && !ended) queueFor(id).asked = now();
      else if (queues.has(id)) queues.get(id).asked = undefined;
    },
    post(id, jobs) {
      if (ended || !jobs.length || jobs.length > 16) return 0;
      const queue = queueFor(id);
      queue.asked = now();
      const worker = idleFor(id, queue.asked);
      if (!worker) return 0;
      const attempts = new Map(jobs.map(({ attempt }) => [keyOf(...attempt), { attempt, wanted: true }]));
      try {
        worker.port.postMessage({ run: true, jobs }, jobs.map(job => job.input.buffer));
      } catch (error) {
        remove(worker, String(error));
        return 0;
      }
      worker.batch = { queue: id, attempts };
      setDeadline(worker, now() + batchTimeout);
      queue.served = ++served;
      return worker.id;
    },
    readResult(id, low, high, length) {
      const queue = queues.get(id), key = keyOf(low, high);
      if (!queue?.results.get(key)?.announced) return null;
      const { bytes } = removeResult(queue, key);
      return bytes.length === length ? bytes : 0;
    },
    acceptAnswer() { losses = 0; },
    reject(id) {
      const worker = workers.get(id);
      if (worker) remove(worker, 'rejected answer');
    },
    forgetAttempt(id, low, high) {
      const queue = queues.get(id);
      if (!queue) return;
      const key = keyOf(low, high);
      removeResult(queue, key);
      queue.events = queue.events.filter(event => {
        if (event[0] === arrived) return event[1] !== low || event[2] !== high;
        event[2] = event[2].filter(attempt => keyOf(...attempt) !== key);
        return event[2].length > 0;
      });
      for (const { batch } of workers.values()) {
        const job = batch?.queue === id && batch.attempts.get(key);
        if (job) job.wanted = false;
      }
    },
    forget,
    retire(id) {
      forget(id);
      queues.delete(id);
    },
    report: () => [readyCount(), workerBytes, handlerMs, peakBuffered, readyMs],
    close: () => end(),
  };
}
