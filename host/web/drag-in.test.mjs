// The web's drag into the page (LLP 1094.001 D1, D2): which drags a node with `drop` takes, a field's text drag
// staying the field's, and what a drop's text and link are. The drop itself is the UI Gallery drive's
// (`apps/ui-gallery/drag.test.contract`).
import { test, expect } from 'bun:test';
import { carries, receive, takes, words } from './drag-in.js';

/** A drag carrying `data`, by type, as the browser hands a page its `DataTransfer`. */
const drag = (data, files = false) => ({
  types: [...(files ? ['Files'] : []), ...Object.keys(data)],
  getData: (type) => data[type] ?? '',
});

test('a node takes files, text and links, and leaves a text drag over a field inside it to the field', () => {
  const field = { closest: () => field }, box = { closest: () => null };
  const node = { contains: (el) => el === field || el === box };
  const over = (target, dataTransfer) => takes(node, { target, dataTransfer });
  expect(over(box, drag({ 'text/plain': 'Mount Fuji' }))).toBe(true);
  expect(over(box, drag({ 'text/uri-list': 'https://example.com/' }))).toBe(true);
  expect(over(box, drag({}, true))).toBe(true);
  expect(over(field, drag({ 'text/plain': 'Mount Fuji' }))).toBe(false);
  expect(over(field, drag({}, true))).toBe(true);
  expect(over(box, drag({ 'text/html': '<b>bold</b>' }))).toBe(false);
  // A field whose own node declares `drop` gives it every drag
  expect(takes(field, { target: field, dataTransfer: drag({ 'text/plain': 'Mount Fuji' }) })).toBe(true);
  expect(carries(undefined)).toBe(false);
});

test("a drop's link is its first line of text/uri-list that is neither a comment nor a file", () => {
  const list = '# a comment\r\nfile:///Users/a/notes.txt\r\nhttps://example.com/fuji\r\nhttps://example.com/other';
  expect(words(drag({ 'text/uri-list': list, 'text/plain': 'Mount Fuji' }))).toEqual({ text: 'Mount Fuji', url: 'https://example.com/fuji' });
  expect(words(drag({ 'text/uri-list': 'mailto:a@example.com' }))).toEqual({ text: '', url: 'mailto:a@example.com' });
  expect(words(drag({ 'text/uri-list': 'FILE:///tmp/a' }))).toEqual({ text: '', url: '' });
  // A lone surrogate, which the runner's JSON cannot read, becomes U+FFFD
  expect(words(drag({ 'text/plain': 'a\ud800b' })).text).toBe('a\ufffdb');
});

test('a node takes a link alone over a field, which HTML inserts only as plain text', () => {
  const field = { closest: () => field }, node = { contains: () => true };
  expect(takes(node, { target: field, dataTransfer: drag({ 'text/uri-list': 'https://example.com/' }) })).toBe(true);
  expect(takes(node, { target: field, dataTransfer: drag({ 'text/uri-list': 'https://example.com/', 'text/plain': 'https://example.com/' }) })).toBe(false);
});

test("a drop's text arrives at once and its files once minted, and a drop that delivered nothing is a leave", async () => {
  const fileItem = { kind: 'file', getAsFileSystemHandle: () => Promise.resolve('handle'), webkitGetAsEntry: () => null };
  const dropOf = (data, items = []) => {
    const heard = [];
    const event = { dataTransfer: { ...drag(data, items.length > 0), items, files: items.map(() => ({ name: 'a.pdf' })) }, exactLeave: () => heard.push('leave') };
    return { event, heard };
  };
  const delivered = [];
  const deliver = (...args) => { delivered.push(args); return true; };
  const text = dropOf({ 'text/plain': 'Mount Fuji' });
  await receive(text.event, null, deliver, () => {});
  expect([delivered.pop(), text.heard]).toEqual([[[], 'Mount Fuji', ''], []]);
  const files = dropOf({ 'text/plain': 'caption' }, [fileItem]);
  await receive(files.event, () => ({ dropped: async (handles) => handles.map((h) => `doc:/1/${h}`) }), deliver, () => {});
  expect([delivered.pop(), files.heard]).toEqual([[['doc:/1/handle'], 'caption', ''], []]);
  // A mint that fails leaves the text, and one with nothing left is the leave
  const failed = dropOf({ 'text/plain': 'kept' }, [fileItem]);
  await receive(failed.event, () => ({ dropped: async () => { throw new Error('no room'); } }), deliver, () => {});
  expect(delivered.pop()).toEqual([[], 'kept', '']);
  const nothing = dropOf({}, [fileItem]), said = [];
  await receive(nothing.event, () => ({ dropped: async () => [] }), deliver, (line) => said.push(line));
  expect(nothing.heard).toEqual(['leave']);
  expect(said).toEqual(['drop: the browser handed over no file', 'drop: refused: nothing it carried arrived']);
  // A node gone by the time its files arrive is the leave too
  const gone = dropOf({}, [fileItem]);
  await receive(gone.event, () => ({ dropped: async () => ['doc:/1/a.pdf'] }), () => false, () => {});
  expect(gone.heard).toEqual(['leave']);
});

test('a drop that waits for its files says so before its action and lands after it', async () => {
  const fileItem = { kind: 'file', getAsFileSystemHandle: () => Promise.resolve('handle'), webkitGetAsEntry: () => null };
  const order = [];
  const event = { dataTransfer: { ...drag({}, true), items: [fileItem], files: [{ name: 'a.pdf' }] },
    exactWaits: () => order.push('waits'), exactLands: () => order.push('lands') };
  await receive(event, () => ({ dropped: async () => ['doc:/1/a.pdf'] }), () => { order.push('drop'); return true; }, () => {});
  expect(order).toEqual(['waits', 'drop', 'lands']);
});
