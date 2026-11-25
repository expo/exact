import { test, expect } from 'bun:test';
import { mkdtempSync, rmSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { dropPushes, pushDrops, readerReply } from './agent-android-requests.mjs';

test('the Android carrier waits for the hold duration acknowledged by the host', async () => {
  const request = { op: 'tap', phase: 'hold', ms: 250 };
  const accepted = '{"phase":"hold","waitMs":250}';
  const waiting = Promise.withResolvers(), elapsed = Promise.withResolvers();
  let delivered = false;
  const reply = readerReply(request, () => accepted, ms => {
    waiting.resolve(ms);
    return elapsed.promise;
  }).then(line => { delivered = true; return line; });
  expect(await Promise.race([waiting.promise, reply])).toBe(250);
  await new Promise(resolve => setImmediate(resolve));
  expect(delivered).toBe(false);
  elapsed.resolve();
  expect(await reply).toBe(accepted);
  for (const line of ['{"phase":"hold","clock":250}', '{"error":"no contact is down"}']) {
    expect(await readerReply(request, () => line, () => { throw new Error('unexpected wait'); })).toBe(line);
  }
});

test("a drop's files go to the device, each in a folder of its own, and the request names them there; an enter's stay", () => {
  const base = '/data/local/tmp/exact/app/drops-1';
  const { request, pushes } = dropPushes({ op: 'tap', id: 7, drop: ['/a/notes.pdf', '/b/notes.pdf'], text: 'Fuji' }, base);
  expect(pushes.map(([local]) => local)).toEqual(['/a/notes.pdf', '/b/notes.pdf']);
  expect(new Set(pushes.map(([, remote]) => remote.split('/').slice(0, -1).join('/'))).size).toBe(2);
  expect(request).toEqual({ op: 'tap', id: 7, drop: pushes.map(([, remote]) => remote), text: 'Fuji' });
  expect(pushes.every(([, remote]) => remote.startsWith(`${base}/`) && remote.endsWith('/notes.pdf'))).toBe(true);
  for (const unchanged of [{ op: 'tap', id: 7, drop: [], text: 'Fuji' }, { op: 'tap', id: 7, dragenter: ['/a/notes.pdf'] }, { op: 'tap', id: 7, dragleave: true }, { op: 'tree' }])
    expect(dropPushes(unchanged, base)).toEqual({ request: unchanged, pushes: [] });
});

test("a folder, a failed push and a failed chmod are each refused by name, and no request goes on", () => {
  const folder = mkdtempSync(join(tmpdir(), 'exact-drop-folder-'));
  try {
    const sh = () => ({ status: 0 });
    expect(pushDrops({ op: 'tap', id: 7, drop: [folder] }, '/base', sh).error).toBe(`${folder} is a folder: a drop on Android takes files alone`);
  } finally { rmSync(folder, { recursive: true }); }
  expect(pushDrops({ op: 'tap', id: 7, drop: ['/no/such/file.pdf'] }, '/base', () => ({ status: 0 })).error)
    .toStartWith("the drop's file /no/such/file.pdf could not be read: ENOENT");
  const chmodFails = (...args) => (args[1] === 'chmod' ? { status: null, stderr: null, stdout: null, error: new Error('adb went away') } : { status: 0 });
  expect(pushDrops({ op: 'tap', id: 7, drop: [import.meta.path] }, '/base', chmodFails).error)
    .toBe("the drop's files could not be made readable on the device: adb went away");
});

test("a file that cannot be put on the device is named, and no request goes on", () => {
  const calls = [];
  const sh = (...args) => { calls.push(args); return args[0] === 'push' ? { status: 1, stderr: 'no space left on device' } : { status: 0 }; };
  expect(pushDrops({ op: 'tap', id: 7, drop: [import.meta.path] }, '/base', sh))
    .toEqual({ error: `the drop's file ${import.meta.path} could not be put on the device: no space left on device` });
  expect(calls.some(([verb]) => verb === 'chmod')).toBe(false);
  expect(pushDrops({ op: 'tree' }, '/base', sh)).toEqual({});
});
