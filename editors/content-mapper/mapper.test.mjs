import { test, expect } from 'bun:test';
import { cpSync, existsSync, mkdirSync, mkdtempSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { open } from './test/lsp.mjs';

// TypeScript 7.1's own `tsc` (content mappers are new in 7.1): `EXACT_TSC71` names it. Without it these tests say so and pass.
const tsc = process.env.EXACT_TSC71, root = resolve(import.meta.dir, '../..');
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
/** A project of `.contract` files that TypeScript reads through the mapper. */
export function project(files) {
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/content-mapper-test-'));
  mkdirSync(resolve(dir, 'node_modules/@exact'), { recursive: true });
  symlinkSync(import.meta.dir, resolve(dir, 'node_modules/@exact/contract-content-mapper'));
  for (const [name, text] of Object.entries(files)) writeFileSync(resolve(dir, name), text);
  writeFileSync(resolve(dir, 'tsconfig.json'), JSON.stringify({ compilerOptions: { noEmit: true, strict: true, skipLibCheck: true }, contentMappers: [{ package: '@exact/contract-content-mapper', extensions: ['.contract'] }], include: Object.keys(files).filter(f => f.endsWith('.contract')) }));
  return { dir, file: name => resolve(dir, name), end: () => rmSync(dir, { recursive: true, force: true }) };
}
export const ready = () => { if (!tsc || !existsSync(tsc)) { console.log('skipped: set EXACT_TSC71 to TypeScript 7.1\'s tsc'); return false; } return true; };

test('TypeScript answers definition, references, rename and hover on a contract as the Contract compiler resolved them', async () => {
  if (!ready()) return;
  const p = project({ 'app.contract': starter }), f = p.file('app.contract'), server = await open(tsc, p.dir);
  try {
    server.openFile(f);
    const places = list => list.map(([file, ...rest]) => [file === f ? 'app.contract' : file, ...rest]);
    // `liked`: declared at 9:9, written and read at 11:5 and 11:17, read at 14:37 and 15:15.
    const all = [['app.contract', 9, 9, 14], ['app.contract', 11, 5, 10], ['app.contract', 11, 17, 22], ['app.contract', 14, 37, 42], ['app.contract', 15, 15, 20]];
    expect(places(await server.definition(f, 11, 17))).toEqual([all[0]]);
    expect(places(await server.references(f, 9, 9))).toEqual(all);
    expect(places(await server.rename(f, 14, 37, 'loved'))).toEqual(all.map(([file, line, col]) => [file, line, col, 'loved']));
    expect(places(await server.definition(f, 14, 20))).toEqual([['app.contract', 10, 10, 14]]); // `press=like` is the action
    expect(await server.hover(f, 10, 10)).toContain('action `like` in component Hello; writes `liked`');
    expect(await server.diagnostics(f)).toEqual([]);
  } finally { server.close(); p.end(); }
}, 60_000);

test('a hover shows each name with the type the Contract compiler gives it', async () => {
  if (!ready()) return;
  const loop = starter.replace('  state liked', '  resource greetings = greetings() as shape Greeting[]\n  state liked').replace(/\n$/, '\n      for g in greetings key=g.text\n        text g.text\n');
  const p = project({ 'app.contract': starter, 'loop.contract': loop }), f = p.file('app.contract'), g = p.file('loop.contract'), server = await open(tsc, p.dir);
  const hover = (file, line, col) => server.hover(file, line, col).then(h => h.replace(/^```typescript\n|```$/gm, ''));
  try {
    server.openFile(f); server.openFile(g);
    expect(await hover(f, 9, 9)).toBe('(property) liked: boolean\n\nstate `liked` in component Hello');
    expect(await hover(f, 1, 7)).toBe('(property) Greeting: {\n    text: string;\n}\n\nshape `Greeting`');
    expect(await hover(f, 2, 3)).toBe('(property) text: string\n\nfield `text` of Greeting');
    expect(await hover(f, 16, 21)).toBe('(property) text: string\n\nfield `text` of Greeting'); // `greeting.text`
    expect(await hover(f, 8, 12)).toBe('(property) greeting: Greeting\n\nresource `greeting` in component Hello');
    expect(await hover(f, 10, 10)).toBe('(property) like: () => void\n\naction `like` in component Hello; writes `liked`');
    expect(await hover(f, 4, 8)).toBe('(property) nav: Router\n\nstate `nav` in component Hello');
    // The compiler gives a route no type.
    expect(await hover(f, 5, 3)).toBe('(property) home: Route\n\nroute `home`');
    expect(await hover(g, 18, 11)).toBe('(property) g: Greeting\n\nlocal `g` in component Hello');
    expect(await hover(g, 19, 16)).toBe('(property) text: string\n\nfield `text` of Greeting');
  } finally { server.close(); p.end(); }
}, 60_000);

test('a contract\'s errors are the Contract compiler\'s, at its places, and TypeScript adds none', async () => {
  if (!ready()) return;
  const p = project({ 'bad.contract': 'component A\n  state n = 0\n  view\n    text n.foo\n    buton "x"\n' }), f = p.file('bad.contract'), server = await open(tsc, p.dir);
  try {
    server.openFile(f);
    expect(await server.diagnostics(f)).toEqual([[4, 12, 'type-not-a-record: `number` has no fields'], [5, 5, 'lower-unknown-tag: unknown tag `buton`; did you mean `button`?']]);
    const run = Bun.spawnSync([tsc, '-p', p.dir, '--runExternalCode'], { stdout: 'pipe', stderr: 'pipe' });
    expect(run.stdout.toString().split('\n').filter(Boolean)).toEqual(['bad.contract(4,12): error contract9100: type-not-a-record: `number` has no fields', 'bad.contract(5,5): error contract9100: lower-unknown-tag: unknown tag `buton`; did you mean `button`?'].map(l => expect.stringContaining(l)));
  } finally { server.close(); p.end(); }
}, 60_000);

test('with no Contract compiler the file says so once, and with one each question is asked once', async () => {
  const { transform } = await import('./mapper.js');
  const missing = transform('/tmp/app.contract', starter, { contract: '/nowhere/contract' });
  expect(missing.diagnostics).toEqual([{ start: 0, length: 1, code: 9199, messageText: expect.stringContaining('was not found at `/nowhere/contract`') }]);
  expect(missing.mappings).toEqual([]);

  const bin = process.env.EXACT_CONTRACT ?? resolve(root, 'target/debug/contract');
  if (!existsSync(bin)) { console.log('skipped: no contract binary'); return; }
  // A stand-in compiler that records each run, then answers as the real one.
  const dir = mkdtempSync(resolve(root, 'target/content-mapper-test-')), log = resolve(dir, 'runs'), stand = resolve(dir, 'contract');
  writeFileSync(stand, `#!/bin/sh\necho "$1" >> ${JSON.stringify(log)}\nexec ${JSON.stringify(bin)} "$@"\n`, { mode: 0o755 });
  try {
    const out = transform(resolve(dir, 'app.contract'), starter, { contract: stand });
    expect(out.mappings.length).toBeGreaterThan(20);
    const runs = (await Bun.file(log).text()).trim().split('\n');
    expect(runs.filter(r => r === 'symbols')).toEqual(['symbols']);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('a project reached through a linked directory imports its other file beside it, and is built with its app', async () => {
  const bin = process.env.EXACT_CONTRACT ?? resolve(root, 'target/debug/contract');
  if (!existsSync(bin)) { console.log('skipped: no contract binary'); return; }
  const { transform } = await import('./mapper.js');
  const dir = mkdtempSync(resolve(root, 'target/content-mapper-test-')), link = `${dir}-link`;
  const shapes = 'shape Greeting\n  text: string\n', app = `use Greeting from "./shapes.contract"\n${starter.slice(starter.indexOf('routes nav'))}`;
  writeFileSync(resolve(dir, 'shapes.contract'), shapes); writeFileSync(resolve(dir, 'app.contract'), app);
  symlinkSync(dir, link);
  try {
    const out = transform(resolve(link, 'app.contract'), app, { contract: bin });
    expect(out.text).toContain('from "./shapes.contract"');
    expect(out.diagnostics).toEqual([]);
    // The used file alone is no app: its errors are its app's, found though the compiler names it by its real path.
    expect(transform(resolve(link, 'shapes.contract'), shapes, { contract: bin }).diagnostics).toEqual([]);
  } finally { rmSync(link, { force: true }); rmSync(dir, { recursive: true, force: true }); }
});
