// The page's `agent` parameter asks for agent mode alone or as `agent=1`, as `EXACT_AGENT=1` asks a native host; any
// other value, `agent=0` among them, asks for none. Only a development build admits it (LLP 1069.007 D2).
import { test, expect } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { runInNewContext } from 'node:vm';
import { parseSync } from 'rolldown/utils';
import { agentGuard, agentUse, gateAgent } from './agent-gate.mjs';
import { bundleModuleGlue } from './glue.mjs';
import { webCompiler } from './module.mjs';
import { chromium, suiteBrowser } from '../../scripts/agent-launch.mjs';
import { need } from '../../scripts/coverage.mjs';
import { agentSeed, storageKey } from '../web/storage-environment.js';

const root = resolve(import.meta.dir, '../..');
// The tests that parse HTML or open pages in Chromium, skipped where it is missing
const chromeTest = need('chrome', chromium().unavailable);

// Every reader of the parameter in the hosts' sources asks the one question, `['', '1'].includes(params.get('agent'))`,
// guarded where it asks it: a module by an AGENT_ADMITTED it declares, as the release gate (agent-gate.mjs `gateAgent`)
// holds a release to, and the inline scripts, one text in every build, which no gate rewrites, by the development
// marker they find. A script a source holds in a string, or an HTML shell holds, has no constant a build could write
// false, so it reads the parameter only where it runs in development alone; a script assembled from fragments, or run
// from a string another way, is code built at run time, beyond any static check, though in the pages the Rust render
// server serves its default Content-Security-Policy refuses to compile one with `eval` or `Function` or to run it as an
// inline script it did not hash, unless the app turns it off or widens it (`host.web.csp`, host/render/src/csp.rs); a
// worker made from a Blob, which the policy admits, can still run one. No source matches the query against a
// `[?&]agent` pattern.
chromeTest('every web reader of the agent parameter asks the one question, guarded where it asks it', async () => {
  // The guard of each source's own reads that is no module's: capture.js finds a development build's marker, scheme.js
  // reads the shell's development attribute (`d`), the Bun request renderer, which no release ships, asks its render's
  // build, and dev.js, which only the dev server serves, has none
  const guards = { 'host/web-js/capture.js': 'd.head.querySelector("meta[name=exact-agent]")', 'host/web/scheme.js': 'd',
    'host/web-js/render.mjs': 'render.production === false', 'host/web/dev.js': null };
  // The modules that read the parameter, each one a release gates (its `.gen` loop, its bundles and its lazy copies,
  // agent-gate.mjs `gateAgent`), so that their AGENT_ADMITTED is written false; a source the build copies raw, the compiler
  // embeds or a package holds ships as written, so a reader goes here only once a release gates it, and any other
  // source may not read the parameter at all
  const readers = ['host/web-js/clock.js', 'host/web-js/facts.js', 'host/web/canvas2d-glue.js', 'host/web/document-glue.js',
    'host/web/faults.js', 'host/web/glue.js', 'host/web/navigation/environment.js', 'host/web/navigation/routes.js', 'host/web/storage-environment.js'];
  // The sources whose held scripts run in development alone, unguarded: the client dev.mjs injects and what the
  // conformance driver and the agent's drivers (`scripts/`) evaluate in a page they drive
  const drivers = file => file.startsWith('scripts/') && file !== 'scripts/install-page.mjs';
  const developmentHolder = file => ['host/web-js/dev.mjs', 'host/web-js/conform.mjs'].includes(file) || drivers(file);
  // A script parsed once, as a module or as the classic script `runs` names, with its reads (agent-gate.mjs `agentUse`)
  const parsed = (script, runs = 'module') => ({ script, ...agentUse(script, 'reader.js', { sourceType: runs }) });
  const guardsOf = (held, bound = false) => !held.parsed ? ['does not parse'] : held.reads.map(read => {
    const guard = agentGuard(read, held.up);
    return !guard ? guard : bound && !held.bound.has(guard.name) ? `${guard.name}, unbound` : held.script.slice(guard.start, guard.end);
  });
  // A URL's percent-decoded text, and then a base64 payload's, or the text as written where it is no valid encoding
  const decoded = (text, base64 = false) => {
    try { const plain = decodeURIComponent(text); return base64 ? atob(plain) : plain; } catch { return text; }
  };
  // The strings a parsed script holds, each substitution of a template read as a name: a template's text, and a tagged
  // template's raw text too, which its tag can read
  const strings = program => {
    const found = [], walk = node => {
      if (node?.type === 'Literal' && typeof node.value === 'string') found.push(node.value);
      if (node?.type === 'TemplateLiteral') found.push(node.quasis.map(quasi => quasi.value.cooked ?? '').join('_'));
      if (node?.type === 'TaggedTemplateExpression') found.push(node.quasi.quasis.map(quasi => quasi.value.raw).join('_'));
      if (node && typeof node === 'object') Object.values(node).forEach(walk);
    };
    walk(program);
    return found;
  };
  const browser = await suiteBrowser();
  try {
    const page = await browser.newPage();
    // Each script that HTML runs as JavaScript in each of these documents, a template's content and a `srcdoc` document
    // included, as a browser parses them, with how it runs: classic where the script block's type string is a JavaScript
    // MIME type essence, or a module (HTML, "prepare the script element"; MIME Sniffing, "JavaScript MIME type"); and the
    // URLs its attributes name
    const scriptsIn = documents => !documents.length ? [] : page.evaluate(documents => {
      const javascript = ['application/ecmascript', 'application/javascript', 'application/x-ecmascript', 'application/x-javascript', 'text/ecmascript',
        'text/javascript', 'text/javascript1.0', 'text/javascript1.1', 'text/javascript1.2', 'text/javascript1.3', 'text/javascript1.4', 'text/javascript1.5',
        'text/jscript', 'text/livescript', 'text/x-ecmascript', 'text/x-javascript'];
      const runs = script => {
        const type = script.getAttribute('type'), language = script.getAttribute('language');
        const string = type === '' || type === null && !language ? 'text/javascript' : type !== null ? type.replace(/^[\t\n\f\r ]+|[\t\n\f\r ]+$/g, '') : `text/${language}`;
        const lower = string.replace(/[A-Z]/g, letter => letter.toLowerCase());
        return javascript.includes(lower) ? 'script' : lower === 'module' ? 'module' : null;
      };
      return documents.map(html => {
        const found = [], urls = [], visit = root => {
          for (const script of root.querySelectorAll('script')) if (runs(script)) found.push([script.textContent, runs(script)]);
          // An event handler attribute runs as a function's body (HTML, "event handler content attributes"), `srcdoc` is a
          // document of its own, and a URL attribute names what a link, a form, a frame or a script loads, an SVG script's
          // `href` among them (SVG 2, "the script element")
          for (const element of root.querySelectorAll('*')) for (const { name, value } of element.attributes) {
            if (/^on/i.test(name)) found.push([`(function (event) {\n${value}\n})`, 'script']);
            else if (name.toLowerCase() === 'srcdoc') visit(new DOMParser().parseFromString(value, 'text/html'));
            else if (['href', 'src', 'action', 'formaction', 'xlink:href', 'data'].includes(name.toLowerCase())) urls.push(value);
          }
          for (const template of root.querySelectorAll('template')) visit(template.content);
        };
        visit(new DOMParser().parseFromString(html, 'text/html'));
        return [found, urls];
      });
    }, documents);
    // The scripts these strings hold, parsed as they run, and those they hold in turn. A `javascript:` URL holds its code,
    // tabs and newlines dropped and percent-decoded, which runs as a classic script, and a `data:` URL its payload,
    // percent-encoded or base64, which a script, a frame or `import()` loads (HTML, "the javascript: URL special case";
    // URL, "basic URL parser"; Fetch, "data: URL processor"); any other string may be a whole script, a module or a
    // classic one, and holds the code HTML runs where it has a tag.
    const heldIn = async texts => {
      const direct = [], documents = [], payloads = [];
      for (const text of texts) {
        const url = text.replace(/[\t\n\r]/g, '').replace(/^[\x00-\x20]+|[\x00-\x20]+$/g, ''), data = /^data:([^,]*),(.*)$/is.exec(url);
        if (/^javascript:/i.test(url)) direct.push(parsed(decoded(url.slice('javascript:'.length)), 'script'));
        else if (data) payloads.push(decoded(data[2], /;base64$/i.test(data[1].trim())));
        else {
          const module = parsed(text), script = module.parsed ? module : parsed(text, 'script');
          if (script.parsed) direct.push(script);
          // A string with a tag is HTML, whose scripts and attributes run, even where it is also a classic script that is
          // all comment
          if (/<[a-z]/i.test(text)) documents.push(text);
        }
      }
      for (const [scripts, urls] of await scriptsIn(documents)) {
        direct.push(...scripts.map(([script, runs]) => parsed(script, runs)));
        payloads.push(...urls);
      }
      const found = [...direct, ...(payloads.length ? await heldIn(payloads) : [])];
      for (const script of direct) found.push(...await heldIn(strings(script.program)));
      return found;
    };
    // A module reader and dev.js ask agent mode of the launch: a read whose parameters come straight from `location`
    // goes through the page's navigation entry, which history entries the app pushes leave as it was (Navigation Timing)
    const current = own => own.reads.map(read => own.script.slice((read.callee ?? read.tag).start, (read.callee ?? read.tag).end))
      .filter(callee => /\blocation\b/.test(callee) && !/getEntriesByType/.test(callee));
    const astrayIn = async (file, text) => {
      // A driver reads the URL of a page it drives, not its own, so it asks the question behind whatever it needs
      const want = file in guards ? guards[file] : readers.includes(file) ? 'AGENT_ADMITTED' : drivers(file) ? 'any' : 'no read';
      const own = !file.endsWith('.html') && parsed(text), held = await heldIn(own ? strings(own.program) : [text]);
      return [...(own ? guardsOf(own, want === 'AGENT_ADMITTED').filter(guard => want === 'any' ? guard === undefined : guard !== want)
        .map(guard => want === 'no read' ? `${file}: reads the parameter, guarded by ${guard}, and is no reader a release gates` : `${file}: guarded by ${guard}`) : []),
        // A driver's message that names a tag in prose makes no script that parses, and a driver runs in development alone
        ...held.flatMap(script => guardsOf(script))
          .filter(guard => !developmentHolder(file) || guard !== null && !(drivers(file) && guard === 'does not parse'))
          .map(guard => `${file}: a script it holds reads the parameter, guarded by ${guard}`),
        ...(/\[\?&\]agent\b/.test(text) ? [`${file}: [?&]agent`] : []),
        ...(own && (readers.includes(file) || file === 'host/web/dev.js') ? current(own).map(() => `${file}: reads the current URL, not the launch's`) : [])];
    };
    const sources = spawnSync('git', ['ls-files', 'host', 'js/src', 'canvas', 'packages', 'scripts'], { cwd: root, encoding: 'utf8' }).stdout.split('\n')
      .filter(file => /\.(m?js|html)$/.test(file) && !/\.test\.m?js$|\/tests\//.test(file) && (!file.startsWith('packages/') || /^packages\/[^/]+\/web\//.test(file)));
    expect(sources.filter(file => file.endsWith('.html')).length).toBeGreaterThan(0);
    expect((await Promise.all(sources.map(file => astrayIn(file, readFileSync(resolve(root, file), 'utf8'))))).flat()).toEqual([]);
    // Reads that are not the question, the question unguarded, guarded otherwise or by an unbound name
    const others = ["x.has ('agent')", "x['has']('agent')", 'x[`has`](`agent`)', "x.get?.('agent')", "x.getAll('agent')", "(x.get)(('agent'))",
      "(x?.get)('agent')", "x.get(/* query */ 'agent') === '0'", '!!x.get`agent`', "x.has(...['agent'])", "x.get(...[], 'agent')", "x.get(['agent'])", "x.get([...['agent']])", "x[['get', ...[]]]('agent')"];
    expect(others.map(script => guardsOf(parsed(script)))).toEqual(others.map(() => [undefined]));
    expect(["['', '1'].includes(x.get('agent'))", "!AGENT_ADMITTED || !['', '1'].includes(x.get('agent'))", "y && AGENT_ADMITTED && ['', '1'].includes(x.get('agent'))",
      "AGENT_ADMITTED && y && ['', '1'].includes(x?.get('agent'))"].map(script => guardsOf(parsed(script)))).toEqual([[null], [null], ['y'], ['AGENT_ADMITTED']]);
    expect(guardsOf(parsed("AGENT_ADMITTED && ['', '1'].includes(x.get('agent'));"), true)).toEqual(['AGENT_ADMITTED, unbound']);
    // A spread computed at run time passes a name the parse cannot see, as `has(key)` does
    expect(guardsOf(parsed('x.has(...keys)'))).toEqual([]);
    // Held scripts: in a development tool, the unguarded question passes and any other read does not, escaped or with a
    // comment in its argument; anywhere else no held script reads the parameter, even beside a constant of its own
    const question = "['', '1'].includes(new URLSearchParams(location.search).get('agent'))", count = async (file, texts) => {
      return Promise.all(texts.map(async text => (await astrayIn(file, text)).length));
    };
    expect(await count('host/web-js/conform.mjs', [`evaluate(\`(() => !${question})()\`);`, `const client = \`<script>const seq=\${n};const driven = ${question};</script>\`;`]))
      .toEqual([0, 0]);
    expect(await count('host/web-js/dev.mjs', [String.raw`evaluate("new URLSearchParams(location.search).has('ag\\u0065nt')");`,
      "const client = `<script>const driven = q.get(/* query */ 'agent') !== null;</script>`;"])).toEqual([1, 1]);
    expect(await count('host/web/page.js', [`evaluate(\`(() => ${question})()\`);`,
      `const page = \`<script>const AGENT_ADMITTED = true; on = AGENT_ADMITTED && ${question};</script>\`;`])).toEqual([1, 1]);
    // A script counts as HTML runs one: whatever the case of its tag, by its type decoded and trimmed, by `language`
    // where it has no type, inside a template; one HTML runs that does not parse fails too, and one it does not run
    // reads nothing
    const reads = 'new URLSearchParams(location.search).has("agent")';
    const run = ['<script type="text/javascript">', '<SCRIPT>', '<script type=" Module ">', '<script language="JavaScript">', '<script data-type="json">',
      '<script type="text/java&#x73;cript">', '<script type="text&sol;javascript">', '<script type language="vbscript">', '<template><script>'];
    expect(await count('host/web/page.js', run.map(tag => `const page = \`${tag}${reads}</script>\`;`))).toEqual(run.map(() => 1));
    expect(await count('host/web/page.js', ['const page = `<script>const = ;</script>`;'])).toEqual([1]);
    expect(await count('host/web/page.js', ['<script type="application/json">', '<script type="importmap">', '<script language="vbscript">']
      .map(tag => `const page = \`${tag}${reads}</script>\`;`))).toEqual([0, 0, 0]);
    expect(await count('host/web/shell.html', [`<!doctype html><script>${reads}</script>`])).toEqual([1]);
    // A script a `data:` URL holds runs too, percent-encoded or base64, and so does a `data:` frame's own script, whether
    // HTML names the URL or a script loads it, and a `javascript:` URL's code runs, percent-encoded
    expect(await count('host/web/page.js', [`<script src="data:text/javascript,${encodeURIComponent(reads)}"></script>`,
      `<script src="data:text/javascript;base64,${btoa(reads)}"></script>`, `<iframe src="data:text/html,${encodeURIComponent(`<script>${reads}</script>`)}"></iframe>`,
      `<svg><script href="data:text/javascript,${encodeURIComponent(reads)}"></script></svg>`].map(tag => `const page = \`${tag}\`;`))).toEqual([1, 1, 1, 1]);
    expect(await count('host/web/page.js', [`import("data:text/javascript,${encodeURIComponent(reads)}");`,
      `frame.src = "data:text/html,${encodeURIComponent(`<script>${reads}</script>`)}";`, `link.href = "javascript:${encodeURIComponent(reads)}";`])).toEqual([1, 1, 1]);
    // A driver asks the question behind whatever it needs, and nothing else
    expect([(await astrayIn('scripts/agent-web.mjs', `const carried = header && ${question.replace('location.search', 'url.search')};`)).length,
      (await astrayIn('scripts/agent-web.mjs', 'const carried = header && url.searchParams.has("agent");')).length]).toEqual([0, 1]);
    // Code an attribute holds runs too: an event handler, a `javascript:` URL, a tab in its scheme dropped, and a
    // `srcdoc` document's script; a title that names `javascript:` runs nothing
    const quoted = reads.replace(/"/g, '&quot;');
    expect(await count('host/web/page.js', [`<button onclick="${quoted}">`, `<a href="java&#x09;script:${quoted}">`,
      `<iframe srcdoc="&lt;script&gt;${quoted}&lt;/script&gt;">`, `<p title="javascript: ${quoted}">`].map(tag => `const page = \`${tag}\`;`))).toEqual([1, 1, 1, 0]);
    // A guarded read in a module a release does not gate ships its constant as written
    const launched = question.replace('new URLSearchParams(location.search)', "new URL(globalThis.performance?.getEntriesByType?.('navigation')[0]?.name ?? location.href).searchParams");
    const guarded = `const AGENT_ADMITTED = true;\nconst on = AGENT_ADMITTED && ${launched};`;
    expect([(await astrayIn('host/web/page.js', guarded)).length, (await astrayIn('host/web-js/clock.js', guarded)).length]).toEqual([1, 0]);
    // A reader that asks the URL the page is at, not the one it launched at
    expect((await astrayIn('host/web-js/clock.js', `const AGENT_ADMITTED = true;\nconst on = AGENT_ADMITTED && ${question};`)).length).toEqual(1);
    // A whole classic script that is no module, a tagged template's raw text and a script a held script holds count
    // too, and a classic script that reads nothing reads nothing
    const single = reads.replace(/"/g, "'");
    expect(await count('host/web/page.js', [`const script = "<!-- legacy\\nwindow.driven = ${single};";`,
      'const script = String.raw`const label = "\\n"; window.driven = ' + reads + ';`;', `const outer = \`const inner = "${single}";\`;`,
      'const note = "<!-- note\\nlet x = 1;";', `const html = "<!--\\n--><script>window.driven = ${single};</script>";`,
      "const script = \"window.driven = new URLSearchParams(location.search).get(...[], 'agent');\";"])).toEqual([1, 1, 1, 0, 1, 1]);
  } finally { await browser.close(); }
}, 60000);

// A release build holds each read of the parameter in a host file to the one question, guarded in place by the file's
// own AGENT_ADMITTED, which it writes false; the app's own code is not the gate's and stays as written, and a string or
// a comment reads nothing
test('a release build refuses a host file whose read of the agent parameter is not the guarded question, and no app code', () => {
  const asks = "const on = AGENT_ADMITTED && ['', '1'].includes(new URLSearchParams(location.search).get('agent'));";
  expect(gateAgent(`const AGENT_ADMITTED = true;\n${asks}`, 'reader.js')).toBe(`const AGENT_ADMITTED = false;\n${asks}`);
  expect(gateAgent(`export const AGENT_ADMITTED=true\n${asks}`, 'reader.js')).toBe(`export const AGENT_ADMITTED=false\n${asks}`);
  // A guard that names no constant of the module's own reads a global, an import whose value the gate cannot see, or
  // another constant than the one it declares
  for (const unbound of [asks, `// const AGENT_ADMITTED = true;\n${asks}`, `import { AGENT_ADMITTED } from './clock.js';\n${asks}`,
    `const AGENT_ADMITTED$1 = true;\n${asks}`]) {
    expect(() => gateAgent(unbound, 'reader.js')).toThrow('reads ?agent other than as');
  }
  for (const read of ["const on = ['', '1'].includes(new URLSearchParams(location.search).get('agent'));",
    "const off = !AGENT_ADMITTED || !['', '1'].includes(params.get('agent'));", "const on = y && AGENT_ADMITTED && ['', '1'].includes(params.get('agent'));",
    'const on = AGENT_ADMITTED && new URLSearchParams(location.search).has ("agent");', "const on = AGENT_ADMITTED && params['has']('agent');",
    'const on = AGENT_ADMITTED && facts?.get("agent");', 'const on = AGENT_ADMITTED && params.get?.("agent") != null;',
    'const on = AGENT_ADMITTED && params.get(/* query */ "agent") === "0";', 'const on = AGENT_ADMITTED && params.get("ag\\u0065nt");',
    'const on = AGENT_ADMITTED && params[`has`](`agent`);', 'const on = AGENT_ADMITTED && (params.get)(("agent"));',
    'const on = AGENT_ADMITTED && params[("get")]("agent");', 'const on = AGENT_ADMITTED && (params?.get)("agent");',
    'const on = AGENT_ADMITTED && !!params.get`agent`;', "const on = AGENT_ADMITTED && !!params.has(...['agent']);",
    "const on = AGENT_ADMITTED && ['', '1'].includes(params.get(...[], 'agent'));", "const on = AGENT_ADMITTED && !!params.get(['agent']);",
    "const on = AGENT_ADMITTED && [1n].includes(params.get('agent'));"]) {
    expect(() => gateAgent(`const AGENT_ADMITTED = true;\n${read}`, 'reader.js')).toThrow('reads ?agent other than as');
    expect(gateAgent(read, 'app.js', { host: false })).toBe(read);
  }
  // Inside the guarded question parentheses, comments and escapes change nothing, and an optional chain is seen through
  for (const read of ["(params?.get)('agent')", "params.get?.('agent')", "(params.get)(('agent'))", "params.get('ag\\u0065nt')", "params.get(/* q */ 'agent')"]) {
    const code = `const AGENT_ADMITTED = true;\nconst on = AGENT_ADMITTED && ['', '1'].includes(${read});`;
    expect(gateAgent(code, 'reader.js')).toBe(code.replace('= true', '= false'));
  }
  // A binding of the name other than the module's one constant, which would let a guard read another value than the one
  // the gate writes false
  for (const declared of ['let AGENT_ADMITTED = true;', 'const AGENT_ADMITTED = !0;', 'export const { AGENT_ADMITTED } = { AGENT_ADMITTED: true };',
    'const { AGENT\\u005fADMITTED } = facts;', 'const admitted = true; export { admitted as AGENT_ADMITTED };', "export * as AGENT_ADMITTED from './x.js';",
    "import { admitted as AGENT_ADMITTED } from './x.js';", 'export function AGENT_ADMITTED() { return true; }', 'function ask(AGENT_ADMITTED = true) {}',
    'try { ask(); } catch (AGENT_ADMITTED) {}', 'function unused() { const AGENT_ADMITTED = true; }', 'if (x) { const AGENT_ADMITTED = true; }']) {
    expect(() => gateAgent(declared, 'admission.js')).toThrow('a guard must read the one constant');
  }
  expect(() => gateAgent("const = ;\n(params.get)(('agent'));", 'reader.js')).toThrow('does not parse');
  expect(() => gateAgent(`const AGENT_ADMITTED = true;\nwith ({ AGENT_ADMITTED: true }) { on = ${asks.slice('const on = '.length)} }`, 'reader.js')).toThrow('uses `with`');
  // A bundle may rename the declaration and export it under the name
  const renamed = 'const AGENT_ADMITTED$1 = true;\nexport { AGENT_ADMITTED$1 as AGENT_ADMITTED };';
  expect(gateAgent(renamed, 'navigation.js')).toBe(renamed.replace('= true', '= false'));
  for (const text of [`text(${JSON.stringify(asks)}) // ${asks}`, "const label = \"roles.get('agent')\";", 'const value = cache.get(...keys);']) {
    expect(gateAgent(text, 'reader.js')).toBe(text);
  }
  const own = 'const AGENT_ADMITTED = true; text("const AGENT_ADMITTED = true;");';
  expect(gateAgent(own, 'app.js', { host: false })).toBe(own);
});

// The data module realm's glue bundles storage-environment.js, where a drive's seed and scratch store are read. The
// bundler folds the admission constant: a development build keeps the reads, and a release, admitting none, drops them.
test("the wasm host's data module glue admits agent mode in development and not in a release", async () => {
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/module-glue-test-'));
  try {
    const reads = await Promise.all([false, true].map(async production => {
      const file = resolve(dir, `${production ? 'release' : 'development'}.js`);
      await bundleModuleGlue(root, file, { production, minify: false });
      return readFileSync(file, 'utf8').match(/\.get\((['"])agent\1\)/g)?.length ?? 0;
    }));
    expect(reads[0]).toBeGreaterThan(0);
    expect(reads[1]).toBe(0);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// The readers the browser test below cannot reach, asked the same queries: the wasm host's seed for a Rust source's
// entropy under either admission, a drive's store and seed, and the dev loop's reload when a rebuild comes
test('the seed, the store and the dev reload ask the one question', () => {
  const glue = readFileSync(resolve(root, 'host/web/glue.js'), 'utf8');
  const imports = parseSync('glue.js', glue).program.body.map(entry => entry.declaration ?? entry)
    .find(statement => statement.type === 'VariableDeclaration' && statement.declarations[0].id.name === 'dataImports');
  const launchURL = parseSync('glue.js', glue).program.body.find(statement => statement.type === 'VariableDeclaration' && statement.declarations[0].id.name === 'launchURL');
  // The page's navigation entry names its launch URL (Navigation Timing)
  const navigation = launch => ({ now: () => performance.now(), getEntriesByType: type => type === 'navigation' ? [{ name: launch }] : [] });
  const seed = (admitted, href, launch = href) => new Function('AGENT_ADMITTED', 'location', 'globalThis',
    `${glue.slice(launchURL.start, launchURL.end)}\n${glue.slice(imports.start, imports.end)}\nreturn dataImports.agent_seed();`)(admitted, { href }, { performance: navigation(launch) });
  const dev = readFileSync(resolve(root, 'host/web/dev.js'), 'utf8');
  const reloads = (search, launch = `http://127.0.0.1/${search}`) => {
    let stream, reloaded = false;
    runInNewContext(dev, {
      TextEncoder, TextDecoder, URL, URLSearchParams, AbortController, Response, setTimeout, clearTimeout, performance: navigation(launch), crypto, Event, console: { error() {} },
      location: { origin: 'http://127.0.0.1', href: `http://127.0.0.1/${search}`, search, reload: () => { reloaded = true; } },
      EventSource: class { constructor() { stream = this; } }, addEventListener() {}, dispatchEvent() {},
      sessionStorage: { getItem: () => null, setItem() {}, removeItem() {} }, localStorage: { getItem: () => '1', setItem() {} },
      document: { body: { appendChild: el => el }, head: { appendChild: el => el }, createElement: () => ({ remove() {} }), getElementById: () => null, querySelector: () => null,
        documentElement: { style: { fontSize: '', getPropertyValue: () => '', setProperty() {}, removeProperty() {} } } },
      getComputedStyle: () => ({ colorScheme: 'light dark', fontSize: '16px' }), navigator: { sendBeacon() {} }, requestAnimationFrame: callback => callback(),
      matchMedia: () => ({ matches: false, addEventListener() {} }), fetch: async () => new Response('{}'), exact: { ready: Promise.resolve() },
    });
    stream.onmessage({ data: JSON.stringify({ fresh: 1 }) });
    return reloaded;
  };
  const queries = ['', '?agent', '?agent=', '?agent=1', '?agent=0', '?agent=true'];
  const got = queries.map(query => {
    const href = `http://127.0.0.1/${query}${query ? '&' : '?'}seed=7&storage=x`;
    return [query, seed(true, href), seed(false, href), agentSeed(href), storageKey('test.agent', href), reloads(query)];
  });
  expect(got).toEqual(queries.map(query => {
    const asks = ['?agent', '?agent=', '?agent=1'].includes(query);
    return [query, asks ? 7 : -1, -1, asks ? 7 : null, asks ? 'test.agent/agent/x' : 'test.agent', !asks];
  }));
  // Agent mode is the launch's: a route the app pushes with `?agent=1` turns no reader to it, and one it pushes without
  // the parameter turns none from it
  const clock = readFileSync(resolve(root, 'host/web-js/clock.js'), 'utf8');
  const declared = parseSync('clock.js', clock).program.body.map(entry => entry.declaration ?? entry)
    .find(statement => statement.type === 'VariableDeclaration' && statement.declarations[0].id.name === 'agentMode').declarations[0].init;
  const agentMode = (search, launch) => new Function('AGENT_ADMITTED', 'location', 'globalThis', `return ${clock.slice(declared.start, declared.end)};`)(
    true, { search, href: `http://127.0.0.1/${search}` }, { performance: navigation(launch) })();
  const launched = (current, launch) => [seed(true, `http://127.0.0.1/${current}`, launch), reloads(current, launch), agentMode(current, launch)];
  expect([launched('?agent=1&seed=7', 'http://127.0.0.1/?seed=7'), launched('?seed=7', 'http://127.0.0.1/?agent=1&seed=7')])
    .toEqual([[-1, true, false], [7, false, true]]);
});

// The capture script starts a page that waits for the reader at once under agent mode, so a driver finds its runtime;
// the runtime then reads the parameter as the capture script did (`clock.agent`, which loads `agent.js`).
chromeTest('a page asks for agent mode with `agent` or `agent=1`, and only a development build admits it', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const dir = mkdtempSync(resolve(root, 'target/agent-parameter-test-')), served = {};
  let browser;
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Agent Parameter Test', app: { id: 'test.agent-parameter', name: 'Agent Parameter Test' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  home "/" render=build activate=interaction

component App
  state n = 0
  action add
    n = n + 1
  view
    main
      button "Add" press=add
`);
    // The two builds write only their own output directories, so they run at once
    const build = async (out, args = []) => {
      const child = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'agent-parameter-test', '--out', out, '--render', 'js', ...args], {
        cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe',
      });
      const [status, stdout, stderr] = await Promise.all([child.exited, new Response(child.stdout).text(), new Response(child.stderr).text()]);
      expect(status, stdout + stderr).toBe(0);
    };
    const development = resolve(dir, 'development'), release = resolve(dir, 'release');
    await Promise.all([build(development), build(release, ['--production'])]);
    // A release writes every admission false, in `.gen` and in what it serves, and what it serves keeps no read of the
    // parameter, each folded away behind its constant
    expect(readdirSync(release, { recursive: true }).filter(file => file.endsWith('.js')).filter(file => {
      const code = readFileSync(resolve(release, file), 'utf8');
      return /\bAGENT_ADMITTED(?:\$\d+)?\s*=\s*(?:true|!0)\b/.test(code) || !file.startsWith('.gen') && /\.(?:get|getAll|has)\(\s*(['"`])agent\1\s*\)/.test(code);
    })).toEqual([]);
    browser = await suiteBrowser();
    for (const [build, dist] of [['development', development], ['release', release]]) {
      served[build] = Bun.serve({ hostname: '127.0.0.1', port: 0, fetch: request => new Response(Bun.file(resolve(dist, new URL(request.url).pathname.slice(1) || 'index.html'))) });
    }
    // Each row: the build, the query, whether the page started without a press, and whether its runtime has the agent's
    // adapter and runs on the agent's clock. Each page has a browser context of its own, so the rows run at once.
    const asks = ['?agent', '?agent=1'];
    const want = ['development', 'release'].flatMap(build => ['', '?agent', '?agent=1', '?agent=0', '?agent=true']
      .map(query => { const on = build === 'development' && asks.includes(query); return [build, query, on, on, on]; }));
    // Whether the started runtime has the agent's adapter and runs on the agent's clock, once every stage of its boot,
    // each of which may name a later one as `exact.ready` (the agent's adapter last), is done
    const agentOf = page => page.evaluate(async () => {
      for (let ready; ready !== globalThis.exact.ready;) await (ready = globalThis.exact.ready);
      return [typeof globalThis.exact.agentSettled === 'function', globalThis.exact.clock.agent];
    });
    const got = await Promise.all(want.map(async ([build, query]) => {
      const page = await browser.newPage();
      try {
        await page.goto(served[build].url.href + query);
        await page.evaluate(() => new Promise(done => {
          new PerformanceObserver((_, o) => { o.disconnect(); requestAnimationFrame(() => requestAnimationFrame(() => setTimeout(done))); })
            .observe({ type: 'paint', buffered: true });
        }));
        const started = await page.evaluate(() => !!globalThis.exact?.ready);
        // A press starts any page that still waits, as a reader's would
        if (!started) await page.getByRole('button', { name: 'Add' }).click();
        await page.waitForFunction(() => globalThis.exact?.ready);
        return [build, query, started, ...await agentOf(page)];
      } finally {
        await page.close();
      }
    }));
    expect(got).toEqual(want);
    // Agent mode is the launch's: a development page launched without the parameter, its URL rewritten to ask for it
    // before a press starts its runtime, stays out of agent mode, since its navigation entry names the launch
    const page = await browser.newPage();
    try {
      await page.goto(served.development.url.href);
      await page.evaluate(() => history.replaceState(null, '', '?agent=1'));
      await page.getByRole('button', { name: 'Add' }).click();
      await page.waitForFunction(() => globalThis.exact?.ready);
      expect(await agentOf(page)).toEqual([false, false]);
    } finally {
      await page.close();
    }
  } finally {
    await browser?.close();
    for (const server of Object.values(served)) server.stop(true);
    rmSync(dir, { recursive: true, force: true });
  }
}, 240000);
