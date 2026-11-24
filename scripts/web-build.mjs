import { createHash } from 'node:crypto';
import { copyFileSync, cpSync, existsSync, lstatSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, readlinkSync, realpathSync, renameSync, rmSync, statSync, utimesSync, writeFileSync } from 'node:fs';
import { basename, dirname, isAbsolute, relative, resolve, sep } from 'node:path';
import { takeLock } from '../host/web/build-lock.mjs';

// LLP 1172.010 E11: a frontend owns its publication; checks own one per invocation.
export function loopWebDist(app, target) {
  const identity = createHash('sha256').update(resolve(app.dir)).digest('hex').slice(0, 16);
  return resolve(app.target, 'web-loops', identity, target);
}

export function privateWebBuild(app, kind) {
  const root = resolve(app.target, 'web-runs');
  mkdirSync(root, { recursive: true });
  const directory = mkdtempSync(resolve(root, `${kind}-`));
  const dist = resolve(directory, 'dist');
  return { dist, env: { EXACT_WEB_DIST: dist, EXACT_BAKE_OUTPUT: resolve(directory, 'bake'), EXACT_WEB_TYPES: 'private' }, close: () => rmSync(directory, { recursive: true, force: true }) };
}

export function privateNativeBuild(app, kind) {
  const root = resolve(app.target, 'native-runs');
  mkdirSync(root, { recursive: true });
  const directory = mkdtempSync(resolve(root, `${kind}-`));
  const env = { EXACT_APPLE_OUTPUT: resolve(directory, 'apple'), EXACT_WINDOWS_OUTPUT: resolve(directory, 'windows'), EXACT_BAKE_OUTPUT: resolve(directory, 'bake'), EXACT_WEB_TYPES: 'private' };
  let keep = () => {}, stale = false;
  return { env, async seed(host, { checkInDrive = false } = {}) {
    host = host === 'mac' ? 'macos' : host;
    const { nativeBuild, planSource, receiptPath, windowsBuild } = await import('./agent-build.mjs');
    const { refuseBackendOrigin } = await import('./agent-launch.mjs');
    const { appleArtifacts } = await import('../host/apple/build.mjs');
    const identity = createHash('sha256').update(resolve(app.dir)).digest('hex').slice(0, 16);
    const cache = resolve(app.target, 'native-reuse', identity, host, process.env.EXACT_UPDATE_TRUST ?? 'development');
    mkdirSync(cache, { recursive: true });
    const current = resolve(cache, 'current');
    let seeded;
    const selected = use => {
      const before = Object.fromEntries(Object.keys(env).map(key => [key, process.env[key]]));
      Object.assign(process.env, env);
      try { return use(); } finally { for (const [key, value] of Object.entries(before)) { if (value === undefined) delete process.env[key]; else process.env[key] = value; } }
    };
    // An Apple build is reusable when its receipt names its binary's inputs and only its plan's sources changed
    // since. One stale in more is kept when the drive's check would rebuild it rather than refuse it for its backend.
    const apple = checked => {
      const state = nativeBuild(host, app, { checked });
      const checkable = !state.missing && !!state.receipt && Array.isArray(JSON.parse(readFileSync(state.receipt, 'utf8')).build?.binary?.inputs);
      const reusable = checkable && state.changed.every(path => planSource(path, app));
      let rebuildable = checkable && !reusable;
      if (rebuildable) try { refuseBackendOrigin(state, app); } catch { rebuildable = false; }
      return { reusable, kept: reusable || rebuildable };
    };
    const reusable = checked => host === 'windows' ? windowsBuild(app).changed?.length === 0 : apple(checked).reusable;
    const locked = use => {
      let release;
      try { release = takeLock(resolve(cache, '.lock'), { waitMs: 0 }); }
      catch (error) { if (error.code === 'ELOCKED') return; throw error; }
      try { return use(); } finally { release(); }
    };
    locked(() => selected(() => {
      try {
        const record = JSON.parse(readFileSync(resolve(current, 'artifact.json'), 'utf8'));
        cpSync(resolve(current, 'files'), directory, { recursive: true, preserveTimestamps: true, verbatimSymlinks: true });
        if (JSON.stringify(nativeFiles(directory)) !== JSON.stringify(record.files)) throw new Error('native artifact changed');
        // A signed bundle stays byte-for-byte intact. Its full receipt is checked
        // beside the product, with captured paths relocated to this invocation.
        if (host === 'ios') {
          const paths = appleArtifacts(app, { destination: 'ios-simulator' }), sidecar = resolve(paths.products, 'receipt.json');
          if (!existsSync(sidecar)) copyFileSync(resolve(paths.bundle, 'receipt.json'), sidecar);
        }
        relocateNativeReceipts(directory, record.origin);
        if (host !== 'windows') {
          const paths = appleArtifacts(app, host === 'ios' ? { destination: 'ios-simulator' } : {}), override = resolve(paths.namespace, 'plan/override.json');
          if (existsSync(override)) {
            const record = JSON.parse(readFileSync(override, 'utf8'));
            record.receipt = createHash('sha256').update(readFileSync(receiptPath(host, app))).digest('hex');
            writeFileSync(override, JSON.stringify(record) + '\n');
          }
        }
        if (checkInDrive) {
          // Let buildFirst check the copied inputs after preparation. For a backend mismatch, keep a fresh or
          // plan-only stale build for buildFirst to refuse, and drop a native stale build so it can rebuild.
          const path = receiptPath(host, app), receipt = path && JSON.parse(readFileSync(path, 'utf8'));
          if (!Array.isArray(receipt?.build?.binary?.inputs)) throw new Error('native artifact has no input receipt');
          try { refuseBackendOrigin(receipt.build, app); }
          catch { if (!apple().reusable) throw new Error('native artifact has another backend'); }
        } else {
          // Keep a stale build so the drive can name what changed and build over it.
          const status = host === 'windows' ? { kept: reusable() } : apple();
          if (!status.kept) throw new Error('native artifact inputs changed');
          stale = host !== 'windows' && !status.reusable;
        }
        seeded = JSON.stringify(nativeFiles(directory));
      } catch (error) {
        rmSync(directory, { recursive: true, force: true }); mkdirSync(directory);
        if (!['ENOENT', 'ENOTDIR'].includes(error.code) && !(error instanceof SyntaxError) && !/native artifact/.test(error.message)) throw error;
      }
    }));
    keep = checked => locked(() => selected(() => {
      if (seeded && JSON.stringify(nativeFiles(directory)) === seeded) return;
      if ((checkInDrive && !checked) || !reusable(checked)) return;
      const stage = mkdtempSync(resolve(cache, '.stage-'));
      try {
        cpSync(directory, resolve(stage, 'files'), { recursive: true, preserveTimestamps: true, verbatimSymlinks: true });
        writeFileSync(resolve(stage, 'artifact.json'), JSON.stringify({ origin: directory, files: nativeFiles(resolve(stage, 'files')) }));
        rmSync(current, { recursive: true, force: true }); renameSync(stage, current);
      } finally { rmSync(stage, { recursive: true, force: true }); }
    }));
  // Apple drives publish with a successful build check. Other callers need a successful command to publish over a
  // stale seed, since a failed build can leave a receipt that matches its sources.
  }, close(code = 0, checked) { if (!existsSync(directory)) return; try { if (!(stale && code)) keep(checked); } finally { rmSync(directory, { recursive: true, force: true }); } } };
}

function nativeFiles(directory, prefix = '') {
  return readdirSync(resolve(directory, prefix)).sort().flatMap(name => {
    const path = prefix ? `${prefix}/${name}` : name, file = resolve(directory, path), stat = lstatSync(file);
    if (stat.isSymbolicLink()) return [[path, 'link', readlinkSync(file)]];
    if (stat.isDirectory()) return nativeFiles(directory, path);
    return [[path, 'file', createHash('sha256').update(readFileSync(file)).digest('hex')]];
  });
}

function relocateNativeReceipts(directory, origin, at = directory) {
  for (const entry of readdirSync(at, { withFileTypes: true })) {
    const path = resolve(at, entry.name);
    if (entry.isDirectory() && !entry.name.endsWith('.app')) relocateNativeReceipts(directory, origin, path);
    else if (entry.isFile() && /(^receipt\.json$|\.build\.json$|\.receipt\.json$)/.test(entry.name)) {
      const info = statSync(path);
      const relocate = value => typeof value === 'string' && (value === origin || value.startsWith(origin + sep)) ? resolve(directory, relative(origin, value)) : value;
      const record = JSON.parse(readFileSync(path, 'utf8'), (key, value) => relocate(value));
      writeFileSync(path, JSON.stringify(record) + '\n'); utimesSync(path, info.atime, info.mtime);
    }
  }
}

/** Copy while the producer still holds its Cargo product claims (LLP 1172.010 E11). */
export function captureWebProducts(receipt, directory) {
  const copied = {};
  const take = product => {
    const path = resolve(directory, createHash('sha256').update(product.path).digest('hex').slice(0, 16), basename(product.path));
    mkdirSync(dirname(path), { recursive: true });
    copyFileSync(product.path, path);
    if (createHash('sha256').update(readFileSync(path)).digest('hex') !== product.sha256) throw new Error(`Cargo product changed before capture: ${product.path}`);
    copied[product.path] = path;
  };
  const paired = (receipt.binary?.inputs ?? []).filter(input => input.name.startsWith('generated:') && /[/\\]app\.(hbc|js|module\.json)$/.test(input.path));
  for (const product of [...receipt.products.filter(product => product.path.endsWith('.wasm')), ...paired]) take(product);
  // Cargo embeds the JS and receipt, but only the receipt names the bytecode it ships beside them.
  for (const input of paired.filter(input => input.path.endsWith('app.module.json'))) {
    const { module } = JSON.parse(readFileSync(copied[input.path], 'utf8'));
    if (module) take({ path: resolve(dirname(input.path), module.file), sha256: module.sha256 });
  }
  return copied;
}

export function cargoContractReads(roots, began = Infinity) {
  // Cargo's dep-info arrives after compilation. A read whose bytes may have moved
  // since the build began is retained as uncertain, never certified (LLP 1172.010 E11).
  const identity = file => [lstatSync(file), statSync(file)].map(s => [s.dev, s.ino, s.size, s.mtimeMs, s.ctimeMs]);
  return Object.fromEntries(roots.filter(root => root.file?.endsWith('.contract')).map(({ file }) => {
    try {
      const before = identity(file), sha256 = createHash('sha256').update(readFileSync(file)).digest('hex'), after = identity(file);
      return [file, after.some(s => s[3] >= began || s[4] >= began) || JSON.stringify(before) !== JSON.stringify(after) ? null : sha256];
    } catch (error) { if (error.code === 'ENOENT' || error.code === 'ENOTDIR') return [file, null]; throw error; }
  }));
}

/** Hashes of the text the compiler consumed, never a reread of moving sources. */
export function contractReadReceipt(directory) {
  const map = JSON.parse(readFileSync(resolve(directory, 'app.plan.map.json'), 'utf8'));
  return Object.fromEntries(map.sources.filter(source => !source.file.startsWith('exact:'))
    .map(source => [resolve(source.file), source.sha256]));
}

export function changedContractReads(reads) {
  return Object.entries(reads).filter(([path, expected]) => {
    try { return createHash('sha256').update(readFileSync(path)).digest('hex') !== expected; }
    catch (error) { if (['ENOENT', 'ENOTDIR', 'EISDIR', 'ELOOP'].includes(error.code)) return true; throw error; }
  }).map(([path]) => path);
}

const digest = bytes => createHash('sha256').update(bytes).digest('hex');

function linkedReceipt(app, out) {
  const destination = resolve(realpathSync(dirname(out)), basename(out));
  return resolve(app, '.exact', 'web-builds', digest(destination) + '.json');
}

function lookup(path, sources, since = Infinity) {
  let target;
  try {
    target = realpathSync(path);
  } catch (error) {
    if (!['ENOENT', 'ENOTDIR', 'ELOOP'].includes(error.code)) throw error;
  }
  // A replaced link can lead to an old file. When the lookup failed, the
  // nearest surviving parent records the removal of a name during the build.
  if (since !== Infinity) {
    let missing = !target;
    for (let part = path; ; part = dirname(part)) {
      try {
        const stat = lstatSync(part);
        if ((missing || stat.isSymbolicLink()) && stat.ctimeMs >= since) return 'changed during build';
        missing = false;
      } catch (error) {
        if (!['ENOENT', 'ENOTDIR', 'ELOOP'].includes(error.code)) throw error;
      }
      if (part === dirname(part)) break;
    }
  }
  if (!target) return null;
  try {
    const stat = statSync(target);
    if (stat.ctimeMs >= since) return 'changed during build';
    return [target, stat.isFile() ? sources[target] ?? digest(readFileSync(target)) : 'directory'];
  } catch (error) {
    if (['ENOENT', 'ENOTDIR', 'ELOOP'].includes(error.code)) return null;
    throw error;
  }
}

/** Keep the compiler's package inputs with the checkout. Only their combined digest goes into a distributable site. */
export function recordLinkedSources(app, out, generated, began) {
  if (!existsSync(resolve(generated, 'app.plan.map.json'))) return null;
  const root = realpathSync(app);
  const outside = path => {
    const fromApp = relative(root, path);
    const parts = fromApp.split(sep);
    return isAbsolute(fromApp) || parts[0] === '..' || parts.includes('node_modules');
  };
  const sources = Object.fromEntries(Object.entries(contractReadReceipt(generated)).filter(([path]) => outside(path)));
  const file = resolve(generated, 'contract-lookups.json');
  const names = existsSync(file) ? JSON.parse(readFileSync(file, 'utf8')) : [];
  const lookups = Object.fromEntries([...new Set(names)].filter(outside).sort().map(path => [path, lookup(path, sources, began)]));
  if (!Object.keys(sources).length && !Object.keys(lookups).length) return null;
  const path = linkedReceipt(app, out), bytes = JSON.stringify({ sources, lookups });
  mkdirSync(dirname(path), { recursive: true });
  const pending = path + '.' + process.pid + '.tmp';
  writeFileSync(pending, bytes);
  renameSync(pending, path);
  return digest(bytes);
}

/** A copied site can recover its local inputs by rebuilding once beside its source checkout. */
export function linkedSourcesChanged(app, out, expected) {
  if (!expected) return false;
  const path = linkedReceipt(app, out);
  let bytes;
  try { bytes = readFileSync(path); } catch (error) { if (error.code === 'ENOENT') return true; throw error; }
  if (digest(bytes) !== expected) return true;
  const { sources, lookups } = JSON.parse(bytes);
  return changedContractReads(sources).length > 0 ||
    Object.entries(lookups).some(([file, was]) => JSON.stringify(lookup(file, sources)) !== JSON.stringify(was));
}
