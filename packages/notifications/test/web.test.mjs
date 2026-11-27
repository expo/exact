import { afterEach, expect, test } from 'bun:test';
import { implementation } from '../web/index.js';

const original = Object.getOwnPropertyDescriptor(globalThis, 'Notification');
afterEach(() => {
  if (original) Object.defineProperty(globalThis, 'Notification', original);
  else delete globalThis.Notification;
});

test('an empty tag is no tag: a second post and a close leave the first up', async () => {
  const closed = [];
  globalThis.Notification = class {
    static permission = 'granted';
    constructor(title) { this.title = title; }
    addEventListener() {}
    close() { closed.push(this.title); }
  };
  const impl = implementation({ changed() {}, activation() {}, emit() {}, foreground: () => true });
  expect(await impl.showNotification({ title: 'first', tag: '' })).toBe(null);
  expect(await impl.showNotification({ title: 'second', tag: '' })).toBe(null);
  expect(await impl.closeNotification({ tag: '' })).toBe(null);
  expect(closed).toEqual([]);
  // A named tag still replaces and closes.
  await impl.showNotification({ title: 'third', tag: 'a' });
  await impl.showNotification({ title: 'fourth', tag: 'a' });
  await impl.closeNotification({ tag: 'a' });
  expect(closed).toEqual(['third', 'fourth']);
});
