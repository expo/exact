// A contract's sources are answered by `app.ts` beside it. Each source call the Contract compiler reports (`contract
// symbols`, kind `source`) is written as a property of app.ts's `sources`, typed by the `Sources` the compiler
// generates (`contract types`, `<file>.d.ts`), so TypeScript itself links the call to the function that answers it.
import { existsSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';

// A source's name is names.js's to define, reference and rename, as the compiler resolved it; this mapping of the same
// place answers only what leads into app.ts.
const quote = JSON.stringify;

export function sources(ctx) {
  const calls = (ctx.contract(['symbols', ctx.fileName, '--stdin'])?.definitions ?? [])
    .filter(d => d.kind === 'source' && ctx.here(d)).map(d => [d.name, ctx.span(d)]).filter(([name, at]) => at && ctx.content.slice(at[0], at[0] + at[1]) === name && /^[A-Za-z_$][\w$]*$/.test(name));
  if (!calls.length) return;
  // The compiler's own paths: app.ts in the contract's directory, the declarations at `<file>.d.ts`.
  if (existsSync(resolve(dirname(ctx.fileName), 'app.ts'))) {
    ctx.write(`import type { Sources as $Sources } from ${quote(`./${basename(ctx.fileName)}.d.ts`)};\nimport type * as $app from "./app.ts";\n`);
    // An app.ts that exports its `sources` unannotated is read as written; otherwise the contract's `Sources`.
    ctx.write('declare const $sources: typeof $app extends { sources: infer S } ? S : $Sources;\n');
  } else {
    // A module, so two contracts' `$sources` never meet.
    ctx.write(`export {};\ndeclare const $sources: {\n${[...new Set(calls.map(([name]) => name))].map(n => `/** source \`${n}\` */\n${n}: unknown;\n`).join('')}};\n`);
  }
  for (const [name, at] of calls) { ctx.write('$sources.'); ctx.name(name, at, ctx.FEATURE.HOVER | ctx.FEATURE.IMPLEMENTATION | ctx.FEATURE.REFERENCES); ctx.write(';\n'); }
}
