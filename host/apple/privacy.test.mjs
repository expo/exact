import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { mkdtempSync, mkdirSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { capturePrivacyManifest, mergePrivacyManifests, privacyManifestForBinaries, readPrivacyManifest, writePrivacyManifest } from './privacy.mjs';
import { signedGpuProduct, plistFile } from './assets.mjs';
import { appleDeadStrip } from './link.mjs';
import { appleImplementations } from '../../js/bake/src/typescript.mjs';

const row = (name, reasons) => ({ NSPrivacyAccessedAPIType: `NSPrivacyAccessedAPICategory${name}`, NSPrivacyAccessedAPITypeReasons: reasons });
const manifest = (...rows) => ({ NSPrivacyAccessedAPITypes: rows });
const command = (cmd, args) => {
  const out = spawnSync(cmd, args, { encoding: 'utf8' });
  assert.equal(out.status, 0, out.stderr); return out.stdout;
};

test.skipIf(process.platform !== 'darwin')('the manifest generator packages parenthesized executable names with linked API reasons and valid plists', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-privacy-'));
  try {
    const source = resolve(dir, 'fixture.m'), bundle = resolve(dir, 'App (Exact).app');
    mkdirSync(bundle);
    const binary = resolve(bundle, 'App (Exact)');
    writeFileSync(source, `#import <Foundation/Foundation.h>
#include <sys/stat.h>
#include <mach/mach_time.h>
int main(int argc, char **argv) {
  struct stat info; stat(argv[0], &info);
  return (int)(info.st_mtime + mach_absolute_time() + [[NSUserDefaults standardUserDefaults] integerForKey:@"fixture"]);
}`);
    command('xcrun', ['clang', source, '-framework', 'Foundation', '-o', binary]);
    const expected = manifest(row('FileTimestamp', ['3B52.1', 'C617.1']), row('SystemBootTime', ['35F9.1']), row('UserDefaults', ['CA92.1']));
    assert.deepEqual(privacyManifestForBinaries([binary]), expected);
    const file = writePrivacyManifest(bundle, [binary]);
    command('plutil', ['-lint', file]);
    assert.deepEqual(readPrivacyManifest(file), expected);
    // Only an Objective-C selector reference, not the UITextInputMode class, establishes use of activeInputModes.
    writeFileSync(source, '#import <Foundation/Foundation.h>\nint main() { return (int)[[NSObject new] performSelector:@selector(activeInputModes)]; }');
    command('xcrun', ['clang', source, '-framework', 'Foundation', '-o', binary]);
    assert.deepEqual(privacyManifestForBinaries([binary]), manifest(row('ActiveKeyboards', ['54BD.1'])));
    writeFileSync(source, '#import <Foundation/Foundation.h>\nint main() { return (int)[NSProcessInfo processInfo].systemUptime; }');
    command('xcrun', ['clang', source, '-framework', 'Foundation', '-o', binary]);
    assert.throws(() => privacyManifestForBinaries([binary], [], { modules: true }), /must declare its SystemBootTime reason/);
    const sensor = manifest(row('SystemBootTime', ['8FFB.1']));
    assert.deepEqual(privacyManifestForBinaries([binary], [sensor], { modules: true }), sensor);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.skipIf(process.platform !== 'darwin')('ordinary C strings are not Objective-C API uses', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-privacy-strings-'));
  try {
    const source = resolve(dir, 'strings.m'), binary = resolve(dir, 'Strings');
    writeFileSync(source, `const char *fields[] = { "systemUptime", "activeInputModes", "fileModificationDate" };
int main(void) { return fields[0][0]; }`);
    command('xcrun', ['clang', source, '-framework', 'Foundation', '-o', binary]);
    assert.deepEqual(privacyManifestForBinaries([binary], [manifest(row('DiskSpace', ['E174.1']))], { modules: true }), manifest());
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.skipIf(process.platform !== 'darwin')('app and package reasons are unioned, with data and tracking disclosures preserved', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-privacy-merge-'));
  try {
    const binary = resolve(dir, 'Defaults'), source = resolve(dir, 'defaults.m');
    writeFileSync(source, '#import <Foundation/Foundation.h>\nint main(void) { return (int)[[NSUserDefaults standardUserDefaults] integerForKey:@"fixture"]; }');
    command('xcrun', ['clang', source, '-framework', 'Foundation', '-o', binary]);
    const app = resolve(dir, 'PrivacyInfo.xcprivacy'), pkg = resolve(dir, 'package/PrivacyInfo.xcprivacy');
    mkdirSync(resolve(dir, 'package'));
    const data = (linked, purposes) => ({ NSPrivacyCollectedDataType: 'NSPrivacyCollectedDataTypeEmailAddress', NSPrivacyCollectedDataTypeLinked: linked, NSPrivacyCollectedDataTypeTracking: false, NSPrivacyCollectedDataTypePurposes: purposes });
    writeFileSync(app, plistFile({ ...manifest(row('UserDefaults', ['CA92.1'])), NSPrivacyTracking: false, NSPrivacyCollectedDataTypes: [data(false, ['NSPrivacyCollectedDataTypePurposeAppFunctionality'])] }));
    writeFileSync(pkg, plistFile({ ...manifest(row('UserDefaults', ['CA92.1', '1C8F.1'])), NSPrivacyTracking: true, NSPrivacyTrackingDomains: ['tracking.example'], NSPrivacyCollectedDataTypes: [data(true, ['NSPrivacyCollectedDataTypePurposeAnalytics'])] }));
    const file = writePrivacyManifest(resolve(dir, 'App.app'), [binary], { manifests: [app, pkg].map(capturePrivacyManifest) });
    command('plutil', ['-lint', file]);
    assert.deepEqual(readPrivacyManifest(file), { ...manifest(row('UserDefaults', ['1C8F.1', 'CA92.1'])), NSPrivacyTracking: true, NSPrivacyTrackingDomains: ['tracking.example'], NSPrivacyCollectedDataTypes: [data(true, ['NSPrivacyCollectedDataTypePurposeAnalytics', 'NSPrivacyCollectedDataTypePurposeAppFunctionality'])] });
    assert.deepEqual(privacyManifestForBinaries([binary]), manifest(row('UserDefaults', ['CA92.1'])));
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.skipIf(process.platform !== 'darwin')('a used package supplies its own declaration from its Apple implementation', () => {
  const app = resolve(import.meta.dir, '../../apps/fieldnotes');
  const packages = appleImplementations(app, 'ios', { altimeter: '@exact/altimeter', clipboard: '@exact/clipboard' });
  assert.ok(packages.length > 0);
  const declarations = packages.map(p => readPrivacyManifest(p.privacyManifest));
  assert.ok(declarations.some(m => m.NSPrivacyAccessedAPITypes?.some(r => r.NSPrivacyAccessedAPITypeReasons.includes('8FFB.1'))));
  for (const p of packages) command('plutil', ['-lint', p.privacyManifest]);
});

test.skipIf(process.platform !== 'darwin')('filesystem-type probes cannot silently claim a disk-space reason', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-privacy-disk-'));
  try {
    const source = resolve(dir, 'disk.c'), binary = resolve(dir, 'Disk');
    writeFileSync(source, '#include <sys/mount.h>\nint main(void) { struct statfs s; return statfs("/", &s); }');
    command('xcrun', ['clang', source, '-o', binary]);
    assert.throws(() => privacyManifestForBinaries([binary]), /disk-space APIs without an audited reason/);
    assert.deepEqual(privacyManifestForBinaries([binary], [manifest(row('DiskSpace', ['E174.1']))]), manifest(row('DiskSpace', ['E174.1'])));
    assert.deepEqual(privacyManifestForBinaries([binary], [], { mac: true }), manifest());
    const product = resolve(dir, 'disk.dylib'), path = resolve(dir, 'PrivacyInfo.xcprivacy');
    command('xcrun', ['clang', '-dynamiclib', source, '-o', product]);
    writeFileSync(path, plistFile(manifest(row('DiskSpace', ['E174.1']))));
    const captured = capturePrivacyManifest(path);
    writeFileSync(path, plistFile(manifest()));
    const signed = signedGpuProduct(product, { app: { id: 'com.exact.privacy', manifest: {}, dir }, framework: 'ExactGpu', privacyManifest: captured });
    assert.deepEqual(readPrivacyManifest(resolve(signed, '../PrivacyInfo.xcprivacy')), manifest(row('DiskSpace', ['E174.1'])));
    command('codesign', ['--verify', '--strict', resolve(signed, '..')]);
    writeFileSync(source, '#include <sys/attr.h>\n#include <unistd.h>\nint main(void) { struct attrlist a = {0}; char b[256]; return getattrlist("/", &a, b, sizeof(b), 0); }');
    command('xcrun', ['clang', source, '-o', binary]);
    assert.throws(() => privacyManifestForBinaries([binary]), /audit getattrlist attributes/);
    const files = manifest(row('FileTimestamp', ['C617.1']));
    assert.deepEqual(privacyManifestForBinaries([binary], [files]), files);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// Hermes's llvh Path.cpp keeps `disk_space` and `is_local` beside the file functions Hermes calls, and a
// link without dead stripping kept their `statfs` imports in every development executable (#408).
test.skipIf(process.platform !== 'darwin')('a development link drops an archived object\'s unreached disk-space call', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-privacy-strip-'));
  try {
    const path = resolve(dir, 'path.c'), main = resolve(dir, 'main.c'), binary = resolve(dir, 'App');
    writeFileSync(path, `#include <sys/mount.h>
__attribute__((visibility("hidden"))) long disk_space(const char *p) { struct statfs s; return statfs(p, &s) ? -1 : (long)s.f_bavail; }
__attribute__((visibility("hidden"))) int open_for_read(const char *p) { return p[0]; }`);
    writeFileSync(main, 'int open_for_read(const char *);\nint main(int argc, char **argv) { return open_for_read(argv[0]); }');
    command('xcrun', ['clang', '-c', path, '-o', resolve(dir, 'Path.o')]);
    command('xcrun', ['ar', 'rcs', resolve(dir, 'libpath.a'), resolve(dir, 'Path.o')]);
    command('xcrun', ['clang', main, '-L', dir, '-lpath', '-o', binary]);
    assert.throws(() => privacyManifestForBinaries([binary]), /disk-space APIs without an audited reason/, 'the whole object, without dead stripping');
    command('xcrun', ['clang', main, '-L', dir, '-lpath', ...appleDeadStrip, '-o', binary]);
    assert.deepEqual(privacyManifestForBinaries([binary]), manifest());
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('malformed supplied declarations fail instead of being dropped', () => {
  for (const input of [{ NSPrivacyTracking: 'false' }, { NSPrivacyAccessedAPITypes: {} }, manifest(row('Unknown', ['CA92.1'])), manifest(row('UserDefaults', [])), manifest(row('UserDefaults', ['bad'])), { NSPrivacyTrackng: false }]) {
    assert.throws(() => mergePrivacyManifests(input), /privacy manifest:/);
  }
});


test.skipIf(process.platform !== 'darwin')('Swift Foundation capacity keys and values require a supplied disk-space reason', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-privacy-swift-'));
  try {
    const source = resolve(dir, 'disk.swift'), binary = resolve(dir, 'Disk');
    const disk = manifest(row('DiskSpace', ['E174.1']));
    for (const property of ['volumeAvailableCapacity', 'volumeTotalCapacity', 'volumeAvailableCapacityForImportantUsage', 'volumeAvailableCapacityForOpportunisticUsage']) {
      for (const expression of [`URLResourceKey.${property}Key`, `URLResourceValues().${property}`]) {
        writeFileSync(source, `import Foundation\nprint(${expression} as Any)\n`);
        command('xcrun', ['swiftc', source, '-o', binary]);
        assert.throws(() => privacyManifestForBinaries([binary]), /disk-space APIs without an audited reason/, expression);
        assert.deepEqual(privacyManifestForBinaries([binary], [disk]), disk, expression);
      }
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60_000);

for (const property of ['creationDate', 'contentModificationDate']) {
  test.skipIf(process.platform !== 'darwin')(`Swift Foundation ${property} values retain supplied timestamp reasons`, () => {
    const dir = mkdtempSync(resolve(tmpdir(), 'exact-privacy-timestamp-'));
    try {
      const source = resolve(dir, 'timestamp.swift'), binary = resolve(dir, 'Timestamp');
      writeFileSync(source, `import Foundation\npublic func timestamp(_ values: URLResourceValues) -> Date? { values.${property} }\n`);
      command('xcrun', ['swiftc', '-emit-library', source, '-o', binary]);
      const supplied = manifest(row('FileTimestamp', ['DDA9.1']));
      const path = resolve(dir, 'PrivacyInfo.xcprivacy');
      writeFileSync(path, plistFile(supplied));
      const output = writePrivacyManifest(resolve(dir, 'Timestamp.framework'), [binary], { manifests: [capturePrivacyManifest(path)], modules: true });
      assert.deepEqual(readPrivacyManifest(output), manifest(row('FileTimestamp', ['3B52.1', 'C617.1', 'DDA9.1'])));
    } finally { rmSync(dir, { recursive: true, force: true }); }
  });
}

for (const reason of ['CA92.1', 'FFFF.999']) {
  test(`a supplied disk-space reason must be approved for that category: ${reason}`, () => {
    assert.throws(() => mergePrivacyManifests(manifest(row('DiskSpace', [reason]))), /privacy manifest:/);
  });
}

test.skipIf(process.platform !== 'darwin')('packaging uses captured manifest bytes after the source changes or disappears', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-privacy-captured-'));
  try {
    const source = resolve(dir, 'empty.c'), binary = resolve(dir, 'Empty'), path = resolve(dir, 'PrivacyInfo.xcprivacy');
    writeFileSync(source, 'int main(void) { return 0; }');
    command('xcrun', ['clang', source, '-o', binary]);
    const absent = capturePrivacyManifest(path);
    const expected = { ...manifest(), NSPrivacyTracking: true, NSPrivacyTrackingDomains: ['original.example'] };
    writeFileSync(path, plistFile(expected));
    const captured = capturePrivacyManifest(path);
    assert.deepEqual(readPrivacyManifest(writePrivacyManifest(resolve(dir, 'Absent.app'), [binary], { manifests: [absent] })), manifest());
    for (const removed of [false, true]) {
      if (removed) rmSync(path); else writeFileSync(path, plistFile({ NSPrivacyTracking: false }));
      const output = writePrivacyManifest(resolve(dir, 'App.app'), [binary], { manifests: [captured] });
      assert.deepEqual(readPrivacyManifest(output), expected);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
