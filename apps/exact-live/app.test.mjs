import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import schema from '../../kernel/tables/schema.json';
import { open } from '../../scripts/agent.mjs';
import { checkOp, ratio } from '../../scripts/agent-check.mjs';

// Build first with `bun host/web/build.mjs exact-live`.
test('the workspace and photo viewer follow the system appearance', async () => {
  const session = await open({ host: 'web', app: 'exact-live', size: [1180, 900],
    env: { EXACT_AGENT_EPHEMERAL_PORT: '1' } });
  try {
    await session.clock('settle');
    await session.tap('open-photo-00000');
    await session.clock('settle');
    const paints = [];
    const failures = [];
    for (const scheme of ['light', 'dark']) {
      await session.prefer({ 'prefers-color-scheme': scheme });
      await session.clock('settle');
      paints.push(await session.carrier.evaluate(`(() => {
        return ['exact-live', 'viewer-clip'].map(id =>
          getComputedStyle(document.querySelector('[data-testid="' + id + '"]')).backgroundColor);
      })()`));
      const result = await checkOp(session, [scheme]);
      failures.push(...result.check[0].findings.filter(f => f.kind === 'contrast'));
    }
    for (let i = 0; i < paints[0].length; i++) expect(paints[0][i]).not.toEqual(paints[1][i]);
    expect(failures).toEqual([]);
  } finally {
    await session.close();
  }
}, 30000);

// Apple's AccentColorText stays white even when the author supplies a pale accent.
test('authored accent fills keep readable ink with Apple and neutral role colours', () => {
  const source = readFileSync(new URL('./app.contract', import.meta.url), 'utf8');
  const accent = source.match(/accent-color="([^"]+)"/)[1];
  const labels = ['Go live', 'Post', 'label'].map(label => {
    const region = label === 'label' ? source.split('component LoadButton')[1].split('component ')[0] : source;
    const line = region.split('\n').find(line => line.trim().startsWith(`text ${label === 'label' ? label : `"${label}"`}`));
    return { label, ink: line.match(/color=(.*)$/)[1] };
  });
  const failures = [];
  for (const column of [4, 6]) for (const scheme of [0, 1]) {
    const colour = value => {
      const pair = value.match(/^light-dark\(([^,]+),\s*([^)]+)\)$/);
      if (pair) return colour(pair[scheme + 1]);
      if (!value.startsWith('#')) value = schema.colors.find(row => row[0] === value)[column + scheme];
      return [1, 3, 5].map(offset => parseInt(value.slice(offset, offset + 2), 16));
    };
    for (const { label, ink } of labels) {
      const value = Function('selected', 'n', 'theme', `return ${ink}`)(1, 1, { ink: '-exact-label' });
      const contrast = ratio(colour(value), colour(accent));
      if (contrast < 4.5) failures.push({ label, column, scheme, contrast });
    }
  }
  expect(failures).toEqual([]);
});
