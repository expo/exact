// Observe's web service, loaded by the host's marks.js after startup. It queues journal
// events as Observe rows in localStorage until the server accepts them, and sends them
// in expo-observe's OTLP/JSON with its retry rules.
// Every tab of the app shares the stored queue. A tab adds its rows to what is stored, and
// sends while holding a Web Lock named for the queue, so one tab at a time sends and each
// row is sent once (https://w3c.github.io/web-locks/). Each change to the stored queue, an
// append or a send's removal, holds a second lock, so tabs in other processes do not save
// over each other's changes (localStorage makes each getItem and setItem atomic, not the
// pair) and an append never waits on a send's fetch.
// On page hide it flushes with fetch(keepalive), limiting each signal's JSON length
// to 60 * 1024 UTF-16 code units. The browser's shared byte limit may still reject
// a request; those rows stay queued for retry.

const SCHEMA_URL = 'https://opentelemetry.io/schemas/1.27.0';
const NAMES = {
  'appStartup/timeToInteractive': 'expo.app_startup.tti', 'appStartup/timeToFirstRender': 'expo.app_startup.ttr',
  'appStartup/coldLaunchTime': 'expo.app_startup.cold_launch_time', 'appStartup/warmLaunchTime': 'expo.app_startup.warm_launch_time',
  'updates/updateDownloadTime': 'expo.updates.download_time',
  'navigation/cold_ttr': 'expo.navigation.cold_ttr', 'navigation/warm_ttr': 'expo.navigation.warm_ttr', 'navigation/tti': 'expo.navigation.tti',
};
const SEVERITY = { trace: 1, debug: 5, info: 9, warn: 13, error: 17, fatal: 21 };
const KEEPALIVE_BUDGET = 60 * 1024, CHUNK = 200, MAX_ROWS = 2000;

const uuid = () => crypto.randomUUID();
const stringAttr = (key, v) => ({ key, value: { stringValue: v } });

/** Same as expo's `EASClientID.deterministicUniformValue`: a stable value in [0, 1) per install. */
export function uniform(id) {
  const hex = id.replace(/-/g, '');
  let z = BigInt('0x' + hex.slice(0, 16)) ^ BigInt('0x' + hex.slice(16, 32));
  const m = (1n << 64n) - 1n;
  z = ((z ^ (z >> 30n)) * 0xbf58476d1ce4e5b9n) & m;
  z = ((z ^ (z >> 27n)) * 0x94d049bb133111ebn) & m;
  z = z ^ (z >> 31n);
  return Number(z >> 11n) / 2 ** 53;
}

function anyValue(v) {
  if (typeof v === 'boolean') return { boolValue: v };
  if (typeof v === 'number') return Number.isFinite(v) ? (Number.isInteger(v) ? { intValue: v } : { doubleValue: v }) : null;
  if (typeof v === 'string') return { stringValue: v };
  if (Array.isArray(v)) { const a = v.map(anyValue); return a.every(Boolean) ? { arrayValue: { values: a } } : null; }
  if (v && typeof v === 'object') {
    const values = [];
    for (const k of Object.keys(v).sort()) { const m = anyValue(v[k]); if (!m) return null; values.push({ key: k, value: m }); }
    return { kvlistValue: { values } };
  }
  return null;
}

/** Observe's custom-event validation, copied from expo-app-metrics (`LogEvents`). */
function rulesName(raw) { const n = String(raw ?? '').trim(); return n && !n.startsWith('expo.') && n.length <= 256 ? n : null; }
const truncate = (s, max) => (s.length <= max ? s : s.slice(0, max - 1) + '…');
function rulesAttributes(raw) {
  const kept = {}; let dropped = 0;
  for (const key of Object.keys(raw ?? {}).sort()) {
    const k = key.trim();
    if (!k || (k.startsWith('expo.') && k.length > 5) || k === 'session.id' || k === 'event.name' || Object.keys(kept).length >= 128) { dropped++; continue; }
    kept[k] = raw[key];
  }
  return { kept, dropped };
}

/** `stored` without one row equal to each of `sent`, matched from the front. */
function without(stored, sent) {
  const left = new Map();
  for (const row of sent) { const k = JSON.stringify(row); left.set(k, (left.get(k) ?? 0) + 1); }
  return stored.filter(row => {
    const k = JSON.stringify(row), n = left.get(k);
    if (!n) return true;
    left.set(k, n - 1);
    return false;
  });
}

export function start(config, { boot = 0 } = {}) {
  const key = `exact-observe:${config['app.CFBundleIdentifier'] ?? location.host}`;
  const clientId = localStorage.getItem('expo.eas-client-id') ?? uuid();
  localStorage.setItem('expo.eas-client-id', clientId);
  const session = uuid();
  const meta = {
    osName: navigator.userAgentData?.platform ?? navigator.platform, language: navigator.language, clientVersion: '0.1.0',
    appIdentifier: config['app.CFBundleIdentifier'], appName: config['app.CFBundleName'],
    environment: config.environment ?? (config['fact.development'] ? 'development' : 'production'),
  };
  const load = () => (() => { try { return JSON.parse(localStorage.getItem(key)); } catch {} })() ?? { sessions: {}, metrics: [], logs: [] };
  const save = q => {
    q.metrics = q.metrics.slice(-MAX_ROWS); q.logs = q.logs.slice(-MAX_ROWS);
    const live = new Set([session, ...q.metrics.map(r => r.session), ...q.logs.map(r => r.session)]);
    for (const id of Object.keys(q.sessions)) {
      if (!live.has(id)) { delete q.sessions[id]; }
    }
    try { localStorage.setItem(key, JSON.stringify(q)); } catch {}
  };
  // A read-modify-write of the stored queue, under the edit lock where there are Web Locks;
  // it resolves once saved. The session's metadata goes with each change, since another
  // tab's save drops it while this session has no queued rows.
  const update = change => {
    const edit = () => { const q = load(); q.sessions[session] = meta; change(q); save(q); };
    return navigator.locks ? navigator.locks.request(`${key}:edit`, edit) : edit();
  };
  const queued = () => { const q = load(); return q.metrics.length > 0 || q.logs.length > 0; };
  update(() => {});
  let globals = {}, gate = { after: 0, failures: 0 }, sending = false, timer = null, launchRoute = null, navigated = false;
  const params = extra => ({ ...globals, ...extra });
  const metric = (category, name, value, wall, route, p) => { update(q => q.metrics.push({ session, time: wall / 1000, category, name, value, route, params: JSON.stringify(p) })); schedule(); };
  const log = (severity, name, body, attributes, dropped, wall) => { update(q => q.logs.push({ session, time: wall / 1000, severity, name, body, attributes, dropped })); schedule(); };

  function startup(e) {
    const m = e.marks ?? {}, from = m.activation ?? 0;
    const common = { 'exact.present.method': e.presentMethod ?? 'raf', 'exact.since_process_start.ttr': m.present !== undefined ? (m.present - from) / 1000 : undefined };
    for (const [name, value] of Object.entries(e.metrics ?? {})) {
      const p = params(common);
      if (name === 'timeToInteractive') { p['exact.tti.reason'] = e.tti; if (e.trace?.length) p['exact.tti.trace'] = e.trace.join(' → '); Object.assign(p, device()); }
      metric('appStartup', name, value, e.wall, undefined, p);
    }
    if (launchRoute && m.present !== undefined) {
      const p = params({ isAppLaunch: true, routeParams: launchRoute.routeParams ?? {}, url: launchRoute.url, 'exact.nav.anchor': 'activation' });
      metric('navigation', 'cold_ttr', (m.present - from) / 1000, e.wall, launchRoute.route, p);
      if (m.interactive !== undefined && e.metrics?.timeToInteractive !== undefined && !navigated) metric('navigation', 'tti', (m.interactive - from) / 1000, e.wall, launchRoute.route, p);
    }
  }
  function device() {
    const c = navigator.connection;
    const d = { 'expo.network.connected': navigator.onLine, 'expo.network.type': !navigator.onLine ? 'none' : c?.type && c.type !== 'unknown' ? c.type : 'unknown' };
    if (c?.effectiveType) d['exact.network.effectiveType'] = c.effectiveType;
    return d;
  }

  const resource = meta => {
    const a = [['os.type', 'browser'], ['os.name', meta.osName], ['browser.language', meta.language], ['telemetry.sdk.name', 'exact-observe'],
      ['telemetry.sdk.version', meta.clientVersion], ['telemetry.sdk.language', 'javascript'], ['expo.eas_client.id', clientId],
      ['service.name', meta.appIdentifier], ['expo.app.name', meta.appName], ['expo.environment', meta.environment]];
    return { attributes: a.filter(([, v]) => v != null).map(([k, v]) => stringAttr(k, String(v))) };
  };
  /** `{outer: [{resource, inner: [{scope, list: items}], schemaUrl}]}`, one resource per session. */
  const envelope = (sessions, by, outer, inner, list) => ({ [outer]: Object.keys(by).filter(s => sessions[s]).map(s => ({
    resource: resource(sessions[s]), [inner]: [{ scope: { name: 'expo-observe', version: sessions[s].clientVersion ?? '0' }, [list]: by[s] }], schemaUrl: SCHEMA_URL })) });
  // A JSON number, as expo-observe sends it.
  const nanos = s => Math.round(s * 1000) * 1e6;
  function metricsBody(rows, sessions) {
    const by = {};
    for (const r of rows) {
      const attrs = [stringAttr('session.id', r.session)];
      if (r.route) attrs.push(stringAttr('expo.route_name', r.route));
      if (r.params && r.params !== '{}') attrs.push(stringAttr('expo.custom_params', r.params));
      (by[r.session] ??= []).push({ unit: 's', name: NAMES[`${r.category}/${r.name}`] ?? `expo.unknown.${r.name}`, gauge: { dataPoints: [{ timeUnixNano: nanos(r.time), asDouble: r.value, attributes: attrs }] } });
    }
    return envelope(sessions, by, 'resourceMetrics', 'scopeMetrics', 'metrics');
  }
  function logsBody(rows, sessions) {
    const by = {};
    for (const r of rows) {
      const attrs = [stringAttr('session.id', r.session), stringAttr('event.name', r.name)];
      let dropped = r.dropped ?? 0;
      for (const k of Object.keys(r.attributes ?? {}).sort()) { const v = anyValue(r.attributes[k]); if (v) attrs.push({ key: k, value: v }); else dropped++; }
      const rec = { timeUnixNano: nanos(r.time), observedTimeUnixNano: nanos(r.time), severityNumber: SEVERITY[r.severity] ?? 9, severityText: r.severity.toUpperCase(), body: { stringValue: r.body ?? '' }, attributes: attrs };
      if (dropped) rec.droppedAttributesCount = dropped;
      (by[r.session] ??= []).push(rec);
    }
    return envelope(sessions, by, 'resourceLogs', 'scopeLogs', 'logRecords');
  }

  const inSample = () => uniform(clientId) < Math.min(Math.max(config.sampleRate ?? 1, 0), 1);
  const shouldDispatch = () => (config.dispatchingEnabled ?? true) && inSample() && (!config['fact.development'] || config.dispatchInDebug === true);
  const backoff = n => Math.min(60 * 2 ** (n - 1), 900) * Math.random();
  const retryAfter = h => { if (!h) return null; const s = Number(h); const clamp = x => Math.min(Math.max(x, 60), 900); if (Number.isFinite(s)) return clamp(s); const d = Date.parse(h); return Number.isNaN(d) ? null : clamp((d - Date.now()) / 1000); };

  /** Sends one signal's rows chunk by chunk, as expo-observe's `DispatchLoop.swift` does.
   * With `keepalive`, it also stops at the per-signal JSON-length budget. */
  async function send(signal, url, keepalive) {
    if (Date.now() < gate.after) return;
    let limit = CHUNK, budget = KEEPALIVE_BUDGET;
    for (;;) {
      const q = load(), rows = q[signal].slice(0, limit);
      if (!rows.length) return;
      const body = JSON.stringify(signal === 'metrics' ? metricsBody(rows, q.sessions) : logsBody(rows, q.sessions));
      if (keepalive && body.length > budget) {
        if (rows.length > 1) { limit = rows.length >> 1; continue; }
        return; // Keep a row over the JSON-length budget for a normal fetch
      }
      let status = null, retry = null;
      try {
        const r = await fetch(url, { method: 'POST', body, keepalive, headers: { 'Content-Type': 'application/json', 'Expo-AppMetrics-Skip': '1' } });
        status = r.status; retry = r.headers.get('Retry-After');
      } catch {}
      if (keepalive) budget -= body.length;
      if (globalThis.EXACT_OBSERVE_LOG) console.log(`observe: ${signal} ${rows.length} rows → ${status ?? 'transport error'}`);
      if (status === 413 && rows.length > 1) { gate.failures = 0; limit = rows.length >> 1; continue; }
      if (status !== null && ![429, 502, 503, 504].includes(status)) {
        gate.failures = 0;
        // Remove only this request's rows: this tab or another may have added rows, and
        // the capacity trim dropped old ones, during the fetch.
        await update(q => { q[signal] = without(q[signal], rows); });
        limit = CHUNK;
        continue;
      }
      gate.failures++; gate.after = Date.now() + 1000 * (retryAfter(retry) ?? backoff(gate.failures));
      return;
    }
  }
  async function drain(keepalive) {
    if (!shouldDispatch() || !config.projectId) { await update(q => { q.metrics = []; q.logs = []; }); return; }
    const base = String(config.endpoint ?? 'https://o.expo.dev').replace(/\/+$/, '');
    await send('metrics', `${base}/${config.projectId}/v1/metrics`, keepalive);
    await send('logs', `${base}/${config.projectId}/v1/logs`, keepalive);
  }
  // Without Web Locks (a page that is not a secure context) each tab sends alone, and a
  // row two tabs read before either removes it is sent twice.
  const exclusive = work => navigator.locks ? navigator.locks.request(key, work) : work();
  async function dispatch(keepalive = false) {
    if (sending || !queued()) return;
    if (Date.now() < gate.after) { schedule(gate.after - Date.now()); return; }
    sending = true;
    try {
      await exclusive(() => drain(keepalive));
    } finally {
      sending = false;
      if (queued()) schedule(Math.max(0, gate.after - Date.now()));
    }
  }
  function schedule(delay = 5000) { if (timer === null) timer = setTimeout(() => { timer = null; return dispatch(); }, delay); }
  schedule();

  return {
    event(e) {
      switch (e.kind) {
        case 'startup': startup(e); break;
        case 'navigation.launch': launchRoute = e; break;
        case 'navigation': navigated = true; metric('navigation', e.name, e.value, e.wall, e.route, params({ isAppLaunch: false, routeParams: e.routeParams ?? {}, url: e.url, ...Object.fromEntries(Object.entries(e).filter(([k]) => k.startsWith('exact.'))) })); break;
        case 'app.attributes': globals = rulesAttributes(e.attributes).kept; break;
        case 'app.event': {
          const name = rulesName(e.name); if (!name) break;
          const a = rulesAttributes(e.attributes);
          const attrs = { ...globals, ...a.kept };
          if (e.displayName?.trim()) attrs['expo.log.display_name'] = truncate(e.displayName.trim(), 128);
          log(SEVERITY[e.severity] ? e.severity : 'info', name, e.body == null ? undefined : truncate(String(e.body), 4096), attrs, a.dropped, e.wall);
          break;
        }
        case 'app.error': log('error', 'js.exception', undefined, { 'expo.error.source': e.source ?? 'reportedByUser', 'expo.error.is_fatal': false, 'exception.type': e.type ?? 'Error', 'exception.message': e.message ?? '', ...(e.stack ? { 'exception.stacktrace': e.stack } : {}) }, 0, e.wall); break;
      }
    },
    background() { return dispatch(true); },
  };
}
