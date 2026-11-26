import { expect, test } from 'bun:test';
import { frameDifferences, placed, treeDifference } from './agent-rebuild.mjs';

test('a box a fresh build places elsewhere is named once, by its outermost node, whatever the rebuilt tree numbers it', () => {
  const tree = (ids) => ({ nodes: [
    { id: ids[0], parent: null, type: 'column', props: {} },
    { id: ids[1], parent: ids[0], type: 'list', props: { testId: 'info-list' } },
    { id: ids[2], parent: ids[1], type: 'row', props: {} },
    { id: ids[3], parent: ids[0], type: 'text', props: {} },
  ] });
  const frames = (ids, row) => ({ [ids[0]]: [0, 0, 364, 600], [ids[1]]: [0, 0, 364, 600], [ids[2]]: row, [ids[3]]: [16, 560, 100, 20.4] });
  const before = placed(tree([1, 2, 3, 4]), frames([1, 2, 3, 4], [0, 10, 364, 44]));
  // The same screen built again, its nodes numbered anew and the text half a point off
  const same = placed(tree([9, 10, 11, 12]), { ...frames([9, 10, 11, 12], [0, 10, 364, 44]), 12: [16, 560.5, 100, 20.4] });
  expect(treeDifference(before.nodes, same.nodes)).toBeNull();
  expect(frameDifferences(before, same)).toEqual([]);
  // The row inset as UIKit insets it when built fresh
  const inset = placed(tree([9, 10, 11, 12]), frames([9, 10, 11, 12], [20, 10, 324, 44]));
  const lines = frameDifferences(before, inset);
  expect(lines).toHaveLength(1);
  expect(lines[0]).toStartWith('row#3 is at (0, 10) 364×44 after the steps and at (20, 10) 324×44 when built fresh');
  // A list that moved names the list, not each row inside it
  const moved = placed(tree([9, 10, 11, 12]), { ...frames([9, 10, 11, 12], [20, 10, 324, 44]), 10: [20, 0, 344, 600] });
  expect(frameDifferences(before, moved).map((l) => l.split(' is at')[0])).toEqual(['info-list']);
});

test('a rebuild that shows another tree is told apart from one that shows the same', () => {
  const tree = (editing) => ({ nodes: [
    { id: 1, parent: null, type: 'column', props: {} },
    { id: 2, parent: 1, type: editing ? 'input' : 'text', props: editing ? { value: 'Towering Peaks' } : {} },
  ] });
  const editing = placed(tree(true), {}), shown = placed(tree(false), {});
  expect(treeDifference(editing.nodes, shown.nodes)).toBe('/column0/input0');
  expect(treeDifference(editing.nodes, placed(tree(true), {}).nodes)).toBeNull();
  const relabeled = placed({ nodes: [{ id: 1, parent: null, type: 'column', props: {} }, { id: 2, parent: 1, type: 'input', props: { value: 'Peaks' } }] }, {});
  expect(treeDifference(editing.nodes, relabeled.nodes)).toBe('/column0/input0');
  // A field the rebuild left without its focus: the keyboard went down with it
  const focused = placed({ nodes: [{ id: 1, parent: null, type: 'column', props: {} }, { id: 2, parent: 1, type: 'input', props: { value: 'Towering Peaks' }, focused: true }] }, {});
  expect(treeDifference(focused.nodes, editing.nodes)).toBe('/column0/input0');
});
