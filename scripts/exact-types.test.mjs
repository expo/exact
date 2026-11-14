import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';

function inDir(body) {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-types-'));
  try { return body(dir); } finally { rmSync(dir, { recursive: true, force: true }); }
}

test('refreshTypes keeps canvas types regardless of how app.ts exports drawing', async () => {
  const { refreshTypes } = await import('./exact.mjs');
  inDir(dir => {
    writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    text "Ready"\n');
    const types = resolve(dir, 'app.contract.d.ts');
    let expected;
    for (const declaration of ["export * from './optional';", 'export var draw;', 'export const { draw } = made;', '// export const draw', 'export {};']) {
      writeFileSync(resolve(dir, 'app.ts'), declaration);
      refreshTypes(dir, undefined, { quiet: true });
      const text = readFileSync(types, 'utf8');
      // Whole, wherever it sits: the language model's tool types follow it (9bbf8784c).
      assert.ok(text.includes(readFileSync(resolve(import.meta.dir, '../canvas/types.d.ts'), 'utf8')), declaration);
      expected ??= text;
      assert.equal(text, expected);
      const written = statSync(types).mtimeMs;
      assert.equal(refreshTypes(dir, undefined, { quiet: true }), false);
      assert.equal(statSync(types).mtimeMs, written);
    }
  });
});

// LLP 1116.001 D4.5: `exact contract build` of an app's app.contract rewrites the types its app.ts imports
// when a source's signature changed, and only then, saying so in one line on stderr (under `--json`, nothing on
// stderr: the array is the whole reply); `contract types` agrees.
test('contract build regenerates app.contract.d.ts when a signature changed, and only then', () => inDir(dir => {
  const contract = (shape) => `shape Greeting\n  text: string\n${shape}\ncomponent App\n  resource greeting = greeting("x") as shape Greeting\n  view\n    text greeting.text\n`;
  writeFileSync(resolve(dir, 'app.contract'), contract(''));
  writeFileSync(resolve(dir, 'app.ts'), "export const appId = 'com.example.types';\n");
  const exact = (...args) => spawnSync(process.execPath, [resolve(import.meta.dir, 'exact.mjs'), ...args], { cwd: dir, encoding: 'utf8', timeout: 600_000 });
  const build = () => exact('contract', 'build', 'app.contract');
  const json = () => exact('contract', 'build', 'app.contract', '--json');
  const types = resolve(dir, 'app.contract.d.ts');
  let r = json();
  assert.equal(r.status, 0, r.stderr);
  assert.equal(r.stdout, '[]\n', 'the build\'s reply is one JSON value');
  assert.equal(r.stderr, '', 'and the whole of it');
  assert.ok(existsSync(resolve(dir, 'app.contract.d.ts')), 'written all the same');
  rmSync(resolve(dir, 'app.contract.d.ts'));
  r = build();
  assert.equal(r.status, 0, r.stderr);
  assert.match(r.stderr, /^app\.contract\.d\.ts: written \(the types app\.ts imports\)$/m);
  assert.equal(readFileSync(types, 'utf8'), exact('contract', 'types', 'app.contract').stdout, 'what contract types prints');
  const written = statSync(types).mtimeMs;
  r = build();
  assert.doesNotMatch(r.stderr, /app\.contract\.d\.ts/, 'nothing changed, nothing said');
  assert.equal(statSync(types).mtimeMs, written, 'nor written');
  writeFileSync(resolve(dir, 'app.contract'), contract('  count: number\n'));
  r = build();
  assert.match(r.stderr, /^app\.contract\.d\.ts: regenerated, since a source's signature changed/m);
  assert.match(readFileSync(types, 'utf8'), /"count": T\d+;/);
  // A build that fails leaves the types as they were; another file than app.contract never writes them.
  writeFileSync(resolve(dir, 'app.contract'), 'not a contract');
  assert.notEqual(build().status, 0);
  assert.match(readFileSync(types, 'utf8'), /"count"/);
  writeFileSync(resolve(dir, 'other.contract'), contract(''));
  assert.doesNotMatch(exact('contract', 'build', 'other.contract').stderr, /app\.contract\.d\.ts/);
  // In an app's directory (an app.json beside it) the build type-checks app.ts as every host's build does,
  // its report on stderr, or under `--json` in the array; outside one (above) it does not.
  writeFileSync(resolve(dir, 'app.contract'), contract(''));
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Types', app: { id: 'com.example.types', name: 'Types' } }));
  writeFileSync(resolve(dir, 'app.ts'), "export const appId: number = 'com.example.types';\n");
  r = build();
  assert.equal(r.status, 1, r.stderr);
  assert.match(r.stderr, /app\.ts\(1,14\): error TS2322/);
  r = json();
  assert.equal(r.status, 1, r.stderr);
  assert.equal(r.stderr, '');
  // One diagnostic per TypeScript error (the fixture's app.ts also lacks the exports the entry reads).
  const error = JSON.parse(r.stdout).find((d) => d.file === 'app.ts');
  assert.deepEqual({ ...error, message: undefined }, { id: 'typescript-TS2322', severity: 'error', message: undefined, file: 'app.ts', line: 1, col: 14, end_col: 14, related: [] });
  assert.match(error.message, /not assignable to type 'number'/);
  // Unsaved text (`--stdin`, issue #21) is the compiler's reply alone: no types are written from it, and app.ts is
  // not checked against types no file holds.
  const saved = readFileSync(types, 'utf8');
  r = spawnSync(process.execPath, [resolve(import.meta.dir, 'exact.mjs'), 'contract', 'build', 'app.contract', '--json', '--stdin'], { cwd: dir, input: contract('  extra: number\n'), encoding: 'utf8', timeout: 600_000 });
  assert.equal(r.status, 0, r.stderr);
  assert.deepEqual([r.stdout, r.stderr], ['[]\n', '']);
  assert.equal(readFileSync(types, 'utf8'), saved);
}), 600_000); // The first call may compile the compiler.


const backendApp = dir => {
  writeFileSync(resolve(dir, 'app.json'), JSON.stringify({ name: 'Backend', app: { id: 'test.backend', name: 'Backend' } }));
  writeFileSync(resolve(dir, 'app.contract'), 'component App\n  view\n    text "Ready"\n');
  writeFileSync(resolve(dir, 'app.ts'), "export const grants = ''; export const answer = () => 'Ready';\n");
};

test('backend refusals identify the selection before the TypeScript check', () => inDir(dir => {
  backendApp(dir);
  const root = resolve(import.meta.dir, '..'), origin = 'https://backend.example';
  writeFileSync(resolve(dir, 'exact.mjs'), `import {run} from ${JSON.stringify(resolve(root, 'scripts/app-cli.mjs'))};
process.exit(await run({dir: import.meta.dir, name: 'backend', exact2: ${JSON.stringify(root)}, args: process.argv.slice(2)}));`);
  const base = { ...process.env, EXACT_APP_DIR: dir, EXACT_FEEDBACK: '0' };
  delete base.EXACT_BACKEND_ORIGIN; delete base.EXACT_BACKEND_PROVENANCE;
  mkdirSync(resolve(dir, '.exact'));
  for (const selector of ['EXACT_BACKEND_ORIGIN', '.exact/backend-origin', '--backend', 'verify --backend']) {
    rmSync(resolve(dir, '.exact/backend-origin'), { force: true });
    const env = { ...base };
    if (selector === 'EXACT_BACKEND_ORIGIN') env.EXACT_BACKEND_ORIGIN = origin;
    if (selector === '.exact/backend-origin') writeFileSync(resolve(dir, '.exact/backend-origin'), origin);
    const args = selector === '--backend'
      ? [resolve(dir, 'exact.mjs'), 'agent', 'web', '--backend', origin, 'tree']
      : selector === 'verify --backend' ? ['scripts/verify.mjs', 'web', '--app', dir, '--backend', origin]
      : ['host/web-js/build.mjs', 'backend', '--check', '--out', resolve(dir, 'dist')];
    const r = spawnSync(process.execPath, args, { cwd: root, env, encoding: 'utf8', timeout: 60000 });
    const report = r.stdout + r.stderr;
    assert.equal(r.status, 1, report);
    assert.ok(report.includes(`error [backend-selection]: ${selector === 'verify --backend' ? '--backend' : selector}:`), report);
    assert.match(report, /const origin/);
    assert.match(report, /import\.meta\.env\.NAME/);
    assert.doesNotMatch(report, /failed the type check/);
  }
}), 60000);

test('contract JSON preserves the backend-selection diagnostic id', () => inDir(dir => {
  backendApp(dir);
  const env = { ...process.env, EXACT_BACKEND_ORIGIN: 'https://backend.example', EXACT_APP_DIR: dir };
  delete env.EXACT_BACKEND_PROVENANCE;
  const r = spawnSync(process.execPath, [resolve(import.meta.dir, 'exact.mjs'), 'contract', 'build', 'app.contract', '--json'], { cwd: dir, env, encoding: 'utf8', timeout: 60000 });
  assert.equal(r.status, 1, r.stderr);
  assert.equal(r.stderr, '');
  const errors = JSON.parse(r.stdout).filter(d => d.severity === 'error');
  assert.equal(errors.length, 1, r.stdout);
  assert.equal(errors[0].id, 'backend-selection');
  assert.equal(errors[0].file, 'app.ts');
  assert.match(errors[0].message, /^EXACT_BACKEND_ORIGIN:/);
}), 60000);

test('a native bake preserves backend flag provenance through its Cargo environment', () => {
  const root = resolve(import.meta.dir, '..'), dir = mkdtempSync(resolve(root, 'target/backend-native-'));
  try {
    backendApp(dir);
    mkdirSync(resolve(dir, 'linux/src'), { recursive: true });
    writeFileSync(resolve(dir, 'Cargo.toml'), '[workspace]\nmembers=["linux"]\nresolver="2"\n');
    writeFileSync(resolve(dir, 'linux/Cargo.toml'), '[package]\nname="backend-linux"\nversion="0.1.0"\nedition="2021"\n');
    writeFileSync(resolve(dir, 'linux/src/main.rs'), 'fn main() {}\n');
    writeFileSync(resolve(dir, 'check.mjs'), `import {configure} from ${JSON.stringify(resolve(root, 'js/bake/src/typescript.mjs'))};
import {parseSync} from 'rolldown/utils';
try { configure(import.meta.dir, false, parseSync); }
catch (error) { console.error('error [' + error.code + ']: ' + error.message); process.exit(1); }
`);
    writeFileSync(resolve(dir, 'linux/build.rs'), `fn main() {
  let status = std::process::Command::new(std::env::var("BUN").unwrap())
    .arg(${JSON.stringify(resolve(dir, 'check.mjs'))}).status().unwrap();
  std::process::exit(status.code().unwrap_or(1));
}\n`);
    writeFileSync(resolve(dir, 'bake.mjs'), `import {buildBake, bakeTarget} from ${JSON.stringify(resolve(root, 'scripts/app.mjs'))};
import {selectBackendOrigin} from ${JSON.stringify(resolve(root, 'js/bake/src/typescript.mjs'))};
const dir = import.meta.dir, env = { EXACT_UPDATE_TRUST: 'development' };
selectBackendOrigin('https://chosen.example', env);
const app = { dir, workspace: dir, target: dir + '/target', name: 'backend', id: 'test.backend',
  manifest: { app: { id: 'test.backend', name: 'Backend' }, rust: false }, crate: kind => 'backend-' + kind };
try { buildBake(app, 'linux', bakeTarget('linux'), { profile: 'dev', output: dir + '/bakes', env }); }
catch (error) { console.error(error.message); process.exit(1); }
`);
    mkdirSync(resolve(dir, '.exact'));
    for (const saved of [false, true]) {
      if (saved) writeFileSync(resolve(dir, '.exact/backend-origin'), 'https://saved.example');
      const r = spawnSync(process.execPath, [resolve(dir, 'bake.mjs')], { cwd: root, env: process.env, encoding: 'utf8', timeout: 60000 });
      assert.equal(r.status, 1, r.stdout + r.stderr);
      assert.match(r.stdout + r.stderr, /error \[backend-selection\]: --backend:.*const origin/);
      assert.doesNotMatch(r.stdout + r.stderr, /failed the type check/);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60000);

test('a checkout without node_modules refuses the type check before it starts, and says to install', async () => {
  const { checkTypes, modulesMissing, modulesRefusal } = await import('./exact.mjs');
  assert.equal(modulesMissing(), false, 'this checkout is installed');
  inDir((dir) => {
    const root = resolve(dir, 'checkout'), app = resolve(dir, 'app'), ran = resolve(dir, 'ran');
    mkdirSync(root);
    mkdirSync(app);
    writeFileSync(resolve(app, 'app.ts'), 'export const grants = \'\';\n');
    writeFileSync(resolve(app, 'app.json'), '{}\n');
    writeFileSync(resolve(dir, 'builder.mjs'), `require('node:fs').writeFileSync(${JSON.stringify(ran)}, '');`);
    assert.equal(modulesMissing(root), true);
    const checked = checkTypes(app, { capture: true, builder: resolve(dir, 'builder.mjs'), root });
    assert.deepEqual(checked, { status: 1, report: `error: ${modulesRefusal(root)}\n` });
    assert.match(checked.report, /node_modules is missing: run `bun install --frozen-lockfile` in .*checkout/);
    assert.ok(!existsSync(ran), 'the check never started');
  });
});

test('a path printed as a command reads back whole in a shell, a space or quote in it too', async () => {
  const { shellPath } = await import('./exact.mjs');
  assert.equal(shellPath('/Users/a/exact/scripts/exact.mjs'), '/Users/a/exact/scripts/exact.mjs');
  for (const path of ['/tmp/exact checkout/scripts/exact.mjs', "/tmp/it's here/exact.mjs"]) {
    const read = spawnSync('/bin/sh', ['-c', `printf %s ${shellPath(path)}`], { encoding: 'utf8' });
    assert.equal(read.stdout, path);
  }
});
