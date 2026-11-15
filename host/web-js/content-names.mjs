// A build's own files named by their content (LLP 1153.004): a file the build makes, which only the build, a renderer
// or code the build bundles names, is written as `<stem>-<hash>.<ext>`, and a host may keep it for good. `routes.json`
// `files` maps each such file from the name it had (a chunk, which had none, from its own) to the name it is written as.
import { createHash } from 'node:crypto';
import { existsSync, readFileSync } from 'node:fs';
import { resolve } from 'node:path';
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
