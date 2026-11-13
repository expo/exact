// The resident's phase lines (dev-phase.mjs; dtool2's pass 3 #8): a build
// says what it builds and how it went; a noisy build that succeeds is not
// cut off for its output; one that fails says the compiler's own error,
// plain, not the colours or the trace after it.
import { expect, test } from 'bun:test';
import { acceptModuleReply, FAILED, gist, phased, phasedAsync } from './dev-phase.mjs';

const said = () => { const lines = []; return { lines, log: (l) => lines.push(l) }; };

test('module warnings are printed only for the current successful candidate', () => {
  const { lines, log } = said();
  const warnings = ['app.contract:4:7 warning: first', 'app.ts:11:34 warning: second'];
  const reply = { id: 3, ok: true, receipt: {}, design: {}, warnings };
  const accept = (value, request = { id: 3 }, run = 3) => acceptModuleReply(JSON.stringify(value), request, run, log);
  expect(accept(reply, { id: 3 }, 4)).toBeNull();
  expect(accept({ ...reply, ok: false, error: 'stale refusal' }, { id: 3 }, 4)).toBeNull();
  expect(() => accept(reply, null)).toThrow('unexpected module producer response');
  expect(() => accept({ ...reply, id: 2 })).toThrow('unexpected module producer response');
  expect(() => accept({ ...reply, ok: false, error: 'bad source' })).toThrow('bad source');
  expect(() => accept({ ...reply, ok: false })).toThrow('module producer refused the candidate');
  expect(lines).toEqual([]);
  expect(accept(reply)).toEqual(reply);
  expect(lines).toEqual(warnings);
  expect(accept({ ...reply, warnings: [] })).toEqual({ ...reply, warnings: [] });
  expect(lines).toEqual(warnings);
});

test('an asynchronous build that cannot start reports its failure', async () => {
  const { lines, log } = said();
  await expect(phasedAsync('a missing tool', `${import.meta.dir}/missing-phase-tool`, [], {}, () => {}, log)).rejects.toThrow();
  expect(lines[0]).toBe('phase: building a missing tool');
  expect(lines.at(-1)).toContain(`${FAILED}a missing tool did not build`);
});

for (const [mode, build] of [
  ['sync', phased],
  ['async', (what, command, args, options, log, echo) => phasedAsync(what, command, args, options, () => {}, log, echo)],
]) {
  test(`${mode} build that writes megabytes of diagnostics and succeeds is built`, async () => {
    const { lines, log } = said();
    const result = await build('a noisy tool', process.execPath, ['-e', "process.stderr.write('w'.repeat(3 << 20)); process.exit(0)"], {}, log, () => {});
    expect(result.status).toBe(0);
    expect(lines[0]).toBe('phase: building a noisy tool');
    expect(lines[1]).toMatch(/^phase: a noisy tool: built in \d+\.\d s$/);
    expect(lines.length).toBe(2);
  });

  test(`${mode} build that fails says its error lines, plain`, async () => {
    const { lines, log } = said();
    const program = "console.error('\\x1b[1m\\x1b[31merror[E0425]\\x1b[0m: cannot find value `x`'); console.error('error: could not compile `exact-js-bake`'); console.error('    at main (/x.rs:1)'); process.exit(101)";
    const result = await build('the module producer', process.execPath, ['-e', program], {}, log, () => {});
    expect(result.status).toBe(101);
    expect(lines.at(-1)).toBe(`${FAILED}error[E0425]: cannot find value \`x\` · error: could not compile \`exact-js-bake\``);
    expect(gist('first\nsecond\nthird\nfourth')).toBe('second · third · fourth');
  });
}
