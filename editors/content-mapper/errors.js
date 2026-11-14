// The file's errors are the Contract compiler's own (`contract build --errors-only`), at the places it names.
import { existsSync } from 'node:fs';
import { basename, dirname, resolve } from 'node:path';

export const DIAGNOSTIC_CODE = 9100; // every Contract diagnostic; its own id leads the message

/** The app this file is built as part of: itself, or the nearest `app.contract` beside or above it whose sources the
 * compiler says include it (an app imports `./lib/row.contract` as well as `./row.contract`). */
function root(ctx) {
  if (basename(ctx.fileName) === 'app.contract') return null;
  const file = ctx.real(ctx.fileName);
  for (let dir = dirname(ctx.fileName), above; ; dir = above) {
    const app = resolve(dir, 'app.contract');
    if (existsSync(app) && (ctx.contract(['sources', app], '')?.sources ?? []).some(s => ctx.real(s.path) === file)) return app;
    if ((above = dirname(dir)) === dir) return null;
  }
}

export function errors(ctx) {
  if (ctx.fileName.endsWith('.test.contract')) return; // a test file is `contract test`'s to run, and no app for `contract build`
  // A file an app uses is built with its app, as the compiler builds it: alone it is no app. The app's other files are
  // read from disk, this one as the editor holds it.
  const app = root(ctx), found = ctx.contract(['build', app ?? ctx.fileName, '--json', '--errors-only', ...(app ? ['--stdin-as', ctx.fileName] : ['--stdin'])]);
  for (const e of (found ?? []).filter(ctx.here)) {
    const at = ctx.span({ ...e, end_col: e.end_col ?? e.col + 1 }); if (!at) continue;
    ctx.out.diagnostics.push({ start: at[0], length: Math.max(1, at[1]), code: DIAGNOSTIC_CODE, messageText: `${e.id}: ${e.message}` });
  }
}
