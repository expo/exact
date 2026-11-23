import { spyOn, test } from 'bun:test';
import * as childProcess from 'node:child_process';
import assert from 'node:assert/strict';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, linkSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync, symlinkSync, utimesSync, writeFileSync } from 'node:fs';
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
const CGU = '8ebodo4awrbx3jxe4yjnnvcga', REUSED = '93nxtgfjgrbildjtag3kno7jr';
const OLD = '0h3oitd', NEW = '1ccoadi';
const SECONDS = 1_700_000_000;
const objectPath = (t, cgu, tag) => resolve(t, 'debug/deps', `${UNIT}.${cgu}.${tag}.rcgu.o`);
function writeAt(path, seconds) {
  writeFileSync(path, 'object');
  utimesSync(path, seconds, seconds);
  return path;
}

test.skipIf(process.platform === 'win32')('a newer partial rebuild leaves both object sets intact', async () => {
  const t = target();
  try {
    const old = writeAt(objectPath(t, CGU, OLD), SECONDS);
    const reused = writeAt(objectPath(t, REUSED, OLD), SECONDS);
    const partial = writeAt(objectPath(t, CGU, NEW), SECONDS + 1);
    await sweep(t);
    for (const file of [old, reused, partial]) assert.ok(existsSync(file), file);
  } finally { rmSync(t, { recursive: true, force: true }); }
});

test.skipIf(process.platform === 'win32')('a dropped codegen unit leaves both object sets intact', async () => {
  const t = target();
  try {
    const dropped = writeAt(objectPath(t, CGU, OLD), SECONDS + 1);
    const reused = writeAt(objectPath(t, REUSED, OLD), SECONDS);
    const current = objectPath(t, REUSED, NEW);
    linkSync(reused, current);
    await sweep(t);
    for (const file of [current, dropped, reused]) assert.ok(existsSync(file), file);
  } finally { rmSync(t, { recursive: true, force: true }); }
});

test.skipIf(process.platform === 'win32')('a complete rebuild supersedes objects within the same millisecond and keeps full-reuse ties and plain CGUs', async () => {
  const t = target(), deps = resolve(t, 'debug/deps');
  try {
    const old = writeAt(objectPath(t, CGU, OLD), SECONDS + 0.0001);
    const current = writeAt(objectPath(t, CGU, NEW), SECONDS + 0.0002);
    const oldNs = statSync(old, { bigint: true }).mtimeNs, currentNs = statSync(current, { bigint: true }).mtimeNs;
    assert.ok(oldNs < currentNs && oldNs / 1_000_000n === currentNs / 1_000_000n);
    const reused = writeAt(objectPath(t, REUSED, OLD), SECONDS);
    const currentReused = objectPath(t, REUSED, NEW);
    linkSync(reused, currentReused);
    const tie = '03s0tzj';
    linkSync(current, objectPath(t, CGU, tie));
    linkSync(reused, objectPath(t, REUSED, tie));
    const plain = (n) => writeAt(resolve(deps, `cc-fb420d4abaebbebd.cc.8f24f1ac075cbf0f-cgu.0${n}.rcgu.o`), SECONDS + n);
    const plainObjects = [plain(0), plain(1), plain(2)];

    assert.deepEqual(await sweep(t), { debug: 2 });
    assert.ok(!existsSync(old) && !existsSync(reused));
    for (const file of [current, currentReused, objectPath(t, CGU, tie), objectPath(t, REUSED, tie), ...plainObjects]) {
      assert.ok(existsSync(file), file);
    }
  } finally { rmSync(t, { recursive: true, force: true }); }
});

function session(cache, prefix, state, time) {
  const dir = resolve(cache, `${prefix}-${state}`), lock = resolve(cache, `${prefix}.lock`);
  mkdirSync(dir, { recursive: true });
  utimesSync(dir, time, time);
  writeAt(lock, time);
  return [dir, lock];
}

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

test.skipIf(process.platform === 'win32')('trimming an abandoned session removes the cache it empties', async () => {
  const t = target(), now = Date.now(), cache = resolve(t, 'debug/incremental/exact_kernel-2ajo1mk2vrgcf');
  try {
    session(cache, 's-hoeq3b8k0z-1xawk5k', 'working', now / 1000);
    const removed = await sweep(t, now);
    assert.ok(!existsSync(cache));
    assert.deepEqual(removed, { debug: 3 });
  } finally { rmSync(t, { recursive: true, force: true }); }
});

for (const lockName of ['.cargo-lock', '.cargo-build-lock']) {
  test.skipIf(process.platform === 'win32')(`a profile held by ${lockName} keeps its fingerprints and working session`, async () => {
    const t = target(), now = Date.now();
    await sweep(t, now);
    const working = resolve(t, 'debug/incremental/a-2ajo1mk2vrgcf/s-hoeq3b8k0z-1xawk5k-working');
    mkdirSync(working);
    const started = new Date(now + 8 * DAY);
    utimesSync(working, started, started);
    const holder = spawn('perl', ['-MFcntl=:flock', '-e', 'open my $f, ">>", $ARGV[0] or die; flock($f, LOCK_EX) or die; print "held\\n"; $| = 1; sleep 30', resolve(t, 'debug', lockName)], { stdio: ['ignore', 'pipe', 'inherit'] });
    const exited = new Promise(ok => holder.once('close', ok));
    try {
      await Promise.race([
        new Promise(ok => holder.stdout.once('data', ok)),
        exited.then(code => { throw new Error(`lock holder exited ${code}`); }),
      ]);
      const removed = await sweep(t, now + 8 * DAY);
      assert.ok(existsSync(working));
      assert.ok(has(t, '.fingerprint', `a-${A}`));
      assert.deepEqual(removed, {});
      const units = JSON.parse(readFileSync(resolve(t, '.exact-sweep.json'), 'utf8')).units;
      assert.equal(units[`debug#${A}`], now);
      holder.kill();
      await exited;
      assert.deepEqual(await sweep(t, now + 9 * DAY), { debug: 3 });
      assert.deepEqual(readdirSync(resolve(t, 'debug/.fingerprint')), []);
    } finally { holder.kill(); await exited; rmSync(t, { recursive: true, force: true }); }
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
  ['malformed sweep ledger', ['vocab', 'button'], 0, ['run', 'metadata', 'sweep'], { brokenLedger: true }],
]) {
  test.skipIf(process.platform === 'win32')(`exact contract handles the target sweep after ${name}`, async () => {
    const { contract } = await import('./exact.mjs');
    const t = target(), checkoutTarget = resolve(import.meta.dir, '../target'), calls = [], warnings = [];
    if (options.brokenLedger) writeFileSync(resolve(t, '.exact-sweep.json'), '{');
    const start = sweepModule.startSweep;
    const compiler = spyOn(tools, 'contractTool').mockImplementation(({ env }) => tools.compilerCargo('run', env));
    const cargo = spyOn(childProcess, 'spawnSync').mockImplementation((cmd, [, verb], { env }) => {
      assert.equal(cmd, 'cargo');
      assert.equal(env.CARGO_TARGET_DIR, undefined);
      calls.push(verb);
      if (verb === 'run') return { status, signal: status === null ? 'SIGTERM' : null, stdout: '' };
      const target_directory = options.redirected ? t : checkoutTarget;
      return { status: 0, stdout: JSON.stringify({ target_directory }) };
    });
    const hook = spyOn(sweepModule, 'startSweep').mockImplementation(path => {
      calls.push('sweep');
      assert.equal(path, checkoutTarget);
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
/** Another process holding `path` the way Cargo does (flock), until released. */
async function hold(path, mode = 'LOCK_EX') {
  const holder = spawn('perl', ['-MFcntl=:flock', '-e', `open my $f, ">>", $ARGV[0] or die; flock($f, ${mode}) or die; $| = 1; print "held\\n"; sleep 30`, path], { stdio: ['ignore', 'pipe', 'inherit'] });
  await new Promise(ok => holder.stdout.once('data', ok));
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
