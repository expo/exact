import { createHash } from 'node:crypto';
import { copyFileSync, cpSync, existsSync, lstatSync, mkdirSync, mkdtempSync, readdirSync, readFileSync, readlinkSync, renameSync, rmSync, statSync, utimesSync, writeFileSync } from 'node:fs';
import { basename, dirname, relative, resolve, sep } from 'node:path';
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
  let keep = () => {};
  return { env, async seed(host) {
    host = host === 'mac' ? 'macos' : host;
    const { nativeBuild, planSource, receiptPath, windowsBuild } = await import('./agent-build.mjs');
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
    const reusable = () => {
      if (host === 'windows') { const state = windowsBuild(app); return state.changed?.length === 0; }
      const state = nativeBuild(host, app);
      return !state.missing && !!state.receipt && Array.isArray(JSON.parse(readFileSync(state.receipt, 'utf8')).build?.binary?.inputs)
        && (!state.changed.length || state.changed.every(path => planSource(path, app)));
    };
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
        if (!reusable()) throw new Error('native artifact inputs changed');
        seeded = JSON.stringify(nativeFiles(directory));
      } catch (error) {
        rmSync(directory, { recursive: true, force: true }); mkdirSync(directory);
        if (!['ENOENT', 'ENOTDIR'].includes(error.code) && !(error instanceof SyntaxError) && !/native artifact/.test(error.message)) throw error;
      }
    }));
    keep = () => locked(() => selected(() => {
      if (seeded && JSON.stringify(nativeFiles(directory)) === seeded) return;
      if (!reusable()) return;
      const stage = mkdtempSync(resolve(cache, '.stage-'));
      try {
        cpSync(directory, resolve(stage, 'files'), { recursive: true, preserveTimestamps: true, verbatimSymlinks: true });
        writeFileSync(resolve(stage, 'artifact.json'), JSON.stringify({ origin: directory, files: nativeFiles(resolve(stage, 'files')) }));
        rmSync(current, { recursive: true, force: true }); renameSync(stage, current);
      } finally { rmSync(stage, { recursive: true, force: true }); }
    }));
  }, close() { if (!existsSync(directory)) return; try { keep(); } finally { rmSync(directory, { recursive: true, force: true }); } } };
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
    catch (error) { if (error.code === 'ENOENT' || error.code === 'ENOTDIR') return true; throw error; }
  }).map(([path]) => path);
}
