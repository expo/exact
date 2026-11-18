import { afterAll, beforeAll, expect } from 'bun:test';
import { readFileSync } from 'node:fs';
import { browserMissing } from '../../scripts/agent-playwright.mjs';
import { suiteBrowser } from '../../scripts/agent-launch.mjs';
import { need } from '../../scripts/coverage.mjs';

const glue = readFileSync(new URL('auth-glue.js', import.meta.url), 'utf8');
const callback = readFileSync(new URL('auth-callback.js', import.meta.url), 'utf8');
const leaseGates = new Map(), providerGates = new Map();
let app, provider;

const html = (body, headers = {}) => new Response(`<!doctype html><meta charset="utf-8">${body}`, {
  headers: { 'content-type': 'text/html; charset=utf-8', ...headers },
});
const gate = () => {
  let arrived, respond;
  const value = {
    arrived: new Promise(resolve => { arrived = resolve; }),
    wait() { return new Promise(resolve => { respond = resolve; arrived(); }); },
    release() { respond?.(new Response(null, { status: 204 })); },
  };
  return value;
};
const appURL = path => `http://localhost:${app.port}${path}`;
const providerURL = path => `http://127.0.0.1:${provider.port}${path}`;

const tabPage = url => html(`<button id="start">Start</button><script type="module">
  const realOpen = window.open.bind(window);
  window.open = (...args) => (globalThis.popup = realOpen(...args));
  const realSetInterval = globalThis.setInterval.bind(globalThis);
  globalThis.authPollPastGrace = false;
  let closedAt = null, pollsAfterClosed = 0;
  globalThis.setInterval = (callback, delay, ...args) => realSetInterval(() => {
    const began = performance.now();
    callback(...args);
    if (delay !== 250) return;
    if (!globalThis.popup?.closed) { closedAt = null; pollsAfterClosed = 0; return; }
    closedAt ??= began;
    pollsAfterClosed++;
    if (pollsAfterClosed >= 14 && began - closedAt >= 3250) authPollPastGrace = true;
  }, delay);
  const { arm, prepare } = await import('/auth-glue.js');
  globalThis.done = [];
  globalThis.delivered = [];
  globalThis.observed = [];
  globalThis.openerObservations = [];
  globalThis.openerMessages = [];
  addEventListener('message', event => openerMessages.push(event.data));
  document.getElementById('start').onclick = async () => {
    const prepared = prepare();
    await fetch('/lease?run=${encodeURIComponent(url.searchParams.get('run'))}');
    arm({ ticket: 1 }, {
      agent: false,
      log() {},
      active: () => true,
      deliver(ticket) { delivered.push(ticket); },
      call(request) {
        if (request.op === 'done') done.push(request);
        return request.op === 'arm' ? { present: {
          url: ${JSON.stringify(url.searchParams.get('provider'))},
          callback: location.origin + '/.exact/auth/callback',
          state: ${JSON.stringify(url.searchParams.get('state'))},
          logout: ${url.searchParams.get('logout') !== 'false'},
          prepared,
        } } : {};
      },
    });
    const observer = new BroadcastChannel('exact-auth');
    observer.onmessage = event => observed.push(event.data);
    globalThis.observer = observer;
    const openerObserver = new BroadcastChannel('exact-auth-opener');
    openerObserver.onmessage = event => openerObservations.push(event.data);
    globalThis.openerObserver = openerObserver;
  };
</script>`);

beforeAll(() => {
  provider = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const url = new URL(request.url);
    if (url.pathname === '/provider') return html(`<script>
      fetch('/hold?run=${encodeURIComponent(url.searchParams.get('run'))}')
        .then(() => location.replace(${JSON.stringify(url.searchParams.get('callback'))}));
    </script>`, { 'Cross-Origin-Opener-Policy': 'same-origin' });
    if (url.pathname === '/hold') {
      const held = providerGates.get(url.searchParams.get('run'));
      return held ? held.wait() : new Response(null, { status: 404 });
    }
    return new Response(null, { status: 404 });
  } });
  app = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(request) {
    const url = new URL(request.url);
    if (url.pathname === '/auth-glue.js') return new Response(glue, { headers: { 'content-type': 'text/javascript' } });
    if (url.pathname === '/auth-callback.js') return new Response(callback, { headers: { 'content-type': 'text/javascript' } });
    if (url.pathname === '/.exact/auth/callback') return html(`<script>
      const probe = new BroadcastChannel('exact-auth-opener');
      probe.postMessage(window.opener === null);
      probe.close();
    </script><script src="/auth-callback.js"></script>`);
    if (url.pathname === '/tab') return tabPage(url);
    if (url.pathname === '/lease') {
      const held = leaseGates.get(url.searchParams.get('run'));
      return held ? held.wait() : new Response(null, { status: 404 });
    }
    return new Response(null, { status: 404 });
  } });
});

afterAll(() => {
  app?.stop(true);
  provider?.stop(true);
});

function heldRoundTrip(state = '', fragment = false) {
  const run = crypto.randomUUID();
  const lease = gate(), heldProvider = gate();
  leaseGates.set(run, lease);
  providerGates.set(run, heldProvider);
  const callbackURL = appURL(`/.exact/auth/callback${fragment ? `?tracking=1#state=${encodeURIComponent(state)}&id_token=fixture-token&code=hybrid` : state ? `?state=${encodeURIComponent(state)}` : ''}`);
  const providerPage = providerURL(`/provider?run=${encodeURIComponent(run)}&callback=${encodeURIComponent(callbackURL)}`);
  const tab = appURL(`/tab?run=${encodeURIComponent(run)}&state=${encodeURIComponent(state || 'logout')}&provider=${encodeURIComponent(providerPage)}&logout=${!fragment}`);
  return { run, lease, provider: heldProvider, callbackURL, tab };
}

for (const engine of ['chromium', 'firefox', 'webkit']) {
  need(engine, await browserMissing(engine))(`${engine}: an addressed logout survives a cross-site COOP round trip`, async () => {
    const browser = await suiteBrowser(engine);
    const context = await browser.newContext();
    const first = heldRoundTrip(), second = heldRoundTrip();
    try {
      expect(new URL(first.tab).hostname).toBe('localhost');
      expect(new URL(new URL(first.tab).searchParams.get('provider')).hostname).toBe('127.0.0.1');
      const firstPage = await context.newPage(), secondPage = await context.newPage();
      await firstPage.goto(first.tab);
      await secondPage.goto(second.tab);
      await firstPage.click('#start');
      await secondPage.click('#start');
      await Promise.all([first.lease.arrived, second.lease.arrived]);
      first.lease.release();
      second.lease.release();
      await Promise.all([first.provider.arrived, second.provider.arrived]);

      second.provider.release();
      await Promise.all([
        firstPage.waitForFunction(() => observed.length === 1 && openerObservations.length === 1),
        secondPage.waitForFunction(() => done.length === 1 && observed.length === 1 && openerObservations.length === 1),
      ]);
      expect(await firstPage.evaluate(() => done)).toEqual([]);
      expect(await secondPage.evaluate(() => done)).toEqual([
        { op: 'done', ticket: 1, url: second.callbackURL },
      ]);
      expect(await secondPage.evaluate(() => openerMessages)).toEqual([]);
      expect(await firstPage.evaluate(() => openerObservations)).toEqual([true]);
      expect(await secondPage.evaluate(() => openerObservations)).toEqual([true]);

      first.provider.release();
      await Promise.all([
        firstPage.waitForFunction(() => done.length === 1 && openerObservations.length === 2),
        secondPage.waitForFunction(() => openerObservations.length === 2),
      ]);
      expect(await firstPage.evaluate(() => done)).toEqual([
        { op: 'done', ticket: 1, url: first.callbackURL },
      ]);
      expect(await firstPage.evaluate(() => openerMessages)).toEqual([]);
      expect(await firstPage.evaluate(() => openerObservations)).toEqual([true, true]);
      expect(await secondPage.evaluate(() => openerObservations)).toEqual([true, true]);
    } finally {
      for (const roundTrip of [first, second]) {
        roundTrip.lease.release();
        roundTrip.provider.release();
        leaseGates.delete(roundTrip.run);
        providerGates.delete(roundTrip.run);
      }
      await context.close();
    }
  }, 30_000);
}

need('chromium', await browserMissing('chromium'))('real Chrome: a severed state-less return completes no logout', async () => {
  const browser = await suiteBrowser('chromium');
  const context = await browser.newContext();
  const roundTrip = heldRoundTrip();
  try {
    const page = await context.newPage();
    await page.goto(roundTrip.tab);
    await page.click('#start');
    await roundTrip.lease.arrived;
    roundTrip.lease.release();
    await roundTrip.provider.arrived;
    const callbackPage = await context.newPage();
    await callbackPage.goto(appURL('/.exact/auth/callback')).catch(() => {});
    await page.waitForFunction(() => observed.length === 1);
    expect(await page.evaluate(() => done)).toEqual([]);
  } finally {
    roundTrip.lease.release();
    roundTrip.provider.release();
    leaseGates.delete(roundTrip.run);
    providerGates.delete(roundTrip.run);
    await context.close();
  }
}, 30_000);

need('chromium', await browserMissing('chromium'))('real Chrome: COOP closure does not cancel a held logout', async () => {
  const browser = await suiteBrowser('chromium');
  const context = await browser.newContext();
  const roundTrip = heldRoundTrip();
  try {
    const page = await context.newPage();
    await page.goto(roundTrip.tab);
    await page.click('#start');
    await roundTrip.lease.arrived;
    roundTrip.lease.release();
    await roundTrip.provider.arrived;
    await page.waitForFunction(() => authPollPastGrace);
    expect(await page.evaluate(() => done)).toEqual([]);
    roundTrip.provider.release();
    await page.waitForFunction(() => done.length === 1);
    expect(await page.evaluate(() => done[0])).toEqual({ op: 'done', ticket: 1, url: roundTrip.callbackURL });
  } finally {
    roundTrip.lease.release();
    roundTrip.provider.release();
    leaseGates.delete(roundTrip.run);
    providerGates.delete(roundTrip.run);
    await context.close();
  }
}, 30_000);

for (const engine of ['chromium', 'firefox', 'webkit']) {
  need(engine, await browserMissing(engine))(`${engine}: a sign-in fragment survives a cross-site COOP round trip`, async () => {
    const browser = await suiteBrowser(engine);
    const context = await browser.newContext();
    const roundTrip = heldRoundTrip('sign-in +😀', true);
    try {
      const page = await context.newPage();
      await page.goto(roundTrip.tab);
      await page.click('#start');
      await roundTrip.lease.arrived;
      roundTrip.lease.release();
      await roundTrip.provider.arrived;
      roundTrip.provider.release();
      await page.waitForFunction(() => done.length === 1 && openerObservations.length === 1);
      expect(await page.evaluate(() => done)).toEqual([{ op: 'done', ticket: 1, url: roundTrip.callbackURL }]);
      expect(await page.evaluate(() => openerObservations)).toEqual([true]);
      expect(await page.evaluate(() => openerMessages)).toEqual([]);
    } finally {
      roundTrip.lease.release();
      roundTrip.provider.release();
      leaseGates.delete(roundTrip.run);
      providerGates.delete(roundTrip.run);
      await context.close();
    }
  }, 30_000);
}
