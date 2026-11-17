#!/usr/bin/env bun
// LLP 1172.019 §3; the census precedes the capability migration in LLP 1172.005 E11–E12.
import { readFileSync, readdirSync } from 'node:fs';
import { join, relative, resolve } from 'node:path';
import { classifications } from './completion-inventory-table.mjs';
import { swiftStringValue, swiftTokens } from './probes/seamcensus.mjs';

const ROOT = resolve(import.meta.dir, '..');

// A lexical census, not a Rust compiler: count the union of cfg branches and macro
// definitions, without expanding a macro once for every app that invokes it.
export function tokens(source) {
  const out = [];
  let i = 0, line = 1;
  function consume(end, value) {
    if (value !== undefined) out.push({ value, at: i, end, line });
    line += source.slice(i, end).split('\n').length - 1;
    i = end;
  }
  while (i < source.length) {
    if (/\s/.test(source[i])) { consume(i + 1); continue; }
    if (source.startsWith('//', i)) {
      const end = source.indexOf('\n', i);
      consume(end < 0 ? source.length : end); continue;
    }
    if (source.startsWith('/*', i)) {
      let end = i + 2, depth = 1;
      while (end < source.length && depth) {
        if (source.startsWith('/*', end)) { depth++; end += 2; }
        else if (source.startsWith('*/', end)) { depth--; end += 2; }
        else end++;
      }
      if (depth) throw new Error(`unterminated comment at line ${line}`);
      consume(end); continue;
    }
    const raw = /^(?:b|c)?r(#{0,255})"/.exec(source.slice(i));
    if (raw) {
      const close = `"${raw[1]}`, end = source.indexOf(close, i + raw[0].length);
      if (end < 0) throw new Error(`unterminated raw string at line ${line}`);
      consume(end + close.length, '<literal>'); continue;
    }
    const literal = /^(?:b|c)?"|^(?:b)?'(?:\\u\{[0-9a-fA-F]+\}|\\.|[^'\\\n])'/.exec(source.slice(i));
    if (literal) {
      if (!literal[0].endsWith('"')) { consume(i + literal[0].length, '<literal>'); continue; }
      let end = i + literal[0].length;
      while (end < source.length && source[end] !== '"') end += source[end] === '\\' ? 2 : 1;
      if (end >= source.length) throw new Error(`unterminated string at line ${line}`);
      const value = source.slice(i + literal[0].length, end);
      consume(end + 1, ['C', 'C-unwind', 'system'].includes(value) ? `"${value}"` : '<literal>');
      continue;
    }
    const word = /^(?:r#)?[A-Za-z_][A-Za-z_0-9]*|^\d+|^(?:->|::|\+=|-=|\*=|\/=|==|!=|=>)/.exec(source.slice(i));
    consume(i + (word?.[0].length ?? 1), word?.[0].replace(/^r#/, '') ?? source[i]);
  }
  return out;
}

function pairs(t) {
  const stack = [], paired = new Map(), close = { ')': '(', ']': '[', '}': '{' };
  for (let i = 0; i < t.length; i++) {
    const v = t[i].value;
    if (['(', '[', '{'].includes(v)) stack.push(i);
    else if (Object.hasOwn(close, v)) {
      const open = stack.pop();
      if (t[open]?.value !== close[v]) throw new Error(`unbalanced ${v} at line ${t[i].line}`);
      paired.set(open, i);
    }
  }
  if (stack.length) throw new Error('unclosed Rust delimiter');
  return paired;
}

export function rustFunctions(file, source) {
  if (/(?:^|\/)(?:tests?|[^/]*_tests)(?:\/|\.rs$)/.test(file)) return { functions: [], t: [], macros: [] };
  const t = tokens(source), paired = pairs(t), ignored = [], impls = [], functions = [], macros = [];
  const words = (a, b) => t.slice(a, b).map(t => t.value).join(' ');
  function generics(at) {
    if (t[at]?.value !== '<') return at;
    let depth = 0;
    do {
      if (t[at].value === '<') depth++;
      if (t[at].value === '>') depth--;
      at++;
    } while (at < t.length && depth);
    return at;
  }
  for (let i = 0; i < t.length; i++) {
    if (t[i].value === 'macro_rules' && t[i + 1]?.value === '!' && t[i + 3]?.value === '{') {
      macros.push({ name: t[i + 2].value, start: i + 3, end: paired.get(i + 3) });
    }
    if (words(i, i + 7) === '# [ cfg ( test ) ]') {
      let end = i + 7;
      while (end < t.length && ![';', '{', ',', '}'].includes(t[end].value)) {
        if (['(', '['].includes(t[end].value)) end = paired.get(end);
        end++;
      }
      ignored.push([i, paired.get(end) ?? end]);
    }
    if (t[i].value === 'impl') {
      let end = i + 1;
      while (end < t.length && !['{', ';'].includes(t[end].value)) end++;
      if (t[end]?.value !== '{') continue;
      let head = words(generics(i + 1), end).split(' where ')[0];
      if (head.includes(' for ')) head = head.split(' for ').at(-1);
      impls.push({ start: end, end: paired.get(end), owner: head.match(/^(?:\w+ :: )*(\w+)/)?.[1] ?? '?' });
    }
  }
  for (let i = 0; i < t.length - 2; i++) {
    if (t[i].value !== 'fn') continue;
    if (ignored.some(([a, b]) => a <= i && i <= b)) continue;
    let start = i;
    while (start && ![';', '{', '}'].includes(t[start - 1].value)) start--;
    const prefix = words(start, i), owner = impls.filter(s => s.start < i && i < s.end).at(-1)?.owner;
    if (t[i + 1].value === '$' && /\b(?:no_mangle|export_name|pub)\b/.test(prefix)) {
      throw new Error(`${file}:${t[i].line}: computed API name needs an explicit census parser`);
    }
    if (!/^[A-Za-z_]\w*$/.test(t[i + 1].value)) continue;
    let args = generics(i + 2);
    while (args < t.length && !['(', ';', '{'].includes(t[args].value)) args++;
    if (t[args]?.value !== '(') continue;
    const afterArgs = paired.get(args) + 1;
    let body = afterArgs, returnEnd;
    while (body < t.length && !['{', ';'].includes(t[body].value)) {
      if (t[body].value === 'where') returnEnd ??= body;
      if (['(', '['].includes(t[body].value)) body = paired.get(body);
      body++;
    }
    const end = paired.get(body) ?? body;
    const named = t.slice(start, i).findIndex((v, n, a) => v.value === 'export_name' && a[n + 1]?.value === '=');
    let exportName;
    if (named >= 0) {
      const literal = t[start + named + 2], raw = source.slice(literal.at, literal.end);
      const unquoted = /^r(#{0,255})"([\s\S]*)"\1$/.exec(raw);
      exportName = unquoted ? unquoted[2] : JSON.parse(raw);
    }
    functions.push({ file, line: t[i].line, name: t[i + 1].value, owner, exportName,
      public: /\bpub\b(?!\s*\()/.test(prefix),
      cExport: /\bextern(?: "(?:C(?:-unwind)?|system)"|$)/.test(prefix) && /\b(?:no_mangle|export_name)\b/.test(prefix),
      jni: prefix.includes('extern "system"'),
      signature: source.slice(t[i].at, t[body]?.at ?? source.length).trim(),
      returnType: t[afterArgs]?.value === '->' ? words(afterArgs + 1, returnEnd ?? body) : '',
      code: words(body + 1, end), start: i, end, body });
  }
  return { functions, t, macros, ignored };
}

function files(dir, extension = '.rs') {
  return readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))
    .flatMap(e => e.isDirectory() && !['target', 'node_modules', 'tests', 'fixtures'].includes(e.name)
      ? files(join(dir, e.name), extension) : e.isFile() && e.name.endsWith(extension) ? [join(dir, e.name)] : []);
}

export function swiftCExports(root = ROOT) {
  const exports = [];
  for (const path of files(join(root, 'host/apple'), '.swift')) {
    const source = readFileSync(path, 'utf8');
    if (!/@\s*_cdecl/.test(source)) continue;
    const t = swiftTokens(source), file = relative(root, path);
    for (let i = 0; i < t.length - 4; i++) {
      if (t[i].text !== '@' || t[i + 1].text !== '_cdecl' || t[i + 2].text !== '(') continue;
      const literal = t[i + 3];
      const name = swiftStringValue(source.slice(literal.at, literal.end));
      let fn = i + 5;
      while (fn < t.length && t[fn].text !== 'func') fn++;
      let body = fn;
      while (body < t.length && t[body].text !== '{') body++;
      if (body === t.length) throw new Error(`${file}:${t[i].line}: @_cdecl without a function body`);
      exports.push({ file, name, line: t[i].line,
        signature: source.slice(t[fn].at, t[body].at).trim() });
    }
  }
  return exports;
}

export function discover(root = ROOT) {
  const entries = [], runner = [], hosts = [];
  const hostDirs = readdirSync(join(root, 'host')).filter(n => /^web(?:-|$)/.test(n));
  for (const dir of ['runner/src', 'host/apple/src', 'host/render/src', 'host/painted/src', ...hostDirs.map(n => `host/${n}`)]) {
    for (const path of files(join(root, dir))) {
      const file = relative(root, path), source = readFileSync(path, 'utf8');
      const parsed = rustFunctions(file, source);
      if (dir === 'runner/src') runner.push({ file, source, ...parsed });
      else hosts.push({ file, source, ...parsed });
    }
  }
  // buildLeaf's lib templates are Rust source too. Retain the surrounding
  // newlines so diagnostics point at the template, not its generated file.
  for (const dir of hostDirs) {
    for (const path of files(join(root, 'host', dir), '.mjs').filter(p => !p.endsWith('.test.mjs'))) {
      const file = relative(root, path), source = readFileSync(path, 'utf8');
      for (const match of source.matchAll(/\blib\s*:\s*`((?:\\[\s\S]|[^\\`])*)`/g)) {
        const start = match.index + match[0].indexOf('`') + 1;
        const code = source.slice(0, start).replace(/[^\n]/g, ' ') + match[1].replace(/\\`/g, '`');
        hosts.push({ file, source: code, ...rustFunctions(file, code) });
      }
    }
  }
  function add(host, kind, f, suffix = '', extra = {}) {
    const name = f.exportName ?? f.name;
    const base = `${host}:${f.file}:${f.owner ? `${f.owner}::` : ''}${name}${suffix}`;
    const definitions = entries.filter(e => e.id === base || e.id.startsWith(`${base}#definition:`)).length;
    // cfg alternatives are separate source definitions of the same API.
    if (definitions) suffix += `#definition:${definitions + 1}`;
    entries.push({ id: `${host}:${f.file}:${f.owner ? `${f.owner}::` : ''}${name}${suffix}`,
      host, kind, file: f.file, line: f.line, symbol: `${f.owner ? `${f.owner}::` : ''}${name}${suffix}`,
      signature: f.signature, ...extra });
  }
  const methods = new Set();
  for (const { functions, t, file, ignored = [] } of runner) {
    for (const f of functions.filter(f => f.public)) {
      if (f.owner === 'Runner') methods.add(f.name);
      add('runner', f.owner ? 'public runner method' : 'public runner function', f, '', {
        returnsReceipt: /\bCommitReceipt\b/.test(f.returnType),
        returnsCommitAggregate: /\b(?:Advanced|Dispatched|Timed)\b/.test(f.returnType),
      });
    }
    // Arithmetic writes must not depend on an allocator's field name. Known
    // identity assignments also include seeds/restores; classify these separately.
    for (let i = 0; i < t.length - 1; i++) {
      const atomic = t[i + 1]?.value === '.' && ['fetch_add', 'try_update', 'fetch_update'].includes(t[i + 2]?.value);
      const member = t[i - 1]?.value === '.' && /^(?:[A-Za-z_]\w*|\d+)$/.test(t[i].value);
      let arithmeticAssignment = false;
      if (member && t[i + 1].value === '=') {
        let end = i + 2;
        while (end < t.length && t[end].value !== ';') end++;
        arithmeticAssignment = t.slice(i + 2, end).some(v =>
          ['+', '-', '*', '/', '%'].includes(v.value) || /^(?:checked|saturating|wrapping)_(?:add|sub)$/.test(v.value));
      }
      const counterWrite = member && (
        ['+=', '-='].includes(t[i + 1].value) || (t[i + 1].value === '=' &&
          (arithmeticAssignment || /^(?:next(?:_\w+)?|\w*(?:serial|sequence|generation)|seq|request)$/.test(t[i].value))));
      if (!atomic && !counterWrite) continue;
      if (ignored.some(([a, b]) => a <= i && i <= b)) continue;
      const f = functions.filter(f => f.start < i && i < f.end).at(-1);
      if (!f) continue;
      const counter = t[i].value + (atomic ? `.${t[i + 2].value}` : '');
      const ordinal = entries.filter(e => e.file === file && e.kind === 'identity write' && e.symbol.startsWith(`${f.owner ? `${f.owner}::` : ''}${f.name}#${counter}:`)).length + 1;
      add('runner', 'identity write', { ...f, line: t[i].line }, `#${counter}:${ordinal}`,
        { statement: t.slice(i, Math.min(t.length, i + 18)).map(t => t.value).join(' ') });
    }
  }
  // Apple re-exports runner-defined macros (frame health today). Count their
  // actual definitions under the consuming host as well as that host's own files.
  const imported = runner.flatMap(r => r.macros.map(m => ({ ...m,
    functions: r.functions.filter(f => f.cExport && m.start < f.start && f.end < m.end),
  }))).filter(m => m.functions.length);
  const importsSeen = new Set();
  for (const { file, t } of hosts) {
    const host = file.split('/')[1];
    for (const m of imported) {
      if (!t.some((v, i) => v.value === m.name && t[i + 1]?.value === '!')) continue;
      const key = `${host}:${m.name}`;
      if (importsSeen.has(key)) continue;
      importsSeen.add(key);
      for (const f of m.functions) add(host, 'C ABI export', f, '', { via: `${file}: ${m.name}!` });
    }
  }
  for (const { file, functions } of hosts) {
    const host = file.split('/')[1];
    for (const f of functions) {
      if (f.cExport) add(host, f.jni ? 'JNI export' : 'C ABI export', f);
      else if (['render', 'painted'].includes(host)) {
        const calls = [...f.code.matchAll(/\. (\w+) \(/g)].map(m => m[1]).filter(n => methods.has(n));
        const hostCalls = [...f.code.matchAll(/\bself \. host \. (\w+) \(/g)].map(m => m[1]).filter(n => methods.has(n));
        // These hosts have Rust interfaces, not a C ABI. Include private adapters
        // too: render's abort settlement is not a public Host method.
        if (f.public ||
            hostCalls.length || (calls.length && /\b(?:runner|Runner|runner_mut)\b/.test(`${f.signature} ${f.code}`))) {
          add(host, 'Rust host equivalent', f, '', { runnerCalls: [...new Set(calls)].sort() });
        }
      }
    }
  }
  for (const f of swiftCExports(root)) add('apple', 'Swift C ABI export', f);
  const seen = new Set();
  return entries.sort((a, b) => a.id.localeCompare(b.id)).map(e => {
    if (seen.has(e.id)) throw new Error(`duplicate inventory identity: ${e.id}`);
    seen.add(e.id); return e;
  });
}

export function inventory(root = ROOT, table = classifications) {
  const entries = discover(root), found = new Set(entries.map(e => e.id)), named = new Map(), errors = [];
  for (const row of table) {
    const protocol = row.protocol;
    if (!row.family || !row.lifetime || !['UI-owned', 'durable'].includes(row.class) || !row.evidence || row.inference !== true ||
        !Array.isArray(protocol) || !protocol.length || new Set(protocol).size !== protocol.length ||
        !protocol.every(p => ['one-shot', 'subscription', 'durable operation'].includes(p))) {
      errors.push(`incomplete classification: ${row.ids?.[0]}`);
    }
    for (const id of row.ids) {
      if (named.has(id)) errors.push(`classified twice: ${id}`);
      if (!found.has(id)) errors.push(`stale classification: ${id}`);
      named.set(id, row);
    }
  }
  for (const e of entries) if (!named.has(e.id)) errors.push(`unclassified: ${e.id} (${e.file}:${e.line})`);
  return { entries: entries.map(e => { const { ids, ...c } = named.get(e.id) ?? {}; return { ...e, ...c }; }), errors };
}

export function markdown(entries) {
  const hosts = [...new Set(entries.map(e => e.host))].sort();
  const families = [...new Set(entries.map(e => e.family))].sort();
  const summary = ['| Family | ' + hosts.join(' | ') + ' | Total |', '|---|' + hosts.map(() => '---:|').join('') + '---:|'];
  for (const family of families) {
    const es = entries.filter(e => e.family === family);
    summary.push(`| ${family} | ${hosts.map(h => es.filter(e => e.host === h).length).join(' | ')} | ${es.length} |`);
  }
  summary.push(`| Total | ${hosts.map(h => entries.filter(e => e.host === h).length).join(' | ')} | ${entries.length} |`);
  const cell = s => String(s ?? '').replace(/\|/g, '\\|').replace(/\n/g, ' ');
  return [
    'The census counts source definitions, including cfg alternatives and macro-defined exports, before app selection.',
    'Classifications are inferences about ownership, not an implemented refusal policy. The census includes queries, configuration and helper exports as well as deliveries.',
    'Generic reply paths name conditional durable ownership beside the UI continuation default. Durable here does not promise process recovery.',
    'Protocol lists name the conformance cases a door needs; a shared door lists each kind it serves. Synchronous helpers are one-shot uses. Durable-operation classifications do not establish crash recovery.',
    '', ...summary, '',
    '| Host / kind | Entry | Family | Protocol | Lifetime | Class (inference) | Evidence |',
    '|---|---|---|---|---|---|---|',
    ...entries.map(e => {
      const observed = [e.statement, e.via && `via ${e.via}`, e.runnerCalls?.length && `runner calls: ${e.runnerCalls.join(', ')}`].filter(Boolean).join('; ');
      return `| ${e.host} / ${e.kind} | ${e.symbol} (${e.file}:${e.line}) | ${e.family} | ${e.protocol?.join(', ')} | ${e.lifetime} | ${e.class} (inference)${e.durableWhen ? '; ' + cell(e.durableWhen) + ' (inference)' : ''} | ${cell(e.evidence)}${observed ? '; source: ' + cell(observed) : ''} |`;
    }),
  ].join('\n');
}

if (import.meta.main) {
  const args = process.argv.slice(2);
  if (args.some(a => !['--json', '--check', '--help'].includes(a))) throw new Error('usage: bun scripts/completion-inventory.mjs [--json|--check]');
  if (args.includes('--help')) console.log('bun scripts/completion-inventory.mjs [--json|--check]\nLists public runner-crate APIs, host C exports (including Rust lib templates), render/painted adapters and runner identity writes. Fails on missing or stale classifications.');
  else {
    const result = inventory();
    if (args.includes('--json')) console.log(JSON.stringify(result, null, 2));
    else if (!args.includes('--check')) console.log(markdown(result.entries));
    if (result.errors.length) { console.error(result.errors.join('\n')); process.exitCode = 1; }
    else if (args.includes('--check')) console.log(`${result.entries.length} entries classified`);
  }
}
