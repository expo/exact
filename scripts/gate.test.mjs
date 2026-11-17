import { expect, test } from 'bun:test';
import { gateMetric, gateParts, runGate } from './gate.mjs';
import { readFileSync } from 'node:fs';

test('all parts run after command failures, including a missing executable', async () => {
  const parts = [['fmt', 'sh', ['-c', 'echo first; exit 3']], ['caps', '/no/such/exact-gate-command', []],
    ['check', 'sh', ['-c', 'echo checked; exit 4']], ['unit', 'sh', ['-c', 'echo tested']]];
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
