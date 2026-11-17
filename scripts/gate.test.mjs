import { expect, test } from 'bun:test';
import { checkFormat, gateMetric, gateParts, runGate } from './gate.mjs';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { tmpdir } from 'node:os';
import { dirname, join, resolve } from 'node:path';

test('Windows formatting batches every target, keeps editions, and reports failures without stopping', async () => {
  const targets = Array.from({ length: 400 }, (_, i) => ({
    src_path: resolve('target', 'long workspace path '.repeat(5), `crate-${i}`, 'src/lib.rs'), edition: i % 2 ? '2021' : '2024',
  }));
  const calls = [];
  const result = await checkFormat({ platform: 'win32', run: async (command, args) => {
    if (command === 'cargo') return { status: 0, stdout: JSON.stringify({ packages: [{ manifest_path: resolve('Cargo.toml'), targets, dependencies: [] }] }) };
    calls.push(args);
    return { status: calls.length === 1 ? 1 : 0, log: calls.length === 1 ? 'unformatted first batch\n' : '' };
  } });
  expect(calls.length).toBeGreaterThan(2);
  expect(result.status).toBe(1);
  expect(result.log).toContain('unformatted first batch');
  const seen = calls.flatMap(args => {
    expect(args.at(-1)).toBe('--check');
    expect(args.at(-3)).toBe('--edition');
    expect(args.reduce((size, arg) => size + arg.length * 2 + 3, 0)).toBeLessThan(25000);
    for (const file of args.slice(0, -3)) expect(targets.find(t => t.src_path === file).edition).toBe(args.at(-2));
    return args.slice(0, -3);
  });
  expect(seen.sort()).toEqual(targets.map(t => t.src_path).sort());
});

test('batched formatting checks the same modules and local dependencies as cargo fmt --all', async () => {
  const root = mkdtempSync(join(tmpdir(), 'exact-format-'));
  const write = (path, text) => { const file = join(root, path); mkdirSync(dirname(file), { recursive: true }); writeFileSync(file, text); };
  const cargo = args => spawnSync('cargo', args, { cwd: root, encoding: 'utf8' });
  try {
    // The repository's pinned toolchain, which has rustfmt, not whatever rustup's default is.
    write('rust-toolchain.toml', readFileSync(resolve(import.meta.dir, '../rust-toolchain.toml'), 'utf8'));
    write('Cargo.toml', '[workspace]\nmembers = ["one", "two"]\nexclude = ["local"]\nresolver = "2"\n');
    write('one/Cargo.toml', '[package]\nname = "one"\nversion = "0.1.0"\nedition = "2021"\n[dependencies]\nlocal = { path = "../local" }\n');
    write('two/Cargo.toml', '[package]\nname = "two"\nversion = "0.1.0"\nedition = "2024"\n');
    write('local/Cargo.toml', '[package]\nname = "local"\nversion = "0.1.0"\nedition = "2018"\n[workspace]\n');
    write('one/src/lib.rs', 'mod child;\n');
    write('one/src/child.rs', 'fn child( ){ }\n');
    write('one/examples/demo.rs', 'fn main( ){ }\n');
    write('two/src/lib.rs', 'pub async fn second( ){ }\n');
    write('local/src/lib.rs', 'pub fn local( ){ }\n');
    const result = await checkFormat({ root, platform: 'win32' });
    expect(result.status).toBe(1);
    for (const name of ['child', 'main', 'second', 'local']) expect(result.log).toContain(`fn ${name}`);
    expect(readFileSync(join(root, 'local/src/lib.rs'), 'utf8')).toBe('pub fn local( ){ }\n');
    expect(cargo(['fmt', '--all', '--', '--check']).status).toBe(1);
    const formatted = cargo(['fmt', '--all']);
    expect(formatted.stderr).toBe('');
    expect(formatted.status).toBe(0);
    expect((await checkFormat({ root, platform: 'win32' })).status).toBe(0);
  } finally { rmSync(root, { recursive: true, force: true }); }
}, 30000);

test('all parts run after command failures, including a missing executable', async () => {
  const parts = [['fmt', process.execPath, ['-e', 'console.log("first"); process.exit(3)']], ['caps', '/no/such/exact-gate-command', []],
    ['check', process.execPath, ['-e', 'console.log("checked"); process.exit(4)']], ['unit', process.execPath, ['-e', 'console.log("tested")']]];
  const lines = [], result = await runGate({ parts, budget: 30, report: line => lines.push(line) });
  expect(result.status).toBe(1);
  expect(result.parts.map(p => p.name).sort()).toEqual(['caps', 'check', 'fmt', 'unit']);
  expect(result.failed.sort()).toEqual(['caps', 'check', 'fmt']);
  expect(lines.join('\n')).toContain('tested');
});

test('a slow passing run reports its budget without failing cold CI or cutting off tests', async () => {
  let elapsed = 0;
  const lines = [];
  const result = await runGate({ parts: [['check', 'cargo', []], ['unit', 'cargo', []]], budget: 30, now: () => elapsed,
    run: async () => { elapsed += 16000; return { status: 0 }; }, report: line => lines.push(line) });
  expect(result.seconds).toBe(32);
  expect(result.failed).toEqual([]);
  expect(result.over).toBe(true);
  expect(result.status).toBe(0);
  expect(lines.at(-1)).toContain('OVER BUDGET');
});

test('a unit crate that cannot compile does not stop the remaining crates', async () => {
  const parts = gateParts.filter(p => p[1] === 'cargo' && p[2][0] === 'test'), ran = [];
  const result = await runGate({ parts, budget: 30, report: () => {}, run: async (_, args) => {
    const packages = args.flatMap((a, i) => a === '-p' ? [args[i + 1]] : []);
    ran.push(...packages);
    return { status: packages.includes('exact-num') ? 101 : 0 };
  } });
  expect(parts).toHaveLength(7);
  expect(result.parts).toHaveLength(7);
  expect(result.failed).toEqual(['unit:exact-num']);
  expect(ran).toEqual(['exact-num', 'exact-color', 'exact-motion', 'exact-plan', 'contract-syntax', 'contract-types', 'contract-checking']);
});

test('hourly metrics record a passing gate over budget as a violation', () => {
  expect(gateMetric(32, 30)).toEqual(['blocking gate (warm)', '32.0 s', 'budget 30 s; VIOLATION: over budget']);
  expect(gateMetric(29, 30)[2]).not.toContain('VIOLATION');
  expect(gateMetric(29, 30, ['fmt'])[2]).toContain('VIOLATION: fmt');
});

test('hourly hosted tests leave macOS runners to the blocking gate and fleet', () => {
  const workflow = Bun.YAML.parse(readFileSync(new URL('../.github/workflows/tests.yml', import.meta.url), 'utf8'));
  expect(Object.values(workflow.jobs).some(job => /macos/.test(job['runs-on']))).toBe(false);
  expect(workflow.jobs.linux['runs-on']).toBe('ubuntu-24.04');
});
