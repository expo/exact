// The wasm host hands its clock from the ordinary ticker to a text-flow controller only once the controller is ready
// (wasm/flow.js): a load that fails leaves the ordinary ticker running timers and is not tried again until the next
// generation, which loads it afresh. flowBatch and startClock as glue.js bundles them, with host doubles.
import { expect, test } from 'bun:test';
import vm from 'node:vm';
import { declaration, deferred, fixture } from './glue-fixture.mjs';

function host() {
  const f = fixture(false), tickers = [], logs = [];
  let loads = 0, loading = deferred();
  f.timerFactory = () => {
    const ticker = { due: [], disposed: false, update(due) { this.due.push(due); }, dispose() { this.disposed = true; } };
    tickers.push(ticker);
    return ticker;
  };
  f.log = line => logs.push(line);
  f.loadAfterPaint = () => { loads++; return loading.promise; };
  vm.runInContext(declaration('flowBatch'), f);
  const batch = due => f.flowBatch({ ops: [{ op: 'textflow', contexts: [1] }], timer_due_ms: due });
  const settle = () => new Promise(resolve => setTimeout(resolve, 0));
  return { f, tickers, logs, batch, settle, loads: () => loads, next: () => (loading = deferred()) };
}

test.each([
  ['the module import', (load) => load.reject(new Error('import failed'))],
  ['the controller creation', (load) => load.resolve(() => Promise.reject(new Error('create failed')))],
])('a failed text-flow load leaves the ordinary ticker the clock (%s)', async (_, fail) => {
  const { f, tickers, logs, batch, settle, loads, next } = host();
  f.startClock();
  expect(tickers).toHaveLength(1);
  const load = next();
  batch(100);
  expect(loads()).toBe(1);
  // While the controller loads, the ordinary ticker keeps the clock.
  expect(tickers[0].disposed).toBe(false);
  expect(tickers[0].due.at(-1)).toBe(100);
  fail(load);
  await settle();
  expect(logs).toHaveLength(1);
  expect(logs[0]).toStartWith('textflow module: Error:');
  expect([f.flowLoading, f.flowFailed, f.textflow]).toEqual([null, true, null]);
  expect(f.ticker).toBe(tickers[0]);
  expect(tickers[0].disposed).toBe(false);
  // A later batch's timer reaches the ticker, and the load is not tried again in this generation.
  batch(200);
  expect(tickers[0].due.at(-1)).toBe(200);
  expect(loads()).toBe(1);
  f.startClock();
  expect(tickers).toHaveLength(1);

  // The next generation loads it afresh, and a controller that is ready takes the clock from the ticker.
  f.incarnation++;
  f.flowFailed = false;
  const again = next(), told = [];
  const controller = { afterBatch: b => told.push(b.timer_due_ms), dispose() {} };
  batch(300);
  expect(loads()).toBe(2);
  again.resolve(async () => controller);
  await settle();
  expect(tickers[0].disposed).toBe(true);
  expect([f.ticker, f.textflow, told]).toEqual([null, controller, [300]]);
  f.startClock();
  expect(tickers).toHaveLength(1);
});

test('a load that fails before the ordinary ticker started starts it', async () => {
  const { f, tickers, batch, settle, next } = host();
  const load = next();
  batch(100);
  load.reject(new Error('fetch failed'));
  await settle();
  expect(tickers).toHaveLength(1);
  expect(f.ticker).toBe(tickers[0]);
  expect(tickers[0].due.at(-1)).toBe(100);
});
