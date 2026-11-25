// @ref LLP 1027 D10 — app-owned browser storage, installed after first pixel.
// IndexedDB publishes namespace changes atomically. OPFS database images are
// read under their pool lock or from the worker that holds it.
// Modification times come from the platform's clock, captured when this
// module evaluates: a realm that later refuses ambient time to app code (the
// module Worker, LLP 1027.002 D2) still stamps files with the real one.
import { agentStorageRefusal, directories as roots, fileVersion, now, storageKey } from './storage-environment.js';
import { coversPath, knownFolderPath } from './grant-admission.js';
const rootPaths = Object.values(roots);
// Calls from one host module retain invocation order. Web Locks queue
// conflicts with a live SQLite handle or another tab, with a bounded wait.
const mutationTails = new Map();

function failure(message, code) {
  const error = new Error(`filesystem: ${message}`);
  error.kind = 'Unavailable';
  if (code) error.code = code;
  return error;
}

export function normalizePath(path) {
  if (knownFolderPath(path)) throw Object.assign(new Error('NotSupportedError: well-known folders are unavailable on this host'), { name: 'NotSupportedError', code: 'failed' });
  if (typeof path !== 'string' || !path.startsWith('app:/')) {
    throw failure('browser storage needs an app:/ path');
  }
  const parts = path.slice(5).split('/').filter(Boolean);
  if (!['data', 'cache', 'tmp'].includes(parts[0])) throw failure('unknown app directory');
  if (parts.some(part => part === '.' || part === '..' || part.includes('\0'))) {
    throw failure('app path contains traversal or NUL');
  }
  return `app:/${parts.join('/')}`;
}

// Grants are the host-admitted declaration text, never module exports. Compare
// components (including namespace); app:/data does not grant app:/database.
export function authorize(grants, operation, path) {
  const normalized = normalizePath(path);
  if (coversPath(grants, operation, normalized)) return normalized;
  const error = new Error(deniedPath(operation, normalized));
  error.kind = 'Unavailable';
  throw error;
}

/** A path refusal names the path and the grant lines that would admit it (app farm round 2: a bare
 * `denied: sqlite.open` left builds guessing which per-persona file it wanted). As js/src/prelude.js says it natively. */
export function deniedPath(operation, path) {
  const dir = path.slice(0, path.lastIndexOf('/'));
  const directory = /^[a-z]+:\/[^/]/.test(dir) ? `, or \`${operation} ${dir}\` for every file there` : '';
  return `denied: ${operation} ${path}: no grant covers it; grant \`${operation} ${path}\`${directory} (a grant covers its path and what is below it, by whole names)`;
}

function parentOf(path) { return path.slice(0, path.lastIndexOf('/')); }
function below(path, directory) { return path.startsWith(`${directory}/`); }
function descendants(path) {
  // '/' sorts immediately before '0': include all descendant names (even
  // U+FFFF) but no sibling path. The directory itself is a separate point read.
  return IDBKeyRange.bound(`${path}/`, `${path}0`, false, true);
}

// A database owns its file exclusively and pins each ancestor with a shared
// lock. Unrelated files remain usable; replacing an open database or moving its
// parent fails instead of silently discarding SQLite's later commit.
const openDatabasePrefix = appId => `exact-sqlite-open:${encodeURIComponent(appId)}:`;
export const pathLock = (appId, path) => `exact-storage:${encodeURIComponent(appId)}:${path}`;
export const databaseLock = (appId, path) => openDatabasePrefix(appId) + path;
export const poolLock = id => 'exact-sqlite-pool:' + id;
export const mutationLock = id => 'exact-sqlite-mutation:' + id;
export async function holdLock(name, options = {}) {
  let release, granted, refused;
  const lifetime = new Promise(resolve => { release = resolve; });
  const ready = new Promise((resolve, reject) => { granted = resolve; refused = reject; });
  const done = navigator.locks.request(name, options, () => { granted(); return lifetime; });
  void done.catch(refused);
  await ready;
  return () => { release(); return done; };
}

const ownTimers = { set: (f, ms) => setTimeout(f, ms), clear: id => clearTimeout(id) };
export async function holdTimed(key, path, { mode = 'exclusive', timeout = 10000, signal, timers = ownTimers } = {}) {
  const abort = new AbortController(), cancel = () => abort.abort();
  const timer = timers.set(cancel, Math.max(0, timeout));
  signal?.addEventListener('abort', cancel, { once: true });
  try {
    if (signal?.aborted) cancel();
    return await holdLock(key, { mode, signal: abort.signal });
  } catch (error) {
    if (signal?.aborted) throw failure('storage was let go before its lock was acquired');
    if (abort.signal.aborted) throw failure(`storage is busy after 10000 ms: ${path}`, 'EBUSY');
    throw error;
  } finally { timers.clear(timer); signal?.removeEventListener('abort', cancel); }
}
// `timers`: the realm's own; a module Worker's are taken before its prelude takes the module's (module-worker.js).
// `cancel`: a signal that ends the wait when whoever asked has gone (storage-worker.js, a disposed client).
export async function lockPaths(appId, paths, timeout = 10000, timers = ownTimers, cancel = null) {
  if (!globalThis.navigator?.locks) throw failure('Web Locks are unavailable');
  const requested = new Map();
  for (let path of paths) {
    path = normalizePath(path);
    requested.set(path, 'exclusive');
    while (!rootPaths.includes(path)) {
      path = parentOf(path);
      if (!requested.has(path)) requested.set(path, 'shared');
    }
  }
  const held = [];
  const abort = new AbortController();
  const timer = timers.set(() => abort.abort(), timeout);
  const cancelled = () => abort.abort();
  async function release() {
    await Promise.all(held.map(unlock => unlock()));
  }
  try {
    if (cancel?.aborted) throw failure('storage was let go before its lock was acquired');
    cancel?.addEventListener('abort', cancelled, { once: true });
    for (const [path, mode] of [...requested].sort(([a], [b]) => a < b ? -1 : a > b ? 1 : 0)) {
      try {
        held.push(await holdLock(pathLock(appId, path), { mode, signal: abort.signal }));
      } catch (error) {
        if (cancel?.aborted) throw failure('storage was let go before its lock was acquired');
        if (abort.signal.aborted) throw failure(`storage is busy after ${timeout} ms: ${path}; close the other database or tab and retry`, 'EBUSY');
        throw failure(error.message);
      }
    }
    return release;
  } catch (error) {
    await release();
    throw error;
  } finally {
    timers.clear(timer);
    cancel?.removeEventListener('abort', cancelled);
  }
}
function requireBelowRoot(path) {
  if (rootPaths.includes(path)) throw failure('operation needs a path below the app root');
}
function entry(records, path) {
  const value = records.get(path);
  if (!value) throw failure(`no such file or directory: ${path}`, 'ENOENT');
  return value;
}
function directory(records, path) {
  const value = entry(records, path);
  if (value.kind !== 'directory') throw failure(`not a directory: ${path}`, 'ENOTDIR');
  return value;
}
function file(records, path) {
  const value = entry(records, path);
  if (value.kind !== 'file' && value.kind !== 'sqlite-opfs') throw failure('operation needs a regular file', 'EISDIR');
  return value;
}
function bytes(data) {
  if (ArrayBuffer.isView(data)) {
    return new Uint8Array(data.buffer, data.byteOffset, data.byteLength).slice().buffer;
  }
  // The argument can belong to the app iframe, so instanceof is insufficient.
  try { return ArrayBuffer.prototype.slice.call(data, 0); }
  catch { throw failure('file data must be an ArrayBuffer or typed array'); }
}

/** Trusted host backend, also used by SQLite after sqlite.open admission.
 * Never expose this grant-free object to app code. No database opens at creation.
 */
export function createFileStore(appId, timers = ownTimers) {
  if (typeof appId !== 'string' || !appId) throw failure('missing admitted app identity');
  const trashPrefix = 'sqlite-trash:';
  const guard = (path, pool) => path + '/\0' + pool;
  async function reclaim(db, names) {
    if (!names.length) return;
    const root = await navigator.storage.getDirectory();
    for (const name of names) {
      const pool = name.slice(trashPrefix.length);
      const remove = async () => {
        try { await (await root.getDirectoryHandle('exact-sqlite-sahpool')).removeEntry(pool, { recursive: true }); }
        catch (error) { if (error.name !== 'NotFoundError') throw error; }
        await new Promise((resolve, reject) => {
          const tx = db.transaction('files', 'readwrite');
          tx.objectStore('files').delete(name);
          tx.oncomplete = resolve;
          tx.onabort = () => reject(tx.error);
        });
      };
      await navigator.locks.request(poolLock(pool), { ifAvailable: true }, lock => {
        if (lock) return remove();
        // Reclaim as soon as the outstanding reader releases the pool. A crash leaves the tombstone.
        void navigator.locks.request(poolLock(pool), remove).catch(() => {});
      });
    }
  }
  const modificationTime = (entry, counter) => entry.beforeCounter === counter ? entry.beforeModifiedMs : entry.modifiedMs;
  async function imageEntry(value, sizeOnly = false, maxBytes = Infinity, metadata = false) {
    const deadline = now() + 10000;
    while (value.kind === 'sqlite-opfs') {
      const { databaseImage } = await import('./storage-sqlite.js');
      const release = metadata && await holdTimed(mutationLock(value.pool), value.path,
        { mode: 'shared', timeout: deadline - now(), timers });
      try {
        let data, failed;
        try { data = await databaseImage(appId, value.path, value.pool, sizeOnly, maxBytes); }
        catch (error) { failed = error; }
        // A rename changes the namespace version. SQL writes keep it, so even
        // a continuously written file can return the snapshot taken in this call.
        const current = await run(false, records => (sizeOnly ? entry : file)(records, value.path), value.path);
        if (current.pool === value.pool && current.version === value.version) {
          if (failed) throw failed;
          if (!metadata) return { ...current, contents: data };
          const counter = sizeOnly ? data.counter : data.byteLength >= 100 ? new DataView(data).getUint32(24) : 0;
          return { ...current, modifiedMs: modificationTime(current, counter), contents: sizeOnly ? data.size : data };
        }
        if (now() >= deadline) throw failure(`storage is busy after 10000 ms: ${value.path}`, 'EBUSY');
        value = current;
      } finally { if (release) await release(); }
    }
    return value;
  }
  let opening;
  let closed = false;
  function open() {
    if (closed) return Promise.reject(failure('storage was unloaded'));
    if (!opening) {
      opening = new Promise((resolve, reject) => {
        let request;
        try { request = indexedDB.open(`exact-storage:${encodeURIComponent(appId)}`, 1); }
        catch (error) { reject(failure(error.message)); return; }
        request.onupgradeneeded = () => {
          const store = request.result.createObjectStore('files', { keyPath: 'path' });
          for (const path of rootPaths) store.put({ path, kind: 'directory', modifiedMs: now() });
        };
        request.onerror = () => reject(failure(request.error?.message || 'IndexedDB open failed'));
        request.onblocked = () => reject(failure('IndexedDB upgrade blocked'));
        request.onsuccess = () => {
          const db = request.result;
          if (closed) { db.close(); reject(failure('storage was unloaded')); return; }
          db.onversionchange = () => db.close();
          // Retry a pool removal interrupted after the namespace transaction committed.
          const scan = db.transaction('files').objectStore('files').getAllKeys(IDBKeyRange.bound(trashPrefix, 'sqlite-trash;'));
          scan.onsuccess = () => { void reclaim(db, scan.result).catch(() => {}); };
          resolve(db);
        };
      });
    }
    return opening;
  }
  async function run(write, operation, path, list, subtree, database = false, discard = []) {
    const db = await open();
    if (closed) throw failure('storage was unloaded');
    return new Promise((resolve, reject) => {
      let transaction;
      try { transaction = db.transaction('files', write ? 'readwrite' : 'readonly', write && (database || discard.length) ? { durability: 'strict' } : undefined); }
      catch (error) { reject(failure(error.message)); return; }
      const store = transaction.objectStore('files');
      let result;
      let error;
      const trash = new Set();
      transaction.oncomplete = () => {
        if (trash.size) void reclaim(db, [...trash]).then(() => resolve(result), () => resolve(result));
        else resolve(result);
      };
      transaction.onabort = () => reject(error || failure(transaction.error?.message || 'transaction aborted'));
      transaction.onerror = () => {}; // onabort reports once, including quota failures.
      const requests = [...new Set(Array.isArray(path) ? path : [path])].map(key => store.get(key));
      if (subtree) requests.push(store.getAll(descendants(subtree)));
      const keys = list ? [store.getKey(list), store.getAllKeys(descendants(list))] : [];
      const staged = discard.flatMap(path => {
        const key = 'sqlite-opfs:' + path;
        return list || subtree ? [store.get(key), store.getAll(descendants(key))] : [store.get(key)];
      });
      let pending = requests.length + keys.length + staged.length;
      const ready = () => {
        if (--pending) return;
        const values = requests.flatMap(request => request.result ?? []);
        const records = new Map(values.map(value => [value.path, value]));
        const changes = {
          put(value, keepVersion = false) {
            const previous = records.get(value.path);
            if (previous?.kind === 'sqlite-opfs' && previous.pool !== value.pool) this.remove(guard(value.path, previous.pool));
            if (!keepVersion && (value.kind === 'file' || value.kind === 'sqlite-opfs')) value = { ...value, version: fileVersion() };
            records.set(value.path, value); store.put(value);
          },
          remove(path, moving = false) {
            if (!moving) {
              const previous = records.get(path);
              if (previous?.kind === 'sqlite-opfs') this.remove(guard(path, previous.pool));
              const offset = path.indexOf('\0');
              if (offset >= 0) {
                const key = trashPrefix + path.slice(offset + 1);
                store.put({ path: key });
                trash.add(key);
              }
            }
            records.delete(path);
            store.delete(path);
          },
          touch(path) { const value = directory(records, path); this.put({ ...value, modifiedMs: now() }); },
        };
        try {
          const names = keys.flatMap(request => request.result ?? []);
          result = operation(records, changes, names);
          for (const reservation of staged.flatMap(request => request.result ?? [])) {
            store.delete(reservation.path);
            const key = trashPrefix + reservation.pool;
            store.put({ path: key });
            trash.add(key);
          }
        } catch (cause) { error = cause; transaction.abort(); }
      };
      for (const request of [...requests, ...keys, ...staged]) request.onsuccess = ready;
    });
  }
  async function write(path, data, append, owned = false, database = false) {
    path = normalizePath(path);
    requireBelowRoot(path);
    // Snapshot before yielding. A private owned buffer is transferred, not copied.
    const copied = owned ? structuredClone(data, { transfer: [data] }) : bytes(data);
    const base = await run(true, (records, changes) => {
      directory(records, parentOf(path));
      const previous = records.get(path);
      if (previous) file(records, path);
      let contents = copied;
      if (append && previous?.pool) return previous;
      if (append && previous) {
        if (previous.contents instanceof Blob) {
          contents = new Blob([previous.contents, copied], { type: previous.contents.type });
        } else {
          const joined = new Uint8Array(previous.contents.byteLength + copied.byteLength);
          joined.set(new Uint8Array(previous.contents));
          joined.set(new Uint8Array(copied), previous.contents.byteLength);
          contents = joined.buffer;
        }
      }
      changes.put({ path, kind: 'file', contents, modifiedMs: now() });
      if (!previous) changes.touch(parentOf(path));
    }, [path, parentOf(path)], undefined, undefined, database, database ? [] : [path]);
    if (base) {
      const blob = new Blob([(await imageEntry(base)).contents, copied]);
      await putBlobs([{ path, blob }]);
    }
  }
  function putBlobs(entries, refusal = () => undefined) {
    entries = entries.map(({ path, blob }) => {
      path = normalizePath(path);
      requireBelowRoot(path);
      return { path, blob };
    });
    if (!entries.length) return Promise.resolve();
    return run(true, (records, changes) => {
      const refused = refusal(); if (refused) throw refused;
      for (const { path, blob } of entries) {
        directory(records, parentOf(path));
        const previous = records.get(path);
        if (previous) file(records, path);
        changes.put({ path, kind: 'file', contents: blob, modifiedMs: now() });
        if (!previous) changes.touch(parentOf(path));
      }
    }, entries.flatMap(({ path }) => [path, parentOf(path)]), undefined, undefined, false, entries.map(entry => entry.path));
  }
  return Object.freeze({
    directories: roots,
    // The path lock protects the choice of backing store across tabs.
    async databaseEntry(path, useOpfs) {
      requireBelowRoot(path);
      const staging = 'sqlite-opfs:' + path;
      const { entry, pending } = await run(false, records => {
        directory(records, parentOf(path));
        const entry = records.get(path);
        if (entry && entry.kind !== 'file' && entry.kind !== 'sqlite-opfs') throw failure('operation needs a regular file', 'EISDIR');
        return { entry, pending: records.get(staging)?.pool };
      }, [path, parentOf(path), staging], undefined, undefined, true);
      if (entry?.kind === 'sqlite-opfs' || !useOpfs) return { entry };
      if (pending) return { entry, pending };
      const pool = crypto.randomUUID();
      await run(true, (_records, changes) => changes.put({ path: staging, pool }), staging, undefined, undefined, true);
      return { entry, pending: pool };
    },
    publishDatabase(path, pool, source) {
      const staging = 'sqlite-opfs:' + path;
      return run(true, (records, changes) => {
        directory(records, parentOf(path));
        if (records.get(staging)?.pool !== pool) throw failure('SQLite destination changed');
        changes.put({ path, kind: 'sqlite-opfs', pool, modifiedMs: source?.modifiedMs ?? now() });
        changes.put({ path: guard(path, pool), kind: 'guard' });
        changes.remove(staging);
        if (!source) changes.touch(parentOf(path));
      }, [staging, parentOf(path)], undefined, undefined, true);
    },
    touchDatabase(path, counter) {
      return run(true, (records, changes) => {
        const value = entry(records, path);
        if (value.kind !== 'sqlite-opfs') throw failure('SQLite database path changed');
        // SQLite advances the file change counter on commit. Until then the file
        // keeps its previous modification time, including after a failed statement.
        changes.put({ ...value, beforeCounter: counter, beforeModifiedMs: modificationTime(value, counter), modifiedMs: now() }, true);
      }, path, undefined, undefined, true);
    },
    async readFile(path) {
      path = normalizePath(path);
      // IndexedDB already returns an independent structured clone for this read.
      // A picked file's entry holds the browser's own File (LLP 1069.002 D5).
      const value = await run(false, records => file(records, path), path);
      const { contents: data } = await imageEntry(value);
      return data instanceof Blob ? data.arrayBuffer() : data;
    },
    // Trusted host only: a file as a Blob, for an `image` or `video` source
    // (LLP 1069.002 D7), its modification time and its version. A picked entry is its File.
    // `bytesOnly` bounds only an entry of bytes, which IndexedDB read whole.
    async blob(path, type, maxBytes = Infinity, label = 'compressImage', bytesOnly = false) {
      path = normalizePath(path);
      const value = await run(false, records => file(records, path), path);
      const { kind, pool, modifiedMs, version, contents: data } = await imageEntry(value, false, maxBytes, true);
      const size = typeof data === 'number' ? data : data.byteLength ?? data.size;
      if (size > maxBytes && !(bytesOnly && data instanceof Blob)) throw failure(`${label}: too-large: ${size} bytes is over ${maxBytes}`, 'too-large');
      // A stored Blob with no type (an `exactBodyTo` reply's) takes the caller's, as bytes do.
      const blob = !(data instanceof Blob) ? new Blob([data], { type })
        : data.type || !type ? data : data.slice(0, data.size, type);
      // The namespace version survives SQL commits. The header's change counter
      // distinguishes images even when two writes share a clock tick.
      const stamp = kind === 'sqlite-opfs' && size >= 100
        ? [version ?? pool, modifiedMs, new DataView(data).getUint32(24)].join('/') : version;
      return { blob, modifiedMs, version: stamp };
    },
    // Trusted host only: a picked file's entry, backed by the browser's File
    // rather than bytes read into memory (LLP 1069.002 D5).
    putBlob: (path, blob, refusal) => putBlobs([{ path, blob }], refusal),
    putBlobs,
    writeFile(path, data) { return write(path, data, false); },
    atomicWriteFile(path, data) { return write(path, data, false); },
    // Trusted host only: takes an ArrayBuffer and detaches it from its caller.
    atomicWriteOwnedFile(path, data, database = false) { return write(path, data, false, true, database); },
    appendFile(path, data) { return write(path, data, true); },
    async readdir(path) {
      path = normalizePath(path);
      return run(false, (records, _changes, keys) => {
        directory(records, path);
        return keys.filter(key => parentOf(key) === path)
          .map(key => key.slice(path.length + 1)).sort();
      }, path, path);
    },
    async mkdir(path) {
      path = normalizePath(path);
      const ancestors = [path];
      while (!rootPaths.includes(ancestors[ancestors.length - 1])) {
        ancestors.push(parentOf(ancestors[ancestors.length - 1]));
      }
      return run(true, (records, changes) => {
        const parts = path.slice(5).split('/');
        let current = `app:/${parts.shift()}`;
        for (const part of parts) {
          const parent = current;
          current += `/${part}`;
          if (records.has(current)) directory(records, current);
          else {
            changes.put({ path: current, kind: 'directory', modifiedMs: now() });
            changes.touch(parent);
          }
        }
      }, ancestors);
    },
    async rm(path) {
      path = normalizePath(path);
      requireBelowRoot(path);
      return run(true, (records, changes, keys) => {
        directory(records, parentOf(path));
        if (!keys.includes(path)) return;
        for (const key of keys) changes.remove(key);
        changes.touch(parentOf(path));
      }, parentOf(path), path, undefined, false, [path]);
    },
    async stat(path) {
      path = normalizePath(path);
      const value = await run(false, records => entry(records, path), path);
      const { kind, contents: data, modifiedMs } = await imageEntry(value, true, Infinity, true);
      const isDirectory = kind === 'directory';
      const size = isDirectory ? 0 : typeof data === 'number' ? data : data.byteLength ?? data.size;
      return { size, isFile: !isDirectory, isDirectory, modifiedMs };
    },
    async rename(from, to) {
      from = normalizePath(from);
      to = normalizePath(to);
      requireBelowRoot(from);
      requireBelowRoot(to);
      return run(true, (records, changes, keys) => {
        const source = entry(records, from);
        directory(records, parentOf(to));
        if (from === to) return;
        if (below(to, from)) throw failure('cannot move a directory into itself');
        const target = records.get(to);
        if (target) {
          // As rename(2) answers natively: a file onto a folder is EISDIR, a folder onto a file ENOTDIR.
          if ((source.kind === 'directory') !== (target.kind === 'directory')) {
            throw failure('rename requires matching file kinds', target.kind === 'directory' ? 'EISDIR' : 'ENOTDIR');
          }
          if (target.kind === 'directory' && keys.some(key => below(key, to))) {
            throw failure('destination directory is not empty', 'ENOTEMPTY');
          }
          changes.remove(to);
        }
        const moving = [...records.values()].filter(value => value.path === from || below(value.path, from));
        for (const value of moving) changes.remove(value.path, true);
        for (const value of moving) changes.put({ ...value, path: to + value.path.slice(from.length) });
        changes.touch(parentOf(from));
        changes.touch(parentOf(to));
      }, [from, to, parentOf(from), parentOf(to)], to, from, false, [from, to]);
    },
    async copyFile(from, to) {
      from = normalizePath(from);
      to = normalizePath(to);
      requireBelowRoot(to);
      let image, locked, release;
      const deadline = now() + 10000;
      try {
        for (;;) {
          const pending = await run(true, (records, changes) => {
            const source = file(records, from);
            directory(records, parentOf(to));
            if (from === to) throw failure('copy requires distinct regular files');
            if (records.has(to)) file(records, to);
            // Publish only if the source still names the version we exported.
            if (source.pool && (locked !== source.pool || image?.pool !== source.pool || image.version !== source.version)) return source;
            const data = source.pool ? image.contents : source.contents;
            const contents = data instanceof Blob ? data.slice(0, data.size, data.type) : data;
            changes.put({ path: to, kind: 'file', contents, modifiedMs: now() });
            changes.touch(parentOf(to));
          }, [from, to, parentOf(to)], undefined, undefined, false, [to]);
          if (!pending) return;
          if (now() >= deadline) throw failure(`storage is busy after 10000 ms: ${from}`, 'EBUSY');
          if (locked !== pending.pool) {
            if (release) await release();
            release = await holdTimed(mutationLock(pending.pool), from, { mode: 'shared', timeout: deadline - now(), timers });
            locked = pending.pool;
          }
          image = await imageEntry(pending);
        }
      } finally { if (release) await release(); }
    },
    async realpath(path) {
      path = normalizePath(path);
      return run(false, records => { entry(records, path); return path; }, path);
    },
    close() {
      closed = true;
      if (opening) void opening.then(db => db.close(), () => {});
    },
  });
}

/** One mutation of `appId`'s store: after the module's earlier ones, under
 * Web Locks on `paths` and their ancestors. */
function mutateIn(appId, paths, operation, timers = ownTimers) {
  const previous = mutationTails.get(appId) || Promise.resolve();
  const result = previous.then(async () => {
    if (!globalThis.navigator?.locks) throw failure('Web Locks are unavailable');
    const prefix = openDatabasePrefix(appId);
    const held = (await navigator.locks.query()).held.find(({ name }) => name.startsWith(prefix)
      && paths.some(path => name === prefix + path || name.startsWith(prefix + path + '/')));
    if (held) throw failure(`database is open: ${held.name.slice(prefix.length)}`, 'EBUSY');
    const release = await lockPaths(appId, paths, undefined, timers);
    try { return await operation(); }
    finally { await release(); }
  });
  const settled = result.then(() => {}, () => {});
  mutationTails.set(appId, settled);
  void settled.then(() => {
    if (mutationTails.get(appId) === settled) mutationTails.delete(appId);
  });
  return result;
}

/** Public capability. Every operation is admitted before opening IndexedDB. */
export function createFileSystem(appId, grants, timers = ownTimers) {
  const store = createFileStore(appId, timers);
  const fs = { directories: roots, dispose() { store.close(); } };
  const mutate = (paths, operation) => mutateIn(appId, paths, operation, timers);
  for (const method of ['readFile', 'readdir', 'stat', 'realpath']) {
    fs[method] = async path => store[method](authorize(grants, 'fs.read', path));
  }
  for (const method of ['writeFile', 'atomicWriteFile', 'appendFile', 'mkdir', 'rm']) {
    fs[method] = async (path, data) => {
      path = authorize(grants, 'fs.write', path);
      // Preserve call-time bytes even when lock acquisition crosses a task.
      if (['writeFile', 'atomicWriteFile', 'appendFile'].includes(method)) data = bytes(data);
      return mutate([path], () => store[method](path, data));
    };
  }
  for (const method of ['rename', 'copyFile']) {
    fs[method] = async (from, to) => {
      from = authorize(grants, 'fs.read', from);
      to = authorize(grants, 'fs.write', to);
      if (method === 'rename') authorize(grants, 'fs.write', from);
      return mutate(method === 'rename' ? [from, to] : [to], () => store[method](from, to));
    };
  }
  // `copyFile` from a document the person chose (documents-glue.js
  // `copyFile`): `to` is admitted before `open` gives the document's File,
  // which becomes the file's contents, as a picked file's does (LLP 1069.002 D5).
  fs.copyIn = async (to, open) => {
    to = authorize(grants, 'fs.write', to);
    requireBelowRoot(to);
    const file = await open();
    return mutate([to], () => store.putBlob(to, file));
  };
  // `compressImage(from, to, {maxDimension, maxBytes, quality, type, rotate,
  // flip, crop})` (LLP 1069.002 A1, A1.12; LLP 1133.019 D15): both grants are checked and the
  // options too before anything is read; the codec runs outside the mutation
  // lock, and only the write takes it.
  fs.compressImage = async (from, to, options) => {
    if (typeof from !== 'string' || typeof to !== 'string' || !from.startsWith('app:/') || !to.startsWith('app:/'))
      throw failure('compressImage: needs app:/ paths', 'failed');
    const image = await import('./storage-image.js');
    const { type, ...budget } = image.limits(options);
    const edit = image.editOptions('storage.fs.compressImage()', options, false);
    from = authorize(grants, 'fs.read', from);
    to = authorize(grants, 'fs.write', to);
    requireBelowRoot(to);
    // An entry is one IndexedDB record, read whole by any operation (`stat`
    // too); its size is checked before it becomes a Blob or is decoded.
    const { blob } = await store.blob(from, undefined, image.MAX_BYTES);
    const out = await image.compress(blob, budget, undefined, type, edit);
    const size = out.bytes.byteLength;
    await mutate([to], () => store.atomicWriteOwnedFile(to, out.bytes));
    return { path: to, type, size, width: out.width, height: out.height };
  };
  // `transformImage(from, to, options)` (LLP 1133.019 D14), as `compressImage`
  // is run; a `to` with no extension gains the written type's, under its grant.
  fs.transformImage = async (from, to, options) => {
    if (typeof from !== 'string' || typeof to !== 'string' || !from.startsWith('app:/') || !to.startsWith('app:/'))
      throw failure('transformImage: needs app:/ paths', 'failed');
    const image = await import('./storage-image.js');
    const checked = image.editOptions('storage.fs.transformImage()', options, true);
    from = authorize(grants, 'fs.read', from);
    to = authorize(grants, 'fs.write', to);
    requireBelowRoot(to);
    const { blob } = await store.blob(from, undefined, image.MAX_BYTES, 'transformImage');
    const out = await image.transform(blob, checked);
    const path = /\.[^/]*$/.test(to) ? to : authorize(grants, 'fs.write', `${to}.${image.extension(out.type)}`);
    const { bytes, ...facts } = out, size = bytes.byteLength; // the write detaches the bytes
    await mutate([path], () => store.atomicWriteOwnedFile(path, bytes));
    return { path, size, ...facts };
  };
  return Object.freeze(fs);
}

/** `fetch(url, {exactBodyFrom})` (LLP 1108 D6 R2): the app file at `path` as
 * a request body, read by the host that sends it, under `fs.read`. A Blob,
 * never bytes handed to app code: a picked file's or a recorded video's entry
 * is its own `File` (LLP 1069.002 D5), and an `exactBodyTo` reply's a Blob,
 * which the browser reads from disk as it sends, with no bound; each is
 * sliced with no type so the browser adds no Content-Type (the author's header
 * is the only one). A byte entry, a file the app wrote, is the record
 * IndexedDB already read whole, wrapped, so it keeps a bound of 64 MiB (LLP
 * 1133.008.000 D4). Every refusal names the path and why, as the native
 * executor's does. Trusted host only. */
export const MAX_WRITTEN_BODY_BYTES = 64 * 1024 * 1024;
export async function requestBody(appId, grants, path) {
  const named = (why, code) => Object.assign(new Error(`exactBodyFrom ${path}: ${why}`), { kind: 'Unavailable', code });
  if (typeof path !== 'string' || !path.startsWith('app:/')) throw named('exactBodyFrom needs an app:/ path');
  let normalized;
  try { normalized = authorize(grants, 'fs.read', path); }
  catch (error) { throw named(String(error.message).replace(/^filesystem: /, ''), /^denied: /.test(error.message) ? 'denied' : 'failed'); }
  const key = typeof appId === 'string' && appId ? storageKey(appId) : null;
  if (key == null) throw named(appId ? agentStorageRefusal : 'this host has no app files (no storage here)', 'agent');
  const store = createFileStore(key);
  try {
    // The size is checked before a Blob is made; a byte entry's record is
    // read whole by IndexedDB first, as for any operation on it.
    const { blob } = await store.blob(normalized, '', MAX_WRITTEN_BODY_BYTES, 'exactBodyFrom', true).catch(error => {
      const why = error.code === 'EISDIR' ? 'not a file'
        : error.code === 'too-large' ? `too-large: an app-written file is read whole on the web, at most ${MAX_WRITTEN_BODY_BYTES} bytes`
        : String(error.message).replace(/^filesystem: (exactBodyFrom: )?/, '');
      throw named(why, error.code);
    });
    return blob.type ? blob.slice(0, blob.size, '') : blob;
  } finally { store.close(); }
}

/** `fetch(url, {exactBodyTo})` (LLP 1108 D6 R2, 2026-10-26): the app file a
 * 2xx reply's body becomes, admitted under `fs.write` with its parent folder
 * checked before the request is sent. `put` counts the body through a stream
 * into a Blob the browser holds (never bytes in the module), refused past its
 * limit, then makes it the file's entry in one IndexedDB transaction under
 * the module's mutation lock, so the file appears whole or not at all.
 * Every refusal names the path and why, as the native executor's does.
 * `close` lets the store go when the reply is never written. Trusted host only. */
export const MAX_BODY_TO_BYTES = 1024 * 1024 * 1024;
export async function bodyTarget(appId, grants, path) {
  const named = (why, code) => Object.assign(new Error(`exactBodyTo ${path}: ${String(why).replace(/^filesystem: /, '')}`), { kind: 'Unavailable', code });
  let normalized;
  try { normalized = authorize(grants, 'fs.write', path); requireBelowRoot(normalized); }
  catch (error) { throw named(error.message, /denied: /.test(error.message) ? 'denied' : 'failed'); }
  const key = typeof appId === 'string' && appId ? storageKey(appId) : null;
  if (key == null) throw named(appId ? agentStorageRefusal : 'this host has no app files (no storage here)', 'agent');
  const store = createFileStore(key);
  try {
    const parent = await store.stat(parentOf(normalized)).catch(error => { throw named(error.message, error.code); });
    if (!parent.isDirectory) throw named(`not a directory: ${parentOf(normalized)}`, 'ENOTDIR');
    const target = await store.stat(normalized).catch(error => { if (error.code !== 'ENOENT') throw named(error.message, error.code); });
    if (target?.isDirectory) throw named('write needs a regular file', 'EISDIR');
  } catch (error) { store.close(); throw error; }
  return {
    // Until the file starts to be replaced, the request's end refuses the write: `refusal()`, asked just before, says
    // whether the request has ended and with what, and `ended` aborts when it may have, rejecting the write at once.
    // Once the file starts to be replaced the write waits for it, so a request never fails while its file changes.
    async put(response, limit, ended, refusal = () => undefined, publishRefusal) {
      try {
        // A browser rejects the read with its own words, so the overflow is kept to say why.
        let size = 0, over = null;
        const counted = response.body?.pipeThrough(new TransformStream({ transform(chunk, out) {
          if ((size += chunk.byteLength) > limit) throw (over = Object.assign(new Error('HTTP response exceeds limit'), { kind: 'Refused' }));
          out.enqueue(chunk);
        } }));
        const blob = counted ? await new Response(counted).blob().catch(error => { throw over ?? error; }) : new Blob([]);
        let replacing = false, refused;
        const replaced = mutateIn(key, [normalized], () => {
          refused = refusal();
          if (refused) throw refused;
          replacing = true;
          return store.putBlob(normalized, blob, publishRefusal);
        }).catch(error => { throw error === refused ? error : named(error.message, error.code); });
        if (ended) {
          await new Promise((done, failed) => {
            const end = () => { if (!replacing) failed(refusal() ?? ended.reason); };
            ended.addEventListener('abort', end, { once: true });
            if (ended.aborted) end();
            replaced.then(done, failed).finally(() => ended.removeEventListener('abort', end));
          });
        } else {
          await replaced;
        }
        return size;
      } finally { store.close(); }
    },
    close() { store.close(); },
  };
}
