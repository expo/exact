// Build swaps and GPU-cache copies each take a persistent lock file beside the tree they write. Removing it would
// let a waiter lock the old inode while another opens a new one under the same name. The PID in it is diagnostic;
// only the OS grants ownership.
import { closeSync, ftruncateSync, openSync, readFileSync, writeSync } from 'node:fs';
import { constants as os } from 'node:os';
import { toNamespacedPath } from 'node:path';

// flock(2)
const LOCK_SH = 1, LOCK_EX = 2, LOCK_NB = 4;
// Win32 CreateFileW and LockFileEx
const GENERIC_READ = 0x80000000, GENERIC_WRITE = 0x40000000, FILE_SHARE_READ = 1, FILE_SHARE_WRITE = 2, OPEN_EXISTING = 3, OPEN_ALWAYS = 4;
const FILE_ATTRIBUTE_NORMAL = 0x80, LOCKFILE_FAIL_IMMEDIATELY = 1, LOCKFILE_EXCLUSIVE_LOCK = 2;
const ERROR_FILE_NOT_FOUND = 2, ERROR_PATH_NOT_FOUND = 3, ERROR_LOCK_VIOLATION = 33, INVALID_HANDLE_VALUE = 0xffffffffffffffffn;

let locking;
function kernel() {
  if (locking) return locking;
  const { dlopen, read } = import.meta.require('bun:ffi');
  if (process.platform === 'win32') {
    const { symbols } = dlopen('kernel32.dll', {
      CreateFileW: { args: ['ptr', 'u32', 'u32', 'ptr', 'u32', 'u32', 'u64'], returns: 'u64' },
      LockFileEx: { args: ['u64', 'u32', 'u32', 'u32', 'u32', 'ptr'], returns: 'i32' },
      WriteFile: { args: ['u64', 'ptr', 'u32', 'ptr', 'ptr'], returns: 'i32' },
      SetEndOfFile: { args: ['u64'], returns: 'i32' },
      CloseHandle: { args: ['u64'], returns: 'i32' },
      GetLastError: { args: [], returns: 'u32' },
    });
    const fail = (operation, code = symbols.GetLastError()) => Object.assign(new Error(`${operation}: Win32 error ${code}`), { code: code === ERROR_FILE_NOT_FOUND || code === ERROR_PATH_NOT_FOUND ? 'ENOENT' : 'EIO' });
    locking = {
      open(path, create) {
        // Read/write sharing keeps all contenders on the same file; deletion is refused while any handle is open.
        const access = create ? GENERIC_READ + GENERIC_WRITE : GENERIC_READ;
        const handle = symbols.CreateFileW(Buffer.from(toNamespacedPath(path) + '\0', 'utf16le'), access, FILE_SHARE_READ + FILE_SHARE_WRITE, null, create ? OPEN_ALWAYS : OPEN_EXISTING, FILE_ATTRIBUTE_NORMAL, 0);
        if (BigInt(handle) === INVALID_HANDLE_VALUE) throw fail(`open build lock ${path}`);
        return handle;
      },
      take(handle, exclusive) {
        // Lock byte 64, beyond the PID: Windows byte locks also deny reads, so diagnostic reads stay outside it.
        // `OVERLAPPED.Offset` is 16 bytes into the structure on 64-bit Windows; the event and high offset stay zero.
        const overlapped = new Uint8Array(32);
        new DataView(overlapped.buffer).setUint32(16, 64, true);
        if (symbols.LockFileEx(handle, LOCKFILE_FAIL_IMMEDIATELY + (exclusive ? LOCKFILE_EXCLUSIVE_LOCK : 0), 0, 1, 0, overlapped)) return true;
        // Read the error in a second call: bun:ffi returns none, and nothing between the calls is expected to set it
        const code = symbols.GetLastError();
        if (code === ERROR_LOCK_VIOLATION) return false;
        throw fail('lock build output', code);
      },
      mark(handle) {
        const pid = Buffer.from(String(process.pid)), written = new Uint32Array(1);
        if (!symbols.WriteFile(handle, pid, pid.length, written, null)) throw fail('write build lock owner');
        if (written[0] !== pid.length) throw new Error(`write build lock owner: wrote ${written[0]} of ${pid.length} bytes`);
        if (!symbols.SetEndOfFile(handle)) throw fail('truncate build lock owner');
      },
      close: handle => { if (!symbols.CloseHandle(handle)) throw fail('close build lock'); },
    };
  } else {
    const errno = process.platform === 'darwin' ? '__error' : '__errno_location';
    const { symbols } = dlopen(process.platform === 'darwin' ? '/usr/lib/libSystem.B.dylib' : 'libc.so.6', {
      flock: { args: ['i32', 'i32'], returns: 'i32' }, [errno]: { args: [], returns: 'ptr' },
    });
    locking = {
      open: (path, create) => openSync(path, create ? 'a+' : 'r'),
      take(fd, exclusive) {
        // NFS needs write access for LOCK_EX; inspection uses LOCK_SH.
        if (symbols.flock(fd, (exclusive ? LOCK_EX : LOCK_SH) + LOCK_NB) === 0) return true;
        // Read errno in a second call: bun:ffi returns none, and nothing between the calls is expected to set it
        const code = read.i32(symbols[errno]());
        if (code === os.errno.EWOULDBLOCK || code === os.errno.EINTR) return false;
        throw new Error(`cannot lock build output: errno ${code}`);
      },
      mark(fd) { ftruncateSync(fd, 0); writeSync(fd, String(process.pid)); },
      close: closeSync,
    };
  }
  return locking;
}

/** `fn()` while this process holds `lock`, waiting up to 30 s for another holder. */
export function withLock(lock, fn) {
  const release = takeLock(lock);
  try { return fn(); } finally { release(); }
}

/** `lock` taken, waiting up to 30 s for another holder: what releases it. */
export function takeLock(lock, { waitMs = 30000 } = {}) {
  const osLock = kernel(), handle = osLock.open(lock, true), started = Date.now();
  try {
    while (!osLock.take(handle, true)) {
      if (Date.now() - started >= waitMs) throw Object.assign(new Error(`${lock}: waited ${waitMs / 1000} s for this build lock, which ${holder(lock)} holds; wait for its build to finish, or stop it if it is stuck`), { code: 'ELOCKED' });
      Atomics.wait(new Int32Array(new SharedArrayBuffer(4)), 0, 0, 10);
    }
    osLock.mark(handle);
  } catch (e) { osLock.close(handle); throw e; }
  let held = true;
  return () => { if (held) { held = false; osLock.close(handle); } };
}

/** The holder the lock file names, for a timeout's message. A new holder writes its PID just after taking the lock,
 * so for a moment the file can still name the one before it. */
function holder(lock) {
  let pid = NaN;
  try { pid = Number.parseInt(readFileSync(lock, 'utf8'), 10); } catch {}
  return Number.isInteger(pid) && pid > 0 ? `process ${pid}` : 'another process';
}

/** Whether a build owns `lock` now. */
export function lockHeld(lock) {
  const osLock = kernel();
  let handle;
  try { handle = osLock.open(lock, false); }
  catch (e) { if (e.code === 'ENOENT') return false; throw e; }
  try { return !osLock.take(handle, false); }
  finally { osLock.close(handle); }
}
