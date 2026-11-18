#!/usr/bin/env bun
/**
 * boot — the gate's module-graph check (rules/RULES.md §The gate): parse and count
 * the module graph reachable before first pixel. A count cannot flake like a
 * timer, and parsing keeps valid HTML/ESM spellings from bypassing the count.
 */
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, realpathSync } from 'node:fs';
import vm from 'node:vm';
import { dirname, relative, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

const MODULES = {
  'host/web/glue.js': [
    'host/web/wasm/agent.js',
    'host/web/wasm/boot.js',
    'host/web/wasm/commit.js',
    'host/web/wasm/context.js',
    'host/web/wasm/flow.js',
    'host/web/wasm/input.js',
    'host/web/wasm/views.js',
    'host/web/engagement.js',
  ],
  'host/web/navigation.js': [
    'host/web/navigation/agent-parameters.js',
    'host/web/navigation/assets.js',
    'host/web/navigation/controls.js',
    'host/web/navigation/environment.js',
    'host/web/navigation/focus.js',
    'host/web/navigation/grant-origins.js',
    'host/web/navigation/layers.js',
    'host/web/navigation/lists.js',
    'host/web/navigation/markup.js',
    'host/web/navigation/motion.js',
    'host/web/navigation/network.js',
    'host/web/navigation/popover.js',
    'host/web/navigation/requests.js',
    'host/web/navigation/routes.js',
    'host/web/navigation/scroll.js',
    'host/web/navigation/selection.js',
    'host/web/navigation/sheet-stack.js',
    'host/web/navigation/store.js',
    'host/web/navigation/type.js',
    'host/web/navigation/urls.js',
    'host/web/grant-admission.js',
  ],
};
const ALLOWED = new Set([...Object.keys(MODULES), ...Object.values(MODULES).flat()]);
// A served document may run exactly one inline script before its glue: the
// host's capture script (LLP 1048.000 D6, 1048.001 D5; Charlie, 2026-09-23),
// which records taps, once the page has painted starts an idle page's wasm
// download, stops it when a link leaves, and restores the saved scheme; it holds no
// app logic. Pinned by hash and byte-capped; changing it means changing this
// hash, in review.
const CAPTURE = { path: 'host/web/capture.js', maxBytes: 1536,
  sha256: 'adbe55dd47b584fee60950d44247a4c91fe2165f34e5731e3633a328c6f8f21c' };
const ROOT = resolve(fileURLToPath(new URL('..', import.meta.url)));
const rootPath = path => relative(ROOT, path).replaceAll('\\', '/');

// The page is deliberately small, so a fail-closed tokenizer is preferable
// to a dependency. It recognizes HTML comments, quoted and unquoted
// attributes, and reports malformed script tags instead of ignoring them.
function scriptTags(html, problems) {
  const scripts = [];
  let at = 0;
  while (at < html.length) {
    const lt = html.indexOf('<', at);
    if (lt < 0) break;
    if (html.startsWith('<!--', lt)) {
      const end = html.indexOf('-->', lt + 4);
      if (end < 0) { problems.push('malformed HTML: unterminated comment'); break; }
      at = end + 3;
      continue;
    }
    const head = html.slice(lt, lt + 8).toLowerCase();
    if (!head.startsWith('<script') || !/[\s/>]/.test(html[lt + 7] ?? '')) { at = lt + 1; continue; }
    let quote = null;
    let end = lt + 7;
    for (; end < html.length; end++) {
      const ch = html[end];
      if (quote) { if (ch === quote) quote = null; continue; }
      if (ch === '"' || ch === "'") quote = ch;
      else if (ch === '>') break;
    }
    if (end >= html.length || quote) { problems.push('malformed HTML: unterminated <script> tag'); break; }
    const text = html.slice(lt + 7, end);
    const attrs = new Map();
    let i = 0;
    while (i < text.length) {
      while (/\s/.test(text[i] ?? '')) i++;
      if (i >= text.length || text[i] === '/') break;
      const start = i;
      while (i < text.length && !/[\s=/>]/.test(text[i])) i++;
      if (i === start) { problems.push('malformed HTML: unsupported <script> attribute syntax'); break; }
      const name = text.slice(start, i).toLowerCase();
      while (/\s/.test(text[i] ?? '')) i++;
      let value = '';
      if (text[i] === '=') {
        i++;
        while (/\s/.test(text[i] ?? '')) i++;
        if (i >= text.length) { problems.push(`malformed HTML: ${name}= has no value`); break; }
        if (text[i] === '"' || text[i] === "'") {
          const q = text[i++];
          const valueStart = i;
          while (i < text.length && text[i] !== q) i++;
          if (i >= text.length) { problems.push(`malformed HTML: unterminated ${name} value`); break; }
          value = text.slice(valueStart, i++);
        } else {
          const valueStart = i;
          while (i < text.length && !/[\s>]/.test(text[i])) i++;
          value = text.slice(valueStart, i);
        }
      }
      if (attrs.has(name)) problems.push(`malformed HTML: duplicate <script> ${name} attribute`);
      else attrs.set(name, value);
    }
    scripts.push(attrs);
    const lower = html.toLowerCase();
    const close = lower.indexOf('</script', end + 1);
    if (close < 0 || !/[\s>]/.test(html[close + 8] ?? '') || html.indexOf('>', close + 8) < 0) {
      problems.push('malformed HTML: <script> has no closing tag');
      break;
    }
    attrs.text = html.slice(end + 1, close);
    at = html.indexOf('>', close + 8) + 1;
  }
  return scripts;
}

// Fail closed: boot sources may not spell a dynamic import, even in comments
// or literals. Comments between the keyword and parenthesis count as whitespace.
// The runtime's module parser below checks static imports and syntax.
function hasDynamicImport(source) {
  for (const match of source.matchAll(/\bimport\b/g)) {
    let at = match.index + match[0].length;
    while (at < source.length) {
      if (/\s/.test(source[at])) { at++; continue; }
      if (source.startsWith('/*', at)) {
        const end = source.indexOf('*/', at + 2);
        if (end < 0) break;
        at = end + 2;
      } else if (source.startsWith('//', at)) {
        at += 2;
        while (at < source.length && !/[\r\n\u2028\u2029]/.test(source[at])) at++;
      } else {
        if (source[at] === '(') return true;
        break;
      }
    }
  }
  return false;
}

function localModule(root, from, specifier, problems) {
  if (specifier.startsWith('./') || specifier.startsWith('../')) return resolve(dirname(from), specifier);
  if (specifier.startsWith('/')) return resolve(root, '.' + specifier);
  problems.push(`unsupported non-local module specifier in ${rootPath(from)}: ${specifier}`);
  return null;
}

function run() {
  const root = ROOT;
  const page = resolve(root, 'host/web/index.html');
  const problems = [];
  if (!existsSync(page)) {
    console.log('boot — host/web/index.html is missing; nothing to count, and the check must not pass on nothing.');
    return 1;
  }
  const html = readFileSync(page, 'utf8');
  if (!existsSync(resolve(root, 'rules/RULES.md')) || !/App JS executed before first pixel\s*\|\s*none/i.test(readFileSync(resolve(root, 'rules/RULES.md'), 'utf8'))) {
    problems.push('rules/RULES.md no longer declares "App JS executed before first pixel | none"; boot has no budget.');
  }
  const scripts = scriptTags(html, problems);
  const external = scripts.filter((attrs) => attrs.has('src'));
  const inline = scripts.length - external.length;
  if (!external.length) problems.push('the page has no external host-glue script to count');
  if (inline) problems.push(`${inline} inline <script> block(s) in the page; only the glue module may run.`);
  const queue = [];
  for (const attrs of external) {
    const specifier = attrs.get('src');
    if (!specifier) { problems.push('a <script src> has an empty source'); continue; }
    const file = localModule(root, page, specifier, problems);
    if (file) queue.push(file);
  }

  const seen = new Set();
  const sources = new Map();
  while (queue.length) {
    const file = queue.shift();
    if (seen.has(file)) continue;
    seen.add(file);
    const rel = rootPath(file);
    if (!ALLOWED.has(rel)) problems.push(`module before first pixel is outside the allowed host paths: ${rel}`);
    if (rel.startsWith('apps/')) problems.push(`app JS before first pixel: ${rel}`);
    if (!existsSync(file)) { problems.push(`missing module: ${rel}`); continue; }
    const source = readFileSync(file, 'utf8');
    sources.set(file, source);
    let requests;
    try { requests = new vm.SourceTextModule(source, { identifier: file }).dependencySpecifiers; }
    catch (error) { problems.push(`invalid module syntax in ${rel}: ${error.message}`); continue; }
    if (hasDynamicImport(source)) problems.push(`dynamic import before first pixel in ${rel}`);
    for (const request of requests) {
      const next = localModule(root, file, request, problems);
      if (next) queue.push(next);
    }
  }
  // Interaction documents replace the ordinary entry with one independent
  // host module. Check that entry too: it may not import a runtime or app graph.
  const documentEntry = 'host/web/document-glue.js';
  let documentBytes = 0;
  try {
    const source = readFileSync(resolve(root, documentEntry), 'utf8');
    documentBytes = Buffer.byteLength(source);
    const imports = new vm.SourceTextModule(source).dependencySpecifiers;
    if (imports.length || hasDynamicImport(source)) problems.push(`${documentEntry} must have no imports before first pixel`);
  } catch (error) { problems.push(`invalid document entry: ${error.message}`); }
  // The capture script: its bytes as pages carry them, pinned by hash, capped,
  // a classic script that imports nothing. A built document, when this
  // checkout has one, may run it and nothing else inline.
  const capture = { bytes: 0, sha256: '', checked: null, found: 0, stale: false };
  try {
    const text = (readFileSync(resolve(root, 'host/web/scheme.js'), 'utf8') + readFileSync(resolve(root, CAPTURE.path), 'utf8')).trimEnd();
    capture.bytes = Buffer.byteLength(text);
    capture.sha256 = createHash('sha256').update(text).digest('hex');
    if (capture.sha256 !== CAPTURE.sha256) problems.push(`${CAPTURE.path} changed (sha256 ${capture.sha256}); served pages may run only the pinned capture script (${CAPTURE.sha256})`);
    if (capture.bytes > CAPTURE.maxBytes) problems.push(`${CAPTURE.path} is ${capture.bytes} B, over its ${CAPTURE.maxBytes} B cap`);
    new vm.Script(text);
    if (/\bimport\b/.test(text)) problems.push(`${CAPTURE.path} must import nothing`);
  } catch (error) { problems.push(`invalid capture script: ${error.message}`); }
  const built = resolve(root, 'host/web/dist/index.html');
  if (existsSync(built)) {
    const page = readFileSync(built, 'utf8');
    const tags = scriptTags(page, problems);
    if (tags.some((attrs) => attrs.get('type') === 'application/vnd.exact.checkpoint')) {
      capture.checked = 'host/web/dist/index.html';
      // The composer writes one inline script, the capture script; a build
      // from before its current bytes carries an older one, which only a
      // rebuild updates. More than one is never the composer's.
      const run = tags.filter((attrs) => !attrs.has('src') && attrs.get('type') !== 'application/vnd.exact.checkpoint');
      capture.found = run.filter((attrs) => createHash('sha256').update(attrs.text).digest('hex') === CAPTURE.sha256).length;
      capture.stale = run.length === 1 && !capture.found;
      if (run.length > 1) problems.push(`${capture.checked}: ${run.length - 1} inline <script> block(s) besides the capture script`);
    }
  }
  const wasm = (html.match(/\.wasm/g) ?? []).length + [...sources.values()].reduce((n, source) => n + (source.match(/\.wasm/g) ?? []).length, 0);
  const modules = [...sources].map(([file, source]) => ({
    path: rootPath(file), bytes: Buffer.byteLength(source),
    sha256: createHash('sha256').update(source).digest('hex'),
  }));
  const shipped = Object.keys(MODULES).filter(file => seen.has(resolve(root, file)));
  const report = { modules: shipped.length, javascript_bytes: modules.reduce((n, m) => n + m.bytes, 0),
    html_bytes: Buffer.byteLength(html), files: modules, wasm_references: wasm,
    document_entry: { modules: 1, javascript_bytes: documentBytes },
    capture_script: { inline: 1, bytes: capture.bytes, max_bytes: CAPTURE.maxBytes, sha256: capture.sha256, checked: capture.checked, found: capture.found, stale: capture.stale }, problems };
  if (process.argv.includes('--json')) console.log(JSON.stringify(report));
  else {
    console.log(`boot — modules reachable before first pixel: ${shipped.length} (${shipped.join(', ') || 'none'}); wasm references: ${wasm}`);
    console.log(`  reachable JavaScript: ${report.javascript_bytes} B; page: ${report.html_bytes} B (diagnostic sizes, no byte budget)`);
    console.log(`  interaction document: 1 host module (${documentBytes} B), no imports`);
    const built = !capture.checked ? '' : capture.found ? `; ${capture.checked} runs it and no other`
      : capture.stale ? `; ${capture.checked} was built before its current bytes (rebuild to check it)` : `; ${capture.checked} runs no inline script`;
    console.log(`  served documents: 1 inline host script, ${CAPTURE.path} (${capture.bytes} B of ${CAPTURE.maxBytes}; sha256 ${capture.sha256}, pinned)${built}`);
    for (const m of modules) console.log(`  ${m.path}: ${m.bytes} B; sha256 ${m.sha256}`);
    for (const problem of problems) console.log('  ' + problem);
    console.log(problems.length ? `${problems.length} violation(s).` : 'Allowed import paths only. This does not prove generic content, constant startup work, or absence of runtime-loaded code; metrics measures built artifacts and browser work.');
  }
  return problems.length ? 1 : 0;
}

const entry = process.argv[1]
  && realpathSync(resolve(process.argv[1])) === realpathSync(fileURLToPath(import.meta.url));
if (entry) process.exitCode = run();
