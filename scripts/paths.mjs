// Filesystem path comparisons that hold on Windows. A test against `dir + '/'`
// never matches a path `resolve` made there, whose separator is `\`; these take
// the platform's path module, a parameter so a test can pass `path.win32`.
import nodePath from 'node:path';

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
