import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { posix, win32 } from 'node:path';
import { slashRelative, within } from './paths.mjs';

test('a path is within a directory by its own platform separator', () => {
  assert.ok(within('/w/app', '/w/app', posix));
  assert.ok(within('/w/app', '/w/app/src/lib.rs', posix));
  assert.ok(within('/', '/w', posix));
  assert.ok(!within('/w/app', '/w/apps/x', posix));
  assert.ok(!within('/w/app', '/w', posix));
  assert.ok(within('C:\\w\\app', 'C:\\w\\app\\src\\lib.rs', win32));
  assert.ok(within('C:\\w\\app', 'C:/w/app/src/lib.rs', win32));
  assert.ok(within('C:\\w\\app', 'c:\\W\\App\\src', win32));
  assert.ok(within('C:\\', 'C:\\w', win32));
  assert.ok(!within('C:\\w\\app', 'C:\\w\\apps\\x', win32));
  assert.ok(!within('C:\\w\\app', 'C:\\w', win32));
  assert.ok(!within('C:\\w\\app', 'D:\\w\\app\\x', win32));
});

test('a relative path is written with slashes on either platform', () => {
  assert.equal(slashRelative('/w/app', '/w/app/assets/a.png', posix), 'assets/a.png');
  assert.equal(slashRelative('C:\\w\\app', 'C:\\w\\app\\assets\\a.png', win32), 'assets/a.png');
  assert.equal(slashRelative('C:\\w\\app', 'C:\\w\\other', win32), '../other');
});
