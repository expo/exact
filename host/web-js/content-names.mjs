// A build's own files named by their content (LLP 1153.004): a file the build makes, which only the build, a renderer
// or code the build bundles names, is written as `<stem>-<hash>.<ext>`, and a host may keep it for good. `routes.json`
// `files` maps each such file from the name it had (a chunk, which had none, from its own) to the name it is written as.
import { createHash } from 'node:crypto';
import { existsSync, readFileSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { gzipSync } from 'node:zlib';
import { rolldown } from 'rolldown';
import { parseSync, Visitor } from 'rolldown/utils';

/** `name` with the first 8 base64url characters of `bytes`' SHA-256 before its extension: 48 bits, as Rolldown's
 * `[hash]`. */
export function contentName(name, bytes) {
  const dot = name.lastIndexOf('.'), hash = createHash('sha256').update(bytes).digest('base64url').slice(0, 8);
  return `${name.slice(0, dot)}-${hash}${name.slice(dot)}`;
}

/** A Rolldown plugin that writes each renamed file's new name where one of Exact's own modules (`own(id)`) names it: a
 * string literal that is its old name, with or without `./`, as the first argument of `new URL(…)`, `import(…)`, or a
 * call to `fetch`, `load` or `__files`, the calls the runtime names its files in. It reads the module's syntax, so a
 * string that only spells a name or a call is left as it is: a generated module embeds author values (an app's id, a
 * launch service's config). Rolldown then names each chunk from bytes that hold the new names. The names are read now,
 * before the bundle names the entry. */
export function contentNames(files, own) {
  const names = Object.keys(files);
  if (!names.length) return { name: 'exact-content-names' };
  const CALLS = new Set(['fetch', 'load', '__files']);
  return { name: 'exact-content-names', transform(code, id) {
    if (!own(id) || !names.some(name => code.includes(name))) return null;
    const edits = [];
    const named = node => {
      const name = node?.type === 'Literal' && typeof node.value === 'string' ? node.value.replace(/^\.\//, '') : null;
      if (name !== null && Object.hasOwn(files, name)) edits.push([node.start, node.end, `${code[node.start]}${node.value.slice(0, -name.length)}${files[name]}${code[node.start]}`]);
    };
    new Visitor({
      NewExpression: node => { if (node.callee.type === 'Identifier' && node.callee.name === 'URL') named(node.arguments[0]); },
      ImportExpression: node => named(node.source),
      CallExpression: node => {
        const callee = node.callee.type === 'MemberExpression' && !node.callee.computed ? node.callee.property : node.callee;
        if (callee.type === 'Identifier' && CALLS.has(callee.name)) named(node.arguments[0]);
      },
    }).visit(parseSync(id, code).program);
    let renamed = code;
    for (const [from, to, literal] of edits.sort((a, b) => b[0] - a[0])) renamed = renamed.slice(0, from) + literal + renamed.slice(to);
    return edits.length ? renamed : null;
  } };
}

/** The name the build at `dist` writes the file `name` under: its content name, or `name` itself when the file keeps its
 * name or the build has no table. */
export function builtFile(dist, name) {
  const table = resolve(dist, 'routes.json');
  return (existsSync(table) && JSON.parse(readFileSync(table, 'utf8')).files?.[name]) || name;
}

/** Split the browser's copy of the compiled strings initializer only when the other tables save a request's worth.
 * Read its argument from the program's syntax: a translation can contain any of the program's punctuation. */
export function splitLanguages(plan, files, out) {
  if (!plan.includes('strings([[')) return null;
  const {body} = parseSync('app.js', plan).program;
  const app = body.find(node => node.type === 'ExportDefaultDeclaration').declaration;
  const init = app.body.body.find(node => node.expression?.callee?.name === 'strings')?.expression.arguments[0];
  if (!init || init.elements.length < 2) return null;
  const others = plan.slice(init.elements[1].start, init.end - 1);
  if (gzipSync('[' + others + ']').length < 4096) return null;
  const rows = JSON.parse(plan.slice(init.start, init.end));
  for (const row of rows.slice(1)) {
    const name = `strings-${row[0]}.json`, bytes = JSON.stringify(row[2]);
    row[2] = files[name] = contentName(name, bytes);
    writeFileSync(resolve(out, row[2]), bytes);
  }
  let page = plan.slice(0, init.start) + 'languageTables' + plan.slice(init.end);
  const choice = body.find(node => node.type === 'ImportDeclaration' && node.source.value === './chosen.js');
  if (choice) page = page.slice(0, choice.source.start) + '"./chosen-language.js"' + page.slice(choice.source.end);
  return {
    page: page + '\nexport const languageTables=' + JSON.stringify(rows) + ';\n',
    choosesLocale: !!choice,
    names: rows.map(([name, , file]) => [name, typeof file === 'string' ? file : null]),
  };
}

/** The shell and both renderers carry this script among their CSP-hashed head entries. */
export async function languageHead(gen, names, storage, entry, development) {
  const input = resolve(gen, 'language-head.js');
  const browser = development ? '(["", "1"].includes(new URLSearchParams(location.search).get("agent")) ? new URLSearchParams(location.search).get("locale") ?? "en-US" : navigator.language)' : 'navigator.language';
  writeFileSync(input, `import {languageHead} from './language-assets.js'; languageHead(${JSON.stringify(names)},${JSON.stringify(storage)},${browser},${JSON.stringify('./' + entry)});`);
  const build = await rolldown({ input, treeshake: { moduleSideEffects: false } });
  try {
    const {output} = await build.generate({format: 'iife', minify: true, codeSplitting: false});
    return `<script>${output[0].code.trim()}</script>\n`;
  } finally { await build.close(); }
}
