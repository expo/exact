import { readFileSync, readdirSync } from 'node:fs';
import { join } from 'node:path';

const SITE = join(import.meta.dir, '..');
const files = ['app.contract', ...readdirSync(join(SITE, 'components')).filter(f => f.endsWith('.contract')).map(f => join('components', f))];
const textLinks = files.flatMap(file => {
  const lines = readFileSync(join(SITE, file), 'utf8').split('\n');
  return lines.flatMap((line, i) => {
    const link = line.match(/^(\s*)link\b/);
    if (!link) return [];
    const depth = link[1].length + 2;
    const children: string[] = [];
    for (const next of lines.slice(i + 1)) {
      const at = next.match(/^(\s*)(\S+)/);
      if (!at) continue;
      if (at[1].length < depth) break;
      if (at[1].length === depth && at[2] !== 'if' && at[2] !== 'else') children.push(at[2]);
    }
    return children.length > 0 && children.every(c => c === 'text' || c === 'image') ? [`${file}:${i + 1}`] : [];
  });
});
if (textLinks.length > 0) {
  console.error(`link-check: the web host draws a link that holds only text and images in the accent colour, with an underline on hover. Put each link's contents in a row:\n${textLinks.map(at => `  ${at}`).join('\n')}`);
  process.exit(1);
}
console.log(`link-check: no link in ${files.length} files holds only text and images`);
