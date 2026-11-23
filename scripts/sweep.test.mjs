import { spyOn, test } from 'bun:test';
import * as childProcess from 'node:child_process';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { EventEmitter } from 'node:events';
import * as fs from 'node:fs';
import { existsSync, linkSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, symlinkSync, utimesSync, writeFileSync } from 'node:fs';
import * as os from 'node:os';
import { tmpdir } from 'node:os';
import { dirname, relative, resolve } from 'node:path';
import process from 'node:process';
import { clean, startSweep, sweep } from './sweep.mjs';
import * as sweepModule from './sweep.mjs';
import * as tools from './tools.mjs';
import { leaseGeneration } from '../host/web/generations.mjs';

const DAY = 86_400_000;
const A = '0123456789abcdef', B = 'fedcba9876543210';

function target() {
  const t = mkdtempSync(resolve(tmpdir(), 'exact-sweep-')), d = resolve(t, 'debug');
  for (const [pkg, hash] of [['a', A], ['b', B]]) {
    mkdirSync(resolve(d, '.fingerprint', `${pkg}-${hash}`), { recursive: true });
    writeFileSync(resolve(d, '.fingerprint', `${pkg}-${hash}`, `lib-${pkg}`), hash);
    mkdirSync(resolve(d, 'deps'), { recursive: true });
    writeFileSync(resolve(d, 'deps', `lib${pkg}-${hash}.rlib`), '');
    writeFileSync(resolve(d, 'deps', `${pkg}-${hash}.cgu.0.rcgu.o`), '');
    mkdirSync(resolve(d, 'build', `${pkg}-${hash}`, 'out'), { recursive: true });
  }
  mkdirSync(resolve(d, 'incremental', 'a-2ajo1mk2vrgcf', 's-old'), { recursive: true });
  return t;
}
const has = (t, ...p) => existsSync(resolve(t, 'debug', ...p));

test.skipIf(process.platform === 'win32')('a detached sweep ignores the caller’s TypeScript config and resolves its target there', async () => {
  const directory = mkdtempSync(resolve(tmpdir(), 'exact-sweep-caller-'));
  const cache = resolve(directory, 'target');
  mkdirSync(cache);
  writeFileSync(resolve(directory, 'tsconfig.json'), JSON.stringify({ compilerOptions: { moduleSuffixes: ['.ios', ''] } }));
  const previous = process.cwd();
  let child;
  try {
    process.chdir(directory);
    child = startSweep('./target');
  } finally { process.chdir(previous); }
  try {
    const status = await new Promise((ok, fail) => {
      child.once('error', fail);
      child.once('close', ok);
    });
    assert.equal(status, 0, readFileSync(resolve(cache, '.exact-sweep.log'), 'utf8'));
    assert.ok(existsSync(resolve(cache, '.exact-sweep.json')));
    assert.equal(readFileSync(resolve(cache, '.exact-sweep.log'), 'utf8'), '');
  } finally { rmSync(directory, { recursive: true, force: true }); }
}, 30_000);

test.skipIf(process.platform === 'win32')('a deferred sweep spawn failure leaves the caller running', async () => {
  const t = target(), child = new EventEmitter();
  child.unref = () => {};
  const error = Object.assign(new Error('spawn EAGAIN'), { code: 'EAGAIN' });
  let failure;
  const start = spyOn(childProcess, 'spawn').mockImplementation(() => {
    failure = Promise.resolve().then(() => child.emit('error', error));
    return child;
  });
  try {
    assert.equal(startSweep(t), child);
    assert.equal(await failure, true);
  } finally { start.mockRestore(); rmSync(t, { recursive: true, force: true }); }
});

test.skipIf(process.platform === 'win32')('a unit no build has read for a week goes; one read since the last sweep stays', async () => {
  const t = target(), now = Date.now(), b = resolve(t, 'debug/.fingerprint', `b-${B}`, 'lib-b');
  try {
    const written = statSync(b, { bigint: true }).mtimeNs;
    assert.deepEqual(await sweep(t, now), {}); // first sight: everything is new
    // Only the access time is reset: Cargo compares inputs against these mtimes to the nanosecond.
    assert.equal(statSync(b, { bigint: true }).mtimeNs, written);
    assert.equal(statSync(b).atimeMs, 86_400_000);
    readFileSync(b); // a build considers b
    const old = new Date(now - 8 * DAY);
    utimesSync(resolve(t, 'debug/incremental/a-2ajo1mk2vrgcf/s-old'), old, old);
    utimesSync(resolve(t, 'debug/incremental/a-2ajo1mk2vrgcf'), old, old);
    assert.deepEqual(await sweep(t, now + 8 * DAY), { debug: 2 });
    assert.ok(!has(t, '.fingerprint', `a-${A}`) && !has(t, 'deps', `liba-${A}.rlib`) && !has(t, 'deps', `a-${A}.cgu.0.rcgu.o`) && !has(t, 'build', `a-${A}`));
    assert.ok(!has(t, 'incremental', 'a-2ajo1mk2vrgcf'));
    assert.ok(has(t, '.fingerprint', `b-${B}`) && has(t, 'deps', `libb-${B}.rlib`) && has(t, 'build', `b-${B}`));
    assert.ok(!existsSync(resolve(t, '.exact-swept')));
  } finally { rmSync(t, { recursive: true, force: true }); }
});

const UNIT = 'exact_plan-7855ad78591661c6';
const CGU = '8ebodo4awrbx3jxe4yjnnvcga', REUSED = '93nxtgfjgrbildjtag3kno7jr', OTHER = 'a3nxtgfjgrbildjtag3kno7jr';
const OLD = '0h3oitd', NEW = '1ccoadi', THIRD = '03s0tzj';
const SECONDS = 1_700_000_000;
const memberName = (cgu, tag) => `${UNIT}.${cgu}.${tag}.rcgu.o`;
const objectPath = (t, cgu, tag) => resolve(t, 'debug/deps', memberName(cgu, tag));
function writeAt(path, seconds) {
  writeFileSync(path, 'object');
  utimesSync(path, seconds, seconds);
  return path;
}

const archivePath = t => resolve(t, 'debug/deps', `lib${UNIT}.rlib`);

function arMember(name, body) {
  const header = `${name.padEnd(16)}${'0'.padEnd(12)}${'0'.padEnd(6)}${'0'.padEnd(6)}${'644'.padEnd(8)}${String(body.length).padEnd(10)}` + '`\n';
  return header + body + (body.length % 2 ? '\n' : '');
}

function archive(members, format = 'BSD') {
  const magic = '!<arch>\n';
  if (format === 'BSD') {
    return magic + ['__.SYMDEF', 'lib.rmeta', ...members].map(name => {
      const padded = name.padEnd(Math.ceil(name.length / 4) * 4, '\0');
      return arMember(`#1/${padded.length}`, padded + 'object');
    }).join('');
  }
  let names = '';
  const objects = members.map(name => {
    const offset = names.length;
    names += `${name}/\n`;
    return arMember(`/${offset}`, 'object');
  });
  return magic + arMember('/', 'symbols') + arMember('//', names) + arMember('lib.rmeta/', 'metadata') + objects.join('');
}

for (const format of ['BSD', 'GNU']) {
  test.skipIf(process.platform === 'win32')(`a ${format} rlib keeps only the tags its members name`, async () => {
    const t = target(), deps = resolve(t, 'debug/deps');
    try {
      const old = writeAt(objectPath(t, CGU, OLD), SECONDS);
      const current = writeAt(objectPath(t, CGU, NEW), SECONDS + 1);
      const currentOther = writeAt(objectPath(t, REUSED, NEW), SECONDS + 1);
      const later = writeAt(objectPath(t, OTHER, THIRD), SECONDS + 3);
      const plain = writeAt(resolve(deps, `${UNIT}.exact_plan.8f24f1ac075cbf0f-cgu.00.rcgu.o`), SECONDS);
      writeAt(resolve(deps, `lib${UNIT}.rmeta`), SECONDS);
      writeAt(resolve(deps, `${UNIT}.d`), SECONDS);
      writeFileSync(archivePath(t), archive([memberName(CGU, NEW), memberName(REUSED, NEW)], format));
      utimesSync(archivePath(t), SECONDS + 2, SECONDS + 2);

      assert.deepEqual(await sweep(t), { debug: 2 });
      for (const path of [old, later]) assert.ok(!existsSync(path), path);
      for (const path of [current, currentOther, plain]) assert.ok(existsSync(path), path);
    } finally { rmSync(t, { recursive: true, force: true }); }
  });
}

for (const [name, rlib, other] of [
  ['an rlib and an executable', true, UNIT],
  ['an executable alone', false, UNIT],
  ['an rlib and a dylib', true, `lib${UNIT}.dylib`],
  ['an rlib and a shared object', true, `lib${UNIT}.so`],
  ['an rlib and a static library', true, `lib${UNIT}.a`],
  ['an rlib and another hashed output', true, `other-${UNIT.slice(-16)}.output`],
  ['no linked output', false, null],
]) {
  test.skipIf(process.platform === 'win32')(`a unit with ${name} keeps every object set`, async () => {
    const t = target();
    try {
      const old = writeAt(objectPath(t, CGU, OLD), SECONDS);
      const later = writeAt(objectPath(t, CGU, NEW), SECONDS + 1);
      if (rlib) {
        writeFileSync(archivePath(t), archive([memberName(CGU, NEW)]));
        utimesSync(archivePath(t), SECONDS + 2, SECONDS + 2);
      }
      if (other) writeAt(resolve(t, 'debug/deps', other), SECONDS + 2);
      assert.deepEqual(await sweep(t), {});
      for (const path of [old, later]) assert.ok(existsSync(path), path);
    } finally { rmSync(t, { recursive: true, force: true }); }
  });
}

for (const [name, contents] of [
  ['an unreadable', null],
  ['a truncated', archive([memberName(CGU, NEW), memberName(REUSED, NEW)]).slice(0, -3)],
  ['an untagged', archive([`${UNIT}.exact_plan.8f24f1ac075cbf0f-cgu.00.rcgu.o`])],
]) {
  test.skipIf(process.platform === 'win32')(`${name} rlib keeps every object set`, async () => {
    const t = target(), rlib = archivePath(t);
    const open = fs.openSync;
    let blocked;
    try {
      const old = writeAt(objectPath(t, CGU, OLD), SECONDS);
      const later = writeAt(objectPath(t, CGU, NEW), SECONDS + 1);
      writeFileSync(rlib, contents ?? archive([memberName(CGU, NEW)]));
      utimesSync(rlib, SECONDS + 2, SECONDS + 2);
      if (contents === null) {
        blocked = spyOn(fs, 'openSync').mockImplementation((path, ...args) => {
          if (path === rlib) throw Object.assign(new Error('permission denied'), { code: 'EACCES' });
          return open(path, ...args);
        });
      }
      assert.deepEqual(await sweep(t), {});
      for (const path of [old, later]) assert.ok(existsSync(path), path);
    } finally { blocked?.mockRestore(); rmSync(t, { recursive: true, force: true }); }
  });
}

test.skipIf(process.platform === 'win32')('the rlib names the fully reused set after an interrupted rebuild', async () => {
  const t = target();
  try {
    const old = writeAt(objectPath(t, CGU, OLD), SECONDS);
    const reused = writeAt(objectPath(t, REUSED, OLD), SECONDS);
    const interrupted = writeAt(objectPath(t, CGU, NEW), SECONDS + 1);
    const interruptedReuse = objectPath(t, REUSED, NEW);
    linkSync(reused, interruptedReuse);
    const live = objectPath(t, CGU, THIRD), liveReuse = objectPath(t, REUSED, THIRD);
    linkSync(old, live);
    linkSync(reused, liveReuse);
    writeFileSync(archivePath(t), archive([memberName(CGU, THIRD), memberName(REUSED, THIRD)]));
    utimesSync(archivePath(t), SECONDS + 2, SECONDS + 2);

    await sweep(t);
    for (const path of [live, liveReuse]) assert.ok(existsSync(path), path);
    for (const path of [old, reused, interrupted, interruptedReuse]) assert.ok(!existsSync(path), path);
  } finally { rmSync(t, { recursive: true, force: true }); }
});

test.skipIf(process.platform === 'win32')('linked deps is skipped during object trimming and unit expiry', async () => {
  const t = target(), now = Date.now(), deps = resolve(t, 'debug/deps'), other = resolve(t, 'other-deps');
  try {
    writeAt(objectPath(t, CGU, OLD), SECONDS);
    writeAt(objectPath(t, CGU, NEW), SECONDS + 1);
    writeFileSync(archivePath(t), archive([memberName(CGU, NEW)]));
    const names = readdirSync(deps).sort();
    fs.renameSync(deps, other);
    symlinkSync(other, deps);
    await sweep(t, now);
    assert.deepEqual(readdirSync(other).sort(), names);
    await sweep(t, now + 8 * DAY);
    assert.deepEqual(readdirSync(resolve(t, 'debug/.fingerprint')), []);
    assert.deepEqual(readdirSync(other).sort(), names);
  } finally { rmSync(t, { recursive: true, force: true }); }
});

function session(cache, prefix, state, time) {
  const dir = resolve(cache, `${prefix}-${state}`), lock = resolve(cache, `${prefix}.lock`);
  mkdirSync(dir, { recursive: true });
  utimesSync(dir, time, time);
  writeAt(lock, time);
  return [dir, lock];
}

test.skipIf(process.platform === 'win32')('linked incremental is skipped during session trimming', async () => {
  const t = target(), now = Date.now(), incr = resolve(t, 'debug/incremental'), other = resolve(t, 'other-incremental');
  try {
    fs.renameSync(incr, other);
    symlinkSync(other, incr);
    const working = session(resolve(other, 'b-2ajo1mk2vrgcf'), 's-hoeq3b8k0z-1xawk5k', 'working', now / 1000);
    await sweep(t, now);
    for (const path of working) assert.ok(existsSync(path), path);
  } finally { rmSync(t, { recursive: true, force: true }); }
});

test.skipIf(process.platform === 'win32')('a cache replaced by a link after listing is skipped', async () => {
  const t = target(), now = Date.now(), incr = resolve(t, 'debug/incremental');
  const cache = resolve(incr, 'a-2ajo1mk2vrgcf'), other = resolve(t, 'other-cache');
  const list = fs.readdirSync;
  const working = session(other, 's-hoeq3b8k0z-1xawk5k', 'working', now / 1000);
  let swapped = false;
  const listing = spyOn(fs, 'readdirSync').mockImplementation((path, ...args) => {
    const entries = list(path, ...args);
    if (path === incr && !swapped) {
      fs.renameSync(cache, resolve(t, 'previous-cache'));
      symlinkSync(other, cache);
      swapped = true;
    }
    return entries;
  });
  try {
    await sweep(t, now);
    assert.ok(swapped);
    for (const path of working) assert.ok(existsSync(path), path);
  } finally { listing.mockRestore(); rmSync(t, { recursive: true, force: true }); }
});

test.skipIf(process.platform === 'win32')('rustc session timestamps choose all newest sessions and their locks', async () => {
  const t = target(), now = Date.now(), incr = resolve(t, 'debug/incremental');
  const cache = resolve(incr, 'exact_kernel-2ajo1mk2vrgcf');
  try {
    const recent = now / 1000;
    const old = session(cache, 's-hoensmxwyy-0jdnav7', '202u7uhbq2x49og73537n6x11', recent - 1);
    const next = session(cache, 's-hoep4z8ar6-1kg16cg', '4j7mundduc6eepln5mwp21tr6', recent - 2);
    const tied = session(cache, 's-hoep4z8ar6-1xawk5k', '4j7mundduc6eepln5mwp21tr6', recent - 3);
    const working = session(cache, 's-hoeq3b8k0z-1xawk5k', 'working', recent);
    const sibling = session(resolve(incr, 'exact_kernel-13qy4wzro974q'), 's-hoensowkvb-15utmu8', '29sc8x5u29tighly4942zf8rc', recent - 7200);

    const removed = await sweep(t, now);
    for (const file of [...next, ...tied, ...sibling]) assert.ok(existsSync(file), file);
    for (const file of [...old, ...working]) assert.ok(!existsSync(file), file);
    assert.deepEqual(removed, { debug: 4 });
  } finally { rmSync(t, { recursive: true, force: true }); }
});

test.skipIf(process.platform === 'win32')('a completed session without a lock cannot displace the last usable session', async () => {
  const t = target(), now = Date.now(), cache = resolve(t, 'debug/incremental/exact_kernel-2ajo1mk2vrgcf');
  try {
    const usable = session(cache, 's-hoensmxwyy-0jdnav7', '202u7uhbq2x49og73537n6x11', now / 1000);
    const [missing, lock] = session(cache, 's-hoep4z8ar6-1kg16cg', '4j7mundduc6eepln5mwp21tr6', now / 1000);
    rmSync(lock);
    await sweep(t, now);
    for (const path of usable) assert.ok(existsSync(path), path);
    assert.ok(!existsSync(missing));
  } finally { rmSync(t, { recursive: true, force: true }); }
});

test.skipIf(process.platform === 'win32')('trimming a session does not restart its cache lifetime', async () => {
  const t = target(), trimmed = Date.now(), cache = resolve(t, 'debug/incremental/exact_kernel-2ajo1mk2vrgcf');
  try {
    const time = (trimmed - 6 * DAY) / 1000;
    session(cache, 's-hoensmxwyy-0jdnav7', '202u7uhbq2x49og73537n6x11', time);
    session(cache, 's-hoep4z8ar6-1kg16cg', '4j7mundduc6eepln5mwp21tr6', time);
    utimesSync(cache, time, time);
    await sweep(t, trimmed);
    assert.ok(existsSync(cache));
    await sweep(t, trimmed + 2 * DAY);
    assert.ok(!existsSync(cache));
  } finally { rmSync(t, { recursive: true, force: true }); }
});

test.skipIf(process.platform === 'win32')('an incremental cache keeps the activity of its newest trimmed child', async () => {
  const t = target(), now = Date.now(), cache = resolve(t, 'debug/incremental/exact_kernel-2ajo1mk2vrgcf');
  try {
    const completed = session(cache, 's-hoensmxwyy-0jdnav7', '202u7uhbq2x49og73537n6x11', (now - 6 * DAY) / 1000);
    const working = session(cache, 's-hoeq3b8k0z-1xawk5k', 'working', (now - DAY) / 1000);
    assert.deepEqual(await sweep(t, now), { debug: 2 });
    for (const file of working) assert.ok(!existsSync(file), file);
    await sweep(t, now + 5 * DAY);
    for (const file of completed) assert.ok(existsSync(file), file);
    await sweep(t, now + 7 * DAY);
    assert.ok(!existsSync(cache));
  } finally { rmSync(t, { recursive: true, force: true }); }
});

test.skipIf(process.platform === 'win32')('an interrupted trim preserves the cache activity already recorded', async () => {
  const t = target(), now = Date.now(), cache = resolve(t, 'debug/incremental/exact_kernel-2ajo1mk2vrgcf');
  const completed = session(cache, 's-hoensmxwyy-0jdnav7', '202u7uhbq2x49og73537n6x11', (now - 6 * DAY) / 1000);
  const [, lock] = session(cache, 's-hoeq3b8k0z-1xawk5k', 'working', (now - DAY) / 1000);
  const rename = fs.renameSync;
  const interrupt = spyOn(fs, 'renameSync').mockImplementation((from, to) => {
    rename(from, to);
    if (from === lock) throw new Error('interrupted trim');
  });
  try {
    try { await assert.rejects(sweep(t, now), /interrupted trim/); }
    finally { interrupt.mockRestore(); }
    await sweep(t, now + 5 * DAY);
    for (const path of completed) assert.ok(existsSync(path), path);
  } finally { rmSync(t, { recursive: true, force: true }); }
});

test.skipIf(process.platform === 'win32')('session lock cleanup keeps unrecognized names', async () => {
  const t = target(), now = Date.now(), cache = resolve(t, 'debug/incremental/exact_kernel-2ajo1mk2vrgcf');
  try {
    const time = (now - 60_000) / 1000;
    const unknown = session(cache, 's-hoeq3b8k0-1xawk5k', 'working', time);
    const orphan = writeAt(resolve(cache, 's-hoeq3b8k0z-1xawk5k.lock'), time);
    assert.deepEqual(await sweep(t, now), { debug: 1 });
    for (const file of unknown) assert.ok(existsSync(file), file);
    assert.ok(!existsSync(orphan));
  } finally { rmSync(t, { recursive: true, force: true }); }
});

for (const [kind, mode] of [['flock', 'LOCK_EX'], ['fcntl', 'F_SETLK']]) {
  test.skipIf(process.platform === 'win32' || kind === 'fcntl' && process.platform !== 'darwin')(`a held rustc session lock via ${kind} protects its session during trimming and cache expiry`, async () => {
    const t = target(), now = Date.now(), cache = resolve(t, 'debug/incremental/exact_kernel-2ajo1mk2vrgcf');
    try {
      const time = (now - 60_000) / 1000;
      const completed = session(cache, 's-hoensmxwyy-0jdnav7', '202u7uhbq2x49og73537n6x11', time);
      const [working, lock] = session(cache, 's-hoeq3b8k0z-1xawk5k', 'working', time);
      const holder = await hold(lock, mode);
      const exited = new Promise(ok => holder.once('close', ok));
      try {
        assert.deepEqual(await sweep(t, now), {});
        for (const file of [working, lock]) assert.ok(existsSync(file), file);
        await sweep(t, now + 8 * DAY);
        for (const file of [...completed, working, lock]) assert.ok(existsSync(file), file);
      } finally { holder.kill(); await exited; }
      await sweep(t, now + 8 * DAY);
      assert.ok(!existsSync(cache));
    } finally { rmSync(t, { recursive: true, force: true }); }
  });
}

test.skipIf(process.platform === 'win32')('trimming an abandoned session removes the cache it empties', async () => {
  const t = target(), now = Date.now(), cache = resolve(t, 'debug/incremental/exact_kernel-2ajo1mk2vrgcf');
  try {
    session(cache, 's-hoeq3b8k0z-1xawk5k', 'working', now / 1000);
    const removed = await sweep(t, now);
    assert.ok(!existsSync(cache));
    assert.deepEqual(removed, { debug: 3 });
  } finally { rmSync(t, { recursive: true, force: true }); }
});

for (const [reason, age] of [['cache expiry', 8 * DAY], ['trimming the last known session', DAY]]) {
  test.skipIf(process.platform === 'win32')(`a session added after listing survives ${reason}`, async () => {
    const t = target(), now = Date.now(), cache = resolve(t, 'debug/incremental/exact_kernel-2ajo1mk2vrgcf');
    const [, knownLock] = session(cache, 's-hoensmxwyy-0jdnav7', 'working', (now - age) / 1000);
    const open = fs.openSync;
    let working;
    const opening = spyOn(fs, 'openSync').mockImplementation((path, ...args) => {
      if (path === knownLock) working = session(cache, 's-hoeq3b8k0z-1xawk5k', 'working', now / 1000);
      return open(path, ...args);
    });
    try {
      await sweep(t, now);
      assert.ok(working);
      for (const path of working) assert.ok(existsSync(path), path);
    } finally { opening.mockRestore(); rmSync(t, { recursive: true, force: true }); }
  });
}

for (const [name, type] of [['NFS', 0x6969], ['a failed filesystem probe', null]]) {
  test.skipIf(process.platform === 'win32')(`on Linux ${name} keeps a profile and its ledger while a local profile is swept`, async () => {
    const t = target(), now = Date.now(), debug = resolve(t, 'debug');
    const release = resolve(t, 'release/.fingerprint', `a-${A}`);
    const path = resolve(t, '.exact-sweep.json'), cacheKey = 'debug#a-2ajo1mk2vrgcf';
    try {
      mkdirSync(release, { recursive: true });
      writeFileSync(resolve(release, 'lib-a'), A);
      await sweep(t, now);
      const ledger = JSON.parse(readFileSync(path, 'utf8'));
      ledger.caches[cacheKey] = now;
      writeFileSync(path, JSON.stringify(ledger));
      const locks = ['.cargo-lock', '.cargo-build-lock'].map(name => resolve(debug, name));
      for (const lock of locks) rmSync(lock);

      const platform = spyOn(os, 'platform').mockReturnValue('linux');
      const disk = spyOn(fs, 'statfsSync').mockImplementation(dir => {
        if (dir !== debug) return { type: 0xef53 };
        if (type === null) throw new Error('statfs failed');
        return { type };
      });
      try { assert.deepEqual(await sweep(t, now + 8 * DAY), { release: 1 }); }
      finally { disk.mockRestore(); platform.mockRestore(); }

      for (const lock of locks) assert.ok(!existsSync(lock), lock);
      assert.ok(has(t, '.fingerprint', `a-${A}`));
      assert.ok(!existsSync(release));
      const { units, caches } = JSON.parse(readFileSync(path, 'utf8'));
      assert.deepEqual(units, { [`debug#${A}`]: now, [`debug#${B}`]: now });
      assert.deepEqual(caches, { [cacheKey]: now });
    } finally { rmSync(t, { recursive: true, force: true }); }
  });
}

for (const lockName of ['.cargo-lock', '.cargo-build-lock']) {
  test.skipIf(process.platform === 'win32')(`a profile held by ${lockName} keeps its fingerprints and working session`, async () => {
    const t = target(), now = Date.now();
    await sweep(t, now);
    const cache = resolve(t, 'debug/incremental/a-2ajo1mk2vrgcf');
    const started = new Date(now + 8 * DAY);
    utimesSync(resolve(cache, 's-old'), started, started);
    const [working, workingLock] = session(cache, 's-hoeq3b8k0z-1xawk5k', 'working', started);
    try {
      const holder = await hold(resolve(t, 'debug', lockName));
      const exited = new Promise(ok => holder.once('close', ok));
      try {
        assert.ok(holder.listenerCount('error'), 'the lock holder must report process errors');
        const removed = await sweep(t, now + 8 * DAY);
        assert.ok(existsSync(working) && existsSync(workingLock));
        assert.ok(has(t, '.fingerprint', `a-${A}`));
        assert.deepEqual(removed, {});
        const units = JSON.parse(readFileSync(resolve(t, '.exact-sweep.json'), 'utf8')).units;
        assert.equal(units[`debug#${A}`], now);
      } finally { holder.kill(); await exited; }
      assert.deepEqual(await sweep(t, now + 9 * DAY), { debug: 4 });
      assert.ok(!existsSync(working) && !existsSync(workingLock));
      assert.deepEqual(readdirSync(resolve(t, 'debug/.fingerprint')), []);
    } finally { rmSync(t, { recursive: true, force: true }); }
  });
}

test.skipIf(process.platform !== 'win32')('Windows keeps Cargo units instead of using an unverified lock protocol', async () => {
  const t = target();
  try {
    startSweep(t);
    await assert.rejects(sweep(t), /unavailable on Windows/);
    assert.ok(has(t, '.fingerprint', `a-${A}`));
    assert.ok(has(t, '.fingerprint', `b-${B}`));
    assert.ok(!existsSync(resolve(t, '.exact-sweep.json')));
    assert.ok(!existsSync(resolve(t, '.exact-swept')));
  } finally { rmSync(t, { recursive: true, force: true }); }
});

for (const [name, args, status, expected, options = {}] of [
  ['successful Cargo run', ['vocab', 'button'], 0, ['run', 'metadata', 'sweep']],
  ['build --json', ['build', '--stdin', '--json'], 0, ['run']],
  ['vocab --json', ['vocab', '--json'], 0, ['run']],
  ['Cargo exit 101', ['vocab', 'button'], 101, ['run', '--version']],
  ['signaled Cargo run', ['vocab', 'button'], null, ['run']],
  ['redirected Cargo target', ['vocab', 'button'], 0, ['run', 'metadata'], { redirected: true }],
  ['split Cargo build directory', ['vocab', 'button'], 0, ['run', 'metadata'], { split: true }],
  ['Cargo target reported through an alias', ['vocab', 'button'], 0, ['run', 'metadata', 'sweep'], { alias: true }],
  ['checkout target linked into another checkout', ['vocab', 'button'], 0, ['run'], { linked: true }],
  ['malformed sweep ledger', ['vocab', 'button'], 0, ['run', 'metadata', 'sweep'], { brokenLedger: true }],
]) {
  test.skipIf(process.platform === 'win32')(`exact contract handles the target sweep after ${name}`, async () => {
    const { contract } = await import('./exact.mjs');
    const t = target(), realpath = fs.realpathSync;
    const root = realpath(mkdtempSync(resolve(tmpdir(), 'exact-sweep-workspace-')));
    const workspace = resolve(root, 'checkout');
    const checkoutTarget = resolve(workspace, 'target'), alias = resolve(workspace, 'alias');
    const calls = [], warnings = [];
    mkdirSync(workspace);
    if (options.linked) {
      const otherTarget = resolve(root, 'other-checkout/target');
      mkdirSync(otherTarget, { recursive: true });
      symlinkSync(otherTarget, checkoutTarget);
    } else mkdirSync(checkoutTarget);
    if (options.alias) symlinkSync(checkoutTarget, alias);
    if (options.brokenLedger) writeFileSync(resolve(t, '.exact-sweep.json'), '{');
    const physical = spyOn(fs, 'realpathSync').mockImplementation(path => path === tools.ROOT ? workspace : realpath(path));
    const start = sweepModule.startSweep;
    const compiler = spyOn(tools, 'contractTool').mockImplementation(({ env }) => tools.compilerCargo('run', env));
    const cargo = spyOn(childProcess, 'spawnSync').mockImplementation((cmd, [, verb], { env }) => {
      assert.equal(cmd, 'cargo');
      assert.equal(env.CARGO_TARGET_DIR, undefined);
      calls.push(verb);
      if (verb === 'run') return { status, signal: status === null ? 'SIGTERM' : null, stdout: '' };
      const target_directory = options.redirected ? t : options.alias ? alias : checkoutTarget;
      const build_directory = options.split ? t : options.alias ? checkoutTarget : undefined;
      return { status: 0, stdout: JSON.stringify({ target_directory, build_directory }) };
    });
    const hook = spyOn(sweepModule, 'startSweep').mockImplementation(path => {
      calls.push('sweep');
      assert.equal(path, realpath(checkoutTarget));
      if (options.brokenLedger) start(t);
    });
    const warning = spyOn(console, 'error').mockImplementation(message => warnings.push(message));
    try {
      assert.equal(await contract(args, { ...process.env, CARGO_TARGET_DIR: t }), status ?? 1);
      assert.deepEqual(calls, expected);
      if (options.brokenLedger) assert.match(warnings.join('\n'), /^warning: could not start the target sweep/);
      else assert.deepEqual(warnings, []);
    } finally {
      warning.mockRestore();
      hook.mockRestore();
      cargo.mockRestore();
      compiler.mockRestore();
      physical.mockRestore();
      rmSync(root, { recursive: true, force: true });
      rmSync(t, { recursive: true, force: true });
    }
  });
}

// An app's target/ as its builds leave it: Cargo output, Exact's remade outputs, its records, and an author's file.
function appTarget() {
  const t = mkdtempSync(resolve(tmpdir(), 'exact-clean-'));
  const files = {
    'debug/.fingerprint/a-0123456789abcdef/lib-a': '', 'debug/.cargo-lock': '', 'debug/my-notes.md': 'user file', 'aarch64-apple-ios/debug/.cargo-lock': '', 'aarch64-apple-ios/debug/.fingerprint/x': '',
    'apple-swift/ios-simulator-17.0/.lock': '', 'apple-swift/ios-simulator-17.0/artifacts/compiled': 'binary', 'apple-swift/ios-simulator-17.0.lock.flock': '', 'web-dist/index.html': '', 'web-dist.lock': '',
    'web-js-dev-compiler-app/x': '', 'kept-crates-host-dev.json': '{}',
    'bake/abcdef0123456789abcdef01/app.id/development/ios-x.build.json': '{}', 'bake/abcdef0123456789abcdef01/app.id/production/ios-x.build.json': '{}',
    'clients/abcdef0123456789abcdef01/app.id/.apple-build.lock.flock': '', 'clients/abcdef0123456789abcdef01/app.id/ios-simulator/aarch64-apple-ios-sim/embedded/development/standalone/App': '',
    'clients/abcdef0123456789abcdef01/app.id/ios/aarch64-apple-ios/embedded/production/standalone/App.dSYM/x': '', 'clients/abcdef0123456789abcdef01/app.id/macos/App.app/Contents/Info.plist': '',
    'bake-compiled/0123/lib.rs': '', '.apple-cargo-locks/t/liba.lock': '', 'bake/abcdef0123456789abcdef01/com.acme.app/development/ios-x.build.json': '{}',
    'clients/abcdef0123456789abcdef01/release.id/.apple-build.lock.flock': '', 'clients/abcdef0123456789abcdef01/release.id/ios/aarch64-apple-ios/embedded/production/standalone/App': '',
    'web-loops/0123/web/index.html': '',
    'traces/trace-1.json': '{}', 'web-receipts/0123/.exact-build.json': '{}', 'native-receipts/0123/receipt.json': '{}',
    'dist/App.app/x': '', 'screenshot.png': '',
  };
  for (const [path, text] of Object.entries(files)) { mkdirSync(resolve(t, path, '..'), { recursive: true }); writeFileSync(resolve(t, path), text); }
  return t;
}
const REMOVED = ['aarch64-apple-ios/debug/.fingerprint', 'apple-swift/ios-simulator-17.0/artifacts', 'bake-compiled',
  'bake/abcdef0123456789abcdef01/app.id/development', 'bake/abcdef0123456789abcdef01/com.acme.app/development', 'clients/abcdef0123456789abcdef01/app.id/ios-simulator/aarch64-apple-ios-sim/embedded/development',
  'debug/.fingerprint', 'web-dist', 'web-loops'];
const KEPT = [
  ['.apple-cargo-locks', 'unlisted'], ['.exact-app-use.lock', 'lock'], ['.exact-sweep.json.lock', 'lock'], ['aarch64-apple-ios/debug/.cargo-lock', 'lock'],
  ['apple-swift/ios-simulator-17.0.lock.flock', 'lock'], ['apple-swift/ios-simulator-17.0/.lock', 'lock'],
  ['bake/abcdef0123456789abcdef01/app.id/production', 'record'], ['clients/abcdef0123456789abcdef01/app.id/.apple-build.lock.flock', 'lock'],
  ['clients/abcdef0123456789abcdef01/app.id/ios/aarch64-apple-ios/embedded/production', 'record'], ['clients/abcdef0123456789abcdef01/app.id/macos/App.app', 'unlisted'],
  ['clients/abcdef0123456789abcdef01/release.id/.apple-build.lock.flock', 'lock'], ['clients/abcdef0123456789abcdef01/release.id/ios/aarch64-apple-ios/embedded/production', 'record'],
  ['debug/.cargo-lock', 'lock'], ['debug/my-notes.md', 'unlisted'], ['dist', 'unlisted'], ['kept-crates-host-dev.json', 'unlisted'],
  ['native-receipts', 'record'], ['screenshot.png', 'unlisted'], ['traces', 'record'], ['web-dist.lock', 'lock'],
  ['web-js-dev-compiler-app', 'unlisted'], ['web-receipts', 'record'],
];
/** Another process holding `path` until released. */
async function hold(path, mode = 'LOCK_EX') {
  // Darwin's struct flock has two off_t fields, then a pid and two shorts.
  const acquire = mode === 'F_SETLK' ? 'my $record = pack("qqiss", 0, 0, 0, F_WRLCK, SEEK_SET); fcntl($f, F_SETLK, $record) or die $!;' : `flock($f, ${mode}) or die $!;`;
  const holder = spawn('perl', ['-MFcntl=:flock,F_SETLK,F_WRLCK,SEEK_SET', '-e', `open my $f, ">>", $ARGV[0] or die $!; ${acquire} $| = 1; print "held\\n"; sleep 30`, path], { stdio: ['ignore', 'pipe', 'inherit'] });
  await new Promise((ok, fail) => {
    holder.stdout.once('data', ok);
    holder.once('error', fail);
    holder.once('close', code => fail(new Error(`lock holder exited ${code}`)));
  });
  return holder;
}
const relativeTo = (t, { remove, kept }) => ({ remove: remove.map(p => relative(t, p)).sort(), kept: kept.map(k => [relative(t, k.path), k.why]).sort() });

test.skipIf(process.platform === 'win32')('clean removes what builds remake, keeps records and what it does not list, and a dry run removes nothing', async () => {
  const t = appTarget();
  try {
    const dry = await clean(t, { dryRun: true });
    assert.equal(dry.status, 'ok', JSON.stringify(dry.held));
    assert.deepEqual(relativeTo(t, dry), { remove: REMOVED, kept: KEPT });
    for (const path of REMOVED) assert.ok(existsSync(resolve(t, path)), path);
    const inode = statSync(resolve(t, 'debug/.cargo-lock')).ino;
    const done = await clean(t);
    assert.equal(statSync(resolve(t, 'debug/.cargo-lock')).ino, inode);
    assert.equal(done.status, 'ok');
    assert.deepEqual(relativeTo(t, done), { remove: REMOVED, kept: KEPT });
    for (const path of REMOVED) assert.ok(!existsSync(resolve(t, path)), path);
    for (const [path] of KEPT) assert.ok(existsSync(resolve(t, path)), path);
    assert.ok(!existsSync(resolve(t, '.exact-swept')));
  } finally { rmSync(t, { recursive: true, force: true }); }
}, 30_000);

test.skipIf(process.platform === 'win32')('clean removes nothing while a build holds a lock in target/ or a drive leases a web build there', async () => {
  // A Cargo build, an Apple build of the app, an Apple build of a release whose output clean keeps, and a drive.
  // And the filesystem helper's build claim, a file naming a live owner; a dead owner's claim holds nothing.
  for (const busyWith of ['.exact-app-use.lock', '.exact-sweep.json.lock', 'debug/.cargo-lock', 'clients/abcdef0123456789abcdef01/app.id/.apple-build.lock.flock', 'clients/abcdef0123456789abcdef01/release.id/.apple-build.lock.flock', 'lease', 'exact-filesystem-tool/.bootstrap.lock']) {
    const t = appTarget(), generation = '0'.repeat(32), claim = resolve(t, 'exact-filesystem-tool/.bootstrap.lock');
    mkdirSync(resolve(claim, '..'));
    writeFileSync(claim, `${busyWith === 'exact-filesystem-tool/.bootstrap.lock' ? process.pid : 2 ** 22 + 1}-0123`);
    const holder = ['lease', 'exact-filesystem-tool/.bootstrap.lock'].includes(busyWith) ? null : await hold(resolve(t, busyWith), 'LOCK_SH');
    const release = holder ? () => holder.kill() : busyWith === 'lease' ? leaseGeneration(resolve(t, 'web-loops/0123/web'), generation) : () => {};
    try {
      for (const dryRun of [true, false]) {
        const busy = await clean(t, { dryRun });
        assert.equal(busy.status, 'busy');
        assert.deepEqual(busy.held, [resolve(t, busyWith === 'lease' ? `web-loops/0123/web.leases/${generation}` : busyWith)]);
        assert.deepEqual(busy.remove, []);
      }
      for (const path of REMOVED) assert.ok(existsSync(resolve(t, path)), path);
    } finally { release(); rmSync(t, { recursive: true, force: true }); }
  }
}, 30_000);

test.skipIf(process.platform === 'win32')('cleanup leaves linked records and unknown siblings of a Cargo profile intact', async () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-clean-links-')), t = resolve(dir, 'target');
  const record = resolve(dir, 'outside/k/app/development/receipt');
  mkdirSync(dirname(record), { recursive: true });
  writeFileSync(record, 'keep');
  mkdirSync(resolve(t, 'cross/profile/.fingerprint'), { recursive: true });
  writeFileSync(resolve(t, 'cross/profile/.cargo-lock'), '');
  mkdirSync(resolve(t, 'photos.leases/family'), { recursive: true });
  writeFileSync(resolve(t, 'photos.leases/family/photo'), 'picture');
  const oldLease = resolve(t, 'web-dist.leases', '0'.repeat(32), '4194305-1');
  mkdirSync(dirname(oldLease), { recursive: true });
  writeFileSync(oldLease, '');
  writeFileSync(resolve(t, 'CACHEDIR.TAG'), 'cache');
  writeFileSync(resolve(t, 'cross/notes'), 'notes');
  writeFileSync(resolve(t, 'web-dist-notes'), 'draft');
  symlinkSync(resolve(dir, 'outside'), resolve(t, 'bake'));
  try {
    for (const dryRun of [true, false]) {
      const result = await clean(t, { dryRun });
      assert.deepEqual(result.remove, []);
      assert.ok(existsSync(oldLease));
      assert.ok(existsSync(resolve(t, 'cross/profile/.fingerprint')));
      assert.equal(readFileSync(resolve(t, 'photos.leases/family/photo'), 'utf8'), 'picture');
    }
    assert.equal(readFileSync(record, 'utf8'), 'keep');
    assert.equal(readFileSync(resolve(t, 'cross/notes'), 'utf8'), 'notes');
    assert.equal(readFileSync(resolve(t, 'web-dist-notes'), 'utf8'), 'draft');
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.skipIf(process.platform === 'win32')('clean leaves a shared target alone', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-clean-shared-'));
  try {
    const run = spawnSync(process.execPath, [resolve(import.meta.dir, 'sweep.mjs'), 'clean', dir, '--json'], { encoding: 'utf8', env: { ...process.env, CARGO_TARGET_DIR: dir } });
    assert.equal(run.status, 2, run.stderr);
    assert.deepEqual({ ...JSON.parse(run.stdout), error: undefined }, { version: 1, status: 'refused', dryRun: false, target: null, remove: [], kept: [], held: [], error: undefined });
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
