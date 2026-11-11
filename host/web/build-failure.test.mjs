import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import process from 'node:process';
import { webBuildFailure } from '../../scripts/app-cli.mjs';

const ROOT = resolve(import.meta.dir, '../..');

test('web failure: a Contract refusal keeps its location, code, reason, related locations and excerpt', () => {
  for (const file of ['app.contract', '/tmp/a chat/MessageRow.contract', 'warning row.contract', 'Compiling row.contract', 'Finished row.contract', 'Running row.contract']) {
    const refusal = [
      `${file}:9:5 [web-target] a dynamic \`exit_animation\` (Animations) is not one declaration; not in the JS target`,
      '  app.contract:4:5: `Greeting` is instantiated here',
      '  9 |     text "Hello" -exact-exit-animation=leaving',
      '    |     ^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^^',
    ].join('\n');
    expect(webBuildFailure(`   Compiling exact-web-js\n    Finished dev profile\n     Running exact-web-js js app.contract\n${refusal}\n`).join('\n')).toBe(refusal);
  }
});

test('web failure: TypeScript, ambient, package and tool diagnostics keep their useful lines', () => {
  const diagnostics = [
    'app.ts(2,8): error TS2305: Module has no exported member missing.',
    'logic clock.ts:2:24: Date.now() is unavailable in data sources; use exactTime',
    'warning clock.ts:3:24: setInterval() is unavailable in data sources; use a task',
    'Compiling clock.ts:4:24: Math.random() is unavailable in data sources; use random()',
    'pkg/index.js imports Node\'s built-in fs',
    'Error: module outside captured app: /tmp/shared.ts',
    'source links are not captured: shared.ts',
    'tsconfig: noEmit must be true',
    'app.ts: import.meta.env.SECRET is not declared in app.json env',
    'Error: two copies of a package were captured',
    'note: use one installed version',
    'hermesc is not on PATH',
    'module.wasm unoptimized',
    'TypeError: cannot read the module',
  ];
  const raw = ['   Compiling exact-web-js', ...diagnostics, '    at build.mjs:10:5', 'Bun v1.4.2'];
  expect(webBuildFailure(raw.join('\n')).join('\n')).toBe(diagnostics.join('\n').replace('Error: module outside captured app: ', 'module outside captured app: '));
  expect(webBuildFailure('one\ntwo\nthree\nfour\n').join('\n')).toBe('two\nthree\nfour');
});

test('web failure: warnings and generated type notices survive beside the refusal once', () => {
  const warning = 'app.contract:2:3 warning [lower-colour-one-scheme] one colour';
  const notice = 'app.contract.d.ts: generated types changed';
  const refusal = 'app.contract:9:5 [web-target] not in the JS target\n  9 | text "Hello"\n    | ^^^^';
  expect(webBuildFailure(`${notice}\nwarning: build note\n${warning}\n${refusal}`).join('\n')).toBe(`${notice}\nwarning: build note\n${refusal}\n${warning}`);
  expect(webBuildFailure(`${refusal}\n${warning}\nwarning: build note`).join('\n')).toBe(`${refusal}\n${warning}\nwarning: build note`);
});

test('web failure: multiple refusals and preceding errors are retained with all following context', () => {
  const output = [
    'TypeError: a separate failure',
    'app.contract:9:5 [web-target] first refusal',
    '  app.contract:4:5: `Greeting` is instantiated here',
    'app.contract:12:5 [lower-style] second refusal',
    ' 12 |     text "Hello"',
    '    |     ^^^^^^^^^^^^',
  ].join('\n');
  expect(webBuildFailure(output).join('\n')).toBe(output);
});

test('the web build reports the JS target Contract refusal with its component location', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact web refusal-'));
  try {
    writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Refusal', app: { id: 'test.exact.refusal', name: 'Refusal' }, rust: false }));
    writeFileSync(resolve(dir, 'app.contract'), [
      'component App', '  view', '    column', '      Notes()', '',
      'component Notes', '  state kind = "markdown"', '  view', '    textarea markup=kind aria-label="Notes"', '',
    ].join('\n'));
    const built = spawnSync(process.execPath, [resolve(ROOT, 'host/web/build.mjs'), 'refusal', '--render', 'none'], {
      cwd: ROOT, encoding: 'utf8', env: { ...process.env, EXACT_APP_DIR: dir, EXACT_WEB_DIST: resolve(dir, 'dist') },
    });
    expect(built.status, built.stderr).toBe(1);
    expect(built.stderr).toMatch(/app\.contract:9:5 \[web-target\] a dynamic `markup` on a text field is not in the JS target/);
    expect(built.stderr).toMatch(/app\.contract:4:7: `Notes` is instantiated here/);
    expect(built.stderr).toContain('9 |');
    expect(built.stderr).toContain('the web build (the JS target) failed');
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60_000);
