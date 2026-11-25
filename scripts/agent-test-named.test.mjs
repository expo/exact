import { test, expect } from 'bun:test';
import { nodeNamed } from './agent-test.mjs';

// Two routes' card menus, each a popover holding a "Favorite" button: the closed one is drawn nowhere
const menus = (firstOpen) => [
  { id: 1, depth: 0, type: 'View', props: {}, parent: undefined },
  { id: 2, depth: 1, type: 'View', props: { id: 'menu-a', popover: 'auto' }, open: firstOpen, parent: 1 },
  { id: 3, depth: 2, type: 'Control', props: {}, handlers: ['press'], parent: 2 },
  { id: 4, depth: 3, type: 'Text', props: { text: 'Favorite' }, parent: 3 },
  { id: 5, depth: 1, type: 'View', props: { id: 'menu-b', popover: 'auto' }, open: true, parent: 1 },
  { id: 6, depth: 2, type: 'Control', props: {}, handlers: ['press'], parent: 5 },
  { id: 7, depth: 3, type: 'Text', props: { text: 'Favorite' }, parent: 6 },
];

test('A name leaves out a closed popover, which is drawn nowhere', () => {
  expect(nodeNamed(menus(false), 'Favorite')?.id).toBe(6);
});

test('A name still refuses two open popovers that both carry it', () => {
  expect(() => nodeNamed(menus(true), 'Favorite')).toThrow(/names 2 views/);
});

test('A name under a hidden box is left out of the live pass', () => {
  const nodes = menus(true);
  nodes[1] = { ...nodes[1], hidden: true };
  expect(nodeNamed(nodes, 'Favorite')?.id).toBe(6);
});
