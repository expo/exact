// Filesystem path comparisons that hold on Windows. A test against `dir + '/'`
// never matches a path `resolve` made there, whose separator is `\`; these take
// the platform's path module, a parameter so a test can pass `path.win32`.
import nodePath, { basename, delimiter, dirname, isAbsolute, parse, relative, resolve, sep } from 'node:path';
import { accessSync, constants, existsSync, lstatSync, mkdirSync, readdirSync, readFileSync, readlinkSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { fileURLToPath } from 'node:url';

const CHECKOUT = resolve(fileURLToPath(new URL('..', import.meta.url)));
const sameFile = (a, b) => {
  try { return realpathSync(a) === realpathSync(b); } catch { return false; }
};
/** Make a user path absolute, keeping the working directory's spelling only when it reaches the physical target. */
export function namedPath(path) {
  const cwd = process.cwd(), app = process.env.EXACT_APP_SPELLING;
  const base = sameFile(app, cwd) ? app : sameFile(process.env.PWD, cwd) ? process.env.PWD : cwd;
  const physical = resolve(cwd, path), named = resolve(base, path);
  return samePath(named, physical) ? named : physical;
}

let entry;
/** Bun resolves both import.meta.dir and argv[1]. Recover the entry's spelling from the OS,
 * accepting a candidate only when it names that same file. */
function invokedPath(file) {
  if (!sameFile(file, process.argv[1])) return file;
  if (entry !== undefined) return entry;
  entry = file;
  try {
    const command = process.platform === 'linux' ? readFileSync('/proc/self/cmdline', 'utf8')
      : process.platform === 'win32' ? spawnSync('powershell.exe', ['-NoProfile', '-Command', `[Console]::OutputEncoding = [Text.UTF8Encoding]::new($false); (Get-CimInstance Win32_Process -Filter 'ProcessId = ${process.pid}').CommandLine`], { encoding: 'utf8' }).stdout
      : spawnSync('ps', ['-ww', '-p', String(process.pid), '-o', 'command='], { encoding: 'utf8' }).stdout;
    entry = commandPath(file, command ?? '', process.argv, process.platform);
  } catch { /* A host without a process-command read keeps Bun's path. */ }
  return entry;
}

/** Find the entry in a platform's process-command representation; arguments cannot redirect it to another file. */
export function commandPath(file, command, argv, platform) {
  let candidates;
  if (platform === 'linux') candidates = command.split('\0');
  else if (platform === 'win32') candidates = [...command.matchAll(/"([^"]*)"|(\S+)/g)].map(m => m[1] ?? m[2]);
  else {
    command = command.replace(/\n$/, '');
    const tail = argv.slice(2).map(arg => ` ${arg}`).join('');
    if (!command.endsWith(tail)) return file;
    command = command.slice(0, command.length - tail.length);
    candidates = [command, ...[...command.matchAll(/ /g)].map(m => command.slice(m.index + 1))];
  }
  return candidates.filter(Boolean).map(namedPath).find(path => sameFile(path, file)) ?? file;
}

/** The checkout's invoked name, or its validated EXACT override, for paths written into apps. */
export function checkoutPath(to) {
  to = resolve(to);
  if (!within(CHECKOUT, to)) return to;
  let root = process.env.EXACT ?? process.env.EXACT2;
  const remembered = process.env.EXACT_CHECKOUT_SPELLING;
  if (!sameFile(root, CHECKOUT) || resolve(root) === CHECKOUT) {
    const main = process.argv[1];
    const invoked = main && within(CHECKOUT, resolve(main))
      ? resolve(invokedPath(main), ...relative(CHECKOUT, resolve(main)).split(sep).map(() => '..')) : CHECKOUT;
    root = sameFile(invoked, CHECKOUT) && invoked !== CHECKOUT ? invoked
      : sameFile(remembered, CHECKOUT) ? remembered : root;
  }
  return sameFile(root, CHECKOUT) ? resolve(namedPath(root), relative(CHECKOUT, to)) : to;
}

function appSpelling(from) {
  const physical = resolvedPath(from), remembered = process.env.EXACT_APP_SPELLING;
  if (remembered && within(resolvedPath(remembered), physical)) return resolve(remembered, relative(resolvedPath(remembered), physical));
  const main = process.argv[1];
  if (main && sameFile(dirname(main), physical)) return dirname(invokedPath(main));
  return namedPath(from);
}

/** Keep validated spellings across Bun's re-exec and child commands. Builds still use fully resolved paths. */
export function preservePaths(app = process.env.EXACT_APP_DIR, checkout) {
  if (app !== undefined) {
    const named = appSpelling(app);
    process.env.EXACT_APP_SPELLING = named;
    const candidate = checkout === undefined ? null : resolve(named, checkout);
    // A restarted launcher receives the physical EXACT alongside the spelling it already captured.
    if (candidate && sameFile(candidate, CHECKOUT) && (candidate !== CHECKOUT || !sameFile(process.env.EXACT_CHECKOUT_SPELLING, CHECKOUT))) {
      process.env.EXACT_CHECKOUT_SPELLING = candidate;
    }
  }
  process.env.EXACT_CHECKOUT_SPELLING = checkoutPath(CHECKOUT);
}

/** Whether `file` is `dir` or under it, both absolute and normalized (as
 * `resolve` returns them). On POSIX a prefix test; on Windows
 * `path.relative`'s, which takes either separator and compares names without
 * case, as NTFS does. */
export function within(dir, file, path = nodePath) {
  if (file === dir) return true;
  if (file.startsWith(dir.endsWith(path.sep) ? dir : dir + path.sep)) return true;
  if (path.sep === '/') return false;
  const child = path.relative(dir, file);
  return child === '' || (!path.isAbsolute(child) && child !== '..' && !child.startsWith('..' + path.sep));
}

/** `path.relative(from, to)` with `/` between its names, the form the name
 * rules (`^assets/`, `app.json`'s paths) are written in. */
export const slashRelative = (from, to, path = nodePath) => path.relative(from, to).split(path.sep).join('/');

/** A Cargo binary's file name: `.exe` on Windows. */
export const EXE = process.platform === 'win32' ? '.exe' : '';

/** Make a path absolute as written, keeping `..` and Windows drive-relative paths. */
export function writtenPath(path) {
  if (isAbsolute(path)) return path;
  const { root } = parse(path);
  const base = resolve(root || '.');
  return base + (base.endsWith(sep) ? '' : sep) + path.slice(root.length);
}

/** Preserve the caller's LLVM directory before a child starts in another directory. */
export function resolveAppleLLVM(env) {
  const bin = env.EXACT_APPLE_LLVM_BIN;
  if (bin && !isAbsolute(bin)) env.EXACT_APPLE_LLVM_BIN = writtenPath(bin);
}

resolveAppleLLVM(process.env);

/** Resolve through the nearest existing ancestor, including an absent main-worktree app. */
function resolvedPath(path) {
  const absolute = resolve(path);
  let parent = absolute;
  while (!existsSync(parent)) {
    if (dirname(parent) === parent) throw new Error(`Cannot resolve ${path}: no existing ancestor (drive or share unavailable at ${parent})`);
    parent = dirname(parent);
  }
  return resolve(realpathSync(parent), relative(parent, absolute));
}

export const samePath = (a, b) => resolvedPath(a) === resolvedPath(b);

/** The first worktree is main; a separate Git directory may not identify its working tree. */
export function worktreePaths(app) {
  const options = { cwd: app, encoding: 'utf8', env: Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('GIT_'))) };
  const git = spawnSync('git', ['rev-parse', '--show-toplevel'], options);
  if (git.status !== 0) return {};
  const top = realpathSync(git.stdout.trim());
  const listed = spawnSync('git', ['worktree', 'list', '--porcelain', '-z'], options);
  const first = listed.status === 0 ? listed.stdout.split('\0')[0] : '';
  const candidate = first.startsWith('worktree ') ? realpathSync(first.slice('worktree '.length)) : top;
  return { top, main: existsSync(resolve(candidate, '.git')) ? candidate : top };
}

function appBoundary(from) {
  let cwd = from, app;
  while (!existsSync(cwd)) cwd = dirname(cwd);
  for (let dir = cwd; ; dir = dirname(dir)) {
    if (!app && existsSync(resolve(dir, 'app.json'))) app = dir;
    if (existsSync(resolve(dir, '.git'))) {
      const { top, main } = worktreePaths(cwd);
      if (top) return { boundary: top, main };
      break;
    }
    if (dirname(dir) === dir) break;
  }
  return { boundary: app ?? from, main: app ?? from };
}

/** Generated workspaces and build consumers use physical paths. */
export function pathWithin(from, to) {
  return slashRelative(resolvedPath(from), resolvedPath(to)) || '.';
}

/** Whether `dir` is part of the checkout (both real): under it, and in its repository rather than one nested below it. */
const inRepository = (checkout, dir) => within(checkout, dir) && appBoundary(dir).boundary === appBoundary(checkout).boundary;

/** Committed checkout references pass through the app's machine-local link, except from the checkout's own repository,
 * where a relative path holds in every worktree and is the one the checkout's own workspace uses (`cargoCheckout`).
 * Cargo paths start at each manifest. */
export function pathFrom(from, to) {
  const base = resolvedPath(from), target = resolvedPath(to);
  for (let app = base; ; app = dirname(app)) {
    const link = resolve(app, '.exact/checkout');
    if (existsSync(link)) {
      const checkout = realpathSync(link);
      if (within(checkout, target) && !inRepository(checkout, base)) return slashRelative(base, resolve(link, relative(checkout, target))) || '.';
    }
    if (existsSync(resolve(app, 'app.json')) || dirname(app) === app) break;
  }
  const named = checkoutPath(to), { boundary } = appBoundary(base);
  const reference = slashRelative(appSpelling(from), named) || '.';
  const result = within(boundary, target) ? slashRelative(base, target) || '.'
    : resolvedPath(resolve(base, reference)) === target ? reference : slashRelative(base, named) || '.';
  if (nodePath.isAbsolute(result)) throw new Error(`${from}: link the checkout from this volume to record a relative seed`);
  return result;
}

/** How one Cargo graph of the app at `app` names the checkout (LLP 1086 D5). Cargo tells path packages apart by the
 * path written, not the directory it reaches, so a crate named both through the app's `.exact/checkout` link and by
 * its real path is two packages of one name, and the graph does not resolve. A workspace exact2 generates (`generated`:
 * an app's `.exact/shells` and `.exact/gpu`, a game's `.shells`) names the real path, so every app of the checkout
 * compiles the same packages and `host/apple/crates.mjs` can give one app's units to the next; its members take
 * exact2's crates from its `[workspace.dependencies]` (`checkoutPathDependencies`). A workspace the app authored is
 * committed, so it names the link (`pathFrom`), and what exact2 adds to its graph does too, unless the app is in the
 * checkout's own repository, whose workspace names it by relative path (expo/exact-build#755). Returns the checkout's
 * directory as that graph names it. */
export function cargoCheckout(app, generated, checkout = CHECKOUT) {
  const link = resolve(app, '.exact/checkout'), real = realpathSync(checkout);
  return !generated && sameFile(link, checkout) && !inRepository(real, resolvedPath(app)) ? link : real;
}

/** A crate manifest's dependency tables, `[keys, table]`, in every form Cargo reads (the underscore spellings too). */
export const dependencyTables = doc => ['dependencies', 'build-dependencies', 'build_dependencies', 'dev-dependencies', 'dev_dependencies']
  .flatMap(kind => [[[kind], doc[kind]], ...Object.entries(doc.target ?? {}).map(([target, value]) => [['target', target, kind], value?.[kind]])])
  .filter(([, table]) => table);

/** The dependencies of the crate whose manifest is `manifest` that reach a directory of the checkout through a link
 * (the app's `.exact/checkout`), each `{ keys, key, dep, dir }` (`keys` the dependency's table, `dir` its real
 * directory). In a workspace exact2 generates, which names the checkout by its real path, each is a second package of
 * its name (`cargoCheckout`); a member writes `<name>.workspace = true` instead. */
export function checkoutPathDependencies(manifest, doc = Bun.TOML.parse(readFileSync(manifest, 'utf8')), checkout = CHECKOUT) {
  const real = realpathSync(checkout), from = realpathSync(dirname(manifest)), found = [];
  for (const [keys, table] of dependencyTables(doc)) {
    for (const [key, dep] of Object.entries(table)) {
      if (typeof dep?.path !== 'string') continue;
      const written = resolve(from, dep.path), dir = resolvedPath(written);
      if (written !== dir && within(real, dir)) found.push({ keys, key, dep, dir });
    }
  }
  return found;
}
/** The keys a dependency that inherits from its workspace may keep beside `workspace = true`. */
export const INHERITING_KEYS = ['features', 'optional', 'default-features', 'default_features', 'public'];

/** The refusal of the members `manifests` of a generated workspace, and of the crates they reach by path outside the
 * checkout, that name exact2's crates through the link (`checkoutPathDependencies`), or null. A renamed dependency is
 * taken under exact2's name; a crate that is not a member cannot inherit at all. */
export function checkoutPathRefusal(manifests, checkout = CHECKOUT) {
  const real = realpathSync(checkout), members = new Set(manifests.map(m => resolvedPath(m))), queue = [...members], seen = new Set(queue), lines = [];
  while (queue.length) {
    const manifest = queue.shift(), doc = Bun.TOML.parse(readFileSync(manifest, 'utf8'));
    for (const [, table] of dependencyTables(doc)) {
      for (const dep of Object.values(table)) {
        if (typeof dep?.path !== 'string') continue;
        const next = resolvedPath(resolve(realpathSync(dirname(manifest)), dep.path, 'Cargo.toml'));
        if (!within(real, next) && !seen.has(next) && existsSync(next)) { seen.add(next); queue.push(next); }
      }
    }
    for (const { keys, key, dep } of checkoutPathDependencies(manifest, doc, checkout)) {
      const name = dep.package ?? key, own = Object.keys(dep).every(k => k === 'path' || INHERITING_KEYS.includes(k));
      const at = `${manifest}: [${keys.join('.')}] ${key} names exact2's ${name} through the app's .exact/checkout link, a second copy of the one the generated workspace names`;
      lines.push(!members.has(manifest) ? `${at}; only a crate beside app.json that joins the workspace can take it from there, so make this crate one`
        : `${at}; take it from that workspace: ${name}.workspace = true` + (own ? ' (`bun exact.mjs update` rewrites it)'
          : `, without ${Object.keys(dep).filter(k => k !== 'path' && !INHERITING_KEYS.includes(k)).concat('path').join(' or ')}${dep.package ? ` (Rust then names it ${name.replaceAll('-', '_')})` : ''}`));
    }
  }
  return lines.length ? lines.join('\n') : null;
}

/** The seed is only a discovery hint: a merge puts the app at its main-worktree location. */
export function checkoutSeed(app, checkout = CHECKOUT) {
  const { boundary, main } = appBoundary(resolvedPath(app));
  const atMain = path => within(boundary, path) ? resolve(main, relative(boundary, path)) : path;
  const named = atMain(checkoutPath(checkout));
  const hint = slashRelative(atMain(resolvedPath(app)), named) || '.';
  // Across Windows volumes only discovery needs a hint; the local junction holds the canonical target.
  return nodePath.isAbsolute(hint) ? '../' + basename(named) : hint;
}

/** Checkout discovery is embedded verbatim in dependency-free launchers. */
export function checkoutLocation(app, seed, override) {
  app = realpathSync(app);
  const link = resolve(app, '.exact/checkout');
  const valid = path => existsSync(resolve(path, 'scripts/app-cli.mjs'));
  const { top, main: mainRoot } = worktreePaths(app);
  const main = mainRoot ? resolve(mainRoot, relative(top, app)) : null;
  let checkout;
  if (override !== undefined) {
    checkout = resolve(app, override);
    if (!valid(checkout)) throw new Error(`no Exact checkout at ${checkout} (no scripts/app-cli.mjs): set EXACT= to the checkout`);
  } else if (valid(link)) checkout = link;
  else {
    checkout = (seed === undefined ? [] : [resolve(app, seed), ...(main ? [resolve(main, seed)] : [])]).find(valid);
    if (!checkout) {
      const candidates = new Map();
      const parents = new Set([dirname(app), dirname(top ?? app), ...(main ? [dirname(main), dirname(mainRoot)] : [])]);
      for (const parent of parents) {
        if (!existsSync(parent)) continue;
        for (const name of readdirSync(parent)) {
          const path = resolve(parent, name);
          if (valid(path)) candidates.set(realpathSync(path), path);
        }
      }
      if (candidates.size !== 1) throw new Error(`no unambiguous Exact checkout for ${app}: set EXACT= to the checkout${candidates.size ? ' (' + [...candidates.values()].join(', ') + ')' : ''}`);
      checkout = [...candidates.values()][0];
    }
  }
  return checkout;
}

/** Readers use the launcher's discovery order without changing the app's local link. */
export function checkoutOf(appDir, env = process.env) {
  const override = env.EXACT ?? env.EXACT2;
  let seed;
  if (override === undefined && !existsSync(resolve(appDir, '.exact/checkout/scripts/app-cli.mjs'))) {
    const file = resolve(appDir, 'exact.mjs');
    const launcher = existsSync(file) ? readFileSync(file, 'utf8') : '';
    const literal = /^const seed = ("(?:\\.|[^"\\])*");$/m.exec(launcher)?.[1]
      ?? /process\.env\.EXACT2?\s*\?\?\s*(?:process\.env\.EXACT2\s*\?\?\s*)?("(?:\\.|[^"\\])*")/.exec(launcher)?.[1];
    if (literal) seed = JSON.parse(literal);
  }
  return realpathSync(checkoutLocation(appDir, seed, override));
}

/** Bootstrap the app-local link before importing any checkout code. */
export function linkCheckout(app, seed, override = process.env.EXACT ?? process.env.EXACT2) {
  app = realpathSync(app);
  const link = resolve(app, '.exact/checkout');
  const checkout = checkoutLocation(app, seed, override);
  const physical = realpathSync(checkout);
  let previous;
  try { previous = realpathSync(link); } catch { /* A fresh working tree or a broken link. */ }
  if (previous && !lstatSync(link).isSymbolicLink()) throw new Error(`${link}: expected a checkout symlink; move this directory before bootstrapping`);
  if (previous !== physical || readlinkSync(link) !== physical) {
    try {
      if (!lstatSync(link).isSymbolicLink()) throw new Error(`${link}: expected a checkout symlink; move this directory before bootstrapping`);
      rmSync(link);
    } catch (error) { if (error.code !== 'ENOENT') throw error; }
    mkdirSync(dirname(link), { recursive: true });
    symlinkSync(physical, link, process.platform === 'win32' ? 'junction' : 'dir');
  }
  const ignore = resolve(app, '.gitignore');
  const text = existsSync(ignore) ? readFileSync(ignore, 'utf8') : '';
  const ignored = spawnSync('git', ['check-ignore', '--quiet', '--no-index', '--', '.exact/checkout'], {
    cwd: app, env: Object.fromEntries(Object.entries(process.env).filter(([key]) => !key.startsWith('GIT_'))),
  });
  const covered = ignored.status === 0 || ignored.status === 128 && /^\/?\.exact(?:\/\*?)?$/m.test(text);
  if (!covered) {
    const rule = /^!?\/?\.exact(?:\/|$)/m.test(text) ? '/.exact/checkout' : '/.exact/';
    writeFileSync(ignore, text + (text && !text.endsWith('\n') ? '\n' : '') + rule + '\n');
  }
  let spelling = checkout === link ? physical : checkout;
  // A Bun restart carries the physical EXACT alongside the spelling captured before the restart.
  const remembered = process.env.EXACT_CHECKOUT_SPELLING;
  if (spelling === physical && remembered) {
    try { if (realpathSync(remembered) === physical) spelling = remembered; } catch {}
  }
  // Installed packages can outlive a missing or broken link.
  return { physical, spelling, retargeted: previous !== physical };
}

/** A Chrome or Chromium installed on this machine outside Playwright's cache, or undefined: named beside a missing
 * pinned browser as a value for CHROME (LLP 1156.006.000 D9), never launched unasked, so drives stay on the pin. */
export function systemChrome({ environment = process.env, platform = process.platform, executable = (path) => { try { accessSync(path, constants.X_OK); return true; } catch { return false; } } } = {}) {
  const apps = platform === 'darwin' ? ['Google Chrome', 'Chromium'].map((name) => `/Applications/${name}.app/Contents/MacOS/${name}`) : [];
  const onPath = platform === 'win32' ? [] : ['google-chrome', 'google-chrome-stable', 'chromium', 'chromium-browser']
    .flatMap((name) => (environment.PATH ?? '').split(delimiter).filter(Boolean).map((dir) => resolve(dir, name)));
  return [...apps, ...onPath].find(executable);
}
/** How "…, or set CHROME" ends: with the browser `systemChrome` found, or as advice to name one. */
export const chromeHere = (here) => (here ? `='${here}' to use the Chrome installed here (not the pinned revision)` : ' to a browser');
