import { test, expect } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { webCompiler } from './module.mjs';

// The date is the reader's device's (LLP 1039 D1): a page the JS renderer makes reads the unknown date, never the
// data module's answer, and its checkpoint leaves the source for the reader's runtime to ask, as the runner's does.
// `shapes` and `component` are the app's Contract after its route; returns the page's title and checkpoint.
async function render(shapes, component) {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/render-date-test-'));
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Render Date Test', app: { id: 'test.render-date', name: 'Render Date Test' }, host: { web: {} } }));
    writeFileSync(resolve(dir, 'app.contract'), `routes nav\n  home "/" render=build\n\n${shapes}\n${component}`);
    writeFileSync(resolve(dir, 'app.ts'), "export const grants = '';\nexport const answer = () => { throw new Error('a host fact reached the data module'); };\n");
    const build = Bun.spawn([process.execPath, 'host/web-js/build.mjs', 'render-date-test', '--out', resolve(dir, 'dist'), '--render', 'js'],
      { cwd: root, env: { ...process.env, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe' });
    const [status, stdout, stderr] = await Promise.all([build.exited, new Response(build.stdout).text(), new Response(build.stderr).text()]);
    expect({ status, error: status ? stdout + stderr : '' }).toEqual({ status: 0, error: '' });
    const page = readFileSync(resolve(dir, 'dist/index.html'), 'utf8');
    return {
      title: /<title>([^<]*)<\/title>/.exec(page)?.[1],
      checkpoint: /<script type="application\/vnd\.exact\.checkpoint"[^>]*>([^<]*)<\/script>/.exec(page)?.[1],
    };
  } finally { rmSync(dir, { recursive: true, force: true }); }
}

test('a JavaScript-rendered page reads the unknown date and leaves it to the reader', async () => {
  const { title, checkpoint } = await render(`shape Time
  epochAtZero: number
  utcOffset: number
  timeZone: string
`, `component Dated
  resource time = exactTime() as shape Time
  view
    main
      head title=\`\${time.epochAtZero}/\${time.utcOffset}/\${time.timeZone}\`
      text "Dated"
`);
  expect(title).toBe('0/0/UTC');
  expect(checkpoint).toBeDefined();
  expect(checkpoint).not.toContain('exactTime');
}, 120000);

// Each reader is answered by its own fields: the source's declared type is its first reader's.
test('each reader of the date reads its own fields', async () => {
  const { title } = await render(`shape Place
  locale: string

shape Zone
  timeZone: string

shape Time
  epochAtZero: number
  utcOffset: number
  timeZone: string

shape Instant
  epochAtZero: number
`, `component Dated
  resource place = exactTime() as shape Place
  resource zone = exactTime() as shape Zone
  resource time = exactTime() as shape Time
  resource instant = exactTime() as shape Instant
  view
    main
      head title=\`\${place.locale}/\${zone.timeZone}/\${time.epochAtZero}/\${time.utcOffset}/\${time.timeZone}/\${instant.epochAtZero}\`
      text "Dated"
`);
  expect(title).toBe('en-US/UTC/0/0/UTC/0');
}, 120000);
