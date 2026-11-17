import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { posix, win32 } from 'node:path';
import { keptInput } from './kept.mjs';

test('a kept product reads its dep-info by the platform separator: a target is skipped, outside keeps nothing', () => {
  const root = 'C:\\exact', skip = ['C:\\exact\\target'];
  assert.equal(keptInput('C:\\exact\\kernel\\src\\lib.rs', root, skip, win32), 'kernel\\src\\lib.rs');
  assert.equal(keptInput('C:\\exact\\target\\debug\\build\\k-0a\\out\\t.rs', root, skip, win32), undefined);
  assert.equal(keptInput('C:\\Users\\me\\.cargo\\registry\\src\\x\\lib.rs', root, skip, win32), null);
  assert.equal(keptInput('C:\\exactly\\lib.rs', root, skip, win32), null);
  assert.equal(keptInput('/exact/kernel/src/lib.rs', '/exact', ['/exact/target'], posix), 'kernel/src/lib.rs');
  assert.equal(keptInput('/exact/target/debug/out/t.rs', '/exact', ['/exact/target'], posix), undefined);
  assert.equal(keptInput('/home/me/.cargo/x.rs', '/exact', ['/exact/target'], posix), null);
});
