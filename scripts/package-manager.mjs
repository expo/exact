// Which package manager installs an app's dependencies, for `exact add`.
import { spawnSync } from 'node:child_process';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { pathFrom } from './paths.mjs';

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

/** Whether `exact add` takes `spec` as a package outside exact2 (LLP 1153.003 D5.2): a directory (`./map`, `../map`,
 *  an absolute path or `file:…`), or a registry name with its scope or version (`@birkskyum/maplibre-exact`,
 *  `maplibre-exact@1`). A bare name is one of exact2's modules, or a typo of one. */
const DIRECTORY = /^(file:|\.{1,2}[\\/]|[\\/]|[A-Za-z]:[\\/])/;
export const outsideSpecifier = spec => typeof spec === 'string'
  && (DIRECTORY.test(spec) || /^(@[a-z0-9][\w.-]*\/[a-z0-9][\w.-]*(@\S+)?|[a-z0-9][\w.-]*@\S+)$/.test(spec));

/** Adds the outside package `spec` to the app in `appDir`: a directory as `file:` with a path from the app (resolved
 *  from the working directory), as a standard module is; a registry name as its package manager adds it. Returns what
 *  `exact add` says: the dependency, whether it is installed, and the GPU surfaces its exact.json declares. */
export function addOutside(appDir, spec, install = true) {
  const at = resolve(appDir, 'package.json'), read = () => JSON.parse(readFileSync(at, 'utf8'));
  const manager = packageManager(appDir);
  let name, installed = true;
  if (DIRECTORY.test(spec)) {
    const dir = resolve(spec.replace(/^file:/, ''));
    name = JSON.parse(readFileSync(resolve(dir, 'package.json'), 'utf8')).name;
    if (!name) throw new Error(`add: ${resolve(dir, 'package.json')} has no name`);
    const manifest = read(), specifier = `file:${pathFrom(appDir, dir)}`;
    if (manifest.dependencies?.[name] !== specifier) {
      writeFileSync(at, JSON.stringify({ ...manifest, dependencies: { ...manifest.dependencies, [name]: specifier } }, null, 2) + '\n');
    }
    installed = installIn(appDir, `add: ${name} is in its package.json`, install).installed;
  } else {
    if (!install) throw new Error(`add: ${spec} comes from the registry, which ${manager} resolves as it installs; run it without --no-install`);
    const args = [manager === 'npm' ? 'install' : 'add', spec];
    const ran = spawnSync(manager === 'bun' ? process.execPath : manager, args, { cwd: appDir, stdio: 'inherit' });
    if (ran.status !== 0) throw new Error(`add: ${manager} add ${spec} failed in ${appDir}`);
    name = spec.replace(/(?<=.)@[^/]*$/, '');
  }
  let surfaces = [];
  try {
    surfaces = Object.keys(JSON.parse(readFileSync(resolve(appDir, 'node_modules', name, 'exact.json'), 'utf8')).gpu?.surfaces ?? {});
  } catch {}
  const placed = surfaces.map(s => `canvas surface=${s}(…)`).join(', ');
  return `add: ${name} is ${read().dependencies?.[name]}${installed ? '' : `, not installed: run ${manager} install`}. `
    + `${placed ? `Place its surfaces with ${placed}; n` : 'N'}ame what else you use: use … from "${name}" in app.contract, or import from it in app.ts.`;
}
