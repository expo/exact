import { test, expect } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { webCompiler } from './module.mjs';

// The date is the reader's device's (LLP 1039 D1): a page the JS renderer makes reads the unknown date, never the
// data module's answer, and its checkpoint leaves the source for the reader's runtime to ask, as the runner's does.
test('a JavaScript-rendered page reads the unknown date and leaves it to the reader', async () => {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/render-date-test-'));
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Render Date Test', app: { id: 'test.render-date', name: 'Render Date Test' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), `routes nav
  home "/" render=build

shape Time
  epochAtZero: number
  utcOffset: number
  timeZone: string

component Dated
  resource time = exactTime() as shape Time
  view
    main
      head title=\`\${time.epochAtZero}/\${time.utcOffset}/\${time.timeZone}\`
      text "Dated"
`);
    writeFileSync(resolve(dir, 'app.ts'), "export const grants = '';\nexport const answer = () => { throw new Error('a host fact reached the data module'); };\n");
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'render-date-test', '--out', resolve(dir, 'dist'), '--render', 'js'],
      { cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe' });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    const page = readFileSync(resolve(dir, 'dist/index.html'), 'utf8');
    expect(/<title>([^<]*)<\/title>/.exec(page)?.[1]).toBe('0/0/UTC');
    const checkpoint = /<script type="application\/vnd\.exact\.checkpoint"[^>]*>([^<]*)<\/script>/.exec(page)?.[1];
    expect(checkpoint).toBeDefined();
    expect(checkpoint).not.toContain('exactTime');
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 120000);
