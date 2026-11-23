import { expect, test } from 'bun:test';
import { cpSync, existsSync, mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, statSync, symlinkSync, writeFileSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { patchLines, WEB_TOOLCHAIN } from './app.mjs';
import { pinned } from './test-workspace.mjs';
import { need, notApplicable } from './coverage.mjs';

const moduleRoot = resolve(import.meta.dir, '..');
const moduleFixtures = resolve(moduleRoot, 'target');
// Keep Cargo's source configuration and confine lockfile writes to each fixture.
const lockfileEnv = 'CARGO_RESOLVER_LOCKFILE_PATH';
const moduleCargoEnv = Object.fromEntries(Object.entries(process.env).filter(([key]) => key.toUpperCase() !== lockfileEnv));
// The fixtures run Cargo from other directories, so a relative home must name the caller's.
if (moduleCargoEnv.CARGO_HOME) moduleCargoEnv.CARGO_HOME = resolve(moduleCargoEnv.CARGO_HOME);
const lockConfig = spawnSync('cargo', [`+${WEB_TOOLCHAIN}`, '-Z', 'unstable-options', 'config', 'get', 'resolver.lockfile-path', '--color=never'],
  { cwd: existsSync(moduleFixtures) ? moduleFixtures : moduleRoot, env: { ...moduleCargoEnv, RUSTUP_AUTO_INSTALL: '0' }, encoding: 'utf8' });
const missing = lockConfig.error?.code === 'ENOENT' || /^error: toolchain '[^'\r\n]+' is not installed\b/m.test(lockConfig.stderr)
  ? `install Cargo and ${WEB_TOOLCHAIN}` : null;
const defaultLock = lockConfig.stderr?.trimEnd().endsWith('error: config value `resolver.lockfile-path` is not set');
if (!missing && lockConfig.status !== 0 && !defaultLock) {
  throw new Error(`Cargo lockfile configuration: ${lockConfig.stderr || lockConfig.error || lockConfig.signal || lockConfig.status}`);
}
const cargoTest = need('web Rust toolchain', missing);
const defaultLockTest = missing ? cargoTest : notApplicable(defaultLock ? null : 'Cargo config selects a lockfile outside the wrapper');

function moduleFixture(kind) {
  mkdirSync(moduleFixtures, { recursive: true });
  return mkdtempSync(resolve(moduleFixtures, `module-${kind}-`));
}

function moduleEnv(dir, { target = resolve(dir, 'target'), lock = defaultLock ? null : resolve(dir, 'Cargo.lock') } = {}) {
  const env = { ...moduleCargoEnv, CARGO_TARGET_DIR: target, CARGO_BUILD_BUILD_DIR: resolve(dir, 'build') };
  if (lock) {
    cpSync(resolve(moduleRoot, 'Cargo.lock'), lock);
    env[lockfileEnv] = lock;
  }
  return env;
}

for (const configured of [false, true]) {
  const stageTest = configured ? cargoTest : defaultLockTest;
  const lockfile = configured ? 'a configured lockfile' : 'the default lockfile';
  stageTest(`a fresh web dev stage reuses the authored Rust module across unchanged builds and heading edits with ${lockfile}`, async () => {
    const { dirKey, webCompiler } = await import('../host/web-js/module.mjs');
    const appDir = resolve(moduleRoot, 'apps/sparkline');
    const compiler = webCompiler();
    if (compiler.cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
    const name = `exact-js-module-sparkline-${dirKey(appDir)}-dev`;
    const dir = moduleFixture('sparkline');
    const out = resolve(dir, 'dist'), contract = resolve(dir, 'app.contract'), target = resolve(dir, 'target');
    const raw = resolve(target, 'web-js-modules/target/wasm32-unknown-unknown/web-dev', `${name.replaceAll('-', '_')}.wasm`);
    const stamps = [raw];
    if (!configured) stamps.push(resolve(target, 'web-js-modules', 'wrappers', name, 'Cargo.lock'));
    const stamp = () => stamps.map(path => statSync(path, { bigint: true }).mtimeNs);
    try {
      if (configured) {
        mkdirSync(resolve(dir, '.cargo'));
        cpSync(resolve(moduleRoot, 'Cargo.lock'), resolve(dir, 'Cargo.lock'));
        writeFileSync(resolve(dir, '.cargo', 'config.toml'), '[resolver]\nlockfile-path = "Cargo.lock"\n');
      }
      const env = { ...moduleEnv(dir, { lock: null }), EXACT_APP_DIR: appDir, EXACT_WEB_COMPILER: compiler.cmd };
      const build = () => {
        const built = spawnSync(process.execPath, [resolve(moduleRoot, 'host/web-js/build.mjs'), 'sparkline', '--contract', contract, '--out', out, '--render', 'none', '--dev'],
          { cwd: dir, env, encoding: 'utf8', timeout: 240_000 });
        expect(built.status, built.stdout + built.stderr).toBe(0);
        return readFileSync(resolve(out, 'rust/wasm/app.module.wasm'));
      };
      const source = readFileSync(resolve(appDir, 'app.contract'), 'utf8');
      writeFileSync(contract, source);
      const wasm = build(), before = stamp();
      expect(build()).toEqual(wasm);
      expect(stamp(), 'an unchanged stage rebuild must preserve the Cargo timestamps').toEqual(before);
      writeFileSync(contract, source.replace('text "Markets"', 'text "Prices"'));
      expect(build()).toEqual(wasm);
      const { files } = JSON.parse(readFileSync(resolve(out, 'routes.json'), 'utf8'));
      expect(readFileSync(resolve(out, files['app.js']), 'utf8')).toContain('Prices');
      expect(stamp(), 'a heading edit must preserve the Cargo timestamps').toEqual(before);
    } finally { rmSync(dir, { recursive: true, force: true }); }
  }, 300_000);
}

function moduleResolver(dir, dependency, packageName) {
  const env = moduleEnv(dir);
  return (dev, overrides = {}) => {
    const crate = resolve(dir, dev ? 'metadata-dev' : 'metadata-fresh');
    if (!dev) rmSync(crate, { recursive: true, force: true });
    mkdirSync(resolve(crate, 'src'), { recursive: true });
    const manifest = resolve(crate, 'Cargo.toml');
    if (!existsSync(manifest)) {
      writeFileSync(manifest, `[package]\nname = "module-fixture"\nversion = "0.1.0"\nedition = "2021"\n[workspace]\n[dependencies]\n${dependency}\n`);
      writeFileSync(resolve(crate, 'src/lib.rs'), '');
      if (!dev) cpSync(resolve(import.meta.dir, '../Cargo.lock'), resolve(crate, 'Cargo.lock'));
    }
    const result = spawnSync(process.execPath, ['--eval', `
      import { moduleMetadata } from ${JSON.stringify(import.meta.resolve('../host/web-js/module.mjs'))};
      const { metadata } = moduleMetadata(${JSON.stringify(manifest)}, process.env, ${dev});
      console.log(JSON.stringify(metadata.packages.find(pkg => pkg.name === ${JSON.stringify(packageName)})?.version));
    `], { cwd: dir, env: { ...env, ...overrides }, encoding: 'utf8', timeout: 30_000 });
    expect(result.status, result.stdout + result.stderr).toBe(0);
    return JSON.parse(result.stdout);
  };
}

for (const input of ['manifest', 'Cargo config']) defaultLockTest(`a Rust dev module resolves ${input} edits with the same pins as a fresh wrapper`, () => {
  const root = resolve(import.meta.dir, '..');
  const dir = moduleFixture('pins');
  const fromConfig = input === 'Cargo config';
  const pins = Bun.TOML.parse(readFileSync(resolve(root, 'Cargo.lock'), 'utf8')).package;
  const versions = pins.filter(pkg => pkg.name === 'base64').map(pkg => pkg.version);
  const dependency = fromConfig ? '"0.1.0"' : `{ path = ${JSON.stringify(resolve(dir, 'helper'))} }`;
  const resolvePins = moduleResolver(dir, `pin-helper = ${dependency}`, 'base64');
  const helper = (base64, version = '0.1.0') => `[package]
name = "pin-helper"
version = "${version}"
edition = "2021"
[dependencies]
base64 = "${base64}"
`;
  try {
    const helperManifest = name => resolve(dir, name, 'Cargo.toml');
    const addHelper = (name, spec) => {
      mkdirSync(resolve(dir, name, 'src'), { recursive: true });
      writeFileSync(resolve(dir, name, 'src/lib.rs'), '');
      writeFileSync(helperManifest(name), spec);
    };
    const patch = name => writeFileSync(resolve(dir, '.cargo/patches.toml'), `[patch.crates-io]
pin-helper = { path = "${name}" }
candidate = { package = "pin-helper", path = "candidate" }
`);
    addHelper('helper', helper('*'));
    if (fromConfig) {
      addHelper('candidate', helper('*', '0.0.1'));
      mkdirSync(resolve(dir, '.cargo'));
      writeFileSync(resolve(dir, '.cargo/config.toml'), 'include = ["patches.toml"]\n');
      patch('helper');
    }
    const pinned = resolvePins(false);
    expect(versions).toContain(pinned);
    const other = versions.find(version => version !== pinned);
    if (fromConfig) {
      addHelper('strict-helper', helper(`=${other}`));
      patch('strict-helper');
    } else writeFileSync(helperManifest('helper'), helper(`=${other}`));
    expect(resolvePins(true)).toBe(other);
    if (fromConfig) patch('helper');
    else writeFileSync(helperManifest('helper'), helper('*'));
    expect(resolvePins(true)).toBe(pinned);
    if (fromConfig) {
      patch('strict-helper');
      expect(resolvePins(true)).toBe(other);
      writeFileSync(helperManifest('candidate'), helper('*', '0.1.1'));
      expect(resolvePins(true)).toBe(pinned);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 30_000);

function rustModuleFixture(dir, dependency) {
  const root = resolve(import.meta.dir, '..');
  const env = moduleEnv(dir, { lock: null });
  const app = basename(dir);
  const sparkline = resolve(root, 'apps/sparkline');
  pinned(dir);
  mkdirSync(resolve(dir, 'data/src'), { recursive: true });
  for (const file of ['app.json', 'app.contract', 'data/src/lib.rs']) cpSync(resolve(sparkline, file), resolve(dir, file));
  writeFileSync(resolve(dir, 'Cargo.toml'), ['[workspace]', 'members = ["data"]', '[patch.crates-io]', ...patchLines(dir)].join('\n'));
  writeFileSync(resolve(dir, 'data/Cargo.toml'), `[package]
name = "sparkline-data"
version = "0.1.0"
edition = "2024"
[dependencies]
exact-plan = { path = ${JSON.stringify(resolve(root, 'plan'))} }
exact-runner = { path = ${JSON.stringify(resolve(root, 'runner'))} }
${dependency}
`);
  return (dev, overrides = {}) => {
    const out = resolve(dir, dev ? 'dev' : 'fresh');
    rmSync(out, { recursive: true, force: true });
    const built = spawnSync(process.execPath, ['--eval', `
      import { buildModule, dirKey } from ${JSON.stringify(import.meta.resolve('../host/web-js/module.mjs'))};
      const [app, dir, out] = ${JSON.stringify([app, dir, out])};
      console.log(dirKey(dir));
      buildModule(app, out, false, dir, { dev: ${dev} });
    `], {
      cwd: dir, encoding: 'utf8', timeout: 240_000,
      env: { ...env, ...overrides },
    });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    const key = built.stdout.trim();
    return `exact-js-module-${app}-${key}${dev ? '-dev' : ''}`;
  };
}

defaultLockTest('a Rust dev module re-resolves pins when the Cargo resolver policy changes', () => {
  const dir = moduleFixture('resolver');
  try {
    const resolveVersion = moduleResolver(dir, 'resolver-helper = "1"', 'resolver-helper');
    mkdirSync(resolve(dir, '.cargo'));
    writeFileSync(resolve(dir, '.cargo/config.toml'), `[patch.crates-io]
older = { package = "resolver-helper", path = "older" }
newer = { package = "resolver-helper", path = "newer" }
`);
    for (const [name, version, extra] of [['older', '1.0.0', ''], ['newer', '1.1.0', 'rust-version = "1.200"\n']]) {
      mkdirSync(resolve(dir, name, 'src'), { recursive: true });
      writeFileSync(resolve(dir, name, 'Cargo.toml'), `[package]\nname = "resolver-helper"\nversion = "${version}"\nedition = "2021"\n${extra}`);
      writeFileSync(resolve(dir, name, 'src/lib.rs'), '');
    }
    const policy = value => ({ CARGO_RESOLVER_INCOMPATIBLE_RUST_VERSIONS: value });
    expect(resolveVersion(true, policy('allow'))).toBe('1.1.0');
    const fresh = resolveVersion(false, policy('fallback'));
    expect(fresh).toBe('1.0.0');
    expect(resolveVersion(true, policy('fallback'))).toBe(fresh);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 30_000);

function withDirectorySource(run) {
  const dir = moduleFixture('directory-source');
  try {
    const registry = basename(dir).toLowerCase();
    const replacement = `${registry}-vendor`;
    const index = `sparse+https://${registry}.invalid/index/`;
    const dependency = `resolver-helper = { version = "1", registry = "${registry}" }`;
    mkdirSync(resolve(dir, '.cargo'));
    writeFileSync(resolve(dir, '.cargo/config.toml'), `[registries.${registry}]
index = "${index}"
[source.${registry}]
registry = "${index}"
replace-with = "${replacement}"
[source.${replacement}]
directory = "vendor"
`);
    const packageDir = version => resolve(dir, 'vendor', `resolver-helper-${version}`);
    const vendor = version => {
      const pkg = packageDir(version);
      mkdirSync(resolve(pkg, 'src'), { recursive: true });
      writeFileSync(resolve(pkg, 'Cargo.toml'), `[package]\nname = "resolver-helper"\nversion = "${version}"\nedition = "2021"\n`);
      writeFileSync(resolve(pkg, 'src/lib.rs'), '');
      writeFileSync(resolve(pkg, '.cargo-checksum.json'), '{"files":{},"package":null}');
    };
    run(dir, dependency, vendor, packageDir);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}

defaultLockTest('a Rust dev module follows Cargo directory source additions and removals', () => {
  withDirectorySource((dir, dependency, vendor, packageDir) => {
    const resolveVersion = moduleResolver(dir, dependency, 'resolver-helper');
    vendor('1.0.0');
    expect(resolveVersion(true)).toBe('1.0.0');

    vendor('1.1.0');
    const afterAdd = [resolveVersion(false), resolveVersion(true)];
    rmSync(packageDir('1.0.0'), { recursive: true });
    expect([...afterAdd, resolveVersion(false), resolveVersion(true)]).toEqual(Array(4).fill('1.1.0'));
  });
}, 30_000);

cargoTest('one-shot and dev Rust modules honor Cargo’s configured lockfile', () => {
  withDirectorySource((dir, dependency, vendor) => {
    const build = rustModuleFixture(dir, dependency);
    mkdirSync(resolve(dir, 'locks'));
    const lock = resolve(dir, 'locks/Cargo.lock');
    const external = { [lockfileEnv]: lock };
    const expectPinned = (dev, env) => {
      const name = build(dev, env);
      const packages = Bun.TOML.parse(readFileSync(lock, 'utf8')).package;
      expect(packages.map(pkg => pkg.name), 'the configured lockfile must record the active wrapper').toContain(name);
      expect(packages.find(pkg => pkg.name === 'resolver-helper')?.version).toBe('1.0.0');
      return name;
    };
    vendor('1.0.0');
    cpSync(resolve(moduleRoot, 'Cargo.lock'), lock);
    build(false, external);
    vendor('1.1.0');
    expectPinned(false, external);
    expectPinned(true, external);

    const config = resolve(dir, '.cargo/config.toml');
    writeFileSync(config, `include = ["lock.toml"]\n${readFileSync(config, 'utf8')}`);
    writeFileSync(resolve(dir, '.cargo/lock.toml'), '[resolver]\nlockfile-path = "locks/Cargo.lock"\n');
    expectPinned(false);
    const name = expectPinned(true);
    const raw = resolve(dir, 'target/web-js-modules/target/wasm32-unknown-unknown/web-dev', `${name.replaceAll('-', '_')}.wasm`);
    const before = statSync(raw, { bigint: true }).mtimeNs;
    expectPinned(true);
    expect(statSync(raw, { bigint: true }).mtimeNs).toBe(before);
  });
}, 300_000);

cargoTest('authored wrappers keep their draw exports in either build order', () => {
  const root = resolve(import.meta.dir, '..');
  const appDir = resolve(root, 'apps/sparkline');
  const dir = moduleFixture('draw');
  try {
    const built = spawnSync(process.execPath, ['--eval', `
      import { readFileSync } from 'node:fs';
      import { buildModule } from ${JSON.stringify(import.meta.resolve('../host/web-js/module.mjs'))};
      const [dir, appDir] = ${JSON.stringify([dir, appDir])};
      const results = [true, false].map(devFirst => {
        const name = devFirst ? 'dev-first' : 'one-shot-first';
        const exportsDraw = (draw, dev) => {
          const wasm = buildModule(name, dir + '/' + name + (dev ? '-dev' : '-one-shot'), draw, appDir, { dev });
          return WebAssembly.Module.exports(new WebAssembly.Module(readFileSync(wasm))).some(e => e.name === 'exact_logic_draw');
        };
        return [exportsDraw(false, devFirst), exportsDraw(true, !devFirst), exportsDraw(false, devFirst)];
      });
      console.log(JSON.stringify(results));
    `], { cwd: dir, env: moduleEnv(dir), encoding: 'utf8', timeout: 240_000 });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    expect(JSON.parse(built.stdout)).toEqual([[false, true, false], [false, true, false]]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 300_000);

cargoTest('a symlinked Cargo target does not put the generated wrapper on the watch list', () => {
  const root = resolve(import.meta.dir, '..');
  const appDir = resolve(root, 'apps/sparkline');
  const dir = moduleFixture('watch');
  const target = resolve(dir, 'alias');
  try {
    mkdirSync(resolve(dir, 'target'));
    symlinkSync(resolve(dir, 'target'), target, 'junction');
    const built = spawnSync(process.execPath, ['--eval', `
      import { buildModule, moduleSourceDirs } from ${JSON.stringify(import.meta.resolve('../host/web-js/module.mjs'))};
      const [name, out, appDir] = ${JSON.stringify([basename(dir), resolve(dir, 'out'), appDir])};
      const wasm = buildModule(name, out, false, appDir, { dev: true });
      console.log(JSON.stringify(moduleSourceDirs(wasm)));
    `], { cwd: dir, env: moduleEnv(dir, { target }), encoding: 'utf8', timeout: 240_000 });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    const dirs = JSON.parse(built.stdout).map(path => realpathSync(path));
    const wrappers = realpathSync(resolve(target, 'web-js-modules', 'wrappers'));
    expect(dirs).toContain(realpathSync(resolve(appDir, 'data')));
    expect(dirs.map(dirname)).not.toContain(wrappers);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 300_000);

// The dev loop's module is built at the dev profile and served as Cargo linked it, marked so a production build refuses
// it, for an app with its own module crate (Caltrain), one whose wrapper exact2 writes (Sparkline) and one whose host
// crates it generates (`exact new --rust`); every other build asks for the profile it shipped with before and is
// optimized. A `cargo` first on PATH stands in for Cargo's build: it records the arguments and the artifact's path and
// links an empty module there; every other command is Cargo's own.
(process.platform === 'win32' ? test.skip : cargoTest)('a dev loop module asks Cargo for the dev profile, and every other module build does not', async () => {
  const { createApp } = await import('../game/new.mjs');
  const dir = moduleFixture('profile'), bin = resolve(dir, 'bin'), log = resolve(dir, 'cargo.log');
  try {
    mkdirSync(bin);
    writeFileSync(resolve(bin, 'cargo'), `#!/bin/sh
sub=
for a in "$@"; do case $a in +*) ;; *) sub=$a; break;; esac; done
[ "$sub" = build ] || exec ${JSON.stringify(Bun.which('cargo'))} "$@"
prev=; profile=; manifest=
for a in "$@"; do
  [ "$prev" = --profile ] && profile=$a
  [ "$prev" = --manifest-path ] && manifest=$a
  [ "$a" = --release ] && profile=release
  prev=$a
done
name=$(sed -n 's/^name = "\\(.*\\)"$/\\1/p' "$manifest" | head -1 | tr - _)
artifact="$CARGO_TARGET_DIR/wasm32-unknown-unknown/$profile/$name.wasm"
echo "$artifact $*" >> ${JSON.stringify(log)}
mkdir -p "$(dirname "$artifact")"
printf '\\000asm\\001\\000\\000\\000' > "$artifact"
`, { mode: 0o755 });
    const generated = resolve(dir, 'tally');
    createApp(generated, { rust: true, title: 'Tally' });
    const built = spawnSync(process.execPath, ['--eval', `
      import { buildModule, moduleGrants, DEV_PROFILE } from ${JSON.stringify(import.meta.resolve('../host/web-js/module.mjs'))};
      import { existsSync, readFileSync } from 'node:fs';
      const [out, apps, log] = ${JSON.stringify([resolve(dir, 'out'), [['caltrain', resolve(moduleRoot, 'apps/caltrain')], ['sparkline', resolve(moduleRoot, 'apps/sparkline')], ['tally', generated]], log])};
      const builds = [];
      for (const [app, at] of apps) for (const dev of [true, false]) {
        const wasm = buildModule(app, out + '/' + app + dev, false, at, { dev });
        const refused = await moduleGrants(wasm, { production: true }).then(() => '', e => e.message);
        const [artifact, ...args] = readFileSync(log, 'utf8').trim().split('\\n').at(-1).split(' ');
        builds.push([app, dev, args, existsSync(artifact.replace(/\\.wasm$/, '.optimized.wasm')), refused.includes("a dev loop's Rust module")]);
      }
      console.log(JSON.stringify({ DEV_PROFILE, builds }));
    `], { cwd: dir, env: { ...moduleEnv(dir), PATH: `${bin}:${process.env.PATH}` }, encoding: 'utf8', timeout: 240_000 });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    const { DEV_PROFILE, builds } = JSON.parse(built.stdout.trim().split('\n').at(-1));
    const dev = [`inherits=`, 'opt-level=1', 'lto=false', 'codegen-units=16', 'incremental=true'].map(setting => `profile.${DEV_PROFILE}.${setting}`);
    const profile = args => args.includes('--release') ? 'release' : args[args.indexOf('--profile') + 1];
    const settings = args => args.filter(a => a.startsWith(`profile.${DEV_PROFILE}.`)).map(a => a.replace(/=".*"$/, '='));
    expect(builds.map(([app, isDev, args, optimized, refused]) => [app, isDev, profile(args), settings(args), optimized, refused])).toEqual([
      ['caltrain', true, DEV_PROFILE, dev, false, true], ['caltrain', false, 'release', [], true, false],
      ['sparkline', true, DEV_PROFILE, dev, false, true], ['sparkline', false, 'release', [], true, false],
      ['tally', true, DEV_PROFILE, dev, false, true], ['tally', false, 'web', [], true, false],
    ]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 300_000);
