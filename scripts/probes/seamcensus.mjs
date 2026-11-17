#!/usr/bin/env bun
// LLP 1172.019 §2–3: the Swift source census used by the probe and seam ratchets.
import { readdirSync, readFileSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';

// Strings (including their interpolations) and nested comments are opaque.
// Offsets and lines survive masking; braces in prose cannot create a scope.
export function swiftTokens(source) {
  const result = [];
  let at = 0, line = 1;
  const advance = end => { line += (source.slice(at, end).match(/\n/g) ?? []).length; at = end; };
  const stringAt = start => source.slice(start).match(/^(#*)("""|")/);
  function commentEnd(start) {
    let end = start + 2, depth = 1;
    while (end < source.length && depth) {
      if (source.startsWith('/*', end)) { depth++; end += 2; }
      else if (source.startsWith('*/', end)) { depth--; end += 2; }
      else end++;
    }
    return end;
  }
  function stringEnd(start, string) {
    const delimiter = string[2] + string[1], escape = '\\' + string[1];
    let end = start + string[0].length;
    while (end < source.length) {
      if (source.startsWith(delimiter, end)) return end + delimiter.length;
      if (source.startsWith(escape + '(', end)) {
        end += escape.length + 1;
        let depth = 1;
        while (end < source.length && depth) {
          const nested = stringAt(end);
          if (nested) end = stringEnd(end, nested);
          else if (source.startsWith('/*', end)) end = commentEnd(end);
          else if (source.startsWith('//', end)) {
            const newline = source.indexOf('\n', end);
            end = newline < 0 ? source.length : newline;
          } else {
            if (source[end] === '(') depth++;
            if (source[end] === ')') depth--;
            end++;
          }
        }
      } else if (source.startsWith(escape, end)) end += escape.length + 1;
      else end++;
    }
    return end;
  }
  while (at < source.length) {
    const tail = source.slice(at);
    if (/^\s/.test(tail)) { advance(at + 1); continue; }
    if (tail.startsWith('//')) { advance(source.indexOf('\n', at) < 0 ? source.length : source.indexOf('\n', at)); continue; }
    if (tail.startsWith('/*')) {
      advance(commentEnd(at)); continue;
    }
    const string = tail.match(/^(#*)("""|")/);
    if (string) {
      const end = stringEnd(at, string);
      result.push({ text: '<string>', at, end, line }); advance(end); continue;
    }
    const text = tail.match(/^(?:[A-Za-z_$][A-Za-z_0-9$]*|[0-9]+|->|===|!==|==|!=|&&|\|\||\?\?|[^\s])/)[0];
    result.push({ text, at, end: at + text.length, line }); advance(at + text.length);
  }
  const stack = [];
  for (let i = 0; i < result.length; i++) {
    const t = result[i]; t.parent = stack.at(-1) ?? -1;
    if (['{', '(', '['].includes(t.text)) stack.push(i);
    if (['}', ')', ']'].includes(t.text)) {
      const open = stack.pop();
      if (open !== undefined) { result[open].close = i; t.open = open; }
    }
  }
  return result;
}

export function swiftStringValue(literal) {
  const [, hashes, quote] = /^(#*)("""|")/.exec(literal);
  let value = literal.slice(hashes.length + quote.length, -hashes.length - quote.length);
  if (quote === '"""') {
    value = value.replace(/\r\n/g, '\n');
    const indent = /(?:^|\n)([ \t]*)$/.exec(value)?.[1] ?? '';
    value = value.slice(1).replace(/\n[ \t]*$/, '').split('\n')
      .map(line => line.startsWith(indent) ? line.slice(indent.length) : line).join('\n');
  }
  const escapes = { '0': '\0', t: '\t', n: '\n', r: '\r', '"': '"', "'": "'", '\\': '\\' };
  const escape = new RegExp('\\\\' + hashes + '(u\\{([0-9a-fA-F]+)\\}|[0tnr"\'\\\\]|\\n[ \\t]*)', 'g');
  return value.replace(escape, (_, code, scalar) => scalar ? String.fromCodePoint(parseInt(scalar, 16))
    : code.startsWith('\n') ? '' : escapes[code]);
}

export function seamCensus({ root = 'host/apple', args = [], countsOnly = false } = {}) {
  if (args.includes('--help')) {
    console.log(`bun scripts/probes/seamcensus.mjs > source.json
  bun scripts/probes/seamcensus.mjs --diagnostics build.log [other.log ...] > errors.json
  bun scripts/probes/seamcensus.mjs --private [all|bridge|presenter] > inventory.json

  Use --private only on a detached scratch commit. It edits five Swift sources.
  The default source census is read-only and includes inactive platform branches.
  The JSON contains per-site rows, filename-based subsystem totals and limitations.`);
    process.exit(0);
  }
  const read = path => readFileSync(path, 'utf8');
  function files(dir) {
    return readdirSync(dir, { withFileTypes: true }).flatMap(entry =>
      entry.isDirectory() ? files(join(dir, entry.name)) : entry.name.endsWith('.swift') ? [join(dir, entry.name)] : []);
  }

  function declarations(path) {
    const source = read(path), ts = swiftTokens(source), found = [];
    const modifiers = new Set(['private', 'fileprivate', 'internal', 'package', 'public', 'open',
      'static', 'class', 'override', 'final', 'required', 'convenience', 'dynamic', 'optional',
      'lazy', 'weak', 'unowned', 'nonisolated', 'isolated', 'mutating', 'nonmutating',
      'borrowing', 'consuming', 'distributed', 'prefix', 'postfix', 'infix']);
    for (let i = 0; i < ts.length; i++) {
      if (!['class', 'extension'].includes(ts[i].text) || !['NodeView', 'Presenter', 'Runtime'].includes(ts[i + 1]?.text)) continue;
      const owner = ts[i + 1].text;
      let body = i + 2;
      while (body < ts.length && ts[body].text !== '{') body++;
      const close = ts[body]?.close;
      if (close === undefined) continue;
      for (let j = body + 1; j < close; j++) {
        if (ts[j].parent !== body || !['var', 'let', 'func'].includes(ts[j].text)) continue;
        let start = j;
        while (start > body + 1) {
          let previous = start - 1;
          if (ts[previous].text === ')') previous = (ts[previous].open ?? previous) - 1;
          if (ts[previous - 1]?.text === '@') start = previous - 1;
          else if (modifiers.has(ts[previous]?.text)) start = previous;
          else break;
        }
        const prefix = ts.slice(start, j).map(t => t.text).join(' ');
        let end = j + 2;
        while (end < close) {
          if (ts[j].text === 'func') {
            if (ts[end].parent === body && ['{', ';'].includes(ts[end].text)) break;
          } else if (ts[end].text === '{' || (ts[end].line > ts[j].line && ts[end].parent === body)) break;
          end++;
        }
        const header = ts.slice(j, end).map(t => t.text).join(' ');
        const stored = ts[j].text !== 'func' && (ts[end]?.text !== '{' ||
          ['didSet', 'willSet'].includes(ts[end + 1]?.text) || header.includes(' = '));
        if (stored && /\b(static|class|override)\b/.test(prefix)) continue;
        // A closure-valued property is stored; computed getters have no initializer.
        const declaration = ts.slice(j, end);
        const returnArrow = declaration.findIndex(t => t.text === '->' && t.parent === body);
        const where = declaration.findIndex((t, k) => k > returnArrow && t.text === 'where' && t.parent === body);
        const returnType = returnArrow < 0 ? '' : declaration.slice(returnArrow + 1, where < 0 ? undefined : where).map(t => t.text).join(' ');
        const returns = ts[j].text === 'func' && returnType !== '' && !/^(Void|\(\s*\))$/.test(returnType);
        const names = [ts[j + 1].text];
        if (stored) {
          for (let k = j + 2; k < end; k++) {
            if (ts[k].parent === body && ts[k].text === ',' && /^[A-Za-z_]\w*$/.test(ts[k + 1]?.text ?? '')) names.push(ts[k + 1].text);
          }
        }
        found.push({ path, owner, names, line: ts[j].line, start: ts[start].at, at: ts[j].at,
          prefix, stored, returns, header, async: declaration.slice(0, returnArrow < 0 ? undefined : returnArrow)
            .some(t => t.text === 'async' && t.parent === body) });
      }
    }
    return found;
  }

  const primary = [
    `${root}/Sources/ExactKit/IOS/NodeViewIOS.swift`, `${root}/Sources/ExactKit/Mac/NodeViewMac.swift`,
    `${root}/Sources/ExactKit/IOS/PresenterIOS.swift`, `${root}/Sources/ExactKit/Mac/PresenterMac.swift`,
    `${root}/Sources/ExactCore/Bridge.swift`,
  ];
  const inventory = primary.flatMap(declarations).filter(d =>
    (d.owner === 'NodeView' && d.stored) || (d.owner === 'Presenter' && d.names.includes('views')) ||
    (d.owner === 'Runtime' && d.returns && !/\bprivate\b/.test(d.prefix)));

  if (args[0] === '--private') {
    const group = args[1] ?? 'all';
    for (const path of primary) {
      const selected = inventory.filter(d => d.path === path && (group === 'all' ||
        (group === 'bridge' ? d.owner === 'Runtime' : d.owner !== 'Runtime')) &&
        !/\bprivate\b/.test(d.prefix.replace(/private\s*\(\s*set\s*\)/g, '')));
      let source = read(path);
      for (const d of selected.sort((a, b) => b.at - a.at)) {
        const prefix = source.slice(d.start, d.at).replace(/\b(package|public|internal|fileprivate)\s+/g, '')
          .replace(/private\s*\(\s*set\s*\)\s*/g, '');
        source = source.slice(0, d.start) + 'private ' + prefix + source.slice(d.at);
      }
      writeFileSync(path, source);
    }
    console.log(JSON.stringify(inventory, null, 2));
    process.exit(0);
  }

  function subsystem(path) {
    const name = path.split('/').at(-1);
    if (/Agent|Inspect|Tamber|Trace|Perf/.test(name)) return 'driver and inspection';
    if (/Anchor|Color|Tint|Paint|Exit|Gone|Ghost|Overflow|Clip|Layer|Backdrop|Grid/.test(name)) return 'anchors, inheritance, paint and lifetime';
    if (/Control|Group|Form|Field|Toggle|Picker|Radio|Button|Keyboard|TextInput/.test(name)) return 'controls and input';
    if (/Accessib|Focus|Key|Responder|Inert|Press|Touch|Gesture|Swipe|Drag|Mouse/.test(name)) return 'focus, accessibility and gestures';
    if (/Scroll|List|Cell|Collection|Pool|Reorder|Refresh/.test(name)) return 'scrolling and collections';
    if (/Navigat|Route|Tab|Split|Menu|Popover|Dialog|Modal|Sheet|Toolbar|Bar|Command|Placement/.test(name)) return 'navigation and presentation';
    if (/Text|Inline|Raster|Font|Selection|Markdown/.test(name)) return 'text and selection';
    if (/Image|Picture|Canvas|Metal|Video|Sound|Web|SVG|Surface|Media/.test(name)) return 'media and surfaces';
    return 'runtime, presenter and other';
  }

  function diagnostics(paths) {
    const unique = new Map(), raw = [];
    for (const path of paths) {
      for (const match of read(path).matchAll(/^(.+?\.swift):(\d+):(\d+): error: (.+)$/gm)) {
        const file = match[1].replace(process.cwd() + '/', '');
        const message = match[4];
        const symbol = message.match(/'([^']+)' is inaccessible/)?.[1] ?? '(cascade or other)';
        const row = { path: file, line: +match[2], column: +match[3], symbol, message, subsystem: subsystem(file), log: path };
        raw.push(row); unique.set(JSON.stringify([file, row.line, row.column, message]), row);
      }
    }
    const rows = [...unique.values()];
    const count = key => Object.fromEntries([...new Set(rows.map(r => r[key]))].sort().map(k => [k, rows.filter(r => r[key] === k).length]));
    return { raw: raw.length, unique: rows.length, byFile: count('path'), bySymbol: count('symbol'), bySubsystem: count('subsystem'), rows };
  }
  if (args[0] === '--diagnostics') {
    console.log(JSON.stringify(diagnostics(args.slice(1)), null, 2)); process.exit(0);
  }

  const stored = new Set(inventory.filter(d => d.owner === 'NodeView').flatMap(d => d.names));
  const sourceFiles = files(`${root}/Sources`);
  const casts = [], walks = [], accesses = [], typeTests = [];
  for (const path of sourceFiles) {
    const source = read(path), ts = swiftTokens(source);
    const text = (a, b) => ts.slice(a, b).map(t => t.text).join(' ');
    const braceParent = index => {
      let p = ts[index]?.parent ?? -1;
      while (p >= 0 && ts[p].text !== '{') p = ts[p].parent;
      return p;
    };
    // A named function or property bounds the local data-flow approximation.
    const member = index => {
      let p = braceParent(index), nearest = p;
      while (p >= 0) {
        let start = p - 1;
        while (start >= 0 && !['{', '}', ';'].includes(ts[start].text) && ts[start].line >= ts[p].line - 12) start--;
        const header = text(start + 1, p);
        if (/\b(func|var|let|init|subscript)\b/.test(header) &&
            !/\b(class|extension|struct|enum|if|while|guard|for|switch|catch|else|do|repeat)\b/.test(header)) return p;
        p = braceParent(p);
      }
      return nearest;
    };
    const groups = new Map();
    const castEnds = new Map();
    for (let i = 0; i < ts.length; i++) {
      const type = ts[i].text === 'is' ? i + 1
        : ts[i].text === 'as' && ['?', '!'].includes(ts[i + 1]?.text) ? i + 2 : -1;
      if (type < 0) continue;
      let name = type;
      while (ts[name]?.text === '(') name++;
      let end = name, open = name - 1;
      while (open >= type && ts[open].close === end + 1) { end++; open--; }
      if (ts[name]?.text === 'NodeView' && open < type) {
        castEnds.set(i, end);
        const m = member(i);
        if (!groups.has(m)) groups.set(m, []);
        groups.get(m).push(i);
      }
    }
    for (const [scope, sites] of groups) {
      const stop = scope < 0 ? 0 : ts[scope].close ?? ts.length;
      const tree = new Set(), node = new Set(), definitions = [], calls = new Set();
      if (!countsOnly) {
        for (let j = scope + 1; j < stop; j++) {
          if (['let', 'var'].includes(ts[j].text) && /^[A-Za-z_$]\w*$/.test(ts[j + 1]?.text ?? '')) {
            let equal = j + 2;
            while (equal < stop && ts[equal].line === ts[j].line && !['=', '{', ';'].includes(ts[equal].text)) equal++;
            if (ts[equal]?.text !== '=') continue;
            let end = equal + 1;
            while (end < stop && ![';', '}'].includes(ts[end].text) && ts[end].line === ts[equal + 1]?.line) end++;
            definitions.push({ name: ts[j + 1].text, from: equal + 1, to: end });
          }
          // Reassignment is how ancestor walks usually advance their cursor.
          if (/^[A-Za-z_$]\w*$/.test(ts[j].text) && ts[j + 1]?.text === '=' && ts[j - 1]?.text !== '.') {
            let end = j + 2;
            while (end < stop && ![';', '}'].includes(ts[end].text) && ts[end].line === ts[j + 2]?.line) end++;
            definitions.push({ name: ts[j].text, from: j + 2, to: end });
          }
        }
        for (let round = 0; round < 8; round++) {
          for (const def of definitions) {
            const rhs = ts.slice(def.from, def.to).map(t => t.text);
            if (rhs.some(t => t === 'superview' || t === 'subviews' || tree.has(t))) tree.add(def.name);
            if (/\bas [?!] NodeView\b/.test(rhs.join(' ')) || rhs.some(t => node.has(t))) node.add(def.name);
          }
        }
      }
      const edges = [];
      for (let j = scope + 1; j < stop; j++) {
        if (['superview', 'subviews'].includes(ts[j].text) && !['var', 'let', 'func'].includes(ts[j - 1]?.text)) edges.push(j);
      }
      for (const i of sites) {
        let start = i - 1;
        while (start > scope && ts[start - 1].line === ts[i].line && ![';', '{', '}', '=', ',', '&&', '||'].includes(ts[start - 1].text)) start--;
        const expression = ts.slice(start, i).map(t => t.text);
        const direct = expression.includes('where') ? null :
          expression.includes('superview') ? 'superview' : expression.includes('subviews') ? 'subviews' : null;
        const derived = !!direct || expression.some(t => tree.has(t)) ||
          (expression.includes('$0') && edges.some(e => ts[e].text === 'subviews'));
        const row = { path, line: ts[i].line, column: ts[i].at - source.lastIndexOf('\n', ts[i].at),
          subsystem: subsystem(path), cast: ts[i].text === 'is' ? 'is' : 'as' + ts[i + 1].text,
          direct, treeDerived: derived, scopeLine: ts[scope]?.line ?? 0,
          expression: source.slice(ts[start].at, ts[castEnds.get(i)].end) };
        if (row.cast === 'is') typeTests.push(row); else casts.push(row);
      }
      // These are candidates: an edge and a NodeView cast in one member need
      // inspection when neither a direct expression nor a cursor ties them.
      for (const j of edges) walks.push({ path, line: ts[j].line, offset: ts[j].at, edge: ts[j].text,
        subsystem: subsystem(path), scopeLine: ts[scope]?.line ?? 0 });
      if (!countsOnly) for (let j = scope + 1; j < stop; j++) {
        if (!stored.has(ts[j].text) || ts[j - 1]?.text !== '.') continue;
        let receiver = j - 2;
        while (['?', '!'].includes(ts[receiver]?.text)) receiver--;
        if (!node.has(ts[receiver]?.text)) continue;
        const after = ts[j + 1]?.text;
        accesses.push({ path, line: ts[j].line, offset: ts[j].at, symbol: ts[j].text, receiver: ts[receiver].text,
          subsystem: subsystem(path), treeDerived: tree.has(ts[receiver].text),
          usage: after === '=' ? 'write' : after === '[' ? 'subscript read or write' : 'read', scopeLine: ts[scope]?.line ?? 0 });
      }
    }
  }
  function distinct(rows, key) { return [...new Map(rows.map(r => [key(r), r])).values()]; }
  const edgeRows = distinct(walks, r => r.path + ':' + r.offset);
  const accessRows = distinct(accesses, r => r.path + ':' + r.offset);
  const systems = [...new Set([...casts, ...edgeRows, ...accessRows].map(r => r.subsystem))].sort();
  const summary = systems.map(subsystem => {
    const c = casts.filter(r => r.subsystem === subsystem), w = edgeRows.filter(r => r.subsystem === subsystem);
    return { subsystem, casts: c.length, files: new Set(c.map(r => r.path)).size,
      directSuperview: c.filter(r => r.direct === 'superview').length,
      directSubviews: c.filter(r => r.direct === 'subviews').length,
      treeDerivedCasts: c.filter(r => r.treeDerived).length,
      superviewCandidates: w.filter(r => r.edge === 'superview').length,
      subviewsCandidates: w.filter(r => r.edge === 'subviews').length,
      storedAccessCandidates: accessRows.filter(r => r.subsystem === subsystem).length };
  });
  return { inventory, scope: `${root}/Sources/**/*.swift`, summary, casts,
    typeTests, walks: edgeRows, storedAccesses: accessRows,
    limits: [
      'Lexical brace scopes, not a Swift AST or type checker; conditional compilation is counted in source, once.',
      'Strings and interpolations are masked; regex literals and generated Swift outside Sources are not understood.',
      'Eight local name-propagation passes over a member; aliases can shadow or be overwritten. Candidates are not exact paths.',
      'Edges co-occurring with NodeView casts or type tests are candidates, including unrelated edges in that member.',
      'Interprocedural walks, protocol/existential or Objective-C dispatch, key paths, returned aliases and custom ancestor helpers can escape.',
      'Stored-state reads cover recognized local cast aliases only; computed accessors, self, closure results and subscripts can escape.',
      'A subscript access can read or write. Visibility errors do not make a cast or a traversal illegal; do not call its whole member compiled.',
      'Subsystems are filename-based groupings in first-match order; per-file rows are the reproducible authority.',
    ] };
}

if (import.meta.main) console.log(JSON.stringify(seamCensus({ args: process.argv.slice(2) }), null, 2));
