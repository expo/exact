// Inspect mode's runtime hooks on the JS target (LLP 1122 T2b) are a
// development build's alone: `bun test host/web-js/inspect.test.mjs`.
import { test, expect } from 'bun:test';
import { readFileSync } from 'node:fs';
import { inspectHooked, withInspectHooks } from './module.mjs';

// The runtime has no inspect hook, so every build but `--dev` copies it as
// it is; a `--dev` build's copy (build.mjs, module.mjs `withInspectHooks`)
// gains the two, and fails loudly when a file loses the line its hook hangs on.
test('the runtime has no inspect hook; a development copy gains both, and the rest of it is as it was', () => {
  expect(inspectHooked).toEqual(['tree.js', 'mount.js']);
  const hooks = {
    'tree.js': ['$exactViewId', 'globalThis.$exactViewId = viewId;'],
    'mount.js': [
      '$exactEaches',
      '(globalThis.$exactEaches ??= new Set()).add(e); onEnd(() => globalThis.$exactEaches.delete(e));',
    ],
  };
  for (const file of inspectHooked) {
    const source = readFileSync(new URL(`./${file}`, import.meta.url), 'utf8');
    const [name, line] = hooks[file];
    for (const hook of ['$exactViewId', '$exactEaches']) expect(source).not.toContain(hook);
    const dev = withInspectHooks(file, source);
    expect(dev).toContain(line);
    // One line added, nothing else changed.
    const lines = dev.split('\n');
    expect(lines.length).toBe(source.split('\n').length + 1);
    expect(lines.filter((l) => !l.includes(name)).join('\n')).toBe(source);
    expect(() => withInspectHooks(file, 'export const nothing = 1;\n')).toThrow(/no line to hang on/);
  }
});
