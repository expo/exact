import { lstatSync, readdirSync, readlinkSync, statSync, watch } from 'node:fs';
import { resolve } from 'node:path';

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
          // A design (`.tamber`) is a source: a value-only edit goes as a batch (LLP 1121 stage 4).
          if (!/\.(ts|contract|json|tamber)$/.test(name) && !['.exact/backend-origin', '.env.local', '.env'].includes(name)) continue;
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
      return stat.isDirectory() ? `directory:${stat.dev}:${stat.ino}`
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
