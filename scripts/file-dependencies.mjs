// An app's `file:` dependencies, as its builds read them (#115). Bun installs a `file:` package inside the app's
// directory as a copy of the package's directory, and one outside it as a directory of links to each of its files.
// Either is the package as it was at the last `bun install`: an edit to a copy, a file added to either and a file
// removed from either are not what `node_modules` holds until the next one. Every reader of a package (the bake's
// resolver, Rolldown, the Swift, Kotlin and Rust builds) reads `node_modules`, so before a build reads it, this makes
// it what the declared directories hold now, by Bun's own install: the one that decides what an install is.
import { spawnSync } from 'node:child_process';
import { existsSync, lstatSync, readdirSync, readFileSync, realpathSync, rmSync, statSync, utimesSync } from 'node:fs';
import { dirname, join, resolve, sep } from 'node:path';
import { installLock, withInstallLock } from './filesystem.mjs';

// Not a package's sources: its own installs, and its repository. Bun copies both; no build reads either from it.
const NOT_SOURCE = new Set(['node_modules', '.git']);
const KINDS = ['dependencies', 'devDependencies', 'optionalDependencies'];

/** The `file:` dependencies of the `package.json` nearest above `appDir` that are installed: each one's name, the
 * declared directory (`source`), where it is installed (`installed`: the nearest `node_modules/<name>` above
 * `appDir`, as the resolver and `directDependencies` find it, so a workspace's hoisted install counts), the directory
 * whose `node_modules` holds it (`modules`), and the directory Bun installs from (`root`, the `package.json`'s). */
export function fileDependencies(appDir) {
  let root = null;
  for (let dir = resolve(appDir); ; dir = dirname(dir)) {
    if (existsSync(join(dir, 'package.json'))) { root = dir; break; }
    if (dirname(dir) === dir) return [];
  }
  let manifest;
  try { manifest = JSON.parse(readFileSync(join(root, 'package.json'), 'utf8')); } catch { return []; }
  const located = (name) => {
    for (let dir = resolve(appDir); ; dir = dirname(dir)) {
      if (existsSync(join(dir, 'node_modules', name, 'package.json'))) return dir;
      if (dirname(dir) === dir) return null;
    }
  };
  const out = [];
  for (const kind of KINDS) for (const [name, spec] of Object.entries(manifest?.[kind] ?? {})) {
    if (typeof spec !== 'string' || !spec.startsWith('file:')) continue;
    // One not installed is the resolver's to report, with the command that installs it.
    const modules = located(name);
    if (!modules) continue;
    out.push({ name, spec, source: resolve(root, spec.slice('file:'.length)), installed: join(modules, 'node_modules', name), modules, root });
  }
  return out;
}

const isDirectory = (path) => { try { return statSync(path).isDirectory(); } catch { return false; } };

/** Every file under `dir` but the package's own installs and repository, as paths relative to it. A link is followed;
 * one that leads nowhere is not a file of a source, and is one of an install (`dangling`), where it is the link to a
 * file removed since. */
function files(dir, dangling = false, at = '') {
  const out = [];
  let entries;
  try { entries = readdirSync(join(dir, at)); } catch { return out; }
  for (const name of entries) {
    if (!at && NOT_SOURCE.has(name)) continue;
    const path = at ? join(at, name) : name;
    let stat;
    try { stat = statSync(join(dir, path)); } catch { if (dangling) out.push(path); continue; }
    if (stat.isDirectory()) out.push(...files(dir, dangling, path));
    else if (stat.isFile()) out.push(path);
  }
  return out;
}

/** Each file whose install differs from its declared directory's, as `[path, what]` with `/` in the path: one not
 * installed, one that differs, one installed but gone from the source. A link that leads to the source file is that
 * file; a copy with the source's size and modification time (Bun's copy keeps both) is taken as the same; any other
 * is compared byte for byte. */
export function installDifferences({ source, installed }) {
  const want = files(source), have = new Set(files(installed, true)), out = [];
  for (const path of want) {
    if (!have.has(path)) { out.push([path, 'is not installed']); continue; }
    have.delete(path);
    const from = join(source, path), to = join(installed, path);
    try {
      if (lstatSync(to).isSymbolicLink() && realpathSync(to) === realpathSync(from)) continue;
      const a = statSync(from), b = statSync(to);
      if (a.size === b.size && (a.mtimeMs === b.mtimeMs || readFileSync(from).equals(readFileSync(to)))) continue;
      out.push([path, 'differs']);
    } catch { out.push([path, 'cannot be read']); }
  }
  for (const path of have) out.push([path, 'is installed but no longer in the source']);
  return out.map(([path, what]) => [path.split(sep).join('/'), what]);
}

/** The first of `installDifferences`, as a sentence, or null when there is none. */
export const installDifference = (dependency) => installDifferences(dependency).map(([path, what]) => `${path} ${what}`)[0] ?? null;

/** Makes each installed `file:` dependency of the app at `appDir` what its declared directory holds, with
 * `bun install --frozen-lockfile` where it is installed (or where the app's `package.json` is) when one differs,
 * and refuses, naming the file and the remedy, when that install fails or a declared directory is gone. Each copied
 * file the install rewrote is then dated now: Bun's copy keeps the source's time, which can be older than a build that read the old copy, and Cargo and the other builds that
 * judge a file by its time would keep that build's output. A package that still differs after the install is what
 * Bun installs, which no install by hand would change, so it is said and the build goes on. One process installs at
 * a time (`withInstallLock`); `install` is the install a test stands in for. */
export function installedFileDependencies(appDir, { install = (root) => spawnSync(process.execPath, ['install', '--frozen-lockfile'], { cwd: root, stdio: ['ignore', 2, 2] }).status === 0 } = {}) {
  const dependencies = fileDependencies(appDir);
  // A declared directory that is gone (removed, renamed) leaves its last install in node_modules, which every reader
  // would go on reading: no install mends that, so the build stops.
  const gone = dependencies.filter(({ source }) => !isDirectory(source));
  if (gone.length) throw new Error(gone.map(({ name, spec, source, installed, root }) => `${name} is ${spec} in ${join(root, 'package.json')}, and ${source} is not a directory, so the build would read the copy at ${installed} as of its last install: restore the directory, or point ${name} at its new place (or remove it) and run bun install`).join('\n'));
  const said = (list) => list.map(({ dependency, found: [[path, what], ...more] }) => `${dependency.name} (${dependency.spec}): ${path} ${what}${more.length ? ` and ${more.length} more` : ''}`).join('; ');
  // Installed again where the install is, when a package.json there declares it (an ancestor's install, or the app's
  // own; Bun finds a workspace from a member), else where the app's package.json is.
  const where = ({ modules, root }) => existsSync(join(modules, 'package.json')) ? modules : root;
  for (const dir of new Set(dependencies.map(where))) {
    const group = dependencies.filter((dependency) => where(dependency) === dir);
    const stale = () => group.flatMap((dependency) => { const found = installDifferences(dependency); return found.length ? [{ dependency, found }] : []; });
    if (!stale().length) continue;
    withInstallLock(installLock(dir), () => {
      const before = stale();
      if (!before.length) return;
      console.error(`${dir}: node_modules holds ${before.length === 1 ? 'a file: package' : 'file: packages'} as of the last bun install (${said(before)}); bun install --frozen-lockfile there`);
      // Each stale install is removed first, so Bun makes it again whole: over one already there it adds and rewrites
      // files but leaves a removed one (Bun 1.4.2's workspace store). Only what lies inside a node_modules and is not
      // the declared directory itself.
      for (const { dependency: { installed, source } } of before) {
        let real;
        try { real = realpathSync(installed); } catch { continue; }
        const own = realpathSync(source);
        if (real.split(sep).includes('node_modules') && real !== own && !real.startsWith(own + sep)) rmSync(real, { recursive: true, force: true });
      }
      if (!install(dir)) throw new Error(`${dir}: bun install --frozen-lockfile failed (above), so the build would read a file: package as of the last install; run bun install there, then build again`);
      const now = new Date();
      for (const { dependency, found } of before) for (const [path] of found) {
        const to = join(dependency.installed, path);
        try { if (lstatSync(to).isFile()) utimesSync(to, now, now); } catch { /* removed, as its source was */ }
      }
      const after = stale();
      if (after.length) console.error(`${dir}: after bun install, node_modules still differs from the file: packages package.json names, and the build reads what is installed (${said(after)})`);
    });
  }
}
