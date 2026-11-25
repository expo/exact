import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { webCompiler } from './module.mjs';
import { contentNames, safeInlineScript } from './content-names.mjs';
import { open } from '../../scripts/agent.mjs';

// LLP 1153.004: a build names by content each file that only it, a renderer or code it bundles names, and `routes.json`
// `files` names each. A path never names two contents: a path two builds share has the same bytes, and the entry and the
// stylesheet, whose bytes change with a text and a style, get new ones. A development build renames the same files.
test('a build names its own files by their content, and no path names two contents', () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/web-js-content-names-'));
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Names', id: 'test.names', app: { id: 'test.names', name: 'Names' } }));
    const build = (name, text, args) => {
      writeFileSync(resolve(dir, 'app.contract'), `component App\n  view\n    column\n      input type="file"\n      text "${text}"${text === 'Two' ? ' padding=7' : ''}\n`);
      const out = resolve(dir, name), result = spawnSync(process.execPath, ['host/web-js/build.mjs', 'names', '--out', out, '--render', 'none', ...args], {
        cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, encoding: 'utf8',
      });
      expect(result.status, result.stdout + result.stderr).toBe(0);
      return { out, files: JSON.parse(readFileSync(resolve(out, 'routes.json'), 'utf8')).files };
    };
    const first = build('first', 'One', ['--production']), second = build('second', 'Two', ['--production']), dev = build('dev', 'One', []);
    for (const { out, files } of [first, second, dev]) for (const name of Object.values(files)) expect(existsSync(resolve(out, name)), name).toBe(true);
    // The documents glue is renamed too: in a build with no storage, no copied file imports it.
    const renamed = ({ files }) => Object.keys(files).filter(name => files[name] !== name).sort();
    expect([renamed(first), renamed(dev)]).toEqual([['app.js', 'documents-glue.js', 'picker-glue.js', 'style.css'], renamed(first)]);
    const shared = Object.values(first.files).filter(name => Object.values(second.files).includes(name));
    expect([shared.length > 0, shared.filter(name => !readFileSync(resolve(first.out, name)).equals(readFileSync(resolve(second.out, name))))]).toEqual([true, []]);
    expect(['app.js', 'style.css'].filter(name => second.files[name] === first.files[name])).toEqual([]);
    // The bundle names each renamed file by its new name, and none by its old one.
    const code = Object.values(first.files).filter(name => name.endsWith('.js')).map(name => readFileSync(resolve(first.out, name), 'utf8')).join('\n');
    expect([['documents-glue.js', 'picker-glue.js', 'style.css'].filter(name => !code.includes(first.files[name])),
      /\b(?:new URL|import|fetch|load|__files)\(\s*[`'"](?:\.\/)?(?:documents-glue\.js|picker-glue\.js|style\.css)[`'"]/.exec(code)?.[0]]).toEqual([[], undefined]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 180_000);

// The bundle rewrites a file's name where Exact's runtime names the file, not a string that only spells it or a call to it,
// such as an app's id or a launch service's config that a generated module embeds; and an app's own module not at all.
test('the bundle rewrites a reference to a renamed file, not a string that spells its name', () => {
  const files = { 'style.css': 'style-Ab12Cd34.css', 'picker-glue.js': 'picker-glue-Ef56Gh78.js', 'markdown.wasm': 'markdown-Ij90Kl12.wasm' };
  const code = (style, picker, markdown) => `const appId = "style.css", config = { url: "fetch('style.css')" }, said = 'picker-glue.js';
const label = "é"; fetch('./${style}'); import(new URL("./${picker}", import.meta.url).href); globalThis.__files('${markdown}');`;
  expect([contentNames(files, () => true).transform(code('style.css', 'picker-glue.js', 'markdown.wasm'), '/gen/x.js'),
    contentNames(files, () => false).transform(code('style.css', 'picker-glue.js', 'markdown.wasm'), '/app/x.js')])
    .toEqual([code('style-Ab12Cd34.css', 'picker-glue-Ef56Gh78.js', 'markdown-Ij90Kl12.wasm'), null]);
});

test('an --inline build neutralizes HTML script-data tags without diverging from app.js and boots cleanly', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const scriptData = /<(?=!--|\/script[\t\n\f\r />])/gi;
  // Unit check: string, template, regex, and odd-backslash escapes preserve runtime values while removing hazards.
  const rewritten = safeInlineScript('const a="<!-- <script></script>",b=`<!-- <SCRIPT>x`,c=/<!--|<\\/script>/i,d="\\\\<!--";');
  expect(scriptData.test(rewritten)).toBe(false);
  expect(new Function(`${rewritten} return [a,b,c.test("<!--"),d];`)()).toEqual([
    '<!-- <script></script>', '<!-- <SCRIPT>x', true, '\\<!--',
  ]);

  const root = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/web-js-inline-script-'));
  let browser, server;
  const appDir = process.env.EXACT_APP_DIR;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Inline', app: { id: 'test.inline', name: 'Inline' } }));
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    column\n      text "<!-- <script></script>" testId="a"\n      text "<!-- <SCRIPT>hello" testId="b"\n');
    const out = resolve(dir, 'dist');
    const built = spawnSync(process.execPath, ['host/web-js/build.mjs', 'inline', '--out', out, '--inline', '--render', 'none'], {
      cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, encoding: 'utf8',
    });
    expect(built.status, built.stdout + built.stderr).toBe(0);
    const entryName = JSON.parse(readFileSync(resolve(out, 'routes.json'), 'utf8')).files['app.js'];
    const onDisk = readFileSync(resolve(out, entryName), 'utf8');
    const html = readFileSync(resolve(out, 'index.html'), 'utf8');
    const openTag = `<script type="module" data-entry="./${entryName}">`;
    const start = html.indexOf(openTag);
    expect(start).toBeGreaterThan(0);
    const end = html.indexOf('</script>', start + openTag.length);
    const inlined = html.slice(start + openTag.length, end);
    expect(inlined).toBe(onDisk);
    scriptData.lastIndex = 0;
    expect(scriptData.test(inlined)).toBe(false);

    server = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch(req) {
      const p = new URL(req.url).pathname;
      const file = resolve(out, p === '/' ? 'index.html' : p.slice(1));
      return new Response(Bun.file(file));
    } });
    browser = await open({ host: 'web', url: server.url.href, webDist: out });
    const nodes = (await browser.tree()).nodes;
    expect(nodes.find(n => n.props.testId === 'a')?.props.text).toBe('<!-- <script></script>');
    expect(nodes.find(n => n.props.testId === 'b')?.props.text).toBe('<!-- <SCRIPT>hello');
  } finally {
    if (appDir === undefined) delete process.env.EXACT_APP_DIR;
    else process.env.EXACT_APP_DIR = appDir;
    await browser?.close();
    server?.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 180_000);
