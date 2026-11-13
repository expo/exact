import { isDeepStrictEqual } from 'node:util';
import { basename, dirname, resolve } from 'node:path';
import { readFileSync, readdirSync, writeFileSync } from 'node:fs';
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
