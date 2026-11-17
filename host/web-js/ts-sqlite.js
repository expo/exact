// The module's SQLite databases on the web JS target (LLP 1027 D10): loaded with the page's SQLite worker at the
// first `storage.sqlite.open` (ts-data.js `databases`), so a module that opens none carries none of this. ts-data.js,
// which loads it, passes its storage queue and its set of open databases in (rt-link.js says why a lazy chunk never
// imports the module that loads it).
import { owned } from './ts-fetch.js';
// A database's and a statement's methods refuse as storage's do, in the queue. A query in an answer watches the
// database's topic, and an execute or a transaction that changed rows tells the app's other tabs (LLP 1030.004 D2;
// js/src/storage.js `changedRows` says why only DML that changed rows counts).
const changedRows = (sql, v) => typeof sql === 'string' && /^\s*(insert|update|delete|replace|with)\b/i.test(sql) && v?.changes > 0;
const changed = (m, args, v, prepared) => m === 'transaction'
  ? Array.isArray(v) && Array.isArray(args[0]) && v.some((r, i) => changedRows(args[0][i]?.sql, r))
  : m === 'execute' && changedRows(prepared ?? args[0], v);
const wrap = (o, convert, path, queued, shared, call, prepared) => Object.freeze(Object.fromEntries(Object.entries(convert).map(([m, then]) =>
  [m, (...args) => {
    const topic = [`sqlite:${path}`];
    if (m === 'query') shared.watch(topic, call);
    const done = queued(`${m} ${path}`, () => o[m](...args)).then(v => { if (changed(m, args, v, prepared)) shared.relay(topic); return v; });
    return owned((then ? done.then(v => then(v, path, args[0])) : done), true);
  }])));
const statement = (queued, shared, call) => (s, path, sql) => wrap(s, { execute: null, query: null, close: null }, path, queued, shared, call, sql);
// Each entry's sql and params read once; options as a record with no
// prototype, so `__proto__` stays a key the validation refuses.
const batchNow = c => Array.isArray(c) ? c.map(e => {
  if (!e || typeof e !== 'object') return e;
  const { sql, params } = e;
  return { sql, params: Array.isArray(params) ? params.map(v => v instanceof Uint8Array ? new Uint8Array(v) : v) : params };
}) : c;
const optionsNow = o => {
  if (o === null || typeof o !== 'object') return o;
  const f = o.foreignKeys, r = Object.create(null);
  for (const k of Object.keys(o)) if (k !== 'foreignKeys') Object.defineProperty(r, k, { value: true, enumerable: true });
  if (f !== undefined) r.foreignKeys = f;
  return r;
};
export const database = (d, path, call, queued, openDatabases, shared) => {
  const handle = { path, call, told: false };
  openDatabases.add(handle);
  const wrapped = wrap(d, { execute: null, query: null, prepare: statement(queued, shared, call), transaction: null, close: null }, path, queued, shared, call);
  // A transaction's batch and options as they are at the call, not when the
  // queue reaches it (js/src/storage.js `batchNow`, `optionsNow`).
  const db = { ...wrapped, transaction: (commands, options) => {
    let args;
    try { args = [batchNow(commands), optionsNow(options)]; }
    catch (e) { return Promise.reject(Object.assign(new Error(String(e?.message ?? e)), { kind: 'Unavailable', code: 'failed' })); }
    return wrapped.transaction(...args);
  } };
  // A close counts once issued; one refused or failed leaves the database open, and tracked.
  return Object.freeze({ ...db, close: (...args) => {
    openDatabases.delete(handle);
    // A failed close still rejects for the caller, so one left unhandled is still reported.
    return db.close(...args).catch((e) => { openDatabases.add(handle); throw e; });
  } });
};
