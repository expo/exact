// The origin a commit made later on behalf of an earlier one carries on the JS target, and the origin data code
// calls a module under (LLP 1133.002 D12), over the runtime's own modules in Bun (js-runtime-harness.mjs).
import { expect, test } from 'bun:test';
import { resolve } from 'node:path';
import { freshRuntime, root } from './js-runtime-harness.mjs';

const flush = async () => { for (let i = 0; i < 32; i++) await Promise.resolve(); };

// A copy of the runtime under the agent clock whose requests each wait for a reply: `requests` lists each one's
// location and origin, and `replies[i]` answers the i-th
async function runtime(tag) {
  const rt = await freshRuntime(tag), requests = [], replies = [];
  rt.data.answer = (source, args) => ({ req: { method: 'GET', url: [source, ...args].join('/'), headers: [] } });
  rt.data.fetch = req => new Promise(resolve => { requests.push([req.url, req.origin]); replies.push(resolve); });
  rt.data.parse = (_source, _args, o) => ({ v: o.value });
  rt.clock.agent = true;
  return { ...rt, requests, replies };
}

// modules.js with a source held to the app (`fx.appOnly`) and a device event (`fx.pinged`) whose handler sends
// `device`; answers the module's context once its implementation has loaded, which happens off the agent clock
async function deviceModule(rt, tag, device) {
  const { modules } = await import(resolve(root, `web-js-${tag}`, 'modules.js'));
  let context;
  rt.clock.agent = false;
  modules({ appOnly: ['fx.appOnly', false, { origin: 'app', default: 'value' }, 's', 'never', null, false] },
    { fx: () => Promise.resolve({ implementation: c => { context = c; return { appOnly: async () => 'value' }; } }) },
    {}, { 'fx.pinged': [() => () => rt.commit(() => rt.M(device, 'device', [])), 'device', 1, [], 'pinged'] });
  await globalThis.exact.extensions.call({ extension: 'fx', source: 'appOnly' });
  rt.clock.agent = true;
  return context;
}

// A queued send's `next` commit runs under the origin of the commit that sent it, as the runner's waiting send keeps
// it (`queue.rs` `Waiting`), and a `then` under its answer's (LLP 1133.002 D12): a wake's sends go out as the wake's
// after its origin is gone, the second once the first is answered, and so does what each `then` asks.
test('a queued send and a then keep the origin of the commit that made them', async () => {
  const { mut, M, sig, queues, commit, advance, clock, Origin, requests, replies } = await runtime('queued-origin');
  const slot = sig('seed'), followed = sig('seed');
  const queued = mut('queued', slot, [], 's', 1), after = mut('after', followed, [], 's');
  queues([[slot, followed], [], []]);
  queued.then = () => M(after, 'after', []);
  Origin.run('wake', () => commit(() => { M(queued, 'queued', [1]); M(queued, 'queued', [2]); }));
  await flush();
  for (const [i, value] of ['one', 'after one', 'two'].entries()) {
    replies[i]({ value });
    await flush();
    advance(clock.now, false);
    await flush();
  }
  expect(requests).toEqual([['queued/1', 'wake'], ['after', 'wake'], ['queued/2', 'wake'], ['after', 'wake']]);
});

// A `then` of the app's that fires while a device event's request is in flight is the app's for a Contract call,
// which carries its commit's origin exactly, while data code in it calls a module as the device, the least privileged
// of what it cannot tell itself apart from (the runner's `data_origin`, LLP 1133.002 D12).
test('a restored origin holds for a Contract call while data code takes the least privileged in flight', async () => {
  const rt = await runtime('data-origin'), { clock, data, commit, M, mut, sig, advance, journal, Origin, replies } = rt;
  const device = mut('device', sig(null), [], 's'), first = mut('first', sig(null), [], 's');
  const context = await deviceModule(rt, 'data-origin', device);
  first.then = () => {
    data.reserved.appOnly(null, [{}], 'contract');
    globalThis.exact.extensions.call({ extension: 'fx', source: 'appOnly', origin: 'app' });
  };
  // The device's request goes out first; then a commit of the app's (as a reply restores it) sends the app's
  context.emit('pinged', []);
  await flush();
  Origin.run('app', () => commit(() => M(first, 'first', [])));
  const before = journal.length;
  replies[1]({ value: 'answered' });
  await flush();
  advance(clock.now, false);
  await flush();
  expect(journal.slice(before).map(l => l.replace(/^t=\S+ /, '')).filter(l => l.startsWith('fx.appOnly'))).toEqual([
    'fx.appOnly: refused, person-required (device origin)',
  ]);
});

// A tool's commit is the tool's though a device's request is in flight, as the runner dispatches it (`Tool`), and so
// is its `then`.
test('a tool keeps its origin beside a device request', async () => {
  const rt = await runtime('tool-origin'), { clock, commit, M, mut, sig, advance, Origin, requests, replies } = rt;
  const device = mut('device', sig(null), [], 's'), sent = mut('sent', sig(null), [], 's'), after = mut('after', sig(null), [], 's');
  const context = await deviceModule(rt, 'tool-origin', device);
  sent.then = () => M(after, 'after', []);
  context.emit('pinged', []);
  await flush();
  // A tool's call: calls.js answers `tool` while its invocation runs
  Origin.call = () => 'tool';
  commit(() => M(sent, 'sent', []));
  delete Origin.call;
  for (const i of [0, 1]) {
    replies[i]({ value: 'answered' });
    await flush();
  }
  advance(clock.now, false);
  await flush();
  expect(requests).toEqual([['device', 'device'], ['sent', 'tool'], ['after', 'tool']]);
});
