// Filesystem path comparisons that hold on Windows. A test against `dir + '/'`
// never matches a path `resolve` made there, whose separator is `\`; these take
// the platform's path module, a parameter so a test can pass `path.win32`.
import nodePath, { dirname, relative, resolve, sep } from 'node:path';
import { existsSync, realpathSync } from 'node:fs';

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

/** Resolve symlinks through the nearest existing ancestor, including an absent main-worktree app. */
function resolvedPaths(from, to) {
  let parent = resolve(from);
  while (!existsSync(parent)) {
    if (dirname(parent) === parent) throw new Error(`Cannot resolve ${from}: no existing ancestor (drive or share unavailable at ${parent})`);
    parent = dirname(parent);
  }
  return [resolve(realpathSync(parent), relative(parent, resolve(from))), existsSync(to) ? realpathSync(to) : resolve(to)];
}

/** Committed app references use relative paths with `/`. Generated workspaces use the same form
 * so a deployment capture keeps their dependencies inside the capture. */
export function pathWithin(from, to) {
  return (relative(...resolvedPaths(from, to)) || '.').split(sep).join('/');
}

/** Authored Cargo paths stay relative below a shared directory; otherwise absolute. */
export function pathFrom(from, to) {
  const [a, b] = resolvedPaths(from, to);
  const shared = a.split('/')[1] === b.split('/')[1] && a.split('/')[1] !== '';
  return shared ? (relative(a, b) || '.').split(sep).join('/') : b;
}
