import { test, expect } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { webCompiler } from './module.mjs';
import { renderer } from './render.mjs';
import { agentStream } from '../web/storage-environment.js';

const root = resolve(import.meta.dir, '../..');

function randomFloat(b) {
  const hi = ((b[0] << 16) | (b[1] << 8) | b[2]) & 0x1fffff;
  const lo = ((b[3] << 24) | (b[4] << 16) | (b[5] << 8) | b[6]) >>> 0;
  return (hi * 4294967296 + lo) / 9007199254740992;
}

async function buildApp(appContract, appTs, extraEnv = {}) {
  if (webCompiler().cmd === 'cargo') throw new Error('build the compiler first: cargo build -p exact-web-js');
  const dir = mkdtempSync(resolve(root, 'target/llp1195-render-test-'));
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({
    name: 'LLP 1195 Test',
    app: { id: 'test.llp1195', name: 'LLP 1195 Test' },
    host: { web: {} },
  }));
  writeFileSync(resolve(dir, 'app.contract'), appContract);
  writeFileSync(resolve(dir, 'app.ts'), appTs);
  const dist = resolve(dir, 'dist');
  const build = Bun.spawn(
    [process.execPath, 'host/web-js/build.mjs', 'llp1195-test', '--out', dist, '--render', 'js'],
    { cwd: root, env: { ...process.env, ...extraEnv, EXACT_APP_DIR: dir }, stdout: 'pipe', stderr: 'pipe' },
  );
  const [status, stdout, stderr] = await Promise.all([
    build.exited,
    new Response(build.stdout).text(),
    new Response(build.stderr).text(),
  ]);
  return { dir, dist, status, output: stdout + stderr };
}

test('test-clock-preload virtualizes Date, performance.now, TZ, and seeded Math.random()', async () => {
  const dir = mkdtempSync(resolve(root, 'target/llp1195-preload-test-'));
  try {
    const expectedStream = agentStream(42, 'ts-random');
    const expectedRandoms = [
      randomFloat(expectedStream(7)),
      randomFloat(expectedStream(7)),
      randomFloat(expectedStream(7)),
    ];
    const probe = resolve(dir, 'probe.test.mjs');
    writeFileSync(probe, `
      import { test, expect } from 'bun:test';
      test('deterministic clock and seed', () => {
        expect(Date.now()).toBe(Date.parse('2026-06-15T12:30:00.000Z'));
        expect(new Date().getTime()).toBe(Date.parse('2026-06-15T12:30:00.000Z'));
        expect(performance.now()).toBe(0);
        expect(new Date(0).getTimezoneOffset()).toBe(0);
        const r = [Math.random(), Math.random(), Math.random()];
        expect(r).toEqual(${JSON.stringify(expectedRandoms)});
        globalThis.__exactTestClock.advance(2500);
        expect(Date.now()).toBe(Date.parse('2026-06-15T12:30:02.500Z'));
        expect(performance.now()).toBe(2500);
      });
    `);
    const run = Bun.spawn(
      [process.execPath, 'test', '--preload', resolve(root, 'scripts/test-clock-preload.mjs'), probe],
      {
        cwd: root,
        env: {
          ...process.env,
          EXACT_TEST_EPOCH: '2026-06-15T12:30:00.000Z',
          EXACT_TEST_SEED: '42',
          EXACT_TEST_TIME_ZONE: 'UTC',
        },
        stdout: 'pipe',
        stderr: 'pipe',
      },
    );
    const [status, stdout, stderr] = await Promise.all([
      run.exited,
      new Response(run.stdout).text(),
      new Response(run.stderr).text(),
    ]);
    expect({ status, output: status ? stdout + stderr : '' }).toEqual({ status: 0, output: '' });
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});

test('render=build pins Date.prototype local getters to UTC even under a non-UTC host TZ', async () => {
  const { dir, dist, status, output } = await buildApp(
    `routes nav\n  home "/" render=build\n\ncomponent App\n  resource stamp = epochUtc() as shape string\n  view\n    main\n      head title=stamp\n      text stamp\n`,
    `export const grants = '';\nexport function answer(source: string) {\n  if (source === 'epochUtc') {\n    const d = new Date(0);\n    return \`\${d.getTimezoneOffset()}/\${d.getHours()}/\${d.toString()}\`;\n  }\n  throw new Error(source);\n}\n`,
    { TZ: 'America/Los_Angeles' },
  );
  try {
    expect({ status, output: status ? output : '' }).toEqual({ status: 0, output: '' });
    const html = readFileSync(resolve(dist, 'index.html'), 'utf8');
    expect(html).toContain('<title>0/0/Thu Jan 01 1970 00:00:00 GMT+0000 (Coordinated Universal Time)</title>');
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}, 120000);

test('render=build refuses ambient Date.now(), Math.random(), and module-init clock reads', async () => {
  const contract = `routes nav\n  home "/" render=build\n\ncomponent App\n  resource v = read() as shape string\n  view\n    main\n      text v\n`;
  const cases = [
    ['in-source Date.now()', `export const grants = '';\nexport const answer = () => String(Date.now());\n`, 'read the ambient clock'],
    ['in-source Math.random()', `export const grants = '';\nexport const answer = () => String(Math.random());\n`, 'drew randomness'],
    ['module top-level Date.now()', `export const grants = '';\nconst boot = Date.now();\nexport const answer = () => 'pure:' + typeof boot;\n`, 'read the ambient clock'],
  ];
  for (const [label, appTs, expectedError] of cases) {
    const res = await buildApp(contract, appTs);
    try {
      expect(res.status, label).not.toBe(0);
      expect(res.output, label).toContain(expectedError);
    } finally {
      rmSync(res.dir, { recursive: true, force: true });
    }
  }
}, 180000);

test('render=request allows ambient Date.now() and marks clocked with keep = 0', async () => {
  const res = await buildApp(
    `routes nav\n  home "/" render=request\n\ncomponent App\n  resource v = nowStr() as shape string\n  view\n    main\n      text v\n`,
    `export const grants = '';\nexport const answer = () => String(Date.now());\n`,
  );
  try {
    expect({ status: res.status, output: res.status ? res.output : '' }).toEqual({ status: 0, output: '' });
    const render = renderer(res.dist);
    render.ahead = false;
    const page = await render('/');
    expect(page.status).toBe(200);
    expect(page.clocked).toBe(true);
    expect(page.keep).toBe(0);
  } finally {
    rmSync(res.dir, { recursive: true, force: true });
  }
}, 120000);
