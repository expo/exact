import { expect, test } from 'bun:test';
import { readFileSync } from 'node:fs';
import schema from '../../../kernel/tables/schema.json';
import { open } from '../../../scripts/agent.mjs';
import { checkOp, ratio } from '../../../scripts/agent-check.mjs';

// Build first with `bun host/web/build.mjs ./examples/calendar`.
test('every wallpaper keeps scheme-aware calendar and editor chrome', async () => {
  const session = await open({ host: 'web', app: './examples/calendar', size: [1180, 800], storage: `q1-calendar-${process.pid}`,
    epoch: '2026-09-16T12:00:00Z', env: { EXACT_AGENT_EPHEMERAL_PORT: '1' } });
  const failures = [];
  try {
    await session.prefer({ 'prefers-reduced-motion': 'reduce' });
    await session.clock('+100');
    await session.clock('settle');
    await session.tap('date-2026-09-2026-09-17');
    await session.tap('new-item');
    await session.tap('kind-todo');
    await session.type('schedule-title', 'Completed colour check');
    await session.tap('color-Lilac');
    await session.tap('save-schedule');
    await session.clock('settle');
    const check = await session.carrier.evaluate(
      `document.querySelector('[aria-label="Complete Completed colour check"]').dataset.testid`);
    await session.tap(check);
    await session.clock('settle');
    const id = check.slice('todo-check-'.length);
    for (let theme = 0; theme <= 6; theme++) {
      await session.tap('theme-button');
      await session.tap(`theme-${theme}`);
      await session.clock('+200');
      await session.clock('settle');
      await session.tap('date-2026-09-2026-09-17');
      for (const scheme of ['light', 'dark']) {
        await session.prefer({ 'prefers-color-scheme': scheme });
        await session.clock('settle');
        const marks = await session.carrier.evaluate(`(() => {
          const rgb = value => value.match(/[\\d.]+/g).map(Number);
          return ['[data-testid="row-mark-${id}"]', '[data-testid^="bar-2026-09-${id}-"]'].map(selector => {
            const svg = document.querySelector(selector + ' svg');
            return { fill: rgb(getComputedStyle(svg.querySelector('circle')).fill),
              ink: rgb(getComputedStyle(svg.querySelector('path')).stroke),
              opacity: Number(getComputedStyle(svg).opacity) };
          });
        })()`);
        for (const mark of marks) {
          if (ratio(mark.ink, mark.fill) < 3 || mark.opacity !== 1) {
            failures.push({ theme, scheme, mark });
          }
        }
        if (theme > 0) {
          const scrim = await session.carrier.evaluate(`(() => {
            const style = getComputedStyle(document.querySelector('[data-testid="theme-veil"]'));
            const colours = [...(style.backgroundImage === 'none' ? style.backgroundColor : style.backgroundImage)
              .matchAll(/rgba?\\(([^)]+)\\)/g)].map(match => match[1].split(',').map(Number));
            return Number(style.opacity) * Math.max(...colours.map(c => c[3] ?? 1));
          })()`);
          expect(scrim).toBeGreaterThan(0);
          expect(scrim).toBeLessThanOrEqual(0.5);
        }
        const checked = await checkOp(session, [scheme]);
        failures.push(...checked.check[0].findings.filter(f => f.kind === 'contrast')
          .map(f => ({ theme, scheme, ...f })));
      }
      await session.tap('new-item');
      await session.clock('settle');
      const paints = [];
      for (const scheme of ['light', 'dark']) {
        await session.prefer({ 'prefers-color-scheme': scheme });
        await session.clock('settle');
        const paint = await session.carrier.evaluate(`(() => {
          const rgb = value => value.match(/[\\d.]+/g).map(Number);
          return ['calendar', 'schedule-editor'].map(id => {
            const style = getComputedStyle(document.querySelector('[data-testid="' + id + '"]'));
            return { id, fill: rgb(style.backgroundColor), ink: rgb(style.color) };
          });
        })()`);
        paints.push(paint);
        for (const surface of paint) {
          if (ratio(surface.ink, surface.fill) < 4.5 ||
              (scheme === 'dark' ? Math.max(...surface.fill.slice(0, 3)) > 65
                : Math.min(...surface.fill.slice(0, 3)) < 200)) {
            failures.push({ theme, scheme, ...surface });
          }
        }
      }
      expect(paints[0]).not.toEqual(paints[1]);
      await session.tap('cancel-editor');
      await session.clock('+200');
      await session.clock('settle');
    }
    expect(failures).toEqual([]);
  } finally {
    await session.close();
  }
}, 60000);

test('every calendar accent and destructive fill has readable ink on native hosts', () => {
  const root = readFileSync(new URL('../src/app.contract', import.meta.url), 'utf8');
  const month = readFileSync(new URL('../src/month.contract', import.meta.url), 'utf8');
  const editor = readFileSync(new URL('../src/editor.contract', import.meta.url), 'utf8');
  const sites = [
    root.split('\n').find(line => line.includes('text "Add"')),
    root.split('\n').find(line => line.includes('button "+"')),
    month.split('\n').find(line => line.includes('text `${cell.number}`')),
    editor.split('\n').find(line => line.includes('text (busy ? "Saving')),
    editor.split('\n').find(line => line.includes('text (busy ? "Deleting')),
  ];
  const failures = [];
  for (let theme = 0; theme <= 6; theme++) {
    const env = { theme, cell: { day: 1 }, today: 1, busy: false, title: 'Event', themeMuted: '-exact-secondary-label' };
    for (const name of ['themeAccent', 'themeAccentInk']) {
      const expression = root.match(new RegExp(`^  derive ${name} = (.+)$`, 'm'))?.[1];
      if (expression) env[name] = Function('theme', `return ${expression}`)(theme);
    }
    const evaluate = expression => Function(...Object.keys(env),
      `return ${expression.replace(/\bor\b/g, '||').replace(/\band\b/g, '&&')}`)(...Object.values(env));
    const fill = editor.split('\n').find(line => line.includes('testId="confirm-delete"')).match(/background-color=(.+)$/)[1];
    for (const column of [4, 6]) for (const scheme of [0, 1]) {
      const colour = value => {
        const pair = value.match(/^light-dark\(([^,]+),\s*([^)]+)\)$/);
        if (pair) return colour(pair[scheme + 1]);
        if (!value.startsWith('#')) value = schema.colors.find(row => row[0] === value.replace(/^-exact-/, ''))[column + scheme];
        return [1, 3, 5].map(offset => parseInt(value.slice(offset, offset + 2), 16));
      };
      for (let site = 0; site < sites.length; site++) {
        const ink = sites[site].match(/ color=("[^"]+"|\(.*\)|[\w]+)(?: |$)/)[1];
        const contrast = ratio(colour(evaluate(ink)), colour(site === 4 ? evaluate(fill) : env.themeAccent));
        if (contrast < 4.5) failures.push({ theme, site, column, scheme, contrast });
      }
    }
  }
  expect(failures).toEqual([]);
});
