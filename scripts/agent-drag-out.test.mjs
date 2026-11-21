// `expect drag` over a tree's admitted props (LLP 1094.000 D10): a file's type is its extension's, `url` is
// `text/uri-list` and `text` `text/plain`; `none` holds while `draggable` is false or nothing is carried.
import { test, expect } from 'bun:test';
import { dragExpectation, dragOf } from './agent-drag-out.mjs';

const card = { props: { testId: 'card', draggable: true, dataTransferFiles: 'assets/apple/1016-thumb.jpg\nassets/notes.unknownext', dataTransferText: 'Mount Fuji', dataTransferUrl: 'https://example.com/1016' } };
const off = { props: { testId: 'off', draggable: 'false', dataTransferText: 'Mount Fuji' } };
const empty = { props: { testId: 'empty', draggable: true, dataTransferFiles: '' } };
const nodes = { card, off, empty };
const holds = (step) => dragExpectation(nodes[step.target], step)?.message ?? null;

test('a drag offers each file by its extension, then its url and text', () => {
  expect(dragOf(card)).toEqual({
    text: 'Mount Fuji', url: 'https://example.com/1016',
    files: [{ path: 'assets/apple/1016-thumb.jpg', name: '1016-thumb.jpg', type: 'image/jpeg' }, { path: 'assets/notes.unknownext', name: 'notes.unknownext', type: 'application/octet-stream' }],
  });
  expect([dragOf(off), dragOf(empty)]).toEqual([null, null]);
});

test('each expectation holds or says what the drag carries', () => {
  expect(holds({ target: 'card', kind: 'image/jpeg', present: true, name: '1016-thumb.jpg' })).toBeNull();
  expect(holds({ target: 'card', kind: 'text/plain', present: true, value: 'Mount Fuji' })).toBeNull();
  expect(holds({ target: 'card', kind: 'text/uri-list', present: true, value: 'https://example.com/1016' })).toBeNull();
  expect(holds({ target: 'off', kind: null, present: false })).toBeNull();
  expect(holds({ target: 'empty', kind: 'text/plain', present: false })).toBeNull();
  expect(holds({ target: 'card', kind: 'text/plain', present: true, value: 'Rainbow' })).toBe('drag of "card" carries image/jpeg 1016-thumb.jpg, application/octet-stream notes.unknownext, text/uri-list "https://example.com/1016", text/plain "Mount Fuji", expected text/plain == "Rainbow"');
  expect(holds({ target: 'card', kind: 'text/plain', present: false })).toContain('expected no text/plain');
  expect(holds({ target: 'card', kind: null, present: false })).toContain('expected nothing');
  expect(holds({ target: 'gone', kind: null, present: false })).toBe('drag of "gone": no view carries that testId');
});
