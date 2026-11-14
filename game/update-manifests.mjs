import { isDeepStrictEqual } from 'node:util';
import { basename, dirname, isAbsolute, resolve } from 'node:path';
import { existsSync, readFileSync, readdirSync, writeFileSync } from 'node:fs';
import { pathFrom } from '../scripts/app.mjs';

const ROOT = resolve(import.meta.dir, '..');
const hosts = ['exact-apple', 'exact-apple-update', 'exact-linux', 'exact-painted', 'exact-linux-update', 'exact-windows'];
const tableAt = (doc, keys) => keys.reduce((value, key) => value?.[key], doc);
const keyText = keys => keys.map(key => /^[\w-]+$/.test(key) ? key : JSON.stringify(key)).join('.');

export const nativeHosts = table => Object.entries(table).some(([name, value]) => hosts.includes(value?.package ?? name));
export function dependencyTables(doc, kinds = ['dependencies']) {
  return kinds.flatMap(kind => [
    [[kind], doc?.[kind]],
    ...Object.entries(doc?.target ?? {}).map(([target, value]) => [['target', target, kind], value[kind]]),
    [['workspace', kind], doc?.workspace?.[kind]],
  ]).filter(([, table]) => table);
}

/** String spans, including quoted keys. The parsed document below decides which ones are dependency paths. */
function strings(text) {
  const found = [];
  for (let i = 0; i < text.length;) {
    if (text[i] === '#') { const end = text.indexOf('\n', i); i = end < 0 ? text.length : end; continue; }
    const quote = ['"""', "'''", '"', "'"].find(q => text.startsWith(q, i));
    if (!quote) { i++; continue; }
    const start = i;
    i += quote.length;
    while (i < text.length) {
      if (text[i] === '\\' && quote[0] === '"') { i += 2; continue; }
      if (text.startsWith(quote, i)) {
        i += quote.length;
        if (quote.length === 3) while (text[i] === quote[0]) i++;
        break;
      }
      i++;
    }
    const literal = text.slice(start, i);
    let value; try { value = Bun.TOML.parse(`value = ${literal}`).value; } catch { continue; }
    found.push({ start, end: i, value, quote });
  }
  return found;
}

function replaceSpans(text, edits) {
  for (const { start, end, value } of edits.sort((a, b) => b.start - a.start)) text = text.slice(0, start) + value + text.slice(end);
  return text;
}

/** Mark candidate strings, then let TOML resolve their locations. This covers dotted keys, child tables and inline
 * parents without treating comments, quoted keys or unrelated strings as dependency declarations. */
function relocate(text, expected, from, crates, advice) {
  const paths = [];
  for (const [keys, table] of dependencyTables(expected, ['dependencies', 'build-dependencies', 'dev-dependencies'])) {
    for (const [name, dep] of Object.entries(table)) {
      const crate = dep?.package ?? name;
      if (typeof dep?.path !== 'string' || !(crate.startsWith('exact-') || crate === 'ibex2-sqlite') || !crates.has(crate)) continue;
      const path = pathFrom(from, crates.get(crate));
      if (dep.path !== path) paths.push({ keys: [...keys, name], old: dep.path, path });
    }
  }
  if (!paths.length) return text;
  let prefix = '__exact_update_path_';
  while (text.includes(prefix)) prefix += '_';
  const candidates = strings(text).filter(s => paths.some(p => p.old === s.value));
  const marked = Bun.TOML.parse(replaceSpans(text, candidates.map((s, i) => ({ ...s, value: JSON.stringify(prefix + i) }))));
  const edits = [];
  for (const { keys, path } of paths) {
    advice.push(`in ${keyText(keys)}, change: path = ${JSON.stringify(path)}`);
    const marker = tableAt(marked, keys)?.path;
    const span = candidates.find((_, i) => marker === prefix + i);
    if (!span) throw new Error('could not locate a dependency path');
    const value = path.includes(span.quote[0]) || path.includes('\\') ? JSON.stringify(path) : span.quote + path + span.quote;
    edits.push({ ...span, value });
    tableAt(expected, keys).path = path;
  }
  return replaceSpans(text, edits);
}

export function editManifest(text, from, crates, { prepared = text, workspaceProvider = false, rootProvider = false } = {}) {
  const before = Bun.TOML.parse(text), advice = [];
  let next = prepared;
  try {
    const preparedDoc = Bun.TOML.parse(next);
    if (!isDeepStrictEqual(dependencyTables(before, ['dependencies', 'build-dependencies', 'dev-dependencies']),
      dependencyTables(preparedDoc, ['dependencies', 'build-dependencies', 'dev-dependencies']))) throw new Error('preparation changed dependencies');
    const resolved = exact2Placeholders(next, from);
    next = resolved.text;
    const expected = Bun.TOML.parse(next);
    next = relocate(next, expected, from, crates, advice);
    const tables = dependencyTables(expected);
    if (rootProvider && !expected.workspace?.dependencies) {
      (expected.workspace ??= {}).dependencies = {};
      tables.push([['workspace', 'dependencies'], expected.workspace.dependencies]);
    }
    for (const [keys, table] of tables) {
      if ('ibex2-sqlite' in table || !(nativeHosts(table) || keys[0] === 'workspace' && rootProvider)) continue;
      const value = keys[0] !== 'workspace' && workspaceProvider ? 'workspace = true'
        : `path = ${JSON.stringify(pathFrom(from, crates.get('ibex2-sqlite')))}`;
      advice.push(`inside ${keyText(keys)}, add: ibex2-sqlite = { ${value} }`);
      table['ibex2-sqlite'] = Bun.TOML.parse(value);
      next = `${next.trimEnd()}\n\n[${keyText(keys)}.ibex2-sqlite]\n${value}\n`;
    }
    // An inline parent cannot be extended by a child header. Refuse that edit before any bytes reach the file.
    if (!isDeepStrictEqual(Bun.TOML.parse(next), expected)) throw new Error('the edit changed the manifest meaning');
    return { text: next, unresolved: resolved.unresolved };
  } catch (error) {
    throw new Error(`${advice.length ? advice.join('; ') : 'restore the Cargo tables and paths by hand'} (${error.message})`);
  }
}
/** `text`, a Cargo.toml in `from`, with each string whose text begins `<exact2>/` (exact2's own token: the docs
 * write a path into this checkout that way) naming that place in this checkout, written with `/` in the string's
 * own quotes. TOML's four string forms and its comments (a `#` outside every string, to the end of the line) are
 * told apart; escapes are not decoded, so a string that holds a backslash or a newline is not rewritten. `unresolved`:
 * a string left with the token at its start, or with the token and a backslash or a newline. Text that only
 * mentions the token is not a path. */
export function exact2Placeholders(text, from) {
  let out = '', at = 0, unresolved = false;
  for (let i = 0; i < text.length;) {
    if (text[i] === '#') { const end = text.indexOf('\n', i); i = end < 0 ? text.length : end; continue; }
    const quote = ['"""', "'''", '"', "'"].find(q => text.startsWith(q, i));
    if (!quote) { i++; continue; }
    // A basic string's backslash escapes the next character; a literal one's does not. A single-line string
    // ends at its line; a multiline one only at its triple quote.
    let end = i + quote.length;
    while (end < text.length && !text.startsWith(quote, end) && (quote.length === 3 || text[end] !== '\n')) end += quote[0] === '"' && text[end] === '\\' ? 2 : 1;
    const value = text.slice(i + quote.length, end).replace(quote.length === 3 ? /^\r?\n/ : /^$/, '');
    if (value.startsWith('<exact2>/') && !/[\\\n]/.test(value)) {
      const to = pathFrom(from, resolve(ROOT, value.slice('<exact2>/'.length)));
      out += text.slice(at, i) + (to.includes(quote[0]) || to.includes('\\') ? JSON.stringify(to) : quote + to + quote);
      at = end + quote.length;
    } else if (value.startsWith('<exact2>') || value.includes('<exact2>') && /[\\\n]/.test(value)) unresolved = true;
    i = end + quote.length;
  }
  return { text: out + text.slice(at), unresolved };
}

/** Validate each edited manifest before writing it; report all refusals together. */
export function updateManifests(dir, crates, rootText) {
  const changed = [], unresolved = [], refused = [];
  const manifests = readdirSync(dir, { recursive: true }).filter(f => basename(f) === 'Cargo.toml' && !/^(target|dist)\//.test(f))
    .map(file => {
      const path = resolve(dir, file), text = readFileSync(path, 'utf8');
      let doc; try { doc = Bun.TOML.parse(text); } catch {} // Cargo below reports invalid authored manifests with their paths.
      return { file, path, text, doc };
    });
  const root = manifests.find(m => m.path === resolve(dir, 'Cargo.toml'));
  const inherited = dependencyTables(root.doc).some(([keys, table]) => keys[0] === 'workspace' && nativeHosts(table)) ||
    manifests.some(m => dependencyTables(m.doc).some(([, table]) => table['ibex2-sqlite']?.workspace));
  const workspaceProvider = root.doc?.workspace?.dependencies?.['ibex2-sqlite'] !== undefined || inherited;
  for (const { file, path, text, doc } of manifests) {
    if (!doc) continue;
    try {
      const result = editManifest(text, dirname(path), crates, {
        prepared: path === root.path ? rootText ?? text : text, workspaceProvider, rootProvider: path === root.path && inherited,
      });
      if (result.unresolved) unresolved.push(file);
      if (result.text !== text) { writeFileSync(path, result.text); changed.push(file); }
    } catch (error) { refused.push(`${path}: left unchanged; ${error.message}`); }
  }
  if (refused.length) throw new Error(`update: edit these manifests by hand, then run update again:\n${refused.join('\n')}`);
  return { changed, unresolved };
}

/** Scan every target's local sources. External modules, generated code and unreadable sources keep the dependency. */
function usesLinux(android, doc, source, migrated) {
  try {
    const paths = [doc.lib, ...['bin', 'example', 'test', 'bench'].flatMap(kind => doc[kind] ?? [])].map(t => t?.path);
    if (typeof doc.package?.build === 'string') paths.push(doc.package.build);
    if (paths.some(path => path && (!path.endsWith('.rs') || !resolve(android, path).startsWith(android + '/') ||
      resolve(android, path).startsWith(resolve(android, 'target') + '/') || !existsSync(resolve(android, path))))) return true;
    const pending = [android];
    while (pending.length) {
      const dir = pending.pop();
      for (const entry of readdirSync(dir, { withFileTypes: true })) {
        const file = resolve(dir, entry.name);
        if (entry.isSymbolicLink()) return true;
        if (entry.isDirectory()) {
          if (dir !== android || entry.name !== 'target') pending.push(file);
        } else if (entry.name.endsWith('.rs')) {
          const text = file === source ? migrated : readFileSync(file, 'utf8');
          // Rust may load sources outside the crate or from OUT_DIR. Do not guess their module graph.
          if (/\bexact_linux\b|\bpath\s*=|\binclude\b/.test(text)) return true;
        }
      }
    }
    return false;
  } catch { return true; }
}

/** Rewrite only the old inline path declaration; authored Cargo forms get the complete manual edit instead. */
function androidCanvas(dir) {
  const manifest = resolve(dir, 'android/Cargo.toml'), source = resolve(dir, 'android/src/lib.rs');
  if (!existsSync(source)) return [];
  const text = readFileSync(source, 'utf8'), macro = /\bexact_linux\s*::\s*canvas_jni\s*!/g;
  if (!macro.test(text)) return [];
  const advice = `android/src/lib.rs: change exact_linux::canvas_jni! to exact_painted::canvas_jni!; android/Cargo.toml: add exact-painted = { path = ${JSON.stringify(pathFrom(resolve(dir, 'android'), resolve(ROOT, 'host/painted')))} } in the macro's dependency table (edit an existing entry instead of adding a second), and remove exact-linux only if no other code uses it`;
  if (!existsSync(manifest)) return [advice];
  const cargo = readFileSync(manifest, 'utf8');
  const entries = [...cargo.matchAll(/^exact-linux[ \t]*=[ \t]*\{[ \t]*path[ \t]*=[ \t]*("(?:\\.|[^"\\])*")[ \t]*\}[ \t]*$/gm)];
  let doc; try { doc = Bun.TOML.parse(cargo); } catch { return [advice]; }
  const tables = dependencyTables(doc).filter(([, table]) => table['exact-linux']);
  if (entries.length !== 1 || tables.length !== 1 || tables[0][0][0] === 'workspace' || (cargo.match(/exact-linux/g) ?? []).length !== 1 || cargo.includes('exact-painted')) return [advice];
  const old = tables[0][1]['exact-linux'];
  if (Object.keys(old).length !== 1 || typeof old.path !== 'string' || !old.path.endsWith('/host/linux')) return [advice];
  const entry = entries[0], keep = usesLinux(resolve(dir, 'android'), doc, source, text.replace(macro, ''));
  const dependency = `exact-painted = { path = ${JSON.stringify(pathFrom(resolve(dir, 'android'), resolve(ROOT, 'host/painted')))} }`;
  const next = cargo.slice(0, entry.index) + (keep ? entry[0] + '\n' : '') + dependency + cargo.slice(entry.index + entry[0].length);
  tables[0][1]['exact-painted'] = { path: pathFrom(resolve(dir, 'android'), resolve(ROOT, 'host/painted')) };
  if (!keep) delete tables[0][1]['exact-linux'];
  try { if (!isDeepStrictEqual(Bun.TOML.parse(next), doc)) return [advice]; } catch { return [advice]; }
  writeFileSync(manifest, next);
  writeFileSync(source, text.replace(macro, 'exact_painted::canvas_jni!'));
  return [`android/src/lib.rs now uses exact_painted::canvas_jni!; android/Cargo.toml ${keep ? 'keeps exact-linux and adds' : 'replaces exact-linux with'} exact-painted`];
}

/** Locate dependency values through parsed JSON so unrelated pairs and the rest of the file keep their bytes. */
function packagePaths(dir) {
  const file = resolve(dir, 'package.json');
  if (!existsSync(file)) return [];
  try {
    const text = readFileSync(file, 'utf8'), doc = Bun.JSONC.parse(text), paths = [];
    for (const group of ['dependencies', 'devDependencies', 'optionalDependencies']) {
      for (const [name, spec] of Object.entries(doc[group] ?? {})) {
        const module = /^@exact\/([a-z0-9-]+)$/.exec(name)?.[1];
        if (!module || typeof spec !== 'string' || !spec.startsWith('file:') || !isAbsolute(spec.slice(5))) continue;
        const target = resolve(ROOT, 'packages', module);
        if (!existsSync(resolve(target, 'package.json'))) continue;
        const next = 'file:' + pathFrom(dir, target);
        if (spec !== next) paths.push({ group, name, spec, next });
      }
    }
    if (!paths.length) return [];
    let prefix = '__exact_update_package_';
    while (text.includes(prefix)) prefix += '_';
    const spans = [...text.matchAll(/\/\/[^\r\n]*|\/\*[\s\S]*?\*\/|"(?:\\.|[^"\\])*"/g)].filter(m => m[0][0] === '"')
      .map(m => ({ start: m.index, end: m.index + m[0].length, value: JSON.parse(m[0]) }))
      .filter(s => paths.some(p => p.spec === s.value));
    const marked = Bun.JSONC.parse(replaceSpans(text, spans.map((s, i) => ({ ...s, value: JSON.stringify(prefix + i) }))));
    const edits = paths.map(p => {
      const span = spans.find((_, i) => marked[p.group]?.[p.name] === prefix + i);
      if (!span) throw new Error(`update: package.json: set ${p.group}.${p.name} to ${JSON.stringify(p.next)} and run bun install`);
      return { ...span, value: JSON.stringify(p.next) };
    });
    writeFileSync(file, replaceSpans(text, edits));
    return ['package.json: refreshed absolute @exact package paths; run bun install'];
  } catch (error) { return [`package.json: left unchanged; could not refresh @exact package paths (${error.message})`]; }
}

export const updateScaffold = dir => [...androidCanvas(dir), ...packagePaths(dir)];
