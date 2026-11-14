// Cargo's exact unit dep-info, including the nightly's copied products.
import { basename, dirname, isAbsolute, relative, resolve, sep } from 'node:path';
import { existsSync, lstatSync, mkdirSync, statSync, readFileSync, readdirSync, readlinkSync, realpathSync, renameSync, writeFileSync } from 'node:fs';
import { createHash, randomUUID } from 'node:crypto';
import { beside } from '../host/web/beside.mjs';
const under = (root, file) => {
  const child = relative(root, file);
  return child === '' || (!isAbsolute(child) && child !== '..' && !child.startsWith('..' + sep));
};

/** The nearest package's top-level tests; app directories keep their own input rules. */
export function packageTests(dir) {
  while (!existsSync(resolve(dir, 'app.json')) && !existsSync(resolve(dir, 'package.json')) && dirname(dir) !== dir) dir = dirname(dir);
  return !existsSync(resolve(dir, 'app.json')) && existsSync(resolve(dir, 'package.json')) ? resolve(dir, 'test') : null;
}

/** The package an explicit watch reads: the watched directory when it is one, else the installed
 * package (`node_modules/<name>` or `node_modules/@scope/<name>`) that holds it. */
function watchedPackage(path) {
  path = resolve(path);
  if (existsSync(resolve(path, 'package.json')) || existsSync(resolve(path, 'Cargo.toml'))) return path;
  const parts = path.split(sep), at = parts.lastIndexOf('node_modules');
  if (at < 0) return null;
  const root = parts.slice(0, at + (parts[at + 1]?.startsWith('@') ? 3 : 2)).join(sep);
  return existsSync(resolve(root, 'package.json')) ? root : null;
}

/** Explicit package watches keep installed sources; incidental tool trees and actual outputs stay excluded. */
export function watchOutput(outputs = [], inputs = []) {
  const roots = [...new Set(outputs.filter(Boolean).map(path => outputPath(resolve(path))))];
  const packages = [...new Set(inputs.map(watchedPackage).filter(Boolean))]
    .map(path => ({path, real:outputPath(path)})).sort((a, b) => b.path.length - a.path.length);
  const toolDirectory = name => /^(target|node_modules|\.git|\.exact|\.gradle|\.build)$/.test(name);
  const appOutput = (path, directory) => {
    const name = basename(path);
    return ((directory && /^(dist|web-dist|dist-windows|artifacts)$/.test(name))
      || beside(name) || beside(name, 'web-dist') || name === 'app.contract.d.ts')
      && (existsSync(resolve(dirname(path), 'app.json')) || existsSync(resolve(dirname(path), 'app.contract')));
  };
  return (path, directory) => {
    const name = basename(path);
    if ((directory && toolDirectory(name)) || name === '.git' || name.startsWith('.exact-js-bake-')
      || appOutput(path, directory)) return true;
    let real; try { real = outputPath(path); } catch { real = resolve(path); }
    const source = packages.find(root => under(root.path, resolve(path)) && under(root.real, real))?.real;
    // Installed packages keep their node_modules ancestors; tool directories
    // inside the package and aliases into other tool outputs remain excluded.
    for (let at = real, isDir = directory; dirname(at) !== at && !under(at, resolve(path)); at = dirname(at), isDir = true) {
      const installed = basename(at) === 'node_modules' && source && under(at, source);
      if ((isDir && toolDirectory(basename(at)) && !installed) || appOutput(at, isDir)) return true;
    }
    return (directory && toolDirectory(basename(real))) || appOutput(real, directory) || roots.some(root => under(root, real));
  };
}

/** A directory watch omits outputs and package tests; a separately named file remains an input. */
export function directoryInputNames(dir, skip = watchOutput()) {
  const tests = packageTests(dir);
  return readdirSync(dir).sort().filter(name => {
    const path = resolve(dir, name);
    let directory = false; try { directory = statSync(path).isDirectory(); } catch { /* dangling link */ }
    return !(directory && path === tests) && !skip(path, directory);
  });
}

const inputIdentity = info => [info.dev, info.ino, info.mode, info.size, info.mtimeMs, info.ctimeMs];

/** File identity detects replacement at an old mtime; directory identity would include ignored output edits. */
export function directoryInputSnapshot(path, skip = () => false) {
  let link;
  try { if (lstatSync(path).isSymbolicLink()) link = readlinkSync(path); } catch { /* unavailable */ }
  try {
    const entries = directoryInputNames(path, skip).map(name => {
      const child = resolve(path, name);
      let target;
      try { if (lstatSync(child).isSymbolicLink()) target = readlinkSync(child); } catch { /* unavailable */ }
      const linked = target === undefined ? {} : {link:target};
      try {
        const info = statSync(child), directory = info.isDirectory();
        return {name, ...linked, directory, ...(!directory ? {identity:inputIdentity(info)} : {})};
      } catch (error) { return {name, ...linked, directory:false, error:error.code}; }
    });
    return {path, ...(link === undefined ? {} : {link}), entries};
  } catch (error) {
    if (!['EACCES', 'EPERM', 'ENOENT', 'ENOTDIR', 'ELOOP'].includes(error.code)) throw error;
    let identity = null; try { identity = inputIdentity(statSync(path)); } catch { /* unavailable */ }
    return {path, ...(link === undefined ? {} : {link}), entries:null, error:error.code, identity};
  }
}

/** The receipt and drive walk the same filtered entries and record links before stopping cycles. */
export function* watchedTree(paths, {outputs = [], skip = () => false} = {}) {
  const ancestors = new Set();
  function* visit(path, excluded, top = false) {
    let info, link;
    try {
      info = lstatSync(path);
      if (info.isSymbolicLink()) { link = readlinkSync(path); info = statSync(path); }
    } catch (error) { yield {path, link, error}; return; }
    if ((!top || info.isDirectory()) && excluded(path, info.isDirectory())) return;
    if (!info.isDirectory()) { yield {path, info, link}; return; }
    let real; try { real = realpathSync(path); } catch { real = path; }
    if (ancestors.has(real)) { yield {path, info, link, cycle:true}; return; }
    const snapshot = directoryInputSnapshot(path, excluded);
    yield {path, info, link, snapshot};
    ancestors.add(real);
    try { for (const entry of snapshot.entries ?? []) yield* visit(resolve(path, entry.name), excluded); }
    finally { ancestors.delete(real); }
  }
  for (const path of paths) {
    const output = watchOutput(outputs, [path]);
    yield* visit(resolve(path), (path, directory) => output(path, directory) || skip(path, directory), true);
  }
}

/** Inventories keep membership and link targets even when timestamps precede the build. */
export function packageDirectories(paths, outputs = []) {
  const directories = new Map();
  for (const {snapshot} of watchedTree(paths, {outputs})) if (snapshot) directories.set(snapshot.path, snapshot);
  // A watched file's `ENOTDIR` snapshot: a directory put in its place at an old mtime still differs.
  for (const path of paths.map(path => resolve(path)).filter(path => !directories.has(path))) {
    const snapshot = directoryInputSnapshot(path);
    if (snapshot.error === 'ENOTDIR') directories.set(path, snapshot);
  }
  return [...directories.values()];
}

/** Capture compiler inputs by bytes, directory membership and the targets written in links. */
export function buildInputCollector({outputs, nameOf, hashes, replaced}) {
  const inputs = new Map(), absent = new Map(), directories = new Map(), links = new Map();
  const add = (path, optional = false, declarer = null) => {
    for (const node of watchedTree([path], {outputs})) {
      const {path:input, info, link, snapshot, error} = node;
      if (replaced.has(input)) continue;
      const name = nameOf(input, declarer);
      if (link !== undefined) links.set(name, {name, path:input, target:link});
      if (error) {
        if (error.code === 'ELOOP' && link !== undefined) continue;
        if (['ENOENT', 'ENOTDIR'].includes(error.code) && (optional || link !== undefined)) { absent.set(name, input); continue; }
        throw new Error(`stale compiler dependency names missing or unreadable input ${input}; rebuild that Cargo unit`, {cause:error});
      }
      if (snapshot) {
        if (!snapshot.entries) throw new Error(`unreadable compiler input ${input}`);
        directories.set(name, {name, path:input, names:snapshot.entries.map(entry => entry.name)});
      } else if (info.isFile() && inputs.get(name)?.path !== input) inputs.set(name, {name, path:input, sha256:hashes.of(input, info)});
      else if (!node.cycle && !info.isFile()) throw new Error(`unsupported compiler input ${input}`);
    }
  };
  return {inputs, absent, directories, links, add};
}

/** The inputs of the SwiftPM targets `product` consumes (`swift package
 * describe`'s targets), added through a `buildInputCollector`. Each target's
 * whole directory is added: SwiftPM compiles every source file under it, so
 * a file added there is a new binary although no recorded file changed. */
export function swiftTargetInputs(description, packageRoot, product, add) {
  const pending = [product], seen = new Set();
  while (pending.length) {
    const name = pending.pop();
    if (seen.has(name)) continue;
    seen.add(name);
    const unit = description.targets.find(target => target.name === name);
    if (!unit) throw new Error(`Swift package has no ${name}`);
    add(resolve(packageRoot, unit.path));
    for (const resource of unit.resources ?? []) add(resolve(packageRoot, resource.path));
    pending.push(...(unit.target_dependencies ?? []));
  }
}

/** Source-owned binary inputs, relocated by root instead of the builder's
 * absolute paths. Generated products and registry caches stay with the bake. */
export function binarySourceInputs({files, directories, absent, links, metadata}, roots, outputs) {
  const skip = watchOutput(outputs);
  const located = path => {
    if (skip(path)) return null;
    const root = roots.find(root => under(root.contains ?? root.path, path));
    return root ? { root: root.name, path: relative(root.path, path).split(sep).join('/') } : null;
  };
  const capture = (rows, pathOf, rest) => rows.flatMap(row => {
    const at = located(pathOf(row));
    return at ? [{ ...at, ...rest(row) }] : [];
  });
  // The receipt holds this as JSON: an undefined field is absent there too.
  return { version: 1, metadata: JSON.parse(JSON.stringify(metadata)),
    files: capture(files, file => file.path, file => ({ sha256: file.sha256 })),
    directories: capture([...directories.values()], dir => dir.path, dir => ({ names: dir.names })),
    absent: capture([...absent.values()], path => path, () => ({})),
    links: capture([...links.values()], link => link.path, link => ({ sha256: createHash('sha256').update(link.target).digest('hex') })),
  };
}

function outputPath(path) {
  try { return realpathSync(path); }
  catch (error) {
    if (!['ENOENT', 'ENOTDIR'].includes(error.code) || dirname(path) === path) throw error;
    let link; try { link = readlinkSync(path); } catch { /* not a link */ }
    return link === undefined ? resolve(outputPath(dirname(path)), basename(path)) : outputPath(resolve(dirname(path), link));
  }
}

// Cargo follows directory links. Track their contents without reading outputs
// through an alias or opening special files the baker may never use.
// With no pre-bake snapshot, `since` rejects inputs whose change time cannot precede the bake.
export function watchedDigest(paths, outputs, { inventory = false, since = Infinity } = {}) {
  const hash = createHash('sha256');
  const excluded = outputs.map(outputPath), ancestors = new Set();
  let stable = true;
  const changed = info => { if (info.ctimeMs >= since) stable = false; };
  const row = (...values) => hash.update(JSON.stringify(values) + '\n');
  const visit = (path, top = false, atRoot = false, output = watchOutput(outputs, [path])) => {
    let info, link;
    try {
      info = lstatSync(path);
      if (info.isSymbolicLink()) {
        link = info;
        row(path, 'link', readlinkSync(path));
        info = statSync(path);
      }
      if (inventory && atRoot && basename(path).startsWith('.') && info.isDirectory()) return;
      const real = realpathSync(path);
      if (excluded.some(root => under(root, real))) { if (!inventory) row(path, 'output'); return; }
      changed(info);
      if (link) changed(link);
      if (info.isDirectory()) {
        if (ancestors.has(real)) { row(path, 'cycle'); return; }
        const names = inventory ? readdirSync(path).sort() : directoryInputNames(path, output);
        if (!inventory) row(path, 'directory', names);
        ancestors.add(real);
        try {
          for (const name of names) {
            // Native producer sources (js/bake/src/lib.rs `sources`) and Cargo's Rust targets.
            if (inventory && (['.git', 'node_modules', 'target', 'dist'].includes(name)
              || name.startsWith('.exact-js-bake-') || name.endsWith('.test.contract')
              || (top && (name === 'dist-windows' || beside(name) || name === 'app.contract.d.ts')))) continue;
            visit(resolve(path, name), false, top, output);
          }
        }
        finally { ancestors.delete(real); }
      } else if (inventory) {
        if (/\.(rs|ts|[cm]?js|json|contract|ttf|otf)$/.test(path)) row(path, 'source');
      } else if (info.isFile()) row(path, 'file', createHash('sha256').update(readFileSync(path)).digest('hex'));
      else row(path, info.isSocket() ? 'socket' : info.isFIFO() ? 'fifo'
        : info.isBlockDevice() ? 'block-device' : info.isCharacterDevice() ? 'character-device' : 'other');
      if (since !== Infinity) {
        changed(lstatSync(path));
        if (link) changed(statSync(path));
      }
    } catch (error) {
      if (info) changed(info);
      if (link) changed(link);
      if (error.code === 'ELOOP') row(path, 'cycle');
      else if (error.code === 'ENOENT' || error.code === 'ENOTDIR') {
        row(path, 'missing');
        // A missing watch has no change time; its nearest existing parent records entry removal.
        if (since !== Infinity) {
          let parent = path;
          do { parent = dirname(parent); } while (!existsSync(parent) && dirname(parent) !== parent);
          changed(statSync(parent));
        }
      }
      // Directory watches include unused files. The baker decides which must be readable.
      else if (error.code === 'EACCES' || error.code === 'EPERM')
        row(path, 'unreadable', info?.mode, info?.size, info?.mtimeMs, info?.ctimeMs);
      else throw error;
    }
  };
  for (const path of paths) visit(path, true);
  return stable ? hash.digest('hex') : null;
}

function sourceInventory(app, outputs) {
  const inventory = {
    paths: [app.dir, ...Object.values(app.manifest.typescript?.sources ?? {}).map(path => resolve(app.dir, path))],
    outputs: [...outputs, ...['macos', 'ios'].flatMap(platform => (app.manifest.host?.[platform]?.resources ?? []).map(({from}) => resolve(app.dir, from)))],
  };
  inventory.sha256 = watchedDigest(inventory.paths, inventory.outputs, {inventory:true});
  return inventory;
}

/** Sources checked for a packaged drive, outside the binary fingerprint.
 * Guarded watches share Cargo's bake revision; the inventory detects new
 * imports and Cargo targets. Include optional asset roots even when Cargo
 * omits their missing-file watches. */
export function bakeSourceInputs(app, paths, outputs) {
  outputs = [...new Set(outputs.filter(Boolean).map(path => resolve(path)))];
  const inventory = sourceInventory(app, outputs);
  return {inputs:[...new Set(paths)].sort().map(path => ({path, sha256:watchedDigest([path], outputs)})), outputs, inventory};
}

export function pendingSourceInputs(record) {
  const changed = record.inputs.filter(({path, sha256}) => watchedDigest([path], record.outputs) !== sha256).map(({path}) => path);
  const inventory = record.inventory;
  if (watchedDigest(inventory.paths, inventory.outputs, {inventory:true}) !== inventory.sha256) changed.push('app source inventory');
  return changed;
}

/** A development receipt names four thousand inputs by SHA-256, on every build,
 * and nearly all of them are the files they were at the last one: reading them
 * again was 0.3 s of a 2.5 s build with nothing changed. Each hash is kept in
 * the target directory with its size, modification and change times, and file
 * identity. Restoring a modification time must not reuse an old digest.
 * A production receipt reads every byte. */
export function inputHashes(app, env) {
  const kept = resolve(app.target, 'bake-input-hashes.json'), production = env.EXACT_UPDATE_TRUST === 'production';
  let known = {}, grew = false;
  if (!production) try { known = JSON.parse(readFileSync(kept, 'utf8')); } catch { /* none yet */ }
  const used = {};
  return {
    of(path, info) {
      const was = known[path];
      const now = was && was[0] === info.mtimeMs && was[1] === info.size && was[3] === info.ctimeMs && was[4] === info.ino && was[5] === info.dev
        ? was : [info.mtimeMs, info.size, createHash('sha256').update(readFileSync(path)).digest('hex'), info.ctimeMs, info.ino, info.dev];
      if (now !== was) grew = true;
      used[path] = now;
      return now[2];
    },
    save() {
      if (production || !grew) return;
      // Every app of a checkout shares the file; one that has grown past any of them starts again from this build's.
      const next = Object.keys(known).length > 40_000 ? used : { ...known, ...used };
      writeFileSync(`${kept}.${process.pid}.tmp`, JSON.stringify(next)); renameSync(`${kept}.${process.pid}.tmp`, kept);
    },
  };
}

/** Products consumed by a later build step, captured before that step reads them. Their directories'
 * inventories omit `outputs` and tool trees, as the freshness walk over them does. */
export function inputProducts(paths, root, outputs = []) {
  const products = [...new Set(paths)].filter(existsSync).map(path => {
    const bytes = readFileSync(path);
    return {path, bytes:bytes.length, sha256:createHash('sha256').update(bytes).digest('hex'), input:true};
  });
  const directories = new Map(), output = watchOutput(outputs, [root]);
  for (const {path} of products) for (let dir = dirname(path); dir !== root && under(root, dir); dir = dirname(dir)) {
    if (!directories.has(dir)) directories.set(dir, directoryInputSnapshot(dir, output));
  }
  return {products, watchedDirectories:[...directories.values()]};
}

/** A bake's file watches, including optional inputs tracked by the builder rather than Cargo's recursive scan. */
export function cargoInputPaths(stdout, base) {
  return [...new Set(stdout.split('\n').flatMap(line => {
    const path = /^cargo::?(?:rerun-if-changed|metadata=exact-optional-input)=(.*)$/.exec(line)?.[1];
    return path === undefined ? [] : [resolve(base, path)];
  }))].sort();
}

/** Supply Cargo with a content revision for the selected bake, then retain
 * its actual watches after success. File watches still serve direct Cargo. */
export function cargoBakeInputs(pkg, target, profile, env, app = null) {
  if (!pkg.targets.some(t => t.kind.includes('custom-build'))) return () => {};
  env.EXACT_BAKE_PACKAGE_DIR = realpathSync(dirname(pkg.manifest_path));
  if (env.EXACT_BAKED) return () => {};
  const key = createHash('sha256').update(JSON.stringify([pkg.manifest_path, target, profile])).digest('hex');
  const file = resolve(env.CARGO_TARGET_DIR, 'bake-inputs', key + '.json');
  let previous;
  try { previous = JSON.parse(readFileSync(file, 'utf8')); }
  catch (error) { if (error.code !== 'ENOENT' && !(error instanceof SyntaxError)) throw error; }
  if (previous?.version !== 2) previous = null;
  const outputs = [env.CARGO_TARGET_DIR, env.CARGO_BUILD_BUILD_DIR, env.EXACT_BAKE_OUTPUT, env.OUT_DIR].filter(Boolean).map(path => resolve(path));
  const digest = previous ? watchedDigest(previous.paths, [...outputs, ...previous.outputs]) : null;
  const inventory = app ? sourceInventory(app, [...outputs, ...(previous?.outputs ?? [])]).sha256 : null;
  // Keep the revision Cargo consumed when the bake discovers new watches.
  // A new revision must not be a content digest: inputs can change during Cargo,
  // then be restored to the pre-build bytes after Cargo consumed that revision.
  const revision = digest !== null && digest === previous.digest && inventory === previous.inventory ? previous.revision : randomUUID();
  env.EXACT_BAKE_INPUTS = revision;
  return messages => {
    const script = messages.find(m => m.reason === 'build-script-executed' && m.package_id === pkg.id);
    if (!script) return;
    const stdout = [resolve(script.out_dir, '../output'), resolve(script.out_dir, '../run/stdout')].find(existsSync);
    const paths = cargoInputPaths(readFileSync(stdout, 'utf8'), dirname(pkg.manifest_path));
    const outDirs = [...new Set(messages.filter(m => m.reason === 'build-script-executed').map(m => m.out_dir))].sort();
    const record = {version:2, paths, outputs:outDirs, digest:watchedDigest(paths, [...outputs, ...outDirs]),
      inventory:app ? sourceInventory(app, [...outputs, ...outDirs]).sha256 : null, revision};
    mkdirSync(dirname(file), {recursive:true});
    writeFileSync(`${file}.${process.pid}.tmp`, JSON.stringify(record));
    renameSync(`${file}.${process.pid}.tmp`, file);
  };
}

export function compilerPaths(text, workspace) {
  const first = text.replace(/\\\r?\n/g, '').split('\n')[0];
  const at = first.indexOf(': ');
  if (at < 0) throw new Error('rustc dep-info has no dependency rule');
  const paths = []; let word = '', escape = false;
  const dependencies = first.slice(at + 2);
  // Nearly every rule has no escape in it, and a receipt reads three hundred of them, each twice.
  if (!dependencies.includes('\\')) return dependencies.split(/\s+/).filter(Boolean).map(path => resolve(workspace, path));
  for (let index = 0; index < dependencies.length; index++) {
    const ch = dependencies[index];
    if (escape) { word += ch; escape = false; }
    // rustc escapes spaces in dep-info but leaves Windows path separators raw.
    else if (ch === '\\' && (process.platform !== 'win32' || /[\s#]/.test(dependencies[index + 1] ?? ''))) escape = true;
    else if (/\s/.test(ch)) { if (word) { paths.push(resolve(workspace, word)); word = ''; } }
    else word += ch;
  }
  if (escape) throw new Error('rustc dep-info has a truncated escape');
  if (word) paths.push(resolve(workspace, word));
  return paths;
}
// One file under two spellings (macOS's /tmp and /private/tmp, a linked
// checkout) is one file: a path that differs as written is compared as the
// file system resolves it (LLP 1143 D6), and only then, since a receipt reads
// hundreds of rules.
// rustc records relative inputs against Cargo's workspace root, even when
// EXACT_APP_DIR makes the invoking directory a nested app in that workspace.
export function unitDepInfo(message, workspace, metadata) {
  const realPaths = new Map();
  // A path the file system will not resolve (unreadable, a link loop) is compared as written.
  const real = path => { let r = realPaths.get(path); if (r === undefined) { try { r = outputPath(path); } catch { r = path; } realPaths.set(path, r); } return r; };
  const holds = (paths, want) => paths.includes(want) || paths.some(path => real(path) === real(want));
  // Cargo reports copied roots in target_directory and units in build_directory.
  // When the directories nest, the more specific root identifies the file.
  const targetDir = metadata && real(metadata.target_directory), buildDir = metadata && real(metadata.build_directory);
  const intermediate = file => metadata && under(targetDir, real(file))
      && (!under(buildDir, real(file)) || targetDir.length > buildDir.length)
    ? resolve(metadata.build_directory, relative(targetDir, real(file))) : file;
  const source = resolve(message.target.src_path);
  // rustc leaves the single output path raw, but escapes dependency paths.
  // In a build directory copied from another checkout (Cargo still finds its
  // units fresh), each .d file names itself at the same place under the old root.
  let builtAt;
  const names = (dep, candidate) => {
    const output = dep.slice(0, dep.indexOf(': ')), written = resolve(workspace, output);
    if (!holds([written], candidate) && !holds(compilerPaths('unit: ' + output, workspace), candidate)) {
      const tail = metadata && under(buildDir, real(candidate)) && sep + relative(buildDir, real(candidate));
      if (tail && written.endsWith(tail)) builtAt ??= written.slice(0, -tail.length);
      return false;
    }
    return holds(compilerPaths(dep, workspace), source);
  };
  const sameBytes = (product, file) => product === file || existsSync(product)
    && statSync(product).size === statSync(file).size && readFileSync(product).equals(readFileSync(file));
  for (const file of message.filenames) {
    const stem = basename(file).replace(/\.[^.]+$/, '').replace(/^lib/, '');
    // Selected libraries are copied out of deps; Cargo's sibling summary .d
    // omits env-dep rows. Read rustc's exact unit file, also on a cache hit,
    // when the product beside it is the reported file's bytes: another
    // toolchain building into this profile directory (the JS target's GPU
    // crate on stable; the web nightly's units sit under build/) leaves a
    // deps/<stem>.d for a unit this build did not produce.
    for (const candidate of [resolve(dirname(intermediate(file)), 'deps', stem + '.d'), resolve(dirname(intermediate(file)), stem + '.d')]) {
      if (!existsSync(candidate)) continue;
      if (names(readFileSync(candidate, 'utf8'), candidate) && sameBytes(resolve(dirname(candidate), basename(file)), file)) return candidate;
    }
  }
  // Cargo copies libraries and executables out of deps, and aliases build scripts
  // in their build directory. Match the copied bytes; refuse ambiguous evidence.
  const custom = message.target.kind?.includes('custom-build');
  // A cdylib (a game's GPU crate: libX.dylib, libX.so) is copied the same way.
  for (const file of message.filenames.filter(path => /\.(rlib|a|dylib|so)$/.test(path) || path === message.executable || custom)) {
    const directory = custom ? dirname(intermediate(file)) : resolve(dirname(intermediate(file)), 'deps');
    if (!existsSync(directory)) continue;
    const executable = custom || file === message.executable;
    const extension = executable ? (file.endsWith('.exe') ? '.exe' : '') : file.slice(file.lastIndexOf('.'));
    const stem = executable ? message.target.name.replaceAll('-', '_') : basename(file, extension);
    const bytes = readFileSync(file), matches = [];
    for (const name of readdirSync(directory)) {
      if (!name.startsWith(stem + '-') || (!executable && !name.endsWith(extension))) continue;
      const unit = executable ? (extension ? name.slice(0, -extension.length) : name) : name.slice(3, -extension.length);
      if (!/^[a-f0-9]+$/.test(unit.slice(unit.lastIndexOf('-') + 1))) continue;
      const product = resolve(directory, name);
      if (statSync(product).size !== bytes.length || !readFileSync(product).equals(bytes)) continue;
      const candidate = resolve(directory, unit + '.d');
      if (!existsSync(candidate)) continue;
      if (names(readFileSync(candidate, 'utf8'), candidate)) matches.push(candidate);
    }
    if (matches.length === 1) return matches[0];
    if (matches.length > 1) throw new Error(`ambiguous rustc unit dep-info for ${message.target.name}`);
  }
  // The pinned nightly puts units under build/<package>/<hash>/out,
  // reporting only the copied product. Watch archives use that layout too.
  const pkg = metadata?.packages?.find(p => p.id === message.package_id);
  for (const file of message.filenames.filter(path => /\.(wasm|a|rlib)$/.test(path))) {
    if (!pkg) continue;
    const directory = resolve(dirname(intermediate(file)), 'build', pkg.name);
    if (!existsSync(directory)) continue;
    const bytes = readFileSync(file), matches = [], extension = file.slice(file.lastIndexOf('.'));
    for (const hash of readdirSync(directory)) {
      if (!/^[a-f0-9]+$/.test(hash)) continue;
      const out = resolve(directory, hash, 'out');
      if (!existsSync(out)) continue;
      const stem = basename(file, extension);
      for (const name of readdirSync(out).filter(name => name === basename(file) || name === `${stem}-${hash}${extension}`)) {
        const product = resolve(out, name);
        const candidate = resolve(out, name.replace(/\.[^.]+$/, '.d').replace(extension === '.wasm' ? /^$/ : /^lib/, ''));
        if (!existsSync(candidate) || statSync(product).size !== bytes.length || !readFileSync(product).equals(bytes)) continue;
        if (names(readFileSync(candidate, 'utf8'), candidate)) matches.push(candidate);
      }
    }
    if (matches.length === 1) return matches[0];
    if (matches.length > 1) throw new Error(`ambiguous rustc unit dep-info for ${message.target.name}`);
  }
  const searched = metadata ? `; searched target directory ${metadata.target_directory}${buildDir !== targetDir ? ` and build directory ${metadata.build_directory}` : ''}` : '';
  const fix = builtAt ? `${metadata.build_directory} was built at ${builtAt}: delete it and build again` : 'rebuild the stale Cargo unit or use a private target directory';
  throw new Error(`no matching rustc unit dep-info for ${message.target.name}${searched}; ${fix}`);
}

/** Rust's canonical source and Cargo's package roots must use the same Windows spelling. */
export const sourcePath = (path, platform = process.platform) => platform === 'win32'
  ? path.replace(/^\\\\\?\\([A-Za-z]:\\)/, '$1').replace(/^\\\\\?\\UNC\\/i, '\\\\') : path;
