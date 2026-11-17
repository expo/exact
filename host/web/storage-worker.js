// @ref LLP 1027 D10 — SQLite runs off the presentation thread.
import sqlite3InitModule from './sqlite3.mjs';
import { createFileStore, lockPaths } from './storage-fs.js';

// No shared memory or isolation headers: keep the database in WASM memory and
// atomically publish its complete SQLite file after each successful mutation.
// This costs O(database size) per write; a connection exclusively locks its file.
const ready = sqlite3InitModule({ locateFile: file => file === 'sqlite3.wasm'
  ? new URL('./sqlite3.wasm', import.meta.url).href : new URL(file, import.meta.url).href });
const clients = new Map();
let retiring = Promise.resolve();
const limit = 16 * 1024 * 1024;
const fail = message => { throw Object.assign(new Error(message), { kind: 'Unavailable' }); };
const allowedPragmas = new Set(['user_version', 'application_id', 'table_info', 'table_xinfo',
  'index_list', 'index_info', 'foreign_key_list', 'foreign_key_check', 'integrity_check', 'quick_check']);

const foreignKeysCheck = 'to rebuild a table other rows reference, run the batch as transaction(statements, { foreignKeys: "check" })';

// Strings by code point, as Rust orders them (UTF-16 code units order a
// character past U+FFFF before U+E000–U+FFFF).
function byCodePoint(a, b) {
  let i = 0, j = 0;
  while (i < a.length && j < b.length) {
    const x = a.codePointAt(i), y = b.codePointAt(j);
    if (x !== y) return x < y ? -1 : 1;
    i += x > 0xffff ? 2 : 1; j += y > 0xffff ? 2 : 1;
  }
  return i < a.length ? 1 : j < b.length ? -1 : 0;
}

// `PRAGMA foreign_key_check` before a `foreignKeys: 'check'` batch commits,
// over `main` and `temp` (unqualified, it reads only `main`; nothing else can
// be attached), in the native provider's words and order. Rows are stepped
// through and counted; only the three to name are kept. A rowid is a BigInt,
// or null for a WITHOUT ROWID table.
function checkForeignKeys(sqlite, session) {
  const { capi: c } = sqlite, failed = 'foreign key check failed, the batch is rolled back';
  const order = (a, b) => byCodePoint(a[0], b[0])
    || (a[1] === b[1] ? 0 : a[1] === null ? -1 : b[1] === null ? 1 : a[1] < b[1] ? -1 : 1)
    || byCodePoint(a[2], b[2]);
  let count = 0, named = [];
  for (const schema of ['main', 'temp']) {
    let statement;
    try {
      statement = session.db.prepare(`PRAGMA ${schema}.foreign_key_check`);
      while (statement.step()) {
        count++;
        const p = statement.pointer;
        const row = [statement.get(0), c.sqlite3_column_type(p, 1) === c.SQLITE_NULL ? null : BigInt(c.sqlite3_column_int64(p, 1)), statement.get(2)];
        if (named.length === 3 && order(row, named[2]) >= 0) continue;
        named = [...named, row].sort(order).slice(0, 3);
      }
    } catch (error) {
      if (!(error instanceof sqlite.SQLite3Error)) throw error;
      fail(`${failed}: ${c.sqlite3_errmsg(session.db.pointer)}`);
    } finally { statement?.finalize(); }
  }
  if (!count) return;
  const said = named.map(([table, id, parent]) => id === null
    ? `a ${table} row (references ${parent})` : `${table} row ${id} (references ${parent})`);
  const more = count > 3 ? `, and ${count - 3} more` : '';
  fail(count === 1 ? `${failed}: 1 foreign-key violation: ${said[0]}`
    : `${failed}: ${count} foreign-key violations: ${said.join(', ')}${more}`);
}

// Foreign key enforcement, set or (`-1`) read.
function enforcement(sqlite, session, value) {
  const { wasm: w, capi: c } = sqlite, scope = w.scopedAllocPush();
  try {
    const out = w.scopedAlloc(4);
    session.db.checkRc(c.sqlite3_db_config(session.db.pointer, c.SQLITE_DBCONFIG_ENABLE_FKEY, value, out));
    return w.peek32(out) === 1;
  } finally { w.scopedAllocPop(scope); }
}

function configure(sqlite, session) {
  const { capi: c } = sqlite, db = session.db;
  db.checkRc(c.sqlite3_db_config(db.pointer, c.SQLITE_DBCONFIG_DEFENSIVE, 1, 0));
  db.checkRc(c.sqlite3_db_config(db.pointer, c.SQLITE_DBCONFIG_TRUSTED_SCHEMA, 0, 0));
  db.exec('PRAGMA foreign_keys=ON; PRAGMA temp_store=MEMORY;');
  c.sqlite3_limit(db.pointer, c.SQLITE_LIMIT_LENGTH, limit);
  c.sqlite3_limit(db.pointer, c.SQLITE_LIMIT_SQL_LENGTH, 1024 * 1024);
  c.sqlite3_limit(db.pointer, c.SQLITE_LIMIT_ATTACHED, 0);
  const together = 'use transaction([...]) for statements that commit together';
  const denied = new Map([
    [c.SQLITE_ATTACH, 'ATTACH and DETACH: a connection reaches only its own file'],
    [c.SQLITE_DETACH, 'ATTACH and DETACH: a connection reaches only its own file'],
    [c.SQLITE_CREATE_VTABLE, 'a virtual table: no virtual table modules are loaded'],
    [c.SQLITE_DROP_VTABLE, 'a virtual table: no virtual table modules are loaded'],
    [c.SQLITE_SAVEPOINT, `SAVEPOINT: ${together}`]]);
  // What is refused, and why, is the error's message (`validate`); the words
  // are the native provider's (ibex2-sqlite).
  const refuse = why => { session.refused = why; return c.SQLITE_DENY; };
  db.checkRc(c.sqlite3_set_authorizer(db.pointer, (_unused, action, first, second) => {
    if (denied.has(action)) return refuse(denied.get(action));
    if (action === c.SQLITE_TRANSACTION && !session.internal) return refuse(`BEGIN, COMMIT and ROLLBACK: ${together}`);
    if (action === c.SQLITE_FUNCTION && String(second).toLowerCase() === 'load_extension') return refuse('load_extension: extensions are not loaded');
    // A `foreignKeys: 'check'` batch's own PRAGMA foreign_keys lines (a
    // migrator writes them around a rebuild) are no-ops; elsewhere refused.
    if (action === c.SQLITE_PRAGMA && String(first).toLowerCase() === 'foreign_keys') {
      return session.checking ? c.SQLITE_IGNORE
        : refuse(`PRAGMA ${first}: the host sets the connection's options; app SQL may run only PRAGMA ${[...allowedPragmas].join(', ')}; ${foreignKeysCheck}`);
    }
    if (action === c.SQLITE_PRAGMA && !allowedPragmas.has(String(first).toLowerCase())
      && !(session.serializing && first === 'page_count')) {
      return refuse(`PRAGMA ${first}: the host sets the connection's options; app SQL may run only PRAGMA ${[...allowedPragmas].join(', ')}`);
    }
    return c.SQLITE_OK;
  }, 0));
}

function validate(sqlite, session, sql) {
  if (typeof sql !== 'string' || sql.includes('\0') || new TextEncoder().encode(sql).length > 1024 * 1024) fail('invalid SQL string');
  const { wasm: w, capi: c } = sqlite;
  const scope = w.scopedAllocPush();
  let count = 0;
  try {
    let cursor = w.scopedAllocCString(sql);
    const [statement, tail] = w.scopedAllocPtr(2);
    while (cursor && w.peek8(cursor)) {
      w.pokePtr(statement, 0);
      session.refused = undefined;
      const rc = c.sqlite3_prepare_v3(session.db.pointer, cursor, -1, 0, statement, tail);
      const pointer = w.peekPtr(statement);
      if (pointer) { c.sqlite3_finalize(pointer); count++; }
      if (rc && session.refused) fail(`not authorized: ${session.refused}`);
      session.db.checkRc(rc);
      cursor = w.peekPtr(tail);
    }
    if (count !== 1) fail('expected one SQL statement');
  } finally { w.scopedAllocPop(scope); }
}

function params(values) {
  if (!Array.isArray(values)) fail('SQLite parameters must be an array');
  let budget = 0;
  for (const value of values) {
    budget += 24;
    if (typeof value === 'number') {
      if (!Number.isFinite(value) || (Number.isInteger(value) && !Number.isSafeInteger(value))) fail('SQLite number must be finite and integral numbers safe');
    } else if (typeof value === 'bigint') {
      if (value < -(1n << 63n) || value >= 1n << 63n) fail('SQLite integer is outside signed 64-bit range');
    } else if (typeof value === 'string') budget += new TextEncoder().encode(value).length;
    else if (value instanceof Uint8Array) budget += value.byteLength;
    else if (value !== null) fail('invalid SQLite parameter');
    if (budget > limit) fail('SQLite parameters exceed 16 MiB limit');
  }
  return values;
}

// SQLite may also refuse while a statement steps (`VACUUM INTO` attaches its
// target then), so a refusal recorded at any point names itself.
function run(sqlite, session, sql, values, query) {
  validate(sqlite, session, sql);
  session.refused = undefined;
  try { return step(sqlite, session, sql, values, query); }
  catch (error) {
    const why = session.refused;
    session.refused = undefined;
    if (why) fail(`not authorized: ${why}`);
    throw error;
  }
}

function step(sqlite, session, sql, values, query) {
  const statement = session.db.prepare(sql), c = sqlite.capi;
  try {
    values = params(values);
    if (values.length !== statement.parameterCount) fail('SQLite parameter count mismatch');
    if (query && !c.sqlite3_stmt_readonly(statement.pointer)) fail('query requires a read-only statement');
    if (!query && statement.columnCount) fail('execute does not accept a statement returning rows; use query');
    if (values.length) statement.bind(values);
    if (!query) {
      statement.step();
      return { changes: c.sqlite3_changes(session.db.pointer), lastInsertRowid: c.sqlite3_last_insert_rowid(session.db.pointer) };
    }
    const columns = statement.getColumnNames([]), rows = [];
    let budget = columns.reduce((n, s) => n + new TextEncoder().encode(s).length, 0);
    while (statement.step()) {
      if (rows.length >= 100000) fail('query result exceeds row limit');
      const row = [];
      for (let i = 0; i < columns.length; i++) {
        const value = c.sqlite3_column_type(statement.pointer, i) === c.SQLITE_INTEGER
          ? c.sqlite3_column_int64(statement.pointer, i) : statement.get(i);
        budget += 24 + (typeof value === 'string' ? new TextEncoder().encode(value).length : value?.byteLength || 0);
        if (budget > limit) fail('query result exceeds 16 MiB limit');
        row.push(value);
      }
      rows.push(row);
    }
    return { columns, rows };
  } finally { statement.finalize(); }
}

function internal(session, sql) {
  session.internal = true;
  try { session.db.exec(sql); } finally { session.internal = false; }
}
async function close(databases, id) {
  const session = databases.get(id);
  if (!session) return;
  databases.delete(id);
  try { session.db.close(); } finally { session.files.close(); await session.release(); }
}
async function persist(sqlite, databases, id, session) {
  try {
    let bytes;
    session.serializing = true;
    try {
      // The official exporter itself executes page_count. Only this synchronous
      // trusted section may use it; app SQL cannot widen the pragma allowlist.
      bytes = session.db.selectValue('PRAGMA page_count')
        ? sqlite.capi.sqlite3_js_db_export(session.db.pointer) : new Uint8Array();
    } finally { session.serializing = false; }
    // The exporter returns a fresh, full-buffer Uint8Array, independent of WASM.
    // This private snapshot has no further owner after the atomic write.
    await session.files.atomicWriteOwnedFile(session.path, bytes.buffer);
  }
  catch (error) {
    // Memory already committed. Invalidate every handle rather than exposing a
    // successful-looking connection whose state differs from the persisted file.
    await close(databases, id);
    throw error;
  }
}

async function dispatch(owner, { appId, op, args }) {
  const { databases } = owner;
  const sqlite = await ready;
  if (op === 'open') {
    const path = args[0], release = await lockPaths(appId, [path], undefined, undefined, owner.gone.signal);
    const files = createFileStore(appId);
    const session = { path, release, files, db: new sqlite.oo1.DB(':memory:'), statements: new Map(), internal: false };
    try {
      let bytes;
      try { bytes = new Uint8Array(await files.readFile(path)); }
      catch (error) { if (error.code !== 'ENOENT') throw error; }
      if (bytes?.length) {
        const pointer = sqlite.wasm.allocFromTypedArray(bytes);
        session.db.checkRc(sqlite.capi.sqlite3_deserialize(session.db.pointer, 'main', pointer,
          BigInt(bytes.length), BigInt(bytes.length), sqlite.capi.SQLITE_DESERIALIZE_FREEONCLOSE | sqlite.capi.SQLITE_DESERIALIZE_RESIZEABLE));
      }
      configure(sqlite, session);
      const id = ++owner.nextDatabase;
      databases.set(id, session);
      if (!bytes) await persist(sqlite, databases, id, session);
      return id;
    } catch (error) {
      for (const [id, candidate] of databases) if (candidate === session) databases.delete(id);
      session.db.close(); files.close(); await release(); throw error;
    }
  }
  const [id, ...rest] = args;
  if (op === 'close') return close(databases, id);
  if (op === 'statementClose') { databases.get(id)?.statements.delete(rest[0]); return; }
  const session = databases.get(id);
  if (!session) fail('database is closed');
  if (op === 'prepare') {
    validate(sqlite, session, rest[0]);
    const statement = ++owner.nextStatement;
    session.statements.set(statement, rest[0]);
    return statement;
  }
  if (op === 'transaction') {
    const [commands, check] = rest;
    if (!Array.isArray(commands) || commands.length > 10000) fail('transaction exceeds 10000 commands or is not an array');
    if (typeof check !== 'boolean') fail('invalid SQLite foreign key mode');
    // SQLite ignores a change to enforcement inside a transaction, so it goes
    // off before BEGIN (https://sqlite.org/lang_altertable.html#otheralter).
    if (check) enforcement(sqlite, session, 0);
    let results;
    try {
      internal(session, 'BEGIN IMMEDIATE');
      try {
        session.checking = check;
        results = commands.map((command, i) => {
          try { return run(sqlite, session, command.sql, command.params ?? [], false); }
          catch (error) {
            const points = Array.from(String(command.sql).trim()); // 200 code points, as the native provider counts
            const shown = points.length > 200 ? `${points.slice(0, 200).join('')}…` : points.join('');
            error.message = `statement ${i + 1} of ${commands.length} (${shown}): ${error.message}`;
            throw error;
          }
        });
        session.checking = false;
        if (check) checkForeignKeys(sqlite, session);
        internal(session, 'COMMIT');
      } catch (error) { internal(session, 'ROLLBACK'); throw error; }
    } finally {
      session.checking = false;
      // Nothing else reaches this database until enforcement is back: the
      // owner's queue runs one operation at a time, and none awaits before
      // here. One that cannot have it back is closed.
      if (check && !enforcement(sqlite, session, 1)) {
        await close(databases, id);
        fail('foreign keys could not be turned back on, so the database is closed');
      }
    }
    await persist(sqlite, databases, id, session);
    return results;
  }
  let sql = rest[0], values = rest[1];
  if (op.startsWith('statement')) {
    sql = session.statements.get(rest[0]);
    if (sql === undefined) fail('statement is closed');
  }
  const query = op === 'query' || op === 'statementQuery';
  if (query) return run(sqlite, session, sql, values, true);
  const c = sqlite.capi, before = c.sqlite3_total_changes(session.db.pointer);
  let result;
  try { result = run(sqlite, session, sql, values, false); }
  catch (error) {
    // A statement that fails under the FAIL conflict resolution keeps the
    // rows it changed before the error, and outside a transaction they are
    // committed (https://www.sqlite.org/lang_conflict.html), so the file
    // must hold them too. A persist that fails closes the connection.
    if (c.sqlite3_get_autocommit(session.db.pointer) && c.sqlite3_total_changes(session.db.pointer) !== before) {
      await persist(sqlite, databases, id, session).catch(() => {});
    }
    throw error;
  }
  await persist(sqlite, databases, id, session);
  return result;
}

// The code is SQLite's own result, never read from the message, which
// carries the app's SQL (ts-data.js `codedError` keeps a code it is given).
const sqliteCode = error => [5, 6].includes(error.resultCode & 0xff) ? 'EBUSY' : 'failed';

onmessage = ({ data }) => {
  let owner = clients.get(data.client);
  if (!owner) {
    owner = { databases: new Map(), nextDatabase: 0, nextStatement: 0, queue: Promise.resolve(), gone: new AbortController() };
    clients.set(data.client, owner);
  }
  // Capture only earlier cleanup: waiting on future disposal could wait on
  // this very operation. A replacement open must see its predecessor unlocked.
  const earlierDisposals = retiring;
  if (data.op === 'dispose') {
    clients.delete(data.client);
    // An open still waiting for its lock ends now: its holder's close may be what waits on this disposal.
    owner.gone.abort();
    retiring = Promise.all([earlierDisposals, owner.queue]).then(async () => {
      await Promise.allSettled([...owner.databases.keys()].map(id => close(owner.databases, id)));
    });
    return;
  }
  owner.queue = owner.queue.then(async () => {
    try {
      await earlierDisposals;
      postMessage({ client: data.client, id: data.id, value: await dispatch(owner, data) });
    } catch (error) {
      postMessage({ client: data.client, id: data.id,
        error: { kind: 'Unavailable', code: error.code ?? sqliteCode(error), message: String(error.message || error) } });
    }
  });
};
