// @ref LLP 1027 D10 — worker-owned SQLite capability.
import { authorize } from './storage-fs.js';

// Owners exist before their first query, but workers still start lazily.
const workers = new Map();
let nextClient = 0;
const failure = message => Object.assign(new Error(message), { kind: 'Unavailable' });

// Absent (`undefined` or `null`), or `{ foreignKeys: 'check' }`; anything
// else is refused, so a misspelt option never runs a migration enforced. The
// words are the native binding's (ibex2 bindings/sqlite.js), and the code is
// `failed`, as the native prelude gives a binding's argument error: never
// read from the words, which name the app's key.
function transactionMode(options) {
  const invalid = message => Object.assign(new Error(message), { kind: 'Unavailable', code: 'failed' });
  if (options === undefined || options === null) return undefined;
  if (typeof options !== 'object') throw invalid('SQLite transaction options must be an object');
  for (const key of Object.keys(options)) if (key !== 'foreignKeys') throw invalid(`SQLite transaction options: unknown option ${key}; the one option is foreignKeys`);
  const { foreignKeys } = options;
  if (foreignKeys !== undefined && foreignKeys !== 'check') throw invalid('SQLite transaction options: foreignKeys is "check" or absent');
  return foreignKeys;
}

function connect(appId) {
  let shared = workers.get(appId);
  if (!shared) {
    shared = { worker: null, clients: new Map() };
    workers.set(appId, shared);
  }
  const client = ++nextClient, pending = new Map();
  let sequence = 0, disposed = false;
  function stop(message) {
    if (disposed) return;
    disposed = true;
    shared.clients.delete(client);
    for (const { reject } of pending.values()) reject(failure(message));
    pending.clear();
    if (!shared.clients.size) {
      shared.worker?.terminate();
      if (workers.get(appId) === shared) workers.delete(appId);
    } else shared.worker?.postMessage({ client, op: 'dispose' });
  }
  shared.clients.set(client, { pending, stop });
  function request(op, args) {
    if (disposed) return Promise.reject(failure('storage runtime is unloaded'));
    if (!shared.worker) {
      const worker = shared.worker = new Worker(new URL('./storage-worker.js', import.meta.url), { type: 'module' });
      worker.onmessage = ({ data }) => {
        if (data.fatal) { failed(data.fatal); return; }
        const calls = shared.clients.get(data.client)?.pending;
        const call = calls?.get(data.id);
        if (!call) return;
        calls.delete(data.id);
        if (data.error) call.reject(Object.assign(failure(data.error.message), data.error));
        else call.resolve(data.value);
      };
      const failed = message => {
        for (const owner of [...shared.clients.values()]) owner.stop(message);
      };
      worker.onerror = event => { event.preventDefault(); failed('SQLite worker failed'); };
      worker.onmessageerror = () => failed('SQLite worker message failed');
    }
    const id = ++sequence;
    return new Promise((resolve, reject) => {
      pending.set(id, { resolve, reject });
      try { shared.worker.postMessage({ client, id, op, args, appId }); }
      catch (error) { pending.delete(id); reject(failure(error.message)); }
    });
  }
  return { request, dispose: () => stop('storage runtime is unloaded') };
}

// The filesystem has already admitted the path and obtained its pool id from IndexedDB.
export async function databaseImage(appId, path, pool, sizeOnly, maxBytes) {
  const client = connect(appId);
  try { return await client.request('image', [path, pool, sizeOnly, maxBytes]); }
  finally { client.dispose(); }
}

export function createSqlite(appId, grants) {
  const { request, dispose } = connect(appId);
  function statement(database, id) {
    return Object.freeze({
      execute: (params = []) => request('statementExecute', [database, id, params]),
      query: (params = []) => request('statementQuery', [database, id, params]),
      close: () => request('statementClose', [database, id]),
    });
  }
  function database(id) {
    return Object.freeze({
      execute: (sql, params = []) => request('execute', [id, sql, params]),
      query: (sql, params = []) => request('query', [id, sql, params]),
      prepare: sql => request('prepare', [id, sql]).then(s => statement(id, s)),
      transaction: (commands, options) => {
        let foreignKeys;
        try { foreignKeys = transactionMode(options); } catch (error) { return Promise.reject(error); }
        return request('transaction', [id, commands, foreignKeys === 'check']);
      },
      close: () => request('close', [id]),
    });
  }
  return Object.freeze({
    async open(path) {
      path = authorize(grants, 'sqlite.open', path);
      return database(await request('open', [path]));
    },
    dispose,
  });
}
