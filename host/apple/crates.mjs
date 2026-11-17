/**
 * Compiled registry crates, kept for the machine (LLP 1036.000 §11).
 *
 * Nine of ten crates under an app are from the registry, the same bytes in
 * every checkout, and the first build of every checkout compiled them again:
 * 190 of the 228 a fresh checkout built for Caltrain on a ten-core M4, 20 of
 * its 78 s. A development build keeps what Cargo compiled of them in
 * `~/.cache/exact/apple-crates`, and a target directory that has compiled
 * nothing starts as a copy-on-write clone of what is kept.
 *
 * Cargo decides whether a unit may be used, by the unit's
 * own fingerprint (compiler, profile, features, target, every crate under it)
 * and the order of its files' times, as it would had the directory been
 * moved. So what is kept is always one target directory's units, whole and
 * with their times (a generation), never units of two directories together:
 * two builds of one unit are not interchangeable to the units compiled
 * against them (a crate that includes a file from its build script's
 * directory has that directory in its hash), and only the times say which
 * were built together. A directory that started from a generation, or made
 * the first one, makes the next when it has compiled a registry unit the
 * newest lacks; any other directory keeps nothing.
 *
 * A kept unit is of a registry package no version of which is a path crate
 * or has one under it: a path crate's unit names its checkout, so it and all
 * above it are no use to another. rustc's own dep-info (`*.d`) names the
 * target directory it was written in, and the bake's receipt reads it as the
 * evidence of what a unit was compiled from (`scripts/app.mjs`), so it is
 * kept with that directory as a token and written out with the taker's.
 *
 * A checkout's own path crates (runner, kernel, hosts, JavaScript engine)
 * are most of an app's development build, and every app naming that checkout
 * compiled them again (LLP 1168.000 §4: 55% of a truly cold build of a chat
 * app, 90% once the registry crates were kept). An app's build therefore also
 * keeps, per checkout, a generation of the registry units and the checkout's
 * path units (`checkout-<hash of its path>`), and the next app's target starts
 * from it. A path unit names the checkout's files by path, so it serves only
 * that checkout; Cargo takes it or not by its fingerprint, the times of the
 * crate's files against its dep-info, so a crate edited, pulled or switched
 * since compiles again with everything above it. The commit is recorded, for
 * diagnosis, not keyed. A path crate's build script names its OUT_DIR in its
 * `output` by path, the maker's; Cargo reads it against the directory's own
 * (`root-output`), as for a moved target, so the instructions travel too. An
 * app's own crates, and anything above them, are never kept, and only an
 * app's own target keeps or takes them: one apps share (the checkout's, a
 * workspace's or a CARGO_TARGET_DIR) holds other apps' crates, and two builds
 * may write it.
 * EXACT_APPLE_KEEP_CHECKOUT=0 turns the scope off.
 *
 * A production build compiles everything it ships and keeps nothing.
 */
import { spawnSync } from 'node:child_process';
import { createHash } from 'node:crypto';
import { existsSync, mkdirSync, readFileSync, readdirSync, realpathSync, renameSync, rmSync, statSync, utimesSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { dirname, resolve } from 'node:path';

/** What stands for the target directory in a kept `.d` file. */
const TOKEN = '@exact-target@';
/** Generations kept: the newest, and the one a build may still be cloning. */
const GENERATIONS = 2;
/** A checkout's scope unused this long, or whose checkout is gone, is removed. */
const UNUSED_MS = 14 * 24 * 3_600_000;
const PARTS = ['build', 'deps', '.fingerprint'];

const text = (path) => { try { return readFileSync(path, 'utf8'); } catch { return null; } };
const names = (dir) => { try { return readdirSync(dir); } catch { return []; } };
const changed = (path) => { try { return statSync(path).mtimeMs; } catch { return 0; } };
/** A path with its existing part's links resolved, so two spellings of one directory compare equal. */
function realOf(path) {
  const at = resolve(path);
  try { return realpathSync(at); } catch { return at === dirname(at) ? at : resolve(realOf(dirname(at)), at.slice(dirname(at).length + 1)); }
}
/** A unit's fingerprint files and their times, which Cargo writes again when it compiles the unit again. */
const fingerprintTimes = (dir) => names(dir).sort().map(file => `${file}@${changed(resolve(dir, file))}`).join('\n');
const unitOf = (name) => /^(.+)-([0-9a-f]{16})$/.exec(name);
const hashOf = (file) => /-([0-9a-f]{16})\.[\w.]+$/.exec(file)?.[1];
/** A dependency-info file with one path prefix swapped for another, its time kept. */
function rewrite(from, to, was, now) {
  const stat = statSync(from);
  mkdirSync(dirname(to), { recursive: true });
  writeFileSync(to, readFileSync(from, 'utf8').replaceAll(was, now));
  utimesSync(to, stat.atime, stat.mtime);
}
/** Every rustc dep-info file of a profile directory, relative to it: the units' in `deps`, the build scripts' beside each script. */
const depInfo = (dir) => [...names(resolve(dir, 'deps')).map(file => `deps/${file}`),
  ...names(resolve(dir, 'build')).flatMap(unit => names(resolve(dir, 'build', unit)).map(file => `build/${unit}/${file}`))].filter(file => file.endsWith('.d'));
/** Clone whole trees into `into` (APFS copy-on-write; a plain copy elsewhere), times kept. */
function clone(sources, into) {
  if (!sources.length) return;
  mkdirSync(into, { recursive: true });
  for (let at = 0; at < sources.length; at += 400) {
    const batch = sources.slice(at, at + 400);
    if (spawnSync('cp', ['-c', '-R', '-p', ...batch, into], { stdio: 'ignore' }).status !== 0
        && spawnSync('cp', ['-R', '-p', ...batch, into], { stdio: 'ignore' }).status !== 0) throw new Error(`cp to ${into} failed`);
  }
}

const lockPackages = (lock) => Bun.TOML.parse(readFileSync(lock, 'utf8')).package ?? [];
/** The names of `packages` that are not `tainted` and have nothing tainted under them. */
function untainted(packages, tainted) {
  // A lock names a dependency by name, and by version where two share it: by name alone here, so either taints both.
  for (let grew = true; grew;) {
    grew = false;
    for (const p of packages) if (!tainted.has(p.name) && (p.dependencies ?? []).some(dep => tainted.has(dep.split(' ')[0]))) { tainted.add(p.name); grew = true; }
  }
  return new Set(packages.map(p => p.name).filter(name => !tainted.has(name)));
}
/** The registry packages of `lock` that hold nothing of a checkout: see the file comment. */
export function registryPackages(lock) {
  const packages = lockPackages(lock);
  return untainted(packages, new Set(packages.filter(p => !p.source).map(p => p.name)));
}
/** The packages of an app's `lock` a checkout's scope keeps: the registry's, and the `checkout`'s own path packages
 * (by name; see `checkoutCrates`), with no other path package under them. */
export function checkoutPackages(lock, checkout) {
  const packages = lockPackages(lock);
  return untainted(packages, new Set(packages.filter(p => !p.source && !checkout.has(p.name)).map(p => p.name)));
}
/** The path packages of the app's graph that are the checkout's own, by name: resolved by Cargo to a manifest under
 * the checkout, outside the app's directory and outside any app of the repository (a directory with an `app.json` or
 * an `app.contract`). A package of the app's, vendored or a member, named like one of the checkout's, is not. */
export function checkoutCrates(root, app) {
  // Locked: the build never changes an authored lock, and a lock that no longer matches keeps nothing.
  const listed = spawnSync('cargo', ['metadata', '--locked', '--offline', '--format-version', '1'], { cwd: app.workspace, encoding: 'utf8', maxBuffer: 256 << 20 });
  if (listed.status !== 0) throw new Error(`cargo metadata in ${app.workspace} exited ${listed.status}`);
  const real = realOf, top = real(root), own = real(app.dir) + '/';
  const inApp = (dir) => { for (let at = dir; at.startsWith(top + '/'); at = dirname(at)) if (existsSync(resolve(at, 'app.json')) || existsSync(resolve(at, 'app.contract'))) return true; return false; };
  const paths = JSON.parse(listed.stdout).packages.filter(p => !p.source).map(p => ({ name: p.name, dir: real(dirname(p.manifest_path)) }));
  const theirs = paths.filter(p => p.dir.startsWith(top + '/') && !(p.dir + '/').startsWith(own) && !inApp(p.dir));
  // A name the graph also gives a package that is not the checkout's is the app's: by name, it is not shared.
  const others = new Set(paths.filter(p => !theirs.includes(p)).map(p => p.name));
  return new Set(theirs.map(p => p.name).filter(name => !others.has(name)));
}

/**
 * The kept crates for one build at `profile`. `take(target, kind)` starts a
 * Cargo target directory that has compiled nothing at the profile from the
 * newest generation of its kind (`app` and `modules` are different graphs):
 * for an `app` build of `app` ({ dir, workspace }), its checkout's when there
 * is one, else the machine's registry generation. `keep(target, kind, lock)`
 * makes the next generation of the scope the directory started from when it
 * may and has something new.
 */
export function keptCrates({ root, env, profile, app = null }) {
  const idle = { take() {}, keep() {} };
  if (typeof Bun === 'undefined') return idle;
  const channel = env.RUSTUP_TOOLCHAIN ?? /channel\s*=\s*"([^"]+)"/.exec(text(resolve(root, 'rust-toolchain.toml')) ?? '')?.[1];
  if (!channel || !/^[\w.+-]+$/.test(channel)) return idle;
  const store = resolve(env.HOME ?? homedir(), '.cache/exact/apple-crates', `${channel}-${profile}`);
  const checkoutRoot = realOf(root);
  // A checkout's scope: an app build's, the switch not off, into the app's own target (one shared by apps, as the
  // checkout's or a CARGO_TARGET_DIR, may be written by two builds at once, and holds other apps' crates).
  const scopeOf = (kind, target) => {
    // A CARGO_TARGET_DIR is a target apps may share, wherever it is.
    if (kind !== 'app' || !app || env.EXACT_APPLE_KEEP_CHECKOUT === '0' || env.CARGO_TARGET_DIR) return null;
    // The app's directory, not its workspace: a workspace outside the checkout may hold several apps and their one target.
    const at = realOf(target), mine = realOf(app.dir);
    if (at.startsWith(checkoutRoot + '/') || mine === checkoutRoot || !at.startsWith(mine + '/')) return null;
    return resolve(store, kind, `checkout-${createHash('sha256').update(checkoutRoot).digest('hex').slice(0, 16)}`);
  };
  const generations = (dir) => names(dir).filter(name => name.startsWith('gen-')).sort();
  // The host's profile directory, and each Apple triple's beside it.
  const profileDirs = (dir) => [profile, ...names(dir).filter(name => /^[a-z0-9_]+-apple-[a-z0-9-]+$/.test(name)).map(triple => `${triple}/${profile}`)].filter(d => existsSync(resolve(dir, d, '.fingerprint')));
  // A path rustc would escape in a `.d` is not worth a second spelling: such a target keeps and takes nothing.
  const plain = (target) => /^[\w./+@-]+$/.test(target);
  const stampOf = (target) => resolve(target, `kept-crates-${profile}.json`);
  const warn = (what, error) => console.warn(`host/apple: ${what} (${error.message}); compiling as usual`);
  /** The scope's record: its checkout, when a build last took from it, and the commit each generation was made at. */
  const record = (scope, change) => {
    const path = resolve(scope, 'checkout.json'), was = JSON.parse(text(path) ?? 'null') ?? { root: checkoutRoot, made: [] };
    const now = change(was) ?? was, at = `${path}.${process.pid}`;
    writeFileSync(at, JSON.stringify(now));
    renameSync(at, path);
  };
  const commitOf = () => {
    const git = (...args) => spawnSync('git', ['-C', checkoutRoot, ...args], { encoding: 'utf8' });
    const head = git('rev-parse', 'HEAD');
    const status = git('status', '--porcelain', '--untracked-files=no');
    return { commit: head.status === 0 ? head.stdout.trim() : null, dirty: status.status === 0 ? status.stdout.trim().length > 0 : null };
  };
  /** Remove the checkout scopes of `kind` whose checkout is gone or that no build took from or made in `UNUSED_MS`. */
  const prune = (kind) => {
    for (const name of names(resolve(store, kind)).filter(name => name.startsWith('checkout-'))) {
      const scope = resolve(store, kind, name), was = JSON.parse(text(resolve(scope, 'checkout.json')) ?? 'null');
      // A scope being made in has a new entry, so its directory's time is a use too.
      const used = Math.max(was?.used ?? 0, changed(resolve(scope, 'checkout.json')), changed(scope));
      if ((was?.root && !existsSync(was.root)) || Date.now() - used > UNUSED_MS) rmSync(scope, { recursive: true, force: true });
    }
  };
  /** Make the next generation in `into` from `target`'s units each `wanted` says to keep (by profile directory). */
  const make = (into, target, dirs, wanted) => {
    const making = resolve(into, `.making-${process.pid}`);
    rmSync(making, { recursive: true, force: true });
    const before = new Map();
    for (const dir of dirs) {
      const units = wanted(dir), taken = new Set(units), hashes = new Set(units.map(name => unitOf(name)[2]));
      const here = resolve(target, dir), files = names(resolve(here, 'deps')).filter(file => hashes.has(hashOf(file)));
      for (const name of units) before.set(`${dir}/${name}`, fingerprintTimes(resolve(here, '.fingerprint', name)));
      // Fingerprints before outputs: a unit another build of this directory compiles meanwhile is then an old
      // fingerprint over new outputs, which Cargo compiles again, never new evidence over old code.
      clone(units.map(name => resolve(here, '.fingerprint', name)), resolve(making, dir, '.fingerprint'));
      clone(units.map(name => resolve(here, 'build', name)).filter(existsSync), resolve(making, dir, 'build'));
      clone(files.filter(file => !file.endsWith('.d')).map(file => resolve(here, 'deps', file)), resolve(making, dir, 'deps'));
      for (const file of depInfo(here)) {
        const [part, name] = file.split('/');
        if (part === 'deps' ? hashes.has(hashOf(name)) : taken.has(name)) rewrite(resolve(here, file), resolve(making, dir, file), target, TOKEN);
      }
    }
    // And a unit compiled while it was copied is not kept at all: this generation is not made.
    for (const [unit, times] of before) if (fingerprintTimes(resolve(target, unit.replace(/\/([^/]+)$/, '/.fingerprint/$1'))) !== times) {
      rmSync(making, { recursive: true, force: true });
      throw new Error(`${unit} was compiled again while it was kept`);
    }
    const made = `gen-${Date.now()}-${process.pid}`;
    renameSync(making, resolve(into, made));
    for (const old of generations(into).slice(0, -GENERATIONS)) rmSync(resolve(into, old), { recursive: true, force: true });
    for (const left of names(into)) if (left.startsWith('.making-') && Date.now() - changed(resolve(into, left)) > 3_600_000) rmSync(resolve(into, left), { recursive: true, force: true });
    return made;
  };
  return {
    take(target, kind) {
      const restored = [];
      try {
        const scope = scopeOf(kind, target), scoped = scope ? generations(scope).at(-1) : null;
        const from = scoped ? scope : resolve(store, kind), newest = scoped ?? generations(from).at(-1);
        if (!plain(target) || existsSync(resolve(target, profile)) || !newest) return;
        // Used before it is read, so no build's pruning removes it under this one.
        if (scoped) record(scope, was => ({ ...was, used: Date.now() }));
        const kept = resolve(from, newest);
        let units = 0;
        for (const dir of profileDirs(kept)) {
          const here = resolve(target, dir);
          if (existsSync(here)) continue;
          restored.push(here);
          clone(PARTS.map(part => resolve(kept, dir, part)).filter(existsSync), here);
          const invalid = new Set();
          for (const file of depInfo(here)) {
            const path = resolve(here, file);
            rewrite(resolve(kept, dir, file), path, TOKEN, target);
            const dep = readFileSync(path, 'utf8'), output = dep.slice(0, dep.indexOf(': '));
            if (resolve(output) !== path) invalid.add(hashOf(file) ?? unitOf(file.split('/')[1])?.[2]);
          }
          // A copied unit may retain another target's output path. Removing its
          // fingerprint makes Cargo regenerate the unit and its source evidence.
          for (const unit of names(resolve(here, '.fingerprint'))) {
            if (invalid.has(undefined) || invalid.has(unitOf(unit)?.[2])) rmSync(resolve(here, '.fingerprint', unit), { recursive: true, force: true });
          }
          units += names(resolve(here, '.fingerprint')).length;
        }
        writeFileSync(stampOf(target), JSON.stringify({ from: newest, scope: scoped ? 'checkout' : 'registry', saw: {} }));
        console.log(`host/apple: ${target.replace(root + '/', '')} starts with the ${units} ${scoped ? 'registry and checkout' : 'registry'} units this machine has compiled (${kept.replace(homedir(), '~')})`);
      } catch (error) {
        // Half a generation is worse than none: what was restored goes, and Cargo compiles it.
        for (const here of restored) rmSync(here, { recursive: true, force: true });
        warn('no kept crates', error);
      }
    },
    keep(target, kind, lock) {
      try {
        if (!plain(target)) return;
        const stamp = JSON.parse(text(stampOf(target)) ?? '{}'), dirs = profileDirs(target);
        const scope = scopeOf(kind, target), byCheckout = stamp.scope === 'checkout' && scope !== null;
        // A build of this checkout is a use of its scope, whether or not it keeps anything.
        if (scope && generations(scope).length) record(scope, was => ({ ...was, used: Date.now() }));
        // A unit compiled again rewrites its fingerprint's files, not its directory: the newest of their times.
        const saw = Object.fromEntries(dirs.map(dir => [dir, Math.max(changed(resolve(target, dir, '.fingerprint')),
          ...names(resolve(target, dir, '.fingerprint')).flatMap(unit => names(resolve(target, dir, '.fingerprint', unit)).map(file => changed(resolve(target, dir, '.fingerprint', unit, file)))))]));
        if (kind === 'app') prune(kind);
        if (dirs.every(dir => stamp.saw?.[dir] === saw[dir])) return;
        // A directory that took a checkout's generation keeps only in that scope; any other may keep in the registry's.
        const into = byCheckout ? scope : resolve(store, kind);
        let from = stamp.from ?? null;
        if (into) {
          const newest = generations(into).at(-1);
          // A directory that compiled its crates apart from every generation is not of their line: its units with theirs would be the two builds.
          if (stamp.from || !newest) {
            const checkout = byCheckout ? checkoutCrates(root, app) : null;
            const keeps = byCheckout ? checkoutPackages(lock, checkout) : registryPackages(lock);
            // The path packages of this app's graph that are not the checkout's: never kept, nor carried over.
            const owned = byCheckout ? new Set(lockPackages(lock).filter(p => !p.source && !checkout.has(p.name)).map(p => p.name)) : new Set();
            const has = (dir) => new Set(newest ? names(resolve(into, newest, dir, '.fingerprint')) : []);
            // This lock's units, and those it took, which another lock (an app outside the repository) may be the one to use; never an app's.
            const mine = (dir, kept = has(dir)) => names(resolve(target, dir, '.fingerprint')).filter(name => !owned.has(unitOf(name)?.[1]) && (keeps.has(unitOf(name)?.[1]) || kept.has(name)));
            // A path unit compiled again keeps its name: its fingerprint's times say it is new.
            const same = (dir, name) => fingerprintTimes(resolve(target, dir, '.fingerprint', name)) === fingerprintTimes(resolve(into, newest, dir, '.fingerprint', name));
            if (dirs.some(dir => { const kept = has(dir); return mine(dir, kept).some(name => !kept.has(name) || !same(dir, name)); })) {
              from = make(into, target, dirs, dir => mine(dir));
              if (byCheckout) record(scope, was => ({ ...was, used: Date.now(), made: [...(was.made ?? []), { gen: from, ...commitOf(), at: Date.now() }].slice(-GENERATIONS) }));
            }
          }
        }
        // The checkout's first generation: a directory that started from the registry's, or from nothing, and compiled the checkout's crates.
        let scoped = byCheckout;
        if (!byCheckout && scope && !generations(scope).length) {
          mkdirSync(scope, { recursive: true });
          const keeps = checkoutPackages(lock, checkoutCrates(root, app));
          const mine = (dir) => names(resolve(target, dir, '.fingerprint')).filter(name => keeps.has(unitOf(name)?.[1]));
          if (dirs.some(dir => mine(dir).length)) {
            const made = make(scope, target, dirs, mine);
            record(scope, was => ({ ...was, used: Date.now(), made: [{ gen: made, ...commitOf(), at: Date.now() }] }));
            from = made; scoped = true;
          }
        }
        writeFileSync(stampOf(target), JSON.stringify({ from, scope: scoped ? 'checkout' : 'registry', saw }));
      } catch (error) { warn('this build\'s crates were not kept', error); }
    },
  };
}
