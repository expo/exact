import { test, expect } from 'bun:test';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { pathToFileURL, fileURLToPath } from 'node:url';
import { project, ready } from './mapper.test.mjs';
import { open } from './test/lsp.mjs';

const root = resolve(import.meta.dir, '../..');
const contract = process.env.EXACT_CONTRACT ?? ['target/release/contract', 'target/debug/contract'].map(p => resolve(root, p)).find(existsSync);
// The hello app of `exact new`: one source, `greeting`, answered by app.ts.
const hello = `shape Greeting
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
const helloTs = `import type { Answer, Result, Sources } from './app.contract.d.ts';

export const grants = '';

const sources: Sources = {
  greeting: ([name]): Result<'greeting'> => ({ text: \`Hello from \${name}.\` }),
};
export const answer: Answer = (source, args, store, storage, native) =>
  sources[source](args, store, storage, native);
`;
/** A copy of an app: its contract, its app.ts, and the declarations the compiler generates for app.ts. */
function app(files) {
  const p = project(files);
  if (files['app.ts']) Bun.spawnSync([contract, 'types', p.file('app.contract'), '-o', p.file('app.contract.d.ts')]);
  return p;
}
const implementation = async (server, file, line, col) =>
  ((await server.request('textDocument/implementation', { textDocument: { uri: pathToFileURL(file).href }, position: { line: line - 1, character: col - 1 } })) ?? [])
    .map(l => [fileURLToPath(l.uri), l.range.start.line + 1, l.range.start.character + 1, l.range.end.character + 1]);
/** The source calls the compiler reports. */
const calls = file => JSON.parse(Bun.spawnSync([contract, 'symbols', file]).stdout.toString()).definitions.filter(d => d.kind === 'source');

test('a source call in the contract is linked to the function in app.ts that answers it', async () => {
  if (!ready()) return;
  const p = app({ 'app.contract': hello, 'app.ts': helloTs }), f = p.file('app.contract'), ts = p.file('app.ts'), server = await open(process.env.EXACT_TSC71, p.dir);
  try {
    server.openFile(f);
    const [call] = calls(f), use = [f, 8, 23, 31], fn = [ts, 6, 3, 11];
    expect([call.line, call.col, call.end_col]).toEqual(use.slice(1));
    expect(await implementation(server, f, 8, 23)).toEqual([fn]);
    expect(await server.hover(f, 8, 23)).toContain('(property) greeting: (args: [string], store: Store, storage: Storage, native?: NativeModule | null) =>');
    expect(await server.references(f, 8, 23)).toEqual([fn, use]);
    server.openFile(ts, 'typescript');
    expect(await server.references(ts, 6, 3)).toEqual([fn, use]);
    expect(await server.diagnostics(f)).toEqual([]);
    expect(await server.diagnostics(ts)).toEqual([]);
  } finally { server.close(); p.end(); }
}, 60_000);

test('an app.ts that exports its sources unannotated gives the contract the function\'s own signature; definition stays the compiler\'s', async () => {
  if (!ready()) return;
  const exported = helloTs.replace('const sources: Sources = {', 'export const sources = {').replace('};\nexport const answer', '} satisfies Sources;\nexport const answer').replace('  sources[source]', '  (sources as Sources)[source]');
  const p = app({ 'app.contract': hello, 'app.ts': exported }), f = p.file('app.contract'), ts = p.file('app.ts'), server = await open(process.env.EXACT_TSC71, p.dir);
  try {
    server.openFile(f);
    expect(await server.definition(f, 8, 23)).toEqual([[f, 8, 23, 31]]); // the source's definition is its call, as the compiler names it (names.js)
    expect(await implementation(server, f, 8, 23)).toEqual([[ts, 6, 3, 11]]);
    expect(await server.hover(f, 8, 23)).toContain('(property) greeting: ([name]: [string]) => Result<\'greeting\'>');
    server.openFile(ts, 'typescript');
    expect(await server.diagnostics(ts)).toEqual([]);
  } finally { server.close(); p.end(); }
}, 60_000);

test('a contract with no app.ts maps cleanly, as does one whose data is Rust', async () => {
  if (!ready()) return;
  const p = app({ 'app.contract': hello, 'rust.contract': readFileSync(resolve(root, 'apps/carousel/app.contract'), 'utf8') }), server = await open(process.env.EXACT_TSC71, p.dir);
  try {
    const f = p.file('app.contract'); server.openFile(f);
    expect(await server.hover(f, 8, 23)).toContain('source `greeting`');
    expect(await server.references(f, 8, 23)).toEqual([[f, 8, 23, 31]]);
    expect(await server.diagnostics(f)).toEqual([]);
    const rust = p.file('rust.contract'); server.openFile(rust);
    const [cards] = calls(rust);
    expect(await server.hover(rust, cards.line, cards.col)).toContain('source `cards`');
    expect(await server.diagnostics(rust)).toEqual([]);
  } finally { server.close(); p.end(); }
}, 60_000);

test('realworld: every source call the compiler reports resolves to its function in app.ts, and back', async () => {
  if (!ready()) return;
  const rw = resolve(root, 'apps/realworld');
  const p = app({ 'app.contract': readFileSync(resolve(rw, 'app.contract'), 'utf8'), 'app.ts': readFileSync(resolve(rw, 'app.ts'), 'utf8') });
  const f = p.file('app.contract'), ts = p.file('app.ts'), server = await open(process.env.EXACT_TSC71, p.dir);
  try {
    server.openFile(f);
    const reported = calls(f), resolved = [];
    for (const c of reported) {
      const found = await implementation(server, f, c.line, c.col);
      if (found.length === 1 && found[0][0] === ts) resolved.push([c, found[0]]);
    }
    console.log(`realworld: ${reported.length} source calls reported, ${resolved.length} resolve into app.ts`);
    expect(reported.length).toBe(19);
    expect(resolved.length).toBe(reported.length);
    server.openFile(ts, 'typescript');
    for (const [c, [, line, col]] of resolved) expect(await server.references(ts, line, col)).toContainEqual([f, c.line, c.col, c.end_col]);
    expect(await server.hover(f, reported[0].line, reported[0].col)).toContain(`(property) ${reported[0].name}: (args: [], store: Store`);
    expect(await server.diagnostics(f)).toEqual([]);
    expect(await server.diagnostics(ts)).toEqual([]);
  } finally { server.close(); p.end(); }
}, 120_000);
