// Observe's web service in two tabs of one Chromium profile, which share its localStorage
// queue: a tab adds its rows to the stored queue, and each row reaches the server once.
// `bun test ./modules/observe/tests/tabs.test.mjs`; it skips when the pinned Chromium is missing.
import { afterAll, expect, test as always } from 'bun:test';
import { readFileSync } from 'node:fs';
import { createServer } from 'node:http';
import { join } from 'node:path';
import { suiteBrowser } from '../../../scripts/agent-launch.mjs';
import { browserMissing } from '../../../scripts/agent-playwright.mjs';
import { need } from '../../../scripts/coverage.mjs';

const service = readFileSync(join(import.meta.dir, '../web/service.js'), 'utf8');
const KEY = 'exact-observe:com.example.tabs';
const page = `<!doctype html><meta charset="utf-8"><script type="module">
  import { start } from './service.js';
  window.observe = start({ projectId: 'p', endpoint: location.origin + '/collect', 'app.CFBundleIdentifier': 'com.example.tabs' });
  window.event = name => observe.event({ kind: 'app.event', name, attributes: {}, wall: Date.now() });
  window.queued = () => JSON.parse(localStorage.getItem(${JSON.stringify(KEY)})).logs.map(r => r.name);
  window.ready = true;
</script>`;

const test = need('chrome', await browserMissing('chromium'));
let browser;
afterAll(() => browser?.close());

test('two tabs add to one stored queue and send each row once', async () => {
  // Each event name, as many times as a logs request carried it.
  const posted = [];
  const server = createServer((req, res) => {
    if (req.method === 'POST') {
      let body = '';
      req.on('data', c => { body += c; });
      req.on('end', () => {
        if (req.url.endsWith('/v1/logs')) {
          for (const r of JSON.parse(body).resourceLogs) for (const s of r.scopeLogs) for (const l of s.logRecords)
            posted.push(l.attributes.find(a => a.key === 'event.name').value.stringValue);
        }
        res.writeHead(200).end();
      });
      return;
    }
    if (req.url === '/service.js') { res.writeHead(200, { 'content-type': 'text/javascript' }).end(service); return; }
    res.writeHead(200, { 'content-type': 'text/html' }).end(page);
  });
  await new Promise(ok => server.listen(0, '127.0.0.1', ok));
  try {
    browser = await suiteBrowser('chromium');
    const context = await browser.newContext();
    // The batch timers never fire: each tab dispatches when the test calls `background()`.
    await context.clock.install();
    await context.clock.pauseAt(Date.now() + 1000);
    const open = async () => {
      const p = await context.newPage();
      await p.goto(`http://127.0.0.1:${server.address().port}/`);
      await p.waitForFunction(() => window.ready);
      return p;
    };
    const one = await open();
    await one.evaluate(() => event('first'));
    const two = await open();
    await two.evaluate(() => event('second'));
    // Wait until the first tab sees the second's row: localStorage reaches another renderer asynchronously.
    await one.waitForFunction(() => queued().includes('second'));
    await one.evaluate(() => event('third'));
    // An append is saved once its edit lock is granted.
    await two.waitForFunction(() => queued().includes('third'));
    expect(await two.evaluate(() => queued())).toEqual(['first', 'second', 'third']);

    await Promise.all([one.evaluate(() => observe.background()), two.evaluate(() => observe.background())]);
    expect(posted.sort()).toEqual(['first', 'second', 'third']);
    for (const p of [one, two]) expect(await p.evaluate(() => queued())).toEqual([]);
  } finally {
    await browser?.close();
    server.close();
  }
}, 30_000);

// Two tabs in separate renderer processes, as a fake: each reads its own copy of the stored
// queue, which learns another tab's save only when told; a lock's grant tells every copy, as
// it comes after the holder's save. A change made without the lock reads a stale copy and
// saves over the other tab's rows.
always('appends and a send\'s removal take the edit lock, so no tab saves over another\'s rows', async () => {
  const run = new Function('localStorage', 'navigator', 'fetch', 'setTimeout',
    `${service.replace(/^export /gm, '')}; return start;`);
  const shared = new Map(), copies = [], held = new Map(), posted = [];
  const sync = () => { for (const copy of copies) { copy.clear(); for (const [k, v] of shared) copy.set(k, v); } };
  const locks = {
    async request(name, work) {
      while (held.has(name)) await held.get(name);
      let release;
      held.set(name, new Promise(r => { release = r; }));
      sync();
      try { return await work(); } finally { held.delete(name); release(); }
    },
  };
  // The first request waits for `reply` and is accepted; a later one is refused, so its rows stay queued.
  let reply = null;
  const fetch = async (_url, init) => {
    if (reply) return new Response(null, { status: 503 });
    for (const r of JSON.parse(init.body).resourceLogs) for (const s of r.scopeLogs) for (const l of s.logRecords) posted.push(l.body.stringValue);
    return new Promise(resolve => { reply = () => resolve(new Response(null, { status: 200 })); });
  };
  const tab = () => {
    const copy = new Map(shared);
    copies.push(copy);
    const storage = { getItem: k => copy.get(k) ?? null, setItem: (k, v) => { copy.set(k, String(v)); shared.set(k, String(v)); } };
    const service = run(storage, { platform: 'MacIntel', language: 'en-US', locks }, fetch, () => 0)(
      { projectId: 'p', endpoint: 'https://example.com', 'app.CFBundleIdentifier': 'com.example.tabs' });
    return { service, log: body => service.event({ kind: 'app.event', name: 'e', body, attributes: {}, wall: 0 }) };
  };
  const settle = async () => { for (let i = 0; i < 20; i++) await Promise.resolve(); };
  const stored = () => JSON.parse(shared.get(KEY)).logs.map(r => r.body);
  const one = tab(), two = tab();
  await settle();
  // Appended in the same moment, before either tab's copy has the other's save.
  one.log('a');
  two.log('b');
  await settle();
  expect(stored()).toEqual(['a', 'b']);
  // One tab sends; the other appends as the send's removal is saved.
  const sending = one.service.background();
  await settle();
  reply();
  two.log('c');
  await sending;
  await settle();
  expect(stored()).toEqual(['c']);
  expect(posted).toEqual(['a', 'b']);
});
