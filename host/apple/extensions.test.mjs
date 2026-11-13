import { expect, test } from 'bun:test';
import { EXTENSION_KINDS, extensionKinds, withoutExtensions } from './extensions.mjs';
import { hasEntries, platformWidgetEntries, activityEntries } from './widgets.mjs';

test('--extensions keeps every kind by default, none, or a named list; refuses an unknown kind', () => {
  expect([...extensionKinds([])]).toEqual(EXTENSION_KINDS);
  expect([...extensionKinds(['--extensions', 'all'])]).toEqual(EXTENSION_KINDS);
  expect([...extensionKinds(['--extensions', 'none'])]).toEqual([]);
  expect([...extensionKinds(['--device', '--extensions', 'activities,widgets', 'apps/shelf'])]).toEqual(['activities', 'widgets']);
  expect(() => extensionKinds(['--extensions', 'widget'])).toThrow(/unknown kind widget/);
  expect(() => extensionKinds(['--extensions'])).toThrow(/takes all, none/);
  expect(() => extensionKinds(['--extensions', '--device'])).toThrow(/takes all, none/);
});

test('a left-out kind drops its manifest entries, so nothing gated on them builds', () => {
  const manifest = { widgets: [{ kind: 'next', families: ['systemSmall'] }], live_activities: [{ kind: 'trip' }], name: 'x' };
  const app = kinds => ({ manifest: withoutExtensions(manifest, new Set(kinds)) });
  expect(hasEntries(app(EXTENSION_KINDS))).toBe(true);
  expect(hasEntries(app([]))).toBe(false);
  expect(platformWidgetEntries(app(['activities'])).length).toBe(0);
  expect(activityEntries(app(['activities'])).length).toBe(1);
  expect(hasEntries(app(['activities']))).toBe(true); // the extension, with activities alone
  expect(activityEntries(app(['widgets'])).length).toBe(0);
  expect(app([]).manifest.name).toBe('x');
  expect(manifest.widgets.length).toBe(1); // the app's own manifest is not changed
});
