// Web launch and navigation timing, imported only when app.json names launch
// modules. Times are performance.now(), the clock Event.timeStamp and
// Performance entries share. A prerendered page measures from its activation.
// Time-to-first-render (TTR) is the boot tree's first paint. Time-to-interactive
// (TTI) is the first commit after which nothing is outstanding. A navigation
// starts at an input within 1 s before it, else at the commit. A page hidden
// during startup reports no further startup marks.
// Events go to each launch module's web service, which receives earlier events first.
import { inflight, Resources, clock, After, Hosts, Routes, routeAt, busy, Adopted } from './rt.js';
import { Counting } from './held.js';

const STARTUP_TIMER_WINDOW_MS = 1000, TTI_TIMEOUT_MS = 30000;
const events = [], services = [];
const marks = {};
let launch = [], outcome = null, dirty = false, lastTrace = '', trace = [], location0 = null, lastInput = null, nav = null, hidden = false;

const record = (kind, fields = {}) => {
  const e = { kind, at: performance.now(), wall: performance.timeOrigin + performance.now(), ...fields };
  events.push(e);
  if (events.length > 4096) events.shift();
  for (const s of services) s.event(e);
};

/** What the screen still lacks, by name. A request counts only for a resource showing its placeholder: a refresh behind a
 * shown answer and a mutation the app sent don't hold TTI. A one-shot timer counts only when due within 1 s. */
export function outstanding() {
  const out = [];
  for (const r of Resources) if (r.ticket && r.placeholder) out.push(`requests:${r.name}`);
  // Count everything in `inflight` that is not a ticket (`Counting`) as the page's own loading (Markdown, canvas, motion, GPU)
  const loading = inflight.n - Counting.size;
  if (loading > 0) out.push(`loading:${loading}`);
  for (const t of clock.timers) if (t.once && t.ms <= STARTUP_TIMER_WINDOW_MS) out.push(`oneShots:${t.ms}`);
  for (const id of busy()) out.push(`busy:${id}`);
  return out;
}

const nextOpportunity = () => new Promise(r => requestAnimationFrame(() => setTimeout(() => r(performance.now()))));

function finish(o) {
  if (outcome) return;
  outcome = o;
  const metrics = {};
  const from = marks.activation ?? 0;
  if (marks.present !== undefined) metrics.timeToFirstRender = (marks.present - from) / 1000;
  if (marks.interactive !== undefined && ['settled', 'declared'].includes(o)) metrics.timeToInteractive = (marks.interactive - from) / 1000;
  record('startup', { metrics, marks: { ...marks }, tti: o, trace, presentMethod: marks.presentMethod, bootPath: 'web' });
  loadServices();
}

/** Runs after each commit. Records TTI once nothing is outstanding after the first paint; if the screen changed, at the next rendering opportunity. */
async function evaluate() {
  if (hidden) return;
  const out = outstanding(), line = out.join(', ');
  if (line !== lastTrace && trace.length < 32) trace.push(`${Math.round(performance.now())} [${line}]`);
  lastTrace = line;
  if (!outcome && marks.present !== undefined && !out.length) {
    const declared = trace.length > 1 && /^\d+ \[(busy:[^,\]]*(, )?)+\]$/.test(trace[trace.length - 2]);
    const at = dirty ? await nextOpportunity() : performance.now();
    if (outstanding().length || outcome) return;
    marks.interactive = Math.max(at, marks.present);
    dirty = false;
    finish(declared ? 'declared' : 'settled');
  }
  const here = location.pathname + location.search;
  if (location0 !== null && here !== location0 && outcome) startNavigation(here);
  location0 = here;
  if (nav && nav.presented !== undefined && !out.length && !nav.done) {
    nav.done = true;
    record('navigation', { ...nav.fields, name: 'tti', value: (Math.max(performance.now(), nav.presented) - nav.start) / 1000 });
  }
}

/** A location's route pattern (`/photo/:photo`) and its bound parameters, as the native hosts send. */
function route(url) {
  const r = Routes[routeAt(url)];
  if (!r || r.notfound) return { route: url.split('?')[0], routeParams: {} };
  const params = {}, pat = r.pattern.split('/'), parts = url.split('?')[0].split('/');
  pat.forEach((s, i) => { if (s[0] === ':' && parts[i]) params[s.slice(1)] = decodeURIComponent(parts[i]); });
  return { route: r.pattern, routeParams: params };
}

const seen = new Set();
function startNavigation(url) {
  const now = performance.now();
  const input = lastInput !== null && now - lastInput <= 1000 ? lastInput : null;
  lastInput = null;
  const cold = !seen.has(url);
  seen.add(url);
  const fields = { ...route(url), url, 'exact.nav.cause': input !== null ? 'input' : 'program', 'exact.present.method': 'raf' };
  const n = nav = { start: input ?? now, fields };
  nextOpportunity().then(t => {
    if (nav !== n) return;
    n.presented = t;
    record('navigation', { ...fields, name: cold ? 'cold_ttr' : 'warm_ttr', value: (t - n.start) / 1000 });
    // A screen shown before has no TTI (NavigationMarks.swift).
    if (!cold) { n.done = true; return; }
    evaluate();
  });
}

function presentFrom(c0, served) {
  // The first of: Element Timing on the boot tree's first text leaves, the
  // first-contentful-paint at or after the commit, or the next rendering
  // opportunity. A document the server sent, which the boot adopted, painted
  // before this script ran, so its first-contentful-paint counts even though
  // it precedes the commit.
  let done = false;
  const take = (t, method) => { if (done || hidden) return; done = true; marks.present = method === 'served_fcp' ? t : Math.max(t, c0); marks.presentMethod = method; record('mark', { mark: 'present', method }); evaluate(); };
  try {
    new PerformanceObserver(list => { for (const e of list.getEntries()) if (e.identifier?.startsWith('exact-boot')) take(e.renderTime || e.loadTime, 'element'); }).observe({ type: 'element', buffered: true });
  } catch {}
  try {
    new PerformanceObserver(list => { for (const e of list.getEntries()) if (e.name === 'first-contentful-paint' && (served || e.startTime >= c0)) take(e.startTime, served ? 'served_fcp' : 'paint'); }).observe({ type: 'paint', buffered: true });
  } catch {}
  nextOpportunity().then(t => setTimeout(() => take(t, served ? 'raf_served' : 'raf'), 200));
}

/** Called by main.js before the app boots, then with `mounted()` after. */
export function install(modules) {
  launch = modules;
  const nav0 = performance.getEntriesByType?.('navigation')?.[0];
  marks.activation = nav0?.activationStart > 0 ? nav0.activationStart : 0;
  marks.process = 0;
  const background = () => { record('background'); for (const s of services) s.background(); };
  document.addEventListener('visibilitychange', () => { if (document.visibilityState === 'hidden') { background(); if (!outcome && !document.prerendering) { hidden = true; finish('interrupted'); } } });
  addEventListener('pagehide', background);
  for (const t of ['pointerup', 'keydown']) addEventListener(t, e => { lastInput = e.timeStamp; }, { capture: true, passive: true });
  setTimeout(() => finish('timeout'), TTI_TIMEOUT_MS);
  // Errors and Contract's observe commands. The commands' arguments end in a
  // record's key/value pairs.
  addEventListener('error', e => record('app.error', { source: 'global', type: e.error?.name ?? 'Error', message: e.message, stack: e.error?.stack }));
  addEventListener('unhandledrejection', e => record('app.error', { source: 'global', type: e.reason?.name ?? 'UnhandledRejection', message: String(e.reason?.message ?? e.reason), stack: e.reason?.stack }));
  const pairs = a => { const o = {}; for (let i = 0; i + 1 < a.length; i += 2) o[a[i]] = a[i + 1]; return o; };
  Hosts.observe = (name, severity, ...rest) => record('app.event', { name, severity, attributes: pairs(rest) });
  Hosts.observeAttributes = (...rest) => record('app.attributes', { attributes: pairs(rest) });
  Hosts.observeError = (message, type) => record('app.error', { source: 'reportedByUser', message, type: type ?? 'ContractError' });
  globalThis.exact = Object.assign(globalThis.exact ?? {}, { observe: () => ({ marks: { ...marks }, outcome, trace, events: events.length }) });
  return {
    mounted() {
      const c0 = performance.now();
      marks.boot = marks.commit = c0;
      record('mark', { mark: 'commit' });
      // Mark the first few text leaves for Element Timing (before this task ends, so before their paint).
      const root = document.getElementById('exact-root');
      let n = 0;
      for (const el of root?.querySelectorAll('*') ?? []) {
        if (n >= 4) break;
        if (el.childElementCount === 0 && el.textContent.trim()) el.setAttribute('elementtiming', `exact-boot-${n++}`);
      }
      // the document the server sent, when the boot adopted it
      presentFrom(c0, Adopted);
      After.push(() => { dirty = true; queueMicrotask(evaluate); });
      location0 = location.pathname + location.search;
      seen.add(location0);
      record('navigation.launch', { ...route(location0), url: location0 });
    },
  };
}

function loadServices() {
  for (const m of launch) {
    import(/* @vite-ignore */ m.url).then(mod => {
      const s = mod.start(m.config, { boot: marks.activation });
      services.push(s);
      for (const e of events) s.event(e);
    }).catch(e => console.error(`exact: ${m.name} service: ${e}`));
  }
}
