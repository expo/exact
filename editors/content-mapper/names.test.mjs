import { test, expect } from 'bun:test';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { dirname, relative, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { open } from './test/lsp.mjs';

// Importing mapper.test.mjs for its helpers would run its tests a second time, so this file has its own.
const tsc = process.env.EXACT_TSC71, root = resolve(import.meta.dir, '../..');
const contract = process.env.EXACT_CONTRACT ?? ['target/release/contract', 'target/debug/contract'].map(p => resolve(root, p)).find(existsSync) ?? 'contract';
const ready = () => { if (!tsc || !existsSync(tsc)) { console.log('skipped: set EXACT_TSC71 to TypeScript 7.1\'s tsc'); return false; } return true; };
const starter = `shape Greeting
  text: string

routes nav
  home "/"

component Hello
  resource greeting = greeting("Hello Exact") as shape Greeting
  state liked = false
  action like
    liked = not liked
  view
    main
      button press=like aria-label=(liked ? "Unlike" : "Like") testId="like"
        text (liked ? "on" : "off")
      text greeting.text testId="greeting"
`;

/** A project of `.contract` files, `files` keyed by path, that TypeScript reads through the mapper. */
function project(files) {
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/content-mapper-names-'));
  mkdirSync(resolve(dir, 'node_modules/@exact'), { recursive: true });
  symlinkSync(import.meta.dir, resolve(dir, 'node_modules/@exact/contract-content-mapper'));
  for (const [name, text] of Object.entries(files)) { mkdirSync(dirname(resolve(dir, name)), { recursive: true }); writeFileSync(resolve(dir, name), text); }
  writeFileSync(resolve(dir, 'tsconfig.json'), JSON.stringify({ compilerOptions: { noEmit: true, strict: true, skipLibCheck: true }, contentMappers: [{ package: '@exact/contract-content-mapper', extensions: ['.contract'] }], include: Object.keys(files).filter(f => f.endsWith('.contract')) }));
  return { dir, file: name => resolve(dir, name), end: () => rmSync(dir, { recursive: true, force: true }) };
}
/** An app's own `.contract` sources, as `contract sources` names them, keyed by path from the app's directory, and its
 * `app.json`, which declares words the sources use. */
function app(name) {
  const main = resolve(root, 'apps', name, 'app.contract'), sources = run(['sources', main]).sources;
  return Object.fromEntries([...sources.filter(s => s.origin === 'app' && s.path.endsWith('.contract')).map(s => s.path), resolve(dirname(main), 'app.json')].map(path => [relative(dirname(main), path), readFileSync(path, 'utf8')]));
}
const run = args => JSON.parse(Bun.spawnSync([contract, ...args], { stdout: 'pipe', stderr: 'pipe' }).stdout.toString());
const symbols = file => run(['symbols', file]);
const errors = file => run(['build', file, '--json', '--errors-only']);

// The compiler's columns count UTF-8 bytes (proved below); LSP's count UTF-16 units.
const text = file => readFileSync(file, 'utf8').split('\n');
const u16 = (file, line, col) => Buffer.from(text(file)[line - 1]).subarray(0, col - 1).toString().length + 1;
/** A symbol's place as LSP gives it, `[file, line, start, end]`, or why TypeScript cannot be shown it. */
function where(s) {
  if (!(s.line > 0 && s.col > 0)) return { skip: `no source position (${s.file.endsWith('.json') ? 'a package manifest' : s.file})` };
  if (!s.file.endsWith('.contract')) return { skip: `in ${s.file}` };
  const start = u16(s.file, s.line, s.col), end = u16(s.file, s.line, s.end_col), authored = text(s.file)[s.line - 1].slice(start - 1, end - 1);
  if (authored === s.name && /^[A-Za-z_$][\w$]*$/.test(s.name)) return { at: [s.file, s.line, start, end] };
  if (authored === JSON.stringify(s.name)) return { at: [s.file, s.line, start, end], quoted: true };
  return { skip: `${s.kind} \`${s.name}\`: the compiler's span is \`${authored}\`` };
}

async function sweep(files) {
  const p = project(files), f = p.file('app.contract'), server = await open(tsc, p.dir), truth = symbols(f);
  const tally = { checked: 0, matched: 0, skipped: {}, mismatched: [] }, skip = why => { tally.skipped[why] = (tally.skipped[why] ?? 0) + 1; };
  try {
    server.openFile(f);
    const here = truth.references.filter(r => r.file === f), same = (a, b) => a.line === b.line && a.col === b.col && a.end_col === b.end_col;
    for (const r of here) {
      tally.checked++;
      const use = where(r), target = where(truth.definitions[r.to]);
      if (!use.at) { skip(`use ${use.skip}`); continue; }
      const got = (await server.definition(f, use.at[1], use.at[2])).sort(order);
      if (!target.at) { skip(`definition ${target.skip}`); if (got.length) tally.mismatched.push({ use: use.at, expected: [], got }); continue; }
      // The compiler may say one use is of several definitions: TypeScript must give them all.
      const expected = here.filter(o => same(o, r)).map(o => where(truth.definitions[o.to]).at).sort(order);
      if (JSON.stringify(got) === JSON.stringify(expected)) tally.matched++;
      else tally.mismatched.push({ use: use.at, name: r.name, expected, got });
    }
  } finally { server.close(); p.end(); }
  return tally;
}
const report = (name, t) => console.log(`${name}: ${t.checked} references checked, ${t.matched} matched, ${t.checked - t.matched - t.mismatched.length} skipped, ${t.mismatched.length} mismatched`, t.skipped, t.mismatched.length ? t.mismatched : '');

for (const name of ['caltrain', 'realworld', 'shelf'])
  test(`every reference in apps/${name}/app.contract goes to the definition the Contract compiler names`, async () => {
    if (!ready()) return;
    const t = await sweep(app(name)); report(`apps/${name}/app.contract`, t);
    expect(t.mismatched).toEqual([]);
    expect(t.matched).toBeGreaterThan(0);
  }, 300_000);

test('every reference in the starter goes to the definition the Contract compiler names', async () => {
  if (!ready()) return;
  const t = await sweep({ 'app.contract': starter }); report('starter', t);
  expect(t.mismatched).toEqual([]);
  expect(t.matched).toBe(t.checked);
}, 60_000);

/** Hovers each definition the compiler lists in the app's `app.contract`: its first line is the name and the
 * compiler's type, and its kind where the compiler gives none. */
async function hovers(files) {
  const p = project(files), f = p.file('app.contract'), server = await open(tsc, p.dir), { definitions } = symbols(f);
  const tally = { listed: 0, typed: 0, untyped: {}, unplaced: {}, wrong: [] }, count = (o, why) => { o[why] = (o[why] ?? 0) + 1; };
  try {
    server.openFile(f);
    for (const d of definitions.filter(d => d.file === f)) {
      tally.listed++;
      const at = where(d);
      if (!at.at) { count(tally.unplaced, `${d.kind}: ${at.skip.replace(/`[^`]*`/g, '…')}`); continue; }
      const hover = await server.hover(f, at.at[1], at.at[2] + (at.quoted ? 1 : 0)), first = hover?.split('\n')[1] ?? '';
      const expected = `(property) ${at.quoted ? JSON.stringify(d.name) : d.name}: ${d.type ?? 'unknown'}`;
      if (d.type === undefined) count(tally.untyped, `${d.kind}: the compiler gives no type`);
      else tally.typed++;
      // TypeScript writes an object type over several lines, so its first line holds only `{`.
      // What the compiler gives no type reads as its kind, a component as its members (names.js `untyped`).
      const kind = d.kind === 'component' ? /^(\{|unknown)$/ : /^(Route|Source|TestId|Id|Style|Task|Font|Keyframes|Atom|Decoration|unknown)$/, head = `(property) ${at.quoted ? JSON.stringify(d.name) : d.name}: `;
      const shown = d.type === undefined ? first.startsWith(head) && kind.test(first.slice(head.length)) : d.type.startsWith('{ ') ? first === `(property) ${d.name}: {` : first === expected;
      if (!shown || (d.type !== undefined && /\bunknown\b/.test(hover.split('```')[1]))) tally.wrong.push({ d: [d.kind, d.name, d.line, d.col, d.type], hover });
    }
  } finally { server.close(); p.end(); }
  return tally;
}

for (const name of ['caltrain', 'realworld', 'starter'])
  test(`each definition in ${name === 'starter' ? 'the starter' : `apps/${name}/app.contract`} hovers with the type the Contract compiler gives it`, async () => {
    if (!ready()) return;
    const t = await hovers(name === 'starter' ? { 'app.contract': starter } : app(name));
    console.log(`${name}: ${t.listed} definitions, ${t.typed} with a type, ${t.listed - t.typed} without`, { untyped: t.untyped, unplaced: t.unplaced });
    expect(t.wrong).toEqual([]);
    expect(t.typed).toBeGreaterThan(0);
  }, 300_000);

test('the compiler\'s columns are UTF-8 bytes, and a name after non-ASCII text maps to its place', async () => {
  if (!ready()) return;
  const source = 'component A\n  state n = 0\n  view\n    text `é😀${n}` testId="é😀x"\n';
  const p = project({ 'app.contract': source }), f = p.file('app.contract'), server = await open(tsc, p.dir);
  try {
    // In `    text \`é😀${n}\``, `n` is UTF-16 column 16, code point 15 and byte 19 (é is 2 bytes, 😀 is 4 and 2 UTF-16 units).
    const { definitions, references } = symbols(f);
    expect(references).toEqual([expect.objectContaining({ name: 'n', line: 4, col: 19, end_col: 20 })]);
    expect(definitions.find(d => d.kind === 'testId')).toMatchObject({ name: 'é😀x', line: 4, col: 30, end_col: 39 });
    server.openFile(f);
    expect(await server.definition(f, 4, 16)).toEqual([[f, 2, 9, 10]]);
    expect(await server.references(f, 2, 9)).toEqual([[f, 2, 9, 10], [f, 4, 16, 17]]);
    expect(await server.hover(f, 4, 28)).toContain('testId `é😀x`');
  } finally { server.close(); p.end(); }
}, 60_000);

test('two definitions of one name in different scopes stay apart', async () => {
  if (!ready()) return;
  const source = 'component One\n  state count = "a"\n  view\n    text count\n\ncomponent Two\n  state count = "b"\n  view\n    text count\n\ncomponent App\n  view\n    column\n      One()\n      Two()\n';
  const p = project({ 'app.contract': source }), f = p.file('app.contract'), server = await open(tsc, p.dir);
  try {
    expect(errors(f)).toEqual([]);
    server.openFile(f);
    expect(await server.definition(f, 4, 10)).toEqual([[f, 2, 9, 14]]);
    expect(await server.definition(f, 9, 10)).toEqual([[f, 7, 9, 14]]);
    expect(await server.references(f, 7, 9)).toEqual([[f, 7, 9, 14], [f, 9, 10, 15]]);
  } finally { server.close(); p.end(); }
}, 60_000);

test('a name the compiler places wrongly, or whose use names several definitions, is shown but not renamed', async () => {
  if (!ready()) return;
  const refuses = (server, file, line, col) => server.request('textDocument/prepareRename', { textDocument: { uri: pathToFileURL(file).href }, position: { line: line - 1, character: col - 1 } }).then(() => 'renames', e => e.message);
  // The compiler gives the lambda parameters `c` and `i` of `coins.map((c, i) => …)` the span of the `(` before them.
  const lists = project({ 'app.contract': readFileSync(resolve(root, 'contract/corpus/lists.contract'), 'utf8') }), f = lists.file('app.contract'), server = await open(tsc, lists.dir);
  try {
    expect(symbols(f).definitions.filter(d => d.line === 29 && d.kind === 'local')).toEqual([expect.objectContaining({ name: 'c', col: 22, end_col: 23 }), expect.objectContaining({ name: 'i', col: 22, end_col: 23 })]);
    server.openFile(f);
    expect(await server.definition(f, 29, 45)).toEqual([]);
    expect(await server.references(f, 29, 45)).toEqual([[f, 29, 45, 46]]);
    expect(await server.hover(f, 29, 45)).toContain('local `c` in component Lists');
    expect(await refuses(server, f, 29, 45)).toContain('You cannot rename this element');
  } finally { server.close(); lists.end(); }
  // `navigationBack="back"` is one use of two `id="back"` definitions.
  const shelf = project(app('shelf')), g = shelf.file('app.contract'), other = await open(tsc, shelf.dir);
  try {
    const { definitions, references } = symbols(g), back = references.filter(r => r.line === 410 && r.name === 'back');
    expect(back.map(r => definitions[r.to].line)).toEqual([553, 590]);
    other.openFile(g);
    expect(await other.definition(g, 410, 23)).toEqual([[g, 553, 31, 37], [g, 590, 31, 37]]);
    expect(await refuses(other, g, 410, 23)).toContain('You cannot rename this element');
    expect(await refuses(other, g, 553, 32)).toContain('You cannot rename this element');
  } finally { other.close(); shelf.end(); }
}, 60_000);

/** Renames the definition of `pick` from the compiler's own analysis, applies TypeScript's edits, and returns what the
 * edits were and what the compiler expected. */
async function rename(files, main, pick, to) {
  const p = project(files), f = p.file(main), before = symbols(f), d = before.definitions.find(e => pick(e, p.dir)), at = where(d).at;
  const expected = [at, ...before.references.filter(r => before.definitions[r.to] === d).map(r => where(r).at)].map(([file, line, start, end]) => [file, line, start, end, to]).sort(order);
  const baseline = errors(f), server = await open(tsc, p.dir);
  try {
    for (const file of Object.keys(files)) server.openFile(p.file(file));
    const answer = await server.request('textDocument/rename', { textDocument: { uri: pathToFileURL(at[0]).href }, position: { line: at[1] - 1, character: at[2] - 1 }, newName: to });
    const edits = Object.entries(answer?.changes ?? {}).flatMap(([uri, es]) => es.map(e => [fileURLToPath(uri), e.range.start.line + 1, e.range.start.character + 1, e.range.end.character + 1, e.newText]))
      .concat((answer?.documentChanges ?? []).flatMap(c => c.edits.map(e => [fileURLToPath(c.textDocument.uri), e.range.start.line + 1, e.range.start.character + 1, e.range.end.character + 1, e.newText]))).sort(order);
    for (const file of new Set(edits.map(e => e[0]))) {
      const lines = text(file);
      for (const [, line, start, end, newText] of edits.filter(e => e[0] === file).reverse()) lines[line - 1] = lines[line - 1].slice(0, start - 1) + newText + lines[line - 1].slice(end - 1);
      writeFileSync(file, lines.join('\n'));
    }
    const after = symbols(f), renamed = after.definitions.findIndex(e => e.file === d.file && e.line === d.line && e.col === d.col && e.kind === d.kind);
    return { d, edits, expected, baseline, errors: errors(f), uses: [before.references.filter(r => before.definitions[r.to] === d).length, after.references.filter(r => r.to === renamed).length], name: after.definitions[renamed]?.name };
  } finally { server.close(); p.end(); }
}
const order = (a, b) => a[0].localeCompare(b[0]) || a[1] - b[1] || a[2] - b[2];
const loops = `shape Greeting
  text: string

component Hello
  resource greetings = greetings() as shape Greeting[]
  state liked = false
  action like
    liked = not liked
  view
    main
      button press=like aria-label=(liked ? "Unlike" : "Like") testId="like"
        text (liked ? "on" : "off")
      for g in greetings key=g.text
        text g.text
`;

for (const [what, pick] of [['a state', d => d.kind === 'state' && d.name === 'liked'], ['an action', d => d.kind === 'action' && d.name === 'like'], ['a shape field', d => d.kind === 'field' && d.name === 'text'], ['a loop local', d => d.kind === 'local' && d.name === 'g']])
  test(`renaming ${what} edits exactly the compiler's references and the file still builds`, async () => {
    if (!ready()) return;
    const r = await rename({ 'app.contract': loops }, 'app.contract', pick, 'renamed');
    expect(r.edits).toEqual(r.expected);
    expect(r.expected.length).toBeGreaterThan(1);
    expect(r.baseline).toEqual([]);
    expect(r.errors).toEqual([]);
    expect(r.name).toBe('renamed');
    expect(r.uses[1]).toBe(r.uses[0]);
  }, 60_000);

/** The definition of `kind` in the app (in `file` of it, if given) with the most references. */
function busiest(main, kind, file) {
  const { definitions, references } = symbols(main), n = i => references.filter(r => r.to === i).length;
  return definitions[definitions.map((d, i) => i).filter(i => definitions[i].kind === kind && where(definitions[i]).at && (!file || relative(dirname(main), definitions[i].file) === file)).sort((a, b) => n(b) - n(a))[0]];
}
for (const [name, kind, file] of [['realworld', 'state'], ['realworld', 'action'], ['realworld', 'field'], ['realworld', 'local'], ['caltrain', 'shape', 'activities.contract']])
  test(`renaming apps/${name}'s busiest ${kind}${file ? ` in ${file}` : ''} edits exactly the compiler's references, in every file, and the app still builds`, async () => {
    if (!ready()) return;
    const main = resolve(root, 'apps', name, 'app.contract'), d = busiest(main, kind, file);
    const r = await rename(app(name), 'app.contract', (e, dir) => e.kind === d.kind && e.name === d.name && e.line === d.line && e.col === d.col && relative(dir, e.file) === relative(dirname(main), d.file), `${d.name}Renamed`);
    console.log(`apps/${name}: renamed ${kind} \`${d.name}\` (${relative(dirname(main), d.file)}:${d.line}): ${r.edits.length} edits in ${new Set(r.edits.map(e => e[0])).size} file(s), compiler expected ${r.expected.length}; references before ${r.uses[0]}, after ${r.uses[1]}`);
    expect(r.edits).toEqual(r.expected);
    expect(r.baseline).toEqual([]);
    expect(r.errors).toEqual([]);
    expect(r.name).toBe(`${d.name}Renamed`);
    expect(r.uses[1]).toBe(r.uses[0]);
  }, 120_000);
