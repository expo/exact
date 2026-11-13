import { test, expect } from 'bun:test';
import { readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { open } from './test/lsp.mjs';
import { project, ready } from './mapper.test.mjs';
import { transform } from './mapper.js';

const tsc = process.env.EXACT_TSC71, root = resolve(import.meta.dir, '../..');
// mapper.test.mjs's starter, with a box of styles, a button whose next attribute is half typed, and a heading.
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
      box padding=8 text-align="center" -exact-control-size="small"
      h2 "Title"
`;
/** The 1-based column of `text` on the starter's 1-based `line`, plus `past`. */
const col = (line, text, past = 0) => starter.split('\n')[line - 1].indexOf(text) + 1 + past;
const items = r => (Array.isArray(r) ? r : r?.items ?? []).map(i => i.insertText ?? i.label);
const complete = (server, file, line, col) => server.request('textDocument/completion', { textDocument: { uri: `file://${file}` }, position: { line: line - 1, character: col - 1 } }).then(items);
const vocabulary = JSON.parse(Bun.spawnSync([process.env.EXACT_CONTRACT ?? 'contract', 'vocab', '--json', '--all']).stdout.toString() || '{}');
const attribute = name => vocabulary.attributes?.find(a => a.name === name);

test('hover on a tag or attribute shows the vocabulary\'s documentation for it', async () => {
  if (!ready()) return;
  const p = project({ 'app.contract': starter }), f = p.file('app.contract'), server = await open(tsc, p.dir);
  try {
    server.openFile(f);
    const button = await server.hover(f, 14, col(14, 'button'));
    expect(button).toContain('tag `button`: a Pressable (html)');
    expect(button).toContain(vocabulary.tags.find(t => t.name === 'button').note.slice(0, 60));
    expect(await server.hover(f, 14, col(14, 'press'))).toContain('handler attribute `press`: the `press` event.');
    expect(await server.hover(f, 14, col(14, 'aria-label'))).toContain('prop attribute `aria-label`: sets `accessibilityLabel`, str.');
    const padding = await server.hover(f, 17, col(17, 'padding'));
    expect(padding).toContain('style attribute `padding`, rows:');
    expect(padding).toContain('`padding_top`: dimension, default `0`');
    expect(await server.hover(f, 17, col(17, 'text-align'))).toContain('`text_align`: enum (`left`, `center`, `right`, `justify`, `start`, `end`), default `start`');
    // JSX cannot spell `-exact-control-size`; it is a key of a spread object, typed all the same.
    // An `h2` is the compiler's `text`.
    expect(await server.hover(f, 18, col(18, 'h2'))).toContain('tag `text`');
    expect(await server.hover(f, 17, col(17, '-exact-control-size'))).toContain('`control_size`: enum (`mini`, `small`, `medium`, `large`), default `medium`');
  } finally { server.close(); p.end(); }
}, 60_000);

test('completion offers an enum attribute\'s values inside its string, and a handler hovers as the actions it takes', async () => {
  if (!ready()) return;
  const p = project({ 'app.contract': starter }), f = p.file('app.contract'), server = await open(tsc, p.dir);
  try {
    server.openFile(f);
    // Inside `text-align="center"` and `-exact-control-size="small"`: exactly the vocabulary's values.
    expect((await complete(server, f, 17, col(17, '"center"', 1))).sort()).toEqual([...attribute('text-align').rows[0].values].sort());
    expect((await complete(server, f, 17, col(17, '"small"', 1))).sort()).toEqual([...attribute('-exact-control-size').rows[0].values].sort());
    // `press` takes an action of the vocabulary's `params`, or of those and its `record`.
    expect(attribute('press')).toMatchObject({ params: [], record: 'MouseEvent' });
    expect(await server.hover(f, 14, col(14, 'press'))).toContain('(() => void) | ((event: MouseEvent) => void)');
  } finally { server.close(); p.end(); }
}, 60_000);

test('every tag and attribute the compiler reports in caltrain and the starter has the vocabulary\'s hover', async () => {
  if (!ready()) return;
  const caltrain = resolve(root, 'apps/caltrain/app.contract');
  const p = project({ 'app.contract': starter }), f = p.file('app.contract');
  writeFileSync(resolve(p.dir, 'tsconfig.json'), JSON.stringify({ compilerOptions: { noEmit: true, strict: true, skipLibCheck: true }, contentMappers: [{ package: '@exact/contract-content-mapper', extensions: ['.contract'] }], files: ['app.contract', caltrain] }));
  const server = await open(tsc, p.dir);
  try {
    const counts = {};
    for (const file of [f, caltrain]) {
      server.openFile(file);
      const content = readFileSync(file, 'utf8');
      // The places the mapper writes: those the compiler reports where the authored text is the name.
      const placed = transform(file, content).mappings.filter(m => m[5] & 1).map(m => m[2]);
      const symbols = JSON.parse(Bun.spawnSync([process.env.EXACT_CONTRACT ?? 'contract', 'symbols', file]).stdout.toString());
      const elements = symbols.elements.filter(e => resolve(e.file) === resolve(file));
      const lines = content.split('\n'), starts = lines.reduce((a, l) => [...a, a.at(-1) + l.length + 1], [0]);
      // The compiler counts UTF-8 bytes; the language server, UTF-16 units.
      const unit = at => Buffer.from(lines[at.line - 1]).subarray(0, at.col - 1).toString().length + 1;
      const sites = elements.flatMap(e => [['tag', e.tag, e], ...e.attrs.map(a => ['attribute', a.name, a])]);
      const missing = [];
      // Not just any hover: the vocabulary's own, which names the tag or attribute.
      for (const [kind, name, at] of sites) if (!(await server.hover(file, at.line, unit(at)))?.includes(kind === 'tag' ? 'tag `' : `attribute \`${name}\``)) missing.push(`${kind} ${name} ${at.line}:${at.col}`);
      counts[file === f ? 'starter' : 'caltrain'] = { tags: elements.length, attributes: sites.length - elements.length, mapped: sites.filter(([, , at]) => placed.includes(starts[at.line - 1] + unit(at) - 1)).length, missing };
    }
    console.log(JSON.stringify(counts, null, 1));
    for (const c of Object.values(counts)) expect(c.missing).toEqual([]);
  } finally { server.close(); p.end(); }
}, 120_000);
