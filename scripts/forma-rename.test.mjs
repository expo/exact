import { expect, test } from 'bun:test';
import { spawn, spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, mkdtempSync, readFileSync, renameSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join, resolve } from 'node:path';
import { createInterface } from 'node:readline';
import { designPath } from './forma-design-path.mjs';

const ROOT = resolve(import.meta.dir, '..');
const compiler = resolve(ROOT, 'target/debug/contract');
const app = f => {
  const dir = mkdtempSync(join(tmpdir(), 'forma-rename-'));
  try {
    writeFileSync(join(dir, 'app.contract'), 'component App\n  view\n    text "x"\n');
    writeFileSync(join(dir, 'app.forma'), 'forma app\n');
    return f(dir);
  } finally { rmSync(dir, { recursive: true, force: true }); }
};
const run = (dir, args, input = '') => spawnSync(compiler, args, { cwd: dir, input, encoding: 'utf8' });

test('rename refusals apply to parsed design paths, leaving prefixes, modes and output paths to their validators', () => app(dir => {
  mkdirSync(join(dir, 'out'));
  for (const args of [
    ['import', 'shadcn', resolve(ROOT, 'forma/tests/import/shadcn-blue.css'), '--prefix', 'brand.tamber'],
    ['import', '--prefix', 'brand.tamber', 'shadcn', resolve(ROOT, 'forma/tests/import/shadcn-blue.css')],
    ['ir', '.', '-o', 'out/tokens.tamber'],
    ['ir', '-o', 'out/tokens.tamber', '.'],
  ]) {
    const r = run(dir, ['forma', ...args]);
    expect(r.status, r.stderr).toBe(0);
  }
  expect(readFileSync(join(dir, 'out/tokens.tamber'), 'utf8')).toContain('"tokens"');
  const mode = run(dir, ['forma', 'eval', '.', '--mode', 'brand.tamber']);
  expect(mode.stderr).toContain('a mode is `axis=value`');
  expect(mode.stderr).not.toContain('rename');
  for (const args of [['check', 'missing.tamber'], ['gen', 'ts', 'missing.tamber'], ['export', 'dtcg', 'missing.tamber'], ['import-plan', 'missing.tamber']]) {
    expect(run(dir, ['forma', ...args]).stderr).toContain('rename missing.tamber to missing.forma');
  }
}));

test('an old journal refuses startup before any design rename and preserves every byte', () => app(dir => {
  mkdirSync(join(dir, '.exact'));
  const files = { '.exact/tamber.journal': '{"renames":[]}', '.exact/tamber-write.tmp': 'unfinished text', 'app.tamber': 'old design' };
  for (const [file, text] of Object.entries(files)) writeFileSync(join(dir, file), text);
  for (const oldDesign of [true, false]) {
    if (!oldDesign) rmSync(join(dir, 'app.tamber'));
    const r = run(dir, ['forma', 'serve', '--stdio', '.']);
    expect(r.status).not.toBe(0);
    expect(r.stderr).toContain('.exact/tamber.journal');
    expect(r.stderr).toContain('resolve the unfinished write');
    expect(r.stderr).not.toContain('rename app.tamber');
    expect(readFileSync(join(dir, '.exact/tamber.journal'), 'utf8')).toBe(files['.exact/tamber.journal']);
    expect(readFileSync(join(dir, '.exact/tamber-write.tmp'), 'utf8')).toBe(files['.exact/tamber-write.tmp']);
    expect(readFileSync(join(dir, 'app.forma'), 'utf8')).toBe('forma app\n');
    if (oldDesign) expect(readFileSync(join(dir, 'app.tamber'), 'utf8')).toBe('old design');
    expect(existsSync(join(dir, '.exact/forma.lock'))).toBe(false);
  }
}));

test('an old lock refuses startup and names how to release it', () => app(dir => {
  mkdirSync(join(dir, '.exact'));
  writeFileSync(join(dir, '.exact/tamber.lock'), '');
  const r = run(dir, ['forma', 'serve', '--stdio', '.']);
  expect(r.status).not.toBe(0);
  expect(r.stderr).toContain('stop the tamber server');
  expect(r.stderr).toContain('remove .exact/tamber.lock');
  expect(existsSync(join(dir, '.exact/forma.lock'))).toBe(false);
  expect(existsSync(join(dir, '.exact/tamber.lock'))).toBe(true);
}));

test('a former journal or lock appearing after startup refuses a commit without writing', async () => {
  const dir = mkdtempSync(join(tmpdir(), 'forma-rename-live-'));
  try {
    const design = 'forma app\n\ntheme\n  ink = #112233\n';
    writeFileSync(join(dir, 'app.contract'), 'component App\n  view\n    text "x" color="ink"\n');
    writeFileSync(join(dir, 'app.forma'), design);
    for (const file of ['tamber.journal', 'tamber.lock']) {
      const child = spawn(compiler, ['forma', 'serve', '--stdio', dir], { timeout: 15_000 });
      const exited = new Promise(done => child.once('exit', done));
      const lines = createInterface({ input: child.stdout })[Symbol.asyncIterator]();
      const ask = async request => {
        child.stdin.write(`${JSON.stringify(request)}\n`);
        return JSON.parse((await lines.next()).value);
      };
      try {
        const hello = await ask({ method: 'hello', id: 1 });
        expect(hello.ok).toBeDefined();
        writeFileSync(join(dir, '.exact', file), 'keep this record');
        writeFileSync(join(dir, '.exact/tamber-write.tmp'), 'keep this text');
        const reply = await ask({ method: 'transaction', id: 2, client: 'test', files: hello.ok.files, ops: [{ op: 'set-token', name: 'ink', value: '#445566' }] });
        expect(reply.error?.message).toContain(`.exact/${file}`);
        expect(readFileSync(join(dir, 'app.forma'), 'utf8')).toBe(design);
        expect(readFileSync(join(dir, '.exact', file), 'utf8')).toBe('keep this record');
        expect(readFileSync(join(dir, '.exact/tamber-write.tmp'), 'utf8')).toBe('keep this text');
        expect(existsSync(join(dir, '.exact/forma.journal'))).toBe(false);
      } finally { child.stdin.end(); await exited; }
      rmSync(join(dir, '.exact', file));
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('package design names refuse instead of silently dropping tokens and library params', () => app(dir => {
  const pkg = join(dir, 'node_modules/@acme/reading');
  mkdirSync(join(pkg, 'src'), { recursive: true });
  writeFileSync(join(pkg, 'src/index.contract'), 'component Card\n  view\n    text "card"\n');
  writeFileSync(join(dir, 'app.contract'), 'use Card from "@acme/reading"\ncomponent App\n  view\n    Card()\n');
  const manifest = { name: '@acme/reading', private: true, exports: { '.': './src/index.contract' } };
  for (const extra of [{ tamber: 'theme.tamber' }, { tamber: null, forma: 'src/reading.forma' }, {}]) {
    writeFileSync(join(pkg, 'package.json'), JSON.stringify({ ...manifest, ...extra }));
    writeFileSync(join(pkg, 'src/reading.forma'), 'forma reading\n');
    writeFileSync(join(pkg, 'src/reading.tamber'), 'old package design');
    const r = run(dir, ['build', 'app.contract']);
    expect(r.status).not.toBe(0);
    expect(r.stderr).toContain('forma-name');
    expect(r.stderr).toContain('rename');
    expect(r.stderr).toContain(Object.hasOwn(extra, 'tamber') ? "package.json's tamber key to forma" : 'reading.tamber');
  }
}));

test('a sibling design with the former extension refuses serve, check and compilation', () => app(dir => {
  writeFileSync(join(dir, 'brand.tamber'), 'old sibling design');
  for (const args of [['forma', 'serve', '--stdio', '.'], ['forma', 'check', '.'], ['build', 'app.contract']]) {
    const r = run(dir, args);
    expect(r.status).not.toBe(0);
    expect(r.stderr).toContain('rename brand.tamber to brand.forma');
  }
  renameSync(join(dir, 'brand.tamber'), join(dir, 'brand.forma'));
  expect(run(dir, ['forma', 'serve', '--stdio', '.']).status).toBe(0);
}));

test('a manifest-named nested former app design refuses check, IR, compilation and serving', () => app(dir => {
  mkdirSync(join(dir, 'design'));
  const before = 'forma app\n\ntheme\n  brand = #123456\n';
  const path = join(dir, 'design/theme.tamber');
  writeFileSync(path, before);
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Rename', app: { id: 'com.exact.rename', name: 'Rename' }, forma: 'design/theme.tamber' }));
  const direct = run(dir, ['forma', 'check', 'design/theme.tamber']);
  expect(direct.status).toBe(2);
  expect(direct.stderr).toContain('rename design/theme.tamber to design/theme.forma');
  const results = [
    ['forma', 'check', '.'], ['forma', 'ir', '.'], ['forma', 'serve', '--stdio', '.'], ['build', 'app.contract'],
  ].map(args => run(dir, args));
  expect(results.map(r => r.status)).toEqual([1, 1, 1, 1]);
  for (const r of results) {
    expect(r.stderr).toMatch(/rename .*design\/theme\.tamber to .*design\/theme\.forma/);
    expect(r.stdout).not.toContain('"brand"');
  }
  expect(results.at(-1).stderr).toContain('forma-name');
  expect(() => designPath(dir)).toThrow(`rename ${path} to ${join(dir, 'design/theme.forma')}`);
  expect(readFileSync(path, 'utf8')).toBe(before);
  expect(existsSync(join(dir, '.exact/forma.lock'))).toBe(false);
  renameSync(path, join(dir, 'design/theme.forma'));
  writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Rename', app: { id: 'com.exact.rename', name: 'Rename' }, forma: 'design/theme.forma' }));
  expect(run(dir, ['forma', 'check', '.']).status).toBe(0);
  expect(designPath(dir)).toBe(join(dir, 'design/theme.forma'));
}));

test('a manifest-named nested former package design refuses compilation', () => app(dir => {
  const pkg = join(dir, 'node_modules/@acme/reading');
  mkdirSync(join(pkg, 'src'), { recursive: true });
  writeFileSync(join(pkg, 'src/index.contract'), 'component Card\n  view\n    text "card"\n');
  writeFileSync(join(dir, 'app.contract'), 'use Card from "@acme/reading"\ncomponent App\n  view\n    Card()\n');
  const manifest = { name: '@acme/reading', exports: { '.': './src/index.contract' } };
  const path = join(pkg, 'src/theme.tamber'), before = 'forma reading\n';
  writeFileSync(path, before);
  writeFileSync(join(pkg, 'package.json'), JSON.stringify({ ...manifest, forma: 'src/theme.tamber' }));
  const r = run(dir, ['build', 'app.contract']);
  expect(r.status).toBe(1);
  expect(r.stderr).toContain('forma-name');
  expect(r.stderr).toMatch(/rename .*src\/theme\.tamber to .*src\/theme\.forma/);
  expect(readFileSync(path, 'utf8')).toBe(before);
  renameSync(path, join(pkg, 'src/theme.forma'));
  writeFileSync(join(pkg, 'package.json'), JSON.stringify({ ...manifest, forma: 'src/theme.forma' }));
  expect(run(dir, ['build', 'app.contract']).status).toBe(0);
}));

for (const [name, script, before, after] of [
  ['serve', 'scripts/exact.mjs', ['forma', 'serve'], []],
  ['tool', 'scripts/exact.mjs', ['forma', 'tool'], ['--no-open']],
  ['web dev', 'host/web/dev.mjs', ['--app'], []],
  ['wasm dev', 'host/web/dev.mjs', ['--app'], ['--wasm']],
  ['remote dev', 'host/web/dev.mjs', ['--app'], ['--allow-plugin-origin']],
]) test(`JS ${name} prints former design refusals and exits 2 without a stack`, () => app(dir => {
  for (const former of ['app.tamber', 'design/theme.tamber']) {
    mkdirSync(join(dir, 'design'), { recursive: true });
    writeFileSync(join(dir, former), 'forma app\n');
    writeFileSync(join(dir, 'app.json'), JSON.stringify({ name: 'Rename', app: { id: 'com.exact.rename', name: 'Rename' }, forma: former }));
    const r = spawnSync(process.execPath, [resolve(ROOT, script), ...before, dir, ...after], {
      cwd: dir, encoding: 'utf8', timeout: 15_000,
    });
    expect(r.status, r.stderr).toBe(2);
    expect(r.stderr.trim()).toMatch(/^rename .*\.tamber to .*\.forma$/);
    expect(r.stdout).toBe('');
    rmSync(join(dir, former));
  }
}));
