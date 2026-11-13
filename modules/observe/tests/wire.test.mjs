// Observe's web and Apple services against wire.json, the bodies all three services must send.
// The Linux service runs the same fixture in host/painted/tests/observe.
// `bun test ./modules/observe/tests/wire.test.mjs`. The Apple half needs swiftc, so it runs on macOS only.
import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';

const dir = import.meta.dir;
const fixture = await Bun.file(join(dir, 'wire.json')).json();

// The resource keys wire.json compares: those in its expected bodies and the platform's.
const shared = fixture.expected.metrics.resourceMetrics[0].resource.attributes;
const resourceKeys = platform => new Set([...shared.map(a => a.key), ...Object.keys(fixture.platforms[platform].resource)]);

/** A body in the form wire.json compares: see its `about`. */
function normalize(body, platform) {
  const keep = resourceKeys(platform);
  for (const r of Object.values(body)[0]) {
    r.resource.attributes = r.resource.attributes.filter(a => keep.has(a.key)).sort((a, b) => a.key.localeCompare(b.key));
    for (const scope of r.scopeMetrics ?? []) {
      for (const m of scope.metrics) for (const p of m.gauge.dataPoints) for (const a of p.attributes)
        if (a.key === 'expo.custom_params' && typeof a.value.stringValue === 'string') a.value.stringValue = JSON.parse(a.value.stringValue);
      scope.metrics.sort((a, b) => a.name.localeCompare(b.name) || a.gauge.dataPoints[0].timeUnixNano - b.gauge.dataPoints[0].timeUnixNano);
    }
  }
  return body;
}

/** wire.json's expected body with the platform's own resource attributes and launch params added. */
function expected(signal, platform) {
  const p = fixture.platforms[platform];
  const body = structuredClone(fixture.expected[signal]);
  for (const r of Object.values(body)[0]) {
    r.resource.attributes.push(...Object.entries(p.resource).map(([key, v]) => ({ key, value: { stringValue: v } })));
    for (const scope of r.scopeMetrics ?? []) for (const m of scope.metrics) for (const a of m.gauge.dataPoints[0].attributes) {
      if (a.key !== 'expo.custom_params') continue;
      const params = JSON.parse(a.value.stringValue);
      const extra = m.name.startsWith('expo.app_startup.') ? p.startupParams : params.isAppLaunch ? p.launchNavigationParams : {};
      a.value.stringValue = { ...params, ...extra };
    }
  }
  return normalize(body, platform);
}

function check(actual, platform) {
  for (const signal of ['metrics', 'logs']) expect(normalize(actual[signal], platform)).toEqual(expected(signal, platform));
}

test("the web service sends wire.json's bodies", async () => {
  const storage = new Map([['expo.eas-client-id', fixture.clientId]]);
  const sent = {};
  const names = ['localStorage', 'navigator', 'setTimeout', 'fetch'];
  const saved = Object.fromEntries(names.map(n => [n, Object.getOwnPropertyDescriptor(globalThis, n)]));
  const randomUUID = crypto.randomUUID;
  Object.assign(globalThis, {
    localStorage: { getItem: k => storage.get(k) ?? null, setItem: (k, v) => storage.set(k, String(v)) },
    navigator: { platform: 'MacIntel', language: 'en-US', onLine: true, connection: { type: 'wifi' } },
    setTimeout: () => 0,
    fetch: async (url, init) => { sent[url.split('/').pop()] = JSON.parse(init.body); return new Response(null, { status: 200 }); },
  });
  crypto.randomUUID = () => fixture.session;
  try {
    const { start } = await import('../web/service.js');
    const service = start({ 'app.CFBundleIdentifier': fixture.app.id, 'app.CFBundleName': fixture.app.name, environment: fixture.environment, projectId: 'p', endpoint: 'https://example.com' });
    for (const e of fixture.events) service.event({ ...e, wall: e.wall * 1000 });
    service.background();
    await new Promise(r => saved.setTimeout.value(r, 0));
  } finally {
    for (const n of names) saved[n] ? Object.defineProperty(globalThis, n, saved[n]) : delete globalThis[n];
    crypto.randomUUID = randomUUID;
  }
  check(sent, 'web');
});

test('the web service retains only current and queued session metadata', async () => {
  const names = ['localStorage', 'navigator', 'setTimeout', 'fetch'];
  const saved = Object.fromEntries(names.map(n => [n, Object.getOwnPropertyDescriptor(globalThis, n)]));
  const randomUUID = crypto.randomUUID;
  try {
    const { start } = await import('../web/service.js');
    for (const dispatchingEnabled of [true, false]) {
      const key = `exact-observe:${fixture.app.id}`;
      const metrics = [{ session: 'metrics-session', time: 1, category: 'navigation', name: 'tti', value: 0.1, params: '{}' }];
      const logs = [{ session: 'logs-session', time: 1, severity: 'info', name: 'retained', attributes: {} }];
      const storage = new Map([['expo.eas-client-id', fixture.clientId], [key, JSON.stringify({
        sessions: { orphan: {}, 'metrics-session': {}, 'logs-session': {} }, metrics, logs,
      })]]);
      let session, flush, nextSession = 0, requests = 0;
      crypto.randomUUID = () => { session = `session-${++nextSession}`; return session; };
      Object.assign(globalThis, {
        localStorage: { getItem: k => storage.get(k) ?? null, setItem: (k, v) => storage.set(k, String(v)) },
        navigator: { platform: 'MacIntel', language: 'en-US' },
        setTimeout: fn => { flush = fn; return 0; },
        fetch: async () => { requests++; return new Response(null, { status: 200 }); },
      });
      for (let launch = 0; launch < 51; launch++) {
        start({ 'app.CFBundleIdentifier': fixture.app.id, projectId: 'p', endpoint: 'https://example.com', dispatchingEnabled });
        const queue = JSON.parse(storage.get(key));
        expect(Object.keys(queue.sessions).sort()).toEqual(['metrics-session', 'logs-session', session].sort());
        expect(queue.metrics).toEqual(metrics);
        expect(queue.logs).toEqual(logs);
      }
      expect(requests).toBe(0);
      await flush();
      const queue = JSON.parse(storage.get(key));
      expect(queue.metrics).toEqual([]);
      expect(queue.logs).toEqual([]);
      expect(Object.keys(queue.sessions)).toEqual([session]);
      expect(requests).toBe(dispatchingEnabled ? 2 : 0);
    }
  } finally {
    for (const n of names) saved[n] ? Object.defineProperty(globalThis, n, saved[n]) : delete globalThis[n];
    crypto.randomUUID = randomUUID;
  }
});

test('the web service removes only processed rows when appends trim a request in flight', async () => {
  const names = ['localStorage', 'navigator', 'setTimeout', 'fetch'];
  const saved = Object.fromEntries(names.map(n => [n, Object.getOwnPropertyDescriptor(globalThis, n)]));
  try {
    const { start } = await import('../web/service.js');
    for (const status of [200, 400, 413]) {
      const key = `exact-observe:${fixture.app.id}`;
      const logs = Array.from({ length: 2000 }, (_, n) => ({ session: fixture.session, time: n, severity: 'info', name: `event-${n}`, attributes: {} }));
      const storage = new Map([['expo.eas-client-id', fixture.clientId], [key, JSON.stringify({ sessions: { [fixture.session]: {} }, metrics: [], logs })]]);
      let dispatch, reply, selected, next;
      const started = new Promise(resolve => { selected = resolve; });
      const continued = new Promise(resolve => { next = resolve; });
      let processed;
      Object.assign(globalThis, {
        localStorage: { getItem: k => storage.get(k) ?? null, setItem: (k, v) => storage.set(k, String(v)) },
        navigator: { platform: 'MacIntel', language: 'en-US' },
        setTimeout: fn => { dispatch = fn; return 1; },
        fetch: async (_url, init) => {
          const rows = JSON.parse(init.body).resourceLogs.flatMap(r => r.scopeLogs.flatMap(s => s.logRecords));
          if (status === 413 && rows.length > 1 && processed === undefined) {
            return new Response(null, { status: 413 });
          }
          if (processed === undefined) {
            processed = rows.length;
            selected();
            return new Promise(resolve => { reply = resolve; });
          }
          next();
          return new Response(null, { status: 503 });
        },
      });
      const service = start({ 'app.CFBundleIdentifier': fixture.app.id, projectId: 'p', endpoint: 'https://example.com' });
      const finished = dispatch();
      await started;
      expect(processed).toBe(status === 413 ? 1 : 200);
      service.event({ kind: 'app.event', name: 'event-2000', wall: 2000, attributes: {} });
      reply(new Response(null, { status }));
      await continued;
      await finished;
      const retained = JSON.parse(storage.get(key)).logs;
      expect(retained.map(row => row.name)).toEqual(Array.from({ length: 2001 - processed }, (_, n) => `event-${n + processed}`));
    }
  } finally {
    for (const n of names) saved[n] ? Object.defineProperty(globalThis, n, saved[n]) : delete globalThis[n];
  }
});

test('the web service retries idle queues after shared backoff and rearms an earlier flush', async () => {
  const names = ['localStorage', 'navigator', 'setTimeout', 'fetch'];
  const saved = Object.fromEntries(names.map(n => [n, Object.getOwnPropertyDescriptor(globalThis, n)]));
  const originalNow = Date.now, originalRandom = Math.random;
  try {
    const { start } = await import('../web/service.js');
    for (const [transportFailure, duringRequest, bothSignals] of [[false, false], [true, false], [false, true], [true, true], [false, false, true]]) {
      const storage = new Map([['expo.eas-client-id', fixture.clientId]]);
      const timers = new Map();
      let now = 0, timerId = 0, requests = 0, service;
      Date.now = () => now;
      Math.random = () => 0.5;
      Object.assign(globalThis, {
        localStorage: { getItem: k => storage.get(k) ?? null, setItem: (k, v) => storage.set(k, String(v)) },
        navigator: { platform: 'MacIntel', language: 'en-US' },
        setTimeout: (fn, delay) => { timers.set(++timerId, { fn, at: now + delay }); return timerId; },
        fetch: async () => {
          requests++;
          if (requests === 1) {
            if (duringRequest) {
              service.event({ kind: 'app.event', name: 'during-request', wall: now, attributes: {} });
            }
            if (transportFailure) {
              throw new TypeError('connection failed');
            }
            return new Response(null, { status: 503, headers: { 'Retry-After': '60' } });
          }
          return new Response(null, { status: 200 });
        },
      });
      const runTimer = async () => {
        expect(timers.size).toBe(1);
        const [id, timer] = timers.entries().next().value;
        timers.delete(id);
        now = timer.at;
        await timer.fn();
      };
      service = start({ 'app.CFBundleIdentifier': fixture.app.id, projectId: 'p', endpoint: 'https://example.com' });
      if (bothSignals) {
        service.event({ kind: 'navigation', name: 'cold_ttr', value: 1, wall: now, route: '/', routeParams: {} });
      }
      service.event({ kind: 'app.event', name: 'initial', wall: now, attributes: {} });
      await runTimer();
      expect(requests).toBe(1);
      if (bothSignals) {
        const queued = JSON.parse(storage.get(`exact-observe:${fixture.app.id}`));
        expect(queued.metrics).toHaveLength(1);
        expect(queued.logs).toHaveLength(1);
      }
      expect(timers.size).toBe(1);
      if (duringRequest) {
        await runTimer();
        expect(now).toBe(10000);
        expect(requests).toBe(1);
        expect(timers.size).toBe(1);
      }
      await runTimer();
      expect(now).toBe(transportFailure ? 35000 : 65000);
      expect(requests).toBe(bothSignals ? 3 : 2);
      const queued = JSON.parse(storage.get(`exact-observe:${fixture.app.id}`));
      expect(queued.metrics).toEqual([]);
      expect(queued.logs).toEqual([]);
      expect(timers.size).toBe(0);
    }
  } finally {
    for (const n of names) saved[n] ? Object.defineProperty(globalThis, n, saved[n]) : delete globalThis[n];
    Date.now = originalNow;
    Math.random = originalRandom;
  }
});

test('the web service schedules remaining metrics after logs finish, including zero-delay retries', async () => {
  const names = ['localStorage', 'navigator', 'setTimeout', 'fetch'];
  const saved = Object.fromEntries(names.map(n => [n, Object.getOwnPropertyDescriptor(globalThis, n)]));
  const originalNow = Date.now, originalRandom = Math.random;
  try {
    const { start } = await import('../web/service.js');
    for (const failMetric of [false, true]) {
      const key = `exact-observe:${fixture.app.id}`;
      const storage = new Map([['expo.eas-client-id', fixture.clientId]]);
      const timers = new Map();
      let now = 0, timerId = 0, requests = 0, reply, started;
      const logsStarted = new Promise(resolve => { started = resolve; });
      Date.now = () => now;
      Math.random = () => 0;
      Object.assign(globalThis, {
        localStorage: { getItem: k => storage.get(k) ?? null, setItem: (k, v) => storage.set(k, String(v)) },
        navigator: { platform: 'MacIntel', language: 'en-US' },
        setTimeout: (fn, delay) => { timers.set(++timerId, { fn, at: now + delay }); return timerId; },
        fetch: async url => {
          requests++;
          if (url.endsWith('/logs')) {
            started();
            return new Promise(resolve => { reply = resolve; });
          }
          return new Response(null, { status: failMetric && requests === 1 ? 503 : 200 });
        },
      });
      const runTimer = () => {
        expect(timers.size).toBe(1);
        const [id, timer] = timers.entries().next().value;
        timers.delete(id);
        now = timer.at;
        return timer.fn();
      };
      const metric = () => service.event({ kind: 'navigation', name: 'cold_ttr', value: 1, wall: now, route: '/', routeParams: {} });
      const service = start({ 'app.CFBundleIdentifier': fixture.app.id, projectId: 'p', endpoint: 'https://example.com' });
      metric();
      service.event({ kind: 'app.event', name: 'initial', wall: now, attributes: {} });
      const finished = runTimer();
      await logsStarted;
      if (!failMetric) {
        metric();
        await runTimer();
      }
      expect(requests).toBe(2);
      expect(timers.size).toBe(0);
      reply(new Response(null, { status: 200 }));
      await finished;
      expect(JSON.parse(storage.get(key)).metrics).toHaveLength(1);
      await runTimer();
      expect(requests).toBe(3);
      expect(JSON.parse(storage.get(key)).metrics).toEqual([]);
      expect(JSON.parse(storage.get(key)).logs).toEqual([]);
      expect(timers.size).toBe(0);
    }
  } finally {
    for (const n of names) saved[n] ? Object.defineProperty(globalThis, n, saved[n]) : delete globalThis[n];
    Date.now = originalNow;
    Math.random = originalRandom;
  }
});

test.skipIf(process.platform !== 'darwin')('the Apple service sends wire.json\'s bodies', () => {
  const work = mkdtempSync(join(tmpdir(), 'observe-wire-'));
  try {
    const exe = join(work, 'harness');
    const service = ['ObserveService', 'ObserveWire', 'ObserveRules', 'ObserveStore'].map(f => join(dir, `../apple/service/${f}.swift`));
    const build = spawnSync('xcrun', ['swiftc', '-parse-as-library', '-swift-version', '5', '-module-cache-path', join(work, 'cache'),
      ...service, join(dir, 'wire.swift'), '-lsqlite3', '-o', exe], { encoding: 'utf8' });
    expect(build.status, build.stderr).toBe(0);
    const run = spawnSync(exe, [join(dir, 'wire.json'), join(work, 'store')], { encoding: 'utf8' });
    expect(run.status, run.stderr).toBe(0);
    const out = JSON.parse(run.stdout);
    check(out, 'apple');
    expect(out.initialization.threads.totalOpens).toBeGreaterThan(0);
    expect(out.initialization.threads.mainOpens).toBe(0);
    // What an app's own screen reads back (`ExactServices.query("observe", …)`):
    // the newest rows first, none sent yet, at most the limit.
    const { session, metrics } = out.recent;
    expect(session).toBe(fixture.session);
    expect(metrics.length).toBeLessThanOrEqual(3);
    expect(metrics.length).toBeGreaterThan(0);
    expect(metrics.every(m => m.sent === false && typeof m.value === 'number' && typeof m.name === 'string')).toBe(true);
    expect(metrics.map(m => m.time)).toEqual([...metrics.map(m => m.time)].sort((a, b) => b - a));
    expect(out.retry.scheduled).toBe(true);
    expect(out.retry.requests).toEqual(['metrics']);
    expect(out.retry.earlyFlushRearms).toBe(true);
    expect(out.retry.finalRequests).toEqual(['metrics', 'metrics', 'logs']);
    expect(out.retry.resent.metrics.length).toBeGreaterThan(0);
    expect(out.retry.resent.metrics.every(metric => metric.sent === true)).toBe(true);
    expect(out.unknown.error).toContain('recent');
    expect(out.refused.metrics.length).toBeGreaterThan(0);
    expect(out.refused.metrics.every(metric => metric.sent === false)).toBe(true);
    expect(out.acknowledged.metrics.filter(metric => metric.sent === true).length).toBe(1);
  } finally { rmSync(work, { recursive: true, force: true }); }
}, 120_000);
