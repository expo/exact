// Which package manager installs an app's dependencies, for `exact add`.
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';

const LOCKS = [['bun.lock', 'bun'], ['bun.lockb', 'bun'], ['pnpm-lock.yaml', 'pnpm'], ['yarn.lock', 'yarn'], ['package-lock.json', 'npm']];

/** The package manager that installs `dir`'s dependencies: the `packageManager` of the nearest package.json that names
 *  one, looking up to the repository's root; else the one whose lockfile is nearest; else Bun. A declaration wins over a
 *  nearer lockfile, which may be left over (a bun.lock inside a pnpm workspace). A pnpm or Yarn workspace resolves its
 *  members from its own lockfile, which `bun install` cannot read. */
export function packageManager(dir) {
  let lock;
  for (let at = resolve(dir); ; at = dirname(at)) {
    let named;
    try { named = JSON.parse(readFileSync(resolve(at, 'package.json'), 'utf8')).packageManager; } catch {}
    if (typeof named === 'string' && /^(bun|pnpm|yarn|npm)@/.test(named)) return named.split('@')[0];
    lock ??= LOCKS.find(([file]) => existsSync(resolve(at, file)))?.[1];
    if (existsSync(resolve(at, '.git')) || dirname(at) === at) return lock ?? 'bun';
  }
}

/** Installs `dir`'s dependencies with its package manager, unless `install` is false; throws `why` and what failed. */
export function installIn(dir, why, install = true) {
  const manager = packageManager(dir);
  if (!install) return { manager, installed: false };
  const ran = spawnSync(manager === 'bun' ? process.execPath : manager, ['install'], { cwd: dir, stdio: 'inherit' });
  if (ran.status !== 0) throw new Error(`${why}, but ${manager} install ${ran.error ? `could not start (${ran.error.code})` : 'failed'} in ${dir}`);
  return { manager, installed: true };
}
