import { lstatSync, readdirSync, readlinkSync, statSync, watch } from 'node:fs';
import { resolve } from 'node:path';
import { within } from '../../scripts/paths.mjs';

import { readBuilds } from '../../scripts/app.mjs';

/** The receipts a dev loop admits native clients by. A resident loop (host/web-js
 * `--serve-as`) bakes into its own EXACT_BAKE_OUTPUT, while `exact mac` / `exact ios`
 * write to the app's usual folder, so its native receipts come from both, its own first. */
export function readDevBuilds(app, env = process.env) {
  const own = readBuilds(app, env);
  if (!env.EXACT_BAKE_OUTPUT) return own;
  const { EXACT_BAKE_OUTPUT, ...usual } = env;
  const key = (r) => `${r.compat.inputs.platform}/${r.compat.target}`;
  const seen = new Set(own.map(key));
  return [...own, ...readBuilds(app, usual).filter((r) => r.compat.inputs.platform !== 'web' && !seen.has(key(r)))];
}
// Resource inventories include empty directories, hidden entries and links;
// compiler directory filters would omit some of these binary inputs.
export function nativeResourceInputs(build) {
  const { nativeResourceApp, metadata } = build.binary;
  return nativeResourceApp ? (metadata?.nativeResources ?? []).map(entry => ({
    path: resolve(nativeResourceApp.dir, entry.from, entry.name), directory: !!entry.directory,
  })) : [];
}

/** What a dev loop watches for its native build receipts: their resource
 * directories, the files a native build reads, and the subset of those paths
 * and the receipts' directories, missing and optional inputs that lie in
 * skipped trees (`outside`). A change to that subset can only invalidate cached
 * native admission (`cacheBuildInputs`), because nothing else watches it. Cargo's
 * registry and git sources (`cargo`) are left out: Cargo never rebuilds them on
 * an edit. */
export function nativeWatchPaths(receipts, outside, cargo) {
  const resources = receipts.flatMap(nativeResourceInputs);
  const directories = new Set(resources.filter(r => r.directory).map(r => r.path));
  const native = receipts.filter(r => r.compat?.inputs?.platform !== 'web');
  const files = [...resources.filter(r => !r.directory).map(r => r.path), ...native.flatMap(r => [...r.binary.inputs, ...(r.binary.links ?? [])].map(f => f.path))];
  const nativeOnly = new Set([...files, ...directories, ...native.flatMap(r => [...r.binary.missing, ...(r.optionalInputs ?? []), ...r.binary.directories.map(d => d.path)])]
    .filter(p => outside(p) && !cargo(p)));
  return { directories, files, nativeOnly };
}

// Receipt validation reads the full native graph only after an input event or
// a changed receipt. The compiler's metadata polls cover lost notifications.
export function cacheBuildInputs(validate) {
  const records = new Map();
  return {
    pending(build) {
      const key = build.compat.target + '/' + build.compat.id;
      const identity = JSON.stringify([build.binary, build.settings, build.optionalInputs]);
      let record = records.get(key);
      if (record?.identity !== identity) {
        const resources = nativeResourceInputs(build);
        record = {identity, files: new Set([...build.binary.inputs, ...(build.binary.links ?? []), ...resources.filter(r => !r.directory)].map(f => f.path)),
          roots: [...build.binary.missing, ...(build.optionalInputs ?? []), ...build.binary.directories.map(d => d.path), ...resources.filter(r => r.directory).map(r => r.path)]};
        records.set(key, record);
      }
      return record.pending ??= validate(build);
    },
    invalidate(path) {
      for (const record of records.values()) {
        if (record.files.has(path) || record.roots.some(root => within(root, path) || within(path, root))) delete record.pending;
      }
    },
    retain(builds) {
      const keys = new Set(builds.map(b => b.compat.target + '/' + b.compat.id));
      for (const key of records.keys()) if (!keys.has(key)) records.delete(key);
    },
  };
}

// Direct file watchers avoid recursive-directory event coalescing on macOS.
// The directory watcher discovers new paths; file metadata suppresses its
// delayed duplicate events. Bounds match the producer's byte limits, with a
// finite traversal/descriptor budget for the development watcher itself.
export function watchModuleSources(directory, ignore, changed) {
  const files = new Map();
  let identity = '', refusal = null, closed = false, queued = false;
  const metadata = stat => [stat.dev,stat.ino,stat.size,stat.mtimeNs,stat.ctimeNs].join(':');
  const rescan = () => {
    if (closed || queued) return;
    queued = true;
    queueMicrotask(() => { queued = false; if (!closed) scan(true); });
  };
  function scan(notify) {
    const next = new Map(), state = [];
    let entries = 0, bytes = 0;
    try {
      const rootStat = lstatSync(directory);
      if (!rootStat.isDirectory() || rootStat.isSymbolicLink()) throw new Error('module source root is not a regular directory');
      function walk(at, prefix = '', depth = 0) {
        if (depth > 64) throw new Error('module watcher source graph is too deep');
        for (const entry of readdirSync(at, { withFileTypes: true })) {
          const name = prefix + entry.name;
          if (ignore(name)) continue;
          if (++entries > 4096) throw new Error('module watcher source graph exceeds 4096 entries');
          const path = resolve(directory, name), stat = lstatSync(path, { bigint: true });
          // A link is what it leads to: retargeting it is an edit.
          if (stat.isSymbolicLink()) {
            let target = '';
            try { const to = statSync(path, { bigint: true }); target = `${readlinkSync(path)}:${metadata(to)}:${to.dev}:${to.ino}`; } catch { target = 'dangling'; }
            state.push([name, `symlink:${target}`]);
            continue;
          }
          if (stat.isDirectory()) { walk(path,name+'/',depth+1); continue; }
          // A design (`.forma`) is a source: a value-only edit goes as a batch (LLP 1121 stage 4).
          if (!/\.(ts|contract|json|forma)$/.test(name) && !['.exact/backend-origin', '.env.local', '.env'].includes(name)) continue;
          if (!stat.isFile()) { state.push([name,'not-regular']); continue; }
          bytes += Number(stat.size);
          if (stat.size > 16n*1024n*1024n || bytes > 64*1024*1024 || next.size >= 2048) throw new Error('module watcher source graph exceeds its file/byte budget');
          const stamp = metadata(stat);
          next.set(path,{ stamp, inode:stat.dev+':'+stat.ino });
          state.push([name,stamp]);
        }
      }
      walk(directory);
      for (const [path, old] of files) if (!next.has(path) || next.get(path).inode !== old.inode) { old.watch.close(); files.delete(path); }
      for (const [path, record] of next) {
        const old = files.get(path);
        const handle = old?.watch ?? watch(path, rescan);
        if (!old) handle.on('error',()=>{handle.close();if(files.get(path)?.watch===handle)files.delete(path);rescan();});
        files.set(path,{ ...record,watch:handle });
      }
      const value = JSON.stringify(state.sort(([a],[b])=>a<b?-1:a>b?1:0));
      const different = value !== identity || refusal !== null;
      identity = value; refusal = null;
      if (notify && different) changed(null);
    } catch (error) {
      const different = refusal?.message !== error.message;
      refusal = error;
      if (notify && different) changed(error);
    }
  }
  const rootStat = lstatSync(directory);
  if (!rootStat.isDirectory() || rootStat.isSymbolicLink()) throw new Error('module source root is not a regular directory');
  scan(false);
  const directoryWatch = watch(directory,{recursive:true},(_event,name)=>{ if(!name || !ignore(String(name))) rescan(); });
  directoryWatch.on('error',error=>{refusal=error;changed(error);});
  // FSEvents delivery is not guaranteed: Bun 1.4.2's fs.watch on macOS can
  // report a save minutes late or never (oven-sh/bun#43870). A stat poll of
  // the same identity bounds edit → plan at the compiler-input watcher's
  // 100 ms, whatever the watchers deliver; a no-change scan is a few lstats.
  const poll = setInterval(() => scan(true), 100);
  poll.unref?.();
  return { get error(){return refusal;}, close(){closed=true;clearInterval(poll);directoryWatch.close();for(const file of files.values())file.watch.close();files.clear();} };
}

/** Compare one directory's entries as well as listening for notifications.
 * Missing roots stay polled; an empty name invalidates the root itself.
 * A callback returning false defers the snapshot until a later scan. */
export function watchDirectoryInputs(directory, changed, { readNames = readdirSync, ignore = () => false } = {}) {
  let entries = new Map(), closed = false, queued = false;
  const metadata = path => {
    try {
      const stat = lstatSync(path, { bigint: true });
      // Directory mtimes change on child saves and would widen GPU-only rebuilds.
      return stat.isDirectory() ? `directory:${stat.dev}:${stat.ino}:${stat.mode}`
        : ['file', stat.dev, stat.ino, stat.mode, stat.size, stat.mtimeNs, stat.ctimeNs].join(':');
    } catch { return null; }
  };
  function scan(notify) {
    if (closed) return;
    const next = new Map([['', metadata(directory)]]);
    try {
      for (const name of readNames(directory)) {
        if (ignore(name)) continue;
        next.set(name, metadata(resolve(directory, name)));
      }
    } catch { /* Root identity still reports disappearance or a change of type. */ }
    const changes = entries.get('') !== next.get('') ? ['']
      : [...new Set([...entries.keys(), ...next.keys()])].filter(name => entries.get(name) !== next.get(name));
    const previous = entries;
    entries = next;
    if (notify) for (const name of changes) if (changed(name) === false) entries = previous;
  }
  const rescan = () => {
    if (closed || queued) return;
    queued = true;
    queueMicrotask(() => { queued = false; scan(true); });
  };
  scan(false);
  let watcher = null;
  try {
    watcher = watch(directory, rescan);
    watcher.on('error', () => { watcher?.close(); watcher = null; });
  } catch { /* Polling also discovers roots that do not exist yet. */ }
  const poll = setInterval(() => scan(true), 100);
  poll.unref?.();
  return { close() { closed = true; clearInterval(poll); watcher?.close(); entries.clear(); } };
}
