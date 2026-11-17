import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { createHash } from 'node:crypto';
import { chmodSync, mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { pendingBuildInputs } from './app.mjs';
import { notificationServiceDirectoryExists } from './notification-service.mjs';

const digest = bytes => createHash('sha256').update(bytes).digest('hex');
const identity = files => {
  const hash = createHash('sha256'), size = Buffer.alloc(8);
  for (const [name, bytes] of files) { const value = Buffer.from(bytes); size.writeBigUInt64LE(BigInt(value.length)); hash.update(name).update(Buffer.from([0])).update(size).update(value); }
  return { tree: hash.digest('hex'), files: files.map(([path, bytes]) => ({ path, sha256: digest(bytes) })) };
};

test('notification service freshness ignores excluded names and names a changed member', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-notification-freshness-'));
  const service = resolve(dir, 'modules/apple/notification-service'), helpers = resolve(service, 'Helpers');
  try {
    mkdirSync(helpers, { recursive: true });
    const files = [['Helpers.swift', 'func helper() {}\n'], ['Helpers/Decode.swift', 'func decode() {}\n'], ['NotificationService.swift', 'class NotificationService {}\n']];
    for (const [name, bytes] of files) writeFileSync(resolve(service, name), bytes);
    writeFileSync(resolve(service, '.DS_Store'), 'Finder metadata');
    mkdirSync(resolve(service, '.hidden')); writeFileSync(resolve(service, '.hidden/Ignored.swift'), 'ignored\n');
    const recorded = identity(files), build = { binary: { inputs: [], missing: [], links: [], watchOutputs: [], directories: [],
      inventories: [{ kind: 'notification-service', name: 'app/modules/apple/notification-service', path: service, identity: recorded }] } };
    assert.deepEqual(pendingBuildInputs(build), []);
    writeFileSync(resolve(service, '.DS_Store'), 'changed Finder metadata');
    writeFileSync(resolve(service, '.hidden/Ignored.swift'), 'changed ignored source\n');
    writeFileSync(resolve(service, '.new'), 'new ignored file'); mkdirSync(resolve(service, 'target')); writeFileSync(resolve(service, 'target/Generated.swift'), 'ignored\n');
    assert.deepEqual(pendingBuildInputs(build), []);
    writeFileSync(resolve(service, 'Helpers/Decode.swift'), 'func changed() {}\n');
    assert.deepEqual(pendingBuildInputs(build), ['app/modules/apple/notification-service/Helpers/Decode.swift']);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.skipIf(process.platform === 'win32' || process.getuid?.() === 0)('notification service discovery reports an unreadable ancestor', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-notification-permission-'));
  const modules = resolve(dir, 'modules'), service = resolve(modules, 'apple/notification-service');
  try {
    mkdirSync(service, { recursive: true });
    writeFileSync(resolve(service, 'NotificationService.swift'), 'class NotificationService {}\n');
    chmodSync(modules, 0o000);
    assert.throws(() => notificationServiceDirectoryExists(dir), error => error?.code === 'EACCES');
  } finally {
    chmodSync(modules, 0o700);
    rmSync(dir, { recursive: true, force: true });
  }
});
