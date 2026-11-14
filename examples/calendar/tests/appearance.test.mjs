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

for (const width of [420, 1180]) {
  test(`calendar popovers and modal editor at ${width}px`, async () => {
    const session = await open({ host: 'web', app: './examples/calendar', size: [width, 900],
      storage: `calendar-presentation-${process.pid}-${width}`, epoch: '2026-09-16T12:00:00Z',
      env: { EXACT_AGENT_EPHEMERAL_PORT: '1' } });
    try {
      await session.clock('settle');
      await session.tap('theme-button');
      await session.clock('settle');
      expect(await session.carrier.evaluate(`document.querySelector('#theme-popup').matches(':popover-open')`)).toBe(true);
      await session.type('close-themes', { key: 'Escape' });
      await session.clock('+200');
      await session.clock('settle');
      expect((await session.state()).slots.themeOpen).toBe(false);
      await session.tap('theme-button');
      await session.clock('settle');
      await session.tap('month-label');
      await session.clock('settle');
      expect(await session.carrier.evaluate(`document.querySelector('#theme-popup').matches(':popover-open')`)).toBe(false);
      await session.tap('new-item');
      if (width < 860) {
        expect(await session.carrier.evaluate(`document.querySelector('#type-picker').matches(':popover-open')`)).toBe(true);
        await session.type('add-event', { key: 'Escape' });
        await session.clock('settle');
        expect(await session.carrier.evaluate(`document.querySelector('#type-picker').matches(':popover-open')`)).toBe(false);
        await session.tap('new-item');
        await session.tap('add-event');
      }
      await session.clock('settle');
      const { ax } = await session.tree(null, { ax: true });
      expect(ax.elements.some(element => element.role === 'dialog' && element.name === 'New Event')).toBe(true);
      expect(ax.modal.present).toBe(true);
      expect(ax.elements.some(element => element.testId === 'theme-button')).toBe(false);
      expect(await session.carrier.evaluate(`document.querySelector('#schedule-editor').contains(document.activeElement)`)).toBe(true);
      await session.type('schedule-title', 'Unsaved draft');
      await session.type('schedule-notes', { key: 'Escape' });
      await session.clock('settle');
      expect((await session.state()).slots.editorOpen).toBe(false);
      expect(await session.carrier.evaluate(`document.activeElement.dataset.testid`)).toBe('new-item');
      await session.tap('new-item');
      if (width < 860) await session.tap('add-todo');
      await session.clock('settle');
      await session.tap('cancel-editor');
      await session.clock('settle');
      expect((await session.state()).slots.editorOpen).toBe(false);
    } finally {
      await session.close();
    }
  }, 30000);
}

test('compact date creation returns focus to the selected date', async () => {
  const session = await open({ host: 'web', app: './examples/calendar', size: [420, 900],
    storage: `calendar-date-focus-${process.pid}`, epoch: '2026-09-16T12:00:00Z',
    env: { EXACT_AGENT_EPHEMERAL_PORT: '1' } });
  try {
    await session.clock('settle');
    const date = 'date-2026-09-2026-09-03';
    for (const dismissal of ['cancel', 'escape', 'save']) {
      if ((await session.state()).slots.popupOpen) await session.tap('close-date');
      await session.tap(date, { at: [10, 10] });
      await session.carrier.evaluate(`document.querySelector('[data-testid="${date}"]')
        .dispatchEvent(new MouseEvent('dblclick', { bubbles: true }))`);
      await session.clock('settle');
      expect((await session.state()).slots.editorOpen).toBe(true);
      await session.type('schedule-title', 'From a date');
      if (dismissal === 'escape') await session.type('schedule-notes', { key: 'Escape' });
      else await session.tap(dismissal === 'save' ? 'save-schedule' : 'cancel-editor');
      await session.clock('settle');
      expect((await session.state()).slots.editorOpen).toBe(false);
      expect(await session.carrier.evaluate(`document.activeElement.id`)).toBe('day-24320-20699');
    }
  } finally {
    await session.close();
  }
}, 30000);

for (const width of [420, 1180]) {
  test(`delete confirmation allows Escape and Cancel at ${width}px`, async () => {
    const session = await open({ host: 'web', app: './examples/calendar', size: [width, 900],
      storage: `calendar-delete-dismiss-${process.pid}-${width}`, epoch: '2026-09-16T12:00:00Z',
      env: { EXACT_AGENT_EPHEMERAL_PORT: '1' } });
    try {
      await session.clock('settle');
      await session.tap('date-2026-09-2026-09-01', { at: [10, 10] });
      const item = await session.carrier.evaluate(`document.querySelector('[data-testid^="agenda-item-"]').dataset.testid`);
      await session.tap(item);
      await session.clock('settle');
      await session.tap('delete-schedule');
      await session.tap('keep-schedule');
      expect((await session.state()).slots.editorOpen).toBe(true);
      expect((await session.state()).slots.confirmDelete).toBe(false);
      await session.tap('delete-schedule');
      await session.type('schedule-notes', { key: 'Escape' });
      await session.clock('settle');
      expect((await session.state()).slots.editorOpen).toBe(false);
      expect(await session.carrier.evaluate(`document.querySelector('[data-testid="${item}"]') !== null`)).toBe(true);
      await session.tap(item);
      await session.clock('settle');
      await session.tap('delete-schedule');
      await session.tap('cancel-editor');
      await session.clock('settle');
      expect((await session.state()).slots.editorOpen).toBe(false);
    } finally {
      await session.close();
    }
  }, 30000);
}
