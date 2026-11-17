import { expect, test } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { open } from '../../scripts/agent.mjs';
import { checkOp, ratio } from '../../scripts/agent-check.mjs';

// Build first with `bun host/web/build.mjs llp`.
test('the welcome document and chrome stay readable in both appearances', async () => {
  const session = await open({ host: 'web', app: 'llp', size: [1280, 900],
    env: { EXACT_AGENT_EPHEMERAL_PORT: '1' } });
  try {
    await session.clock('settle');
    const failures = [];
    for (const scheme of ['light', 'dark']) {
      await session.prefer({ 'prefers-color-scheme': scheme });
      await session.clock('settle');
      const paint = await session.carrier.evaluate(`(() => {
        const style = getComputedStyle(document.querySelector('[data-testid="llp-main"]'));
        const rgb = value => value.match(/[\\d.]+/g).map(Number);
        return { accent: rgb(style.accentColor), page: rgb(style.backgroundColor) };
      })()`);
      expect(ratio(paint.accent, paint.page)).toBeGreaterThanOrEqual(4.5);
      const result = await checkOp(session, [scheme]);
      failures.push(...result.check[0].findings.filter(f => f.kind === 'contrast'));
    }
    expect(failures).toEqual([]);
  } finally {
    await session.close();
  }
}, 30000);

test('every outline depth keeps readable selected and unselected ink', async () => {
  const directory = mkdtempSync(new URL('../../target/llp-appearance-', import.meta.url));
  let session;
  try {
    const source = readFileSync(new URL('./app.contract', import.meta.url), 'utf8');
    const theme = source.match(/^theme\n[\s\S]*?\n\n/m)[0];
    const heading = source.match(/^shape Heading\n[\s\S]*?\n\n/m)[0];
    const outline = source.match(/^component OutlineRow\n[\s\S]*?(?=\n\/\/ ----)/m)[0];
    const contract = `${directory}/outline.contract`, plan = `${directory}/outline.plan`;
    writeFileSync(contract, `${theme}${heading}
component Preview
  state selected = -1
  action show(at: number)
    selected = at
  view
    main width="100%" height="100%" background-color="chrome" color="ink"
      for level in [1, 2, 3, 4, 5, 6] key=level
        OutlineRow(heading=Heading(id="heading", text="Outline heading", level=level, at=level, current=selected == level), show=show)
${outline}
`);
    const compiler = resolve(process.env.CARGO_TARGET_DIR ?? resolve(import.meta.dir, '../../target'), 'debug/contract');
    const built = spawnSync(compiler, ['build', contract, '-o', plan], { encoding: 'utf8' });
    expect({ status: built.status, error: built.error, stderr: built.stderr }).toEqual({ status: 0, error: undefined, stderr: '' });
    session = await open({ host: 'web', app: 'llp', plan, size: [300, 400],
      env: { EXACT_AGENT_EPHEMERAL_PORT: '1' } });
    const failures = [];
    for (const scheme of ['light', 'dark']) {
      await session.prefer({ 'prefers-color-scheme': scheme });
      for (let level = 1; level <= 6; level++) {
        await session.tap(`outline-${level}`);
        await session.clock('settle');
        const result = await checkOp(session, [scheme]);
        failures.push(...result.check[0].findings.filter(f => f.kind === 'contrast'));
      }
    }
    expect(failures).toEqual([]);
  } finally {
    await session?.close();
    rmSync(directory, { recursive: true, force: true });
  }
}, 30000);

test('the LLP document keeps the shared Markdown reading palette', () => {
  const palette = path => readFileSync(new URL(path, import.meta.url), 'utf8')
    .match(/^theme\n([\s\S]*?)\n\n/m)[1].split('\n').map(line => line.trim().split(' //')[0]);
  expect(palette('./app.contract')).toEqual(palette('../markdown/app.contract'));
  expect(palette('./app.contract')).toEqual(palette('../markdown-stress/app.contract'));
});
