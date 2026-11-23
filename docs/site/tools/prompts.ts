import { existsSync, readFileSync } from 'node:fs';
import { dirname, join } from 'node:path';

// The prompt shown above a page: its own file under `dir`, else its folder's, with {{SOURCE}} the page's source.
// A top-level document has no folder prompt.
export function promptOf(dir: string, source: string): { text: string; fromFolder: boolean } {
  const own = join(dir, source);
  const folder = dirname(source);
  const fallback = folder === '.' ? null : join(dir, `${folder}.md`);
  const file = existsSync(own) ? own : fallback && existsSync(fallback) ? fallback : null;
  if (!file) return { text: '', fromFolder: false };
  // A function replacement keeps `$&` and its kin in a source literal.
  const text = readFileSync(file, 'utf8').trim().replaceAll('{{SOURCE}}', () => source);
  return { text: `${text}\n\n`, fromFolder: file === fallback };
}
