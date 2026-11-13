// `exact install` and `exact uninstall` (scripts/exact.mjs): a built macOS
// bundle copied where the OS looks for apps, and a shim named by the app's
// command where a shell looks for names. Each replaces or removes only what
// is this app's: a bundle with its bundle identifier, or the shim `install`
// writes. Anything else at either path is refused before either changes.
import { spawnSync } from 'node:child_process';
import { accessSync, chmodSync, constants, lstatSync, mkdirSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { homedir } from 'node:os';
import { basename, dirname, resolve } from 'node:path';
import { appleExecutableName } from './app.mjs';

const APPLICATIONS = resolve(homedir(), 'Applications');
/** The name this app answers to on the command line (`app.command`, else its directory's name). */
export const commandOf = (app) => app.manifest.app?.command ?? app.name;
/** Where `install` puts the app. */
export const installedAt = (app) => resolve(APPLICATIONS, `${appleExecutableName(app)}.app`);

/** Where a shim goes: `EXACT_BIN_DIR`, else the first of these already on PATH, else `~/.local/bin` (made, and named in the advice). */
export function binDirectory() {
  if (process.env.EXACT_BIN_DIR) return resolve(process.env.EXACT_BIN_DIR);
  const path = (process.env.PATH ?? '').split(':').map((p) => p && resolve(p));
  const candidates = [resolve(homedir(), '.local/bin'), '/usr/local/bin', resolve(homedir(), 'bin')];
  return candidates.find((dir) => path.includes(dir) && writable(dir)) ?? candidates[0];
}

const writable = (dir) => { try { accessSync(dir, constants.W_OK); return true; } catch { return false; } };
const onPath = (dir) => (process.env.PATH ?? '').split(':').some((p) => p && resolve(p) === dir);
const statOf = (path) => { try { return lstatSync(path); } catch { return null; } };

/** The line that marks a shim as this app's: every shim `install` has written carries it. */
const shimMark = (app) => `written by \`exact install ${app.name}\``;
// `open` and not the executable: it hands the files to a copy that is
// already running instead of starting a second one, and it returns to the
// shell instead of holding it. `--` ends `open`'s options, so a file whose
// name starts with `-` is still a file to open.
/** The bundle path is single-quoted: inside double quotes `sh` still expands `$` and backticks, which a bundle or home directory name may hold. */
export const shimText = (app, target) => `#!/bin/sh
# ${app.displayName} — ${shimMark(app)}. Delete it or
# run \`exact uninstall ${app.name}\` to take it away.
exec /usr/bin/open -a '${target.replaceAll("'", "'\\''")}' -- "$@"
`;

/** A bundle's `CFBundleIdentifier`, or null. */
function bundleId(bundle) {
  const r = spawnSync('/usr/bin/plutil', ['-extract', 'CFBundleIdentifier', 'raw', '-o', '-', resolve(bundle, 'Contents/Info.plist')], { encoding: 'utf8' });
  return r.status === 0 ? r.stdout.trim() : null;
}

/** Why `target` is not this app's bundle to replace or remove, or null: nothing is there, or a bundle (not a link) with its identifier. */
function bundleRefusal(app, target) {
  const stat = statOf(target);
  if (!stat) return null;
  if (!stat.isDirectory()) return `${target} is not an app bundle${stat.isSymbolicLink() ? ' (a symbolic link)' : ''}`;
  const id = bundleId(target);
  return id === app.id ? null : `${target} is another app (${id ?? 'no bundle identifier'}), not ${app.id}`;
}

/** Why `shim` is not this app's shim to replace or remove, or null: nothing is there, or a file (not a link) that `install` wrote. */
function shimRefusal(app, shim) {
  const stat = statOf(shim);
  if (!stat) return null;
  if (stat.isFile() && readFileSync(shim, 'utf8').includes(shimMark(app))) return null;
  return `${shim} is ${stat.isSymbolicLink() ? 'a symbolic link' : 'not'} the command \`exact install ${app.name}\` writes`;
}

/** A name beside `path` for a copy this process makes and alone uses. */
const besidePath = (path, what) => resolve(dirname(path), `.${basename(path)}.${what}-${process.pid}`);

/** Put the bundle at `target` and the shim at `shim`, each replacing only this app's, and a failure leaving what was there. */
export function place(app, bundle, target, shim) {
  const refused = [bundleRefusal(app, target), shimRefusal(app, shim)].filter(Boolean);
  if (refused.length) {
    throw new Error(`nothing was installed:\n  ${refused.join('\n  ')}\n  Move it aside, or name the command with \`app.command\` in app.json.`);
  }
  mkdirSync(dirname(target), { recursive: true });
  // A copy, not a symlink: Launch Services registers what it finds at the
  // path, and a symlinked bundle registers the repo's copy — which moves,
  // and which a `cargo clean` deletes underneath the Dock. It is copied
  // beside the old one and checked before it takes the old one's place.
  const staged = besidePath(target, 'installing'), old = besidePath(target, 'replaced');
  try {
    const copy = spawnSync('/bin/cp', ['-R', bundle, staged], { stdio: 'inherit' });
    if (copy.status !== 0) throw new Error(`copying ${bundle} failed (exit ${copy.status ?? 'signal'})`);
    if (bundleId(staged) !== app.id) throw new Error(`the copy of ${bundle} is not ${app.id}`);
    if (statOf(target)) renameSync(target, old);
    try { renameSync(staged, target); } catch (e) { if (statOf(old)) renameSync(old, target); throw e; }
  } finally {
    rmSync(staged, { recursive: true, force: true });
  }
  rmSync(old, { recursive: true, force: true });

  // Written beside the shim and renamed over it, which replaces the name
  // itself and never follows what it pointed to.
  mkdirSync(dirname(shim), { recursive: true });
  const written = besidePath(shim, 'installing');
  try {
    writeFileSync(written, shimText(app, target), { flag: 'wx' });
    chmodSync(written, 0o755);
    renameSync(written, shim);
  } finally {
    rmSync(written, { force: true });
  }
}

/** Remove this app's bundle at `target` and shim at `shim`, and nothing else: what it did, by path. */
export function remove(app, target, shim) {
  return [[target, bundleRefusal(app, target)], [shim, shimRefusal(app, shim)]].map(([path, refused]) => {
    if (!statOf(path)) return `not installed: ${path}`;
    if (refused) return `left ${refused}`;
    rmSync(path, { recursive: true, force: true });
    return `removed ${path}`;
  });
}

/** `exact install` — the app built at `bundle` where the OS looks for apps, and its name where a shell looks for names. */
export function install(app, bundle) {
  const target = installedAt(app), command = commandOf(app), dir = binDirectory(), shim = resolve(dir, command);
  place(app, bundle, target, shim);
  // Register it now rather than whenever the OS next rescans, so `open -a`
  // and Open With work in the same second this returns.
  spawnSync('/System/Library/Frameworks/CoreServices.framework/Frameworks/LaunchServices.framework/Support/lsregister', ['-f', target], { stdio: 'ignore' });

  console.log(`${app.displayName}: ${target}`);
  console.log(`${command}: ${shim}`);
  if (!onPath(dir)) console.log(`\n  ${dir} is not on your PATH. Add it:\n    echo 'export PATH="${dir}:$PATH"' >> ~/.zshrc && exec zsh`);
  else console.log(`\n  ${command} <file> opens it — in the copy already running, if there is one.`);
}

/** `exact uninstall` — both halves, and nothing that is not this app's. */
export function uninstall(app) {
  const lines = remove(app, installedAt(app), resolve(binDirectory(), commandOf(app)));
  for (const line of lines) console.log(line);
  if (lines.some((line) => line.startsWith('left '))) process.exitCode = 1;
}
