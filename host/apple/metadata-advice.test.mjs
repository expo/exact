// The bake supplies inputs derived from exactly the grant lines printed in each diagnostic.
import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { appleMetadataOwners } from './plist.mjs';
import { infoPlist, macInfoPlist, entitlements } from './build.mjs';

const derived = JSON.parse(process.env.EXACT_METADATA_CASES ?? 'null');
test.skipIf(!derived)('every specific recommendation emits its key through the platform writer', () => {
  assert.equal(process.platform, 'darwin', 'the Apple integration must run on a Mac');
  const app = { id: 'com.example.advice', displayName: 'Advice', manifest: {} };
  const checked = new Set();
  for (const { platform, kind, key, owner, reach } of derived) {
    if (kind === 'entitlements') assert.equal(platform, 'ios', 'add the Mac entitlement writer before advising a Mac grant');
    const output = kind === 'entitlements'
      ? entitlements(app, 'TEAM', true, reach, { allows: ['com.apple.developer.applesignin'] })
      : platform === 'ios' ? infoPlist(app, false, { reach }) : macInfoPlist(app, { reach });
    assert.ok(output.includes(`\n  <key>${key}</key>`), `${platform}.${kind}.${key}: ${owner}`);
    checked.add(JSON.stringify([platform, kind, key, owner]));
  }
  for (const [platform, kinds] of Object.entries(appleMetadataOwners()))
    for (const [kind, keys] of Object.entries(kinds)) for (const [key, owners] of keys) for (const owner of owners)
      assert.ok(checked.has(JSON.stringify([platform, kind, key, owner])), `untested advice: ${platform}.${kind}.${key}: ${owner}`);
  for (const key of ['aps-environment', 'com.apple.developer.applesignin', 'NSPhotoLibraryAddUsageDescription', 'NSMicrophoneUsageDescription'])
    assert.ok(derived.some(c => c.key === key), `missing diary key: ${key}`);
  console.log(`${checked.size} platform/key/owner writes verified`);
});
