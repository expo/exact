#!/usr/bin/env bun
// Units and whole incremental caches expire after a week (KEEP). Within a cache, rustc
// reads completed sessions newest first by the timestamp in their names. Reclaim
// loose objects only for rlib-only units. Their callers' debug maps name archive members.
//
// Cargo reads fingerprints even on a no-op build. Reset their access times to detect
// the next read (macOS and Linux relatime). Preserve the nanoseconds of the mtime
// because Cargo compares inputs against the dep-info file at that precision.
//
// Cargo can hold separate locks for target and build directories. Rename under both
// locks, fingerprints first. Take rustc's session locks before moving its caches.
// Delete after releasing the locks.
import { constants, closeSync, existsSync, fstatSync, lstatSync, mkdirSync, mkdtempSync, openSync, readdirSync, readFileSync, readSync, realpathSync, renameSync, rmSync, statfsSync, statSync, writeFileSync } from 'node:fs';
import { spawn } from 'node:child_process';
import { takeLock } from '../host/web/build-lock.mjs';
import { platform } from 'node:os';
import { basename, dirname, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import process from 'node:process';

const KEEP = 7 * 86_400_000, EVERY = 3_600_000;
const RESET = 86_400; // 1970-01-02, in seconds: older than any real mtime
const LEDGER = '.exact-sweep.json', LOG = '.exact-sweep.log', ASIDE = '.exact-swept';
const HASH = /-([0-9a-f]{16})(?:\.|$)/;
// Incremental objects only: `<unit>-<16 hex>.<25 base-36 CGU id>.<7 base-36 tag>.rcgu.o`.
// Non-incremental objects (`-cgu.<N>.rcgu.o`) carry codegen unit indices, not rebuild tags.
const RCGU = /^(.+-[0-9a-f]{16})\.[0-9a-z]{25}\.([0-9a-z]{7})\.rcgu\.o$/;
const SESSION_PREFIX = 's-([0-9a-z]{10})-[0-9a-z]+';
const SESSION = new RegExp(`^(${SESSION_PREFIX})-([0-9a-z]+)$`);
const SESSION_LOCK = new RegExp(`^${SESSION_PREFIX}\\.lock$`);

/** Start a sweep of `target` in the background if the last one finished over an hour ago. A
 * sweep that failed leaves its error in the log, reported on every build until one succeeds. */
export function startSweep(target, now = Date.now()) {
  // This is optional cache housekeeping. Windows has neither this flock
  // protocol nor reliable immediate access-time updates; keep its build units.
  if (process.platform === 'win32') return;
  if (!existsSync(target)) return;
  const log = resolve(target, LOG);
  if (existsSync(log) && statSync(log).size) process.stderr.write(`target sweep failed (${log}):\n${readFileSync(log, 'utf8')}`);
  const ledger = resolve(target, LEDGER);
  if (existsSync(ledger) && now - JSON.parse(readFileSync(ledger, 'utf8')).at < EVERY) return;
  const err = openSync(log, 'a');
  const child = spawn(process.execPath, [fileURLToPath(import.meta.url), resolve(target)], { cwd: import.meta.dirname, detached: true, stdio: ['ignore', 'ignore', err] });
  child.on('error', () => {});
  child.unref();
  closeSync(err);
  return child;
}

async function libc() {
  const { dlopen, FFIType } = await import('bun:ffi');
  const mac = process.platform === 'darwin';
  const { symbols } = dlopen(mac ? '/usr/lib/libSystem.B.dylib' : 'libc.so.6', {
    flock: { args: [FFIType.i32, FFIType.i32], returns: FFIType.i32 },
    utimensat: { args: [FFIType.i32, FFIType.ptr, FFIType.ptr, FFIType.i32], returns: FFIType.i32 },
  });
  // AT_FDCWD and UTIME_OMIT, from <fcntl.h> and <sys/stat.h> on each system (POSIX utimensat).
  const [AT_FDCWD, UTIME_OMIT] = mac ? [-2, -2] : [-100, (1 << 30) - 2];
  // Two 64-bit timespecs: the access time is RESET, the modification time is left as it is.
  const times = new BigInt64Array([BigInt(RESET), 0n, 0n, BigInt(UTIME_OMIT)]);
  return {
    // LOCK_EX | LOCK_NB, the same flock Cargo takes. Closing the descriptor releases it.
    lock: (path, create = true) => {
      // NFS grants LOCK_EX only on a descriptor open for writing.
      const fd = openSync(path, constants.O_RDWR | constants.O_NOFOLLOW | (create ? constants.O_CREAT : 0));
      if (symbols.flock(fd, 6) === 0) return fd;
      closeSync(fd);
      return null;
    },
    resetAccess: path => {
      if (symbols.utimensat(AT_FDCWD, Buffer.from(`${path}\0`), times, 0) !== 0) throw new Error(`${path}: could not reset its access time`);
    },
  };
}

/** Without access times the sweep cannot tell a used unit from an abandoned one, so it refuses. */
function assertAccessTimes(target, resetAccess) {
  const probe = resolve(target, `${LEDGER}.probe`);
  writeFileSync(probe, 'x');
  resetAccess(probe);
  readFileSync(probe);
  const seen = statSync(probe).atimeMs > RESET * 1000;
  rmSync(probe);
  if (!seen) throw new Error(`${target}: this filesystem does not record reads (noatime), so the sweep cannot tell which units builds use`);
}

const profiles = target => [target, ...readdirSync(target, { withFileTypes: true }).filter(e => e.isDirectory()).map(e => resolve(target, e.name))]
  .flatMap(dir => readdirSync(dir, { withFileTypes: true }).filter(e => e.isDirectory()).map(e => resolve(dir, e.name)))
  .filter(dir => existsSync(resolve(dir, '.fingerprint')));

function cargoUsesProfileLocks(dir) {
  if (platform() !== 'linux') return true;
  // Cargo skips locking on filesystems with NFS_SUPER_MAGIC.
  try { return statfsSync(dir).type !== 0x6969; }
  catch { return false; }
}

const isDirectory = path => lstatSync(path, { throwIfNoEntry: false })?.isDirectory();

/** Read archive metadata by offset. Object payloads can be hundreds of MB. */
function archiveTags(path, unit) {
  let fd;
  try {
    fd = openSync(path, constants.O_RDONLY | constants.O_NOFOLLOW | constants.O_NONBLOCK);
    const file = fstatSync(fd), header = Buffer.alloc(60);
    const read = (buffer, length, offset) => readSync(fd, buffer, 0, length, offset) === length;
    if (!file.isFile() || !read(header, 8, 0) || header.toString('ascii', 0, 8) !== '!<arch>\n') return;
    const tags = new Set();
    let names;
    for (let offset = 8; offset < file.size;) {
      if (!read(header, 60, offset) || header.toString('ascii', 58) !== '`\n') return;
      let name = header.toString('utf8', 0, 16).trimEnd();
      const size = header.toString('ascii', 48, 58).trimEnd();
      if (!/^\d+$/.test(size)) return;
      const length = Number(size), start = offset + header.length;
      offset = start + length + length % 2;
      if (offset > file.size) return;
      if (name === '/' || name === '/SYM64/') continue;
      if (name === '//') {
        names = Buffer.alloc(length);
        if (!read(names, length, start)) return;
        continue;
      }
      if (name.startsWith('#1/')) {
        const count = name.slice(3);
        if (!/^\d+$/.test(count) || Number(count) > length) return;
        const bytes = Buffer.alloc(Number(count));
        if (!read(bytes, bytes.length, start)) return;
        name = bytes.toString('utf8').replace(/\0+$/, '');
      } else if (name.startsWith('/')) {
        if (!names || !/^\/\d+$/.test(name)) return;
        const index = Number(name.slice(1)), end = names.indexOf('/\n', index);
        if (end <= index || index && names[index - 1] !== 10) return;
        name = names.toString('utf8', index, end);
      } else {
        name = name.replace(/\/$/, '');
      }
      // The object pattern excludes symbol tables and Rust metadata.
      const match = RCGU.exec(name);
      if (match?.[1] === unit) tags.add(match[2]);
    }
    return tags;
  } catch {
    return undefined;
  } finally { if (fd !== undefined) closeSync(fd); }
}

function* obsoleteRcguObjects(depsDir) {
  const objects = new Map(), outputs = new Map();
  for (const entry of readdirSync(depsDir, { withFileTypes: true })) {
    const { name } = entry;
    if (name.endsWith('.rcgu.o')) {
      const match = RCGU.exec(name);
      if (!match || !entry.isFile()) continue;
      const [, unit, tag] = match;
      if (!objects.has(unit)) objects.set(unit, []);
      objects.get(unit).push({ name, tag });
    } else if (!name.endsWith('.rmeta') && !name.endsWith('.d')) {
      for (const [, hash] of name.matchAll(/-([0-9a-f]{16})(?=[.-]|$)/g)) {
        outputs.set(hash, outputs.has(hash) || !entry.isFile() ? null : name);
      }
    }
  }
  for (const [unit, files] of objects) {
    const archive = `lib${unit}.rlib`;
    if (outputs.get(unit.slice(-16)) !== archive) continue;
    const tags = archiveTags(resolve(depsDir, archive), unit);
    if (!tags?.size) continue;
    for (const { name, tag } of files) if (!tags.has(tag)) yield resolve(depsDir, name);
  }
}

function withSessionLocks(cache, names, lock, move) {
  // On macOS flock also excludes the fcntl record locks rustc takes (lockf(3)).
  const fds = [];
  try {
    for (const name of names) {
      let fd;
      try { fd = lock(resolve(cache, name), false); }
      catch (error) {
        // rustc takes the lock before creating the session directory.
        if (error.code === 'ENOENT') continue;
        return false;
      }
      if (fd === null) return false;
      fds.push(fd);
    }
    move();
    return true;
  } finally { for (const fd of fds) closeSync(fd); }
}

function sweepIncremental(incrementalDir, now, move, lock, { prefix, oldCaches, caches, remember }) {
  let removed = 0;
  for (const entry of readdirSync(incrementalDir, { withFileTypes: true })) {
    if (!entry.isDirectory()) continue;
    const cache = resolve(incrementalDir, entry.name);
    const stat = lstatSync(cache, { throwIfNoEntry: false });
    if (!stat?.isDirectory()) continue;
    const key = prefix + entry.name, recorded = oldCaches[key];
    if (recorded !== undefined) caches[key] = recorded;
    const lockFiles = new Set();
    const listed = readdirSync(cache, { withFileTypes: true });
    const children = listed.map(c => {
      const path = resolve(cache, c.name);
      const [, prefix, timestamp, state] = (c.isDirectory() ? SESSION.exec(c.name) : null) ?? [];
      const lockName = prefix ? `${prefix}.lock` : !c.isDirectory() && SESSION_LOCK.test(c.name) ? c.name : null;
      if (lockName && c.isFile()) lockFiles.add(lockName);
      return { path, timestamp, state, lockName, mtime: lstatSync(path).mtimeMs };
    });
    const moveCache = () => {
      const names = new Set(listed.map(c => c.name));
      // A standalone rustc can start a session while the listed locks are held.
      if (readdirSync(cache).some(name => !names.has(name))) return;
      move(cache);
      delete caches[key];
      removed++;
    };
    const activity = children.reduce((latest, c) => Math.max(latest, c.mtime), recorded ?? (children.length ? -Infinity : stat.mtimeMs));
    if (now - activity > KEEP) {
      const locks = new Set(children.map(c => c.lockName).filter(Boolean));
      withSessionLocks(cache, locks, lock, moveCache);
      continue;
    }
    // rustc encodes the timestamp as ten base-36 digits, so string order is chronological.
    let newest = '';
    const keepLocks = new Set();
    for (const { lockName, timestamp, state } of children) {
      if (!timestamp || state === 'working' || !lockFiles.has(lockName) || timestamp < newest) continue;
      if (timestamp > newest) keepLocks.clear();
      newest = timestamp;
      keepLocks.add(lockName);
    }
    const hasSurvivors = children.some(c => !c.lockName || keepLocks.has(c.lockName));
    const discard = new Map();
    for (const child of children) {
      if (!child.lockName || keepLocks.has(child.lockName)) continue;
      if (!discard.has(child.lockName)) discard.set(child.lockName, []);
      discard.get(child.lockName).push(child);
    }
    // Move the session before its lock so a failed move leaves the lock in place.
    const moveGroup = group => {
      for (const child of group) if (child.timestamp) move(child.path);
      for (const child of group) if (!child.timestamp) move(child.path);
    };
    if (children.length && !hasSurvivors) {
      withSessionLocks(cache, discard.keys(), lock, () => {
        for (const group of discard.values()) moveGroup(group);
        removed += children.length;
        moveCache();
      });
    } else {
      for (const [name, group] of discard) {
        if (withSessionLocks(cache, [name], lock, () => {
          // A sweep can be interrupted after removing the child that dates this cache.
          if (activity > (caches[key] ?? -Infinity)) {
            remember(key, activity);
            caches[key] = activity;
          }
          moveGroup(group);
        })) removed += group.length;
      }
    }
  }
  return removed;
}

/** Sweep `target` as of `now`. Returns what was moved out, per profile directory. */
export async function sweep(target, now = Date.now()) {
  if (process.platform === 'win32') throw new Error('Cargo cache sweeping is unavailable on Windows: its lock and access-time semantics have not been verified');
  const { lock, resetAccess } = await libc(), own = lock(resolve(target, `${LEDGER}.lock`));
  if (own === null) return {};
  try {
    assertAccessTimes(target, resetAccess);
    const path = resolve(target, LEDGER), old = existsSync(path) ? JSON.parse(readFileSync(path, 'utf8')) : {};
    const { units: oldUnits = {}, caches: oldCaches = {} } = old;
    const units = {}, caches = {}, removed = {}, aside = resolve(target, ASIDE, String(now));
    const save = ledger => {
      writeFileSync(`${path}.tmp`, JSON.stringify(ledger));
      renameSync(`${path}.tmp`, path);
    };
    const remember = (key, activity) => {
      oldCaches[key] = activity;
      save({ ...old, caches: oldCaches });
    };
    let n = 0;
    const move = p => { mkdirSync(aside, { recursive: true }); renameSync(p, resolve(aside, String(n++))); };
    for (const dir of profiles(target)) {
      const at = relative(target, dir), prefix = `${at}#`, locks = [];
      try {
        if (cargoUsesProfileLocks(dir)) {
          for (const name of ['.cargo-lock', '.cargo-build-lock']) {
            const fd = lock(resolve(dir, name));
            if (fd === null) break;
            locks.push(fd);
          }
        }
        if (locks.length < 2) {
          for (const [key, t] of Object.entries(oldUnits)) if (key.startsWith(prefix)) units[key] = t;
          for (const [key, t] of Object.entries(oldCaches)) if (key.startsWith(prefix)) caches[key] = t;
          continue;
        }
        const stale = new Set(), fingerprints = resolve(dir, '.fingerprint');
        for (const name of readdirSync(fingerprints)) {
          const hash = HASH.exec(name)?.[1];
          if (!hash) continue;
          const unit = resolve(fingerprints, name), files = readdirSync(unit).map(f => resolve(unit, f));
          const key = prefix + hash, read = files.some(f => statSync(f).atimeMs > RESET * 1000);
          units[key] = read || !(key in oldUnits) ? now : oldUnits[key];
          if (now - units[key] > KEEP) { stale.add(hash); delete units[key]; move(unit); continue; }
          for (const f of files) resetAccess(f);
        }
        let count = stale.size;
        for (const sub of ['deps', 'build', 'examples']) {
          const d = resolve(dir, sub);
          if (sub === 'deps' ? !isDirectory(d) : !existsSync(d)) continue;
          if (stale.size) for (const name of readdirSync(d)) if (stale.has(HASH.exec(name)?.[1])) move(resolve(d, name));
          if (sub === 'deps') for (const path of obsoleteRcguObjects(d)) { move(path); count++; }
        }
        const incremental = resolve(dir, 'incremental');
        if (isDirectory(incremental)) count += sweepIncremental(incremental, now, move, lock, { prefix, oldCaches, caches, remember });
        if (count) removed[at] = count;
      } finally { for (const fd of locks) closeSync(fd); }
    }
    save({ at: now, units, caches });
    rmSync(resolve(target, ASIDE), { recursive: true, force: true });
    writeFileSync(resolve(target, LOG), '');
    return removed;
  } finally { closeSync(own); }
}

const APP_USE = '.exact-app-use.lock';
const appUses = new Set();

/** Builds, drives and dev servers share a dedicated app target until their process exits.
 * Check the actual target: another app can select it through CARGO_TARGET_DIR or a symlink. */
export function holdAppOutput(target) {
  if (process.platform === 'win32' || appUses.has(target)) return;
  const dir = existsSync(target) ? realpathSync(target) : target;
  if (basename(dir) !== 'target' || !existsSync(resolve(dirname(dir), 'app.contract'))) return;
  mkdirSync(dir, { recursive: true });
  takeLock(resolve(dir, APP_USE), { shared: true, noFollow: true });
  appUses.add(target);
}

const REMADE = new Set(['apple-extension-module-cache', 'apple-modules', 'apple-painted', 'apple-swift', 'bake-compiled',
  'bake-input-hashes.json', 'bake-inputs', 'bake-metadata', 'dev-gpu', 'exact-filesystem-tool', 'metal-kept', 'module-dev',
  'native-reuse', 'native-runs', 'proof-web', 'proof-web-inputs.json', 'push-tools', 'rust-bake', 'rust-dev', 'rust-modules',
  'rust-sign', 'web-bakes', 'web-dist', 'web-dist-resident', 'web-dist-stages', 'web-js-gpu', 'web-js-modules', 'web-loops',
  'web-opt', 'web-runs']);
const CARGO_CONTENTS = new Set(['.fingerprint', 'build', 'deps', 'examples', 'incremental']);
const RECORDS = new Set(['traces', 'web-receipts', 'native-receipts']);
// bakeOutput and appleArtifacts put the trust below these components. A Mac's assembled
// .app sits beside them, with no trust in its name, so keep it for symbolication.
const sourceKey = /^[a-f0-9]{24}$/;
const TRUSTED = {
  bake: [sourceKey, null],
  clients: [sourceKey, null, /^(macos|ios|tvos)(-simulator)?$/, /^[a-z0-9_]+-apple-(darwin|ios|tvos)(-sim)?$/, /^(embedded|updating)$/],
};
// The flock files builds hold: Cargo's, build-lock.mjs's, claimBuildOutput's `.flock`, SwiftPM's scratch `.lock`.
const LOCK = /^\.cargo(-build|-artifact)?-lock$|\.(lock|flock)$/;
// The filesystem helper's build claim (scripts/filesystem.mjs) is a file naming its owner, `<pid>-<nonce>`, not a flock.
const BOOTSTRAP = '.bootstrap.lock';
const bootstrapping = path => {
  let owner;
  try { owner = readFileSync(path, 'utf8'); } catch (error) { if (error.code === 'ENOENT') return false; throw error; }
  const pid = Number.parseInt(owner, 10);
  if (!(pid > 0)) return true;
  try { return process.kill(pid, 0); }
  catch (error) { if (error.code === 'ESRCH') return false; if (error.code === 'EPERM') return true; throw error; }
};
const cargoLock = dir => lstatSync(resolve(dir, '.cargo-lock'), { throwIfNoEntry: false })?.isFile();
const cacheTag = dir => lstatSync(resolve(dir, 'CACHEDIR.TAG'), { throwIfNoEntry: false })?.isFile();
const entries = dir => readdirSync(dir, { withFileTypes: true });

function trustOutputs(dir, components, remove, kept) {
  for (const entry of entries(dir)) {
    const name = entry.name, path = resolve(dir, name);
    if (entry.isDirectory() && components.length && (!components[0] || components[0].test(name)))
      trustOutputs(path, components.slice(1), remove, kept);
    else if (entry.isDirectory() && !components.length && name === 'development') remove.push(path);
    else {
      const record = entry.isDirectory() && !components.length && name === 'production';
      kept.push({ path, why: record ? 'record' : entry.isFile() && LOCK.test(name) ? 'lock' : 'unlisted' });
    }
  }
}

/** Find the locks in Exact's outputs, and the Cargo profiles whose build files it can reclaim.
 * Cargo can omit CACHEDIR.TAG when Exact created the target first. Its profile lock and
 * fingerprint directory identify a profile under the target or a target triple. */
function buildLocks(target) {
  const locks = [], leases = [], profiles = new Set();
  const visit = (dir, managed = false) => {
    const profile = cargoLock(dir) && lstatSync(resolve(dir, '.fingerprint'), { throwIfNoEntry: false })?.isDirectory();
    if (profile) profiles.add(dir);
    for (const entry of entries(dir)) {
      const name = entry.name, path = resolve(dir, name);
      if (name === APP_USE || path === resolve(target, LEDGER + '.lock')) continue;
      if (entry.isSymbolicLink() && (LOCK.test(name) || name.endsWith('.leases'))) throw new Error(`cannot inspect linked build lock or lease directory ${path}`);
      if (entry.isFile() && LOCK.test(name)) locks.push(path);
      else if (entry.isDirectory() && name.endsWith('.leases')) leases.push(path);
      else if (entry.isDirectory() && !name.endsWith('.app') && name !== 'node_modules' && !(profile && CARGO_CONTENTS.has(name))) {
        if (managed || dir === target && (REMADE.has(name) || Object.hasOwn(TRUSTED, name) || name === '.apple-cargo-locks')) visit(path, true);
        else if (cargoLock(path) || dir === target && (cacheTag(path) || /^[a-z0-9_]+-[a-z0-9_]+-[a-z0-9_-]+$/.test(name))) visit(path);
      }
    }
  };
  visit(target);
  return { locks, leases, profiles };
}

/** What `clean` would remove from `target` and what it keeps, each an absolute path. */
function cleanable(target, profiles, locks) {
  const remove = [], kept = [];
  const anchors = new Set(locks);
  const parents = new Set();
  for (const path of [...anchors, ...profiles]) {
    if (path === target) continue;
    for (let dir = dirname(path); dir !== target; dir = dirname(dir)) parents.add(dir);
  }
  const candidate = path => {
    if (anchors.has(path)) kept.push({ path, why: 'lock' });
    else if (profiles.has(path)) {
      for (const name of readdirSync(path)) {
        const child = resolve(path, name);
        if (CARGO_CONTENTS.has(name)) candidate(child);
        else kept.push({ path: child, why: anchors.has(child) ? 'lock' : 'unlisted' });
      }
    } else if (parents.has(path)) {
      for (const name of readdirSync(path)) candidate(resolve(path, name));
    } else remove.push(path);
  };
  for (const entry of readdirSync(target, { withFileTypes: true })) {
    const name = entry.name, path = resolve(target, name);
    if (name === APP_USE || name === LEDGER + '.lock') kept.push({ path, why: 'lock' });
    else if (entry.isSymbolicLink()) kept.push({ path, why: 'unlisted' });
    else if (Object.hasOwn(TRUSTED, name) && entry.isDirectory()) {
      const development = [];
      trustOutputs(path, TRUSTED[name], development, kept);
      for (const dir of development) candidate(dir);
    } else if (RECORDS.has(name)) kept.push({ path, why: 'record' });
    else if (profiles.has(path) || REMADE.has(name) && (name.endsWith('.json') ? entry.isFile() : entry.isDirectory())) candidate(path);
    else if (entry.isDirectory() && [...profiles].some(profile => dirname(profile) === path)) {
      for (const name of readdirSync(path)) {
        const child = resolve(path, name);
        if (profiles.has(child)) candidate(child);
        else kept.push({ path: child, why: 'unlisted' });
      }
    } else kept.push({ path, why: anchors.has(path) ? 'lock' : 'unlisted' });
  }
  return { remove, kept };
}

/** Exact's app lease closes admission before the inventory. Keep Cargo and SwiftPM lock
 * paths in place so a waiting build and a later one still acquire the same inode. */
export async function clean(target, { dryRun = false } = {}) {
  if (process.platform === 'win32') throw new Error('clean is available on macOS and Linux');
  target = resolve(target);
  const { lock } = await libc(), fds = [], held = [], removed = [];
  let aside;
  const acquire = (path, create) => {
    const fd = lock(path, create);
    if (fd === null) held.push(path); else fds.push(fd);
    return fd !== null;
  };
  try {
    for (const name of [APP_USE, LEDGER + '.lock']) {
      if (!acquire(resolve(target, name), true)) return { status: 'busy', remove: [], kept: [], held };
    }
    const { locks, leases, profiles } = buildLocks(target);
    for (const path of locks) {
      if (basename(path) === BOOTSTRAP) {
        if (bootstrapping(path)) held.push(path);
      } else acquire(path, false);
    }
    if (leases.length) {
      const { leasedGenerations } = await import('../host/web/generations.mjs');
      for (const path of leases) {
        const live = leasedGenerations(path.slice(0, -'.leases'.length), { prune: false });
        for (const id of live) held.push(resolve(path, id));
      }
    }
    if (held.length) return { status: 'busy', remove: [], kept: [], held };
    const { remove, kept } = cleanable(target, profiles, locks);
    if (dryRun) return { status: 'ok', remove, kept, held };
    aside = mkdtempSync(resolve(target, '.exact-clean-'));
    for (const path of remove) {
      renameSync(path, resolve(aside, String(removed.length)));
      removed.push(path);
    }
    return { status: 'ok', remove: removed, kept, held };
  } catch (error) { throw Object.assign(error, { removed }); }
  finally {
    for (const fd of fds) closeSync(fd);
    if (aside) {
      try { rmSync(aside, { recursive: true }); }
      catch (error) { throw Object.assign(error, { removed }); }
    }
  }
}

/** `clean <app-dir> [--dry-run] [--json]`: the exit code. With `--json`, one object on stdout however it ends:
 * `{version: 1, status: "ok"|"busy"|"refused"|"failed", dryRun, target, remove, kept, held, error?}`. */
async function cleanCommand(args) {
  const json = args.includes('--json'), dryRun = args.includes('--dry-run'), words = args.filter(a => !['--json', '--dry-run'].includes(a));
  let target = null;
  const report = (status, { remove = [], kept = [], held = [], error } = {}) => {
    if (json) console.log(JSON.stringify({ version: 1, status, dryRun, target, remove, kept, held, ...(error ? { error } : {}) }));
    else {
      for (const path of remove) console.log(`${dryRun ? 'would remove' : 'removed'} ${path}`);
      for (const { path, why } of kept) console.log(`kept ${path} (${why})`);
      if (held.length) console.error(`clean: in use, so nothing was removed: ${held.join(', ')}`);
      if (error) console.error(`clean: ${error}`);
    }
    return { ok: 0, busy: 1, refused: 2, failed: 1 }[status];
  };
  if (words.length !== 1 || words[0].startsWith('-')) return report('refused', { error: 'usage: bun scripts/sweep.mjs clean <app-dir> [--dry-run] [--json]' });
  if (process.env.CARGO_TARGET_DIR) return report('refused', { error: 'CARGO_TARGET_DIR is set: that target is shared, so clean leaves it alone' });
  let app;
  try { app = (await import('./app.mjs')).resolveApp(resolve(words[0]), { useOutput: false }); }
  catch (error) { return report('refused', { error: error.message }); }
  target = app.target;
  if (target !== resolve(app.dir, 'target')) return report('refused', { error: `${target} belongs to a shared workspace` });
  if (process.platform === 'win32') return report('refused', { error: 'clean is available on macOS and Linux' });
  try {
    const info = lstatSync(target, { throwIfNoEntry: false });
    if (!info) return report('ok');
    if (!info.isDirectory() || realpathSync(target) !== target) return report('refused', { error: `${target} must be a dedicated directory in the app` });
    const result = await clean(target, { dryRun });
    return report(result.status, result);
  } catch (error) { return report('failed', { remove: error.removed, error: error.message }); }
}

if (import.meta.main) {
  if (process.argv[2] === 'clean') process.exit(await cleanCommand(process.argv.slice(3)));
  const removed = await sweep(resolve(process.argv[2] ?? 'target'));
  if (process.stdout.isTTY) console.log(Object.keys(removed).length ? removed : 'nothing unused for a week');
}
