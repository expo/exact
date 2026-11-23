import { test, expect, spyOn } from 'bun:test';
import { closeSync, copyFileSync, cpSync, fstatSync, mkdtempSync, mkdirSync, existsSync, openSync, readFileSync, readdirSync, renameSync, rmSync, statSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { placeMacDylibs, placeMacLibraries } from './bundle.mjs';
import { spawnSync } from 'node:child_process';
import * as appleToolchain from './toolchain.mjs';
import { packageCompiler, swiftPrivacyManifests, swiftResourceOverlay } from './compose.mjs';
import { plistFile } from './assets.mjs';
import { useXcode } from './devices.mjs';
import { readPrivacyManifest, writePrivacyManifest } from './privacy.mjs';

test.skipIf(process.platform !== 'darwin')('SwiftPM Swift and Objective-C resources survive a signed Mac app relocation', () => {
  useXcode();
  mkdirSync(join(import.meta.dir, '../../target'), { recursive: true });
  const dir = mkdtempSync(join(import.meta.dir, '../../target/swift-resources-'));
  const write = (file, contents) => writeFileSync(join(dir, file), contents);
  const run = (command, args) => {
    const result = spawnSync(command, args, { cwd: dir, encoding: 'utf8' });
    expect(result.status, result.stderr).toBe(0);
    return result.stdout.trim();
  };
  try {
    for (const name of ['SwiftResources', 'ClangResources/include', 'ReadResources']) mkdirSync(join(dir, 'Sources', name), { recursive: true });
    write('Package.swift', `// swift-tools-version:5.9
import PackageDescription
let package = Package(name: "Resources",
  products: [.library(name: "SharedResources", type: .dynamic, targets: ["ReadResources"])], targets: [
    .target(name: "SwiftResources", resources: [.copy("value.txt"), .copy("PrivacyInfo.xcprivacy")]),
    .target(name: "ClangResources", resources: [.copy("value.txt")]),
    .target(name: "ReadResources", dependencies: ["SwiftResources", "ClangResources"])
])
`);
    write('Sources/SwiftResources/value.txt', 'swift resource');
    const sdkPrivacy = { NSPrivacyAccessedAPITypes: [{ NSPrivacyAccessedAPIType: 'NSPrivacyAccessedAPICategoryUserDefaults', NSPrivacyAccessedAPITypeReasons: ['CA92.1'] }],
      NSPrivacyTracking: true, NSPrivacyTrackingDomains: ['sdk.example'] };
    write('Sources/SwiftResources/PrivacyInfo.xcprivacy', plistFile(sdkPrivacy));
    write('Sources/SwiftResources/Read.swift', `import Foundation
public func swiftResource() throws -> String {
    let value = try String(contentsOf: Bundle.module.url(forResource: "value", withExtension: "txt")!, encoding: .utf8)
    UserDefaults.standard.set(value, forKey: "exact-resource-fixture")
    return UserDefaults.standard.string(forKey: "exact-resource-fixture")!
}
`);
    write('Sources/ClangResources/value.txt', 'clang resource');
    write('Sources/ClangResources/include/Read.h', 'const char *clangResource(void);\n');
    write('Sources/ClangResources/Read.m', `#import <Foundation/Foundation.h>
#import "Read.h"
const char *clangResource(void) {
    NSURL *url = [SWIFTPM_MODULE_BUNDLE URLForResource:@"value" withExtension:@"txt"];
    return [[NSString stringWithContentsOfURL:url encoding:NSUTF8StringEncoding error:NULL] UTF8String];
}
`);
    write('Sources/ReadResources/Read.swift', `import SwiftResources
import ClangResources
@_cdecl("read_resources") public func readResources() {
    precondition(try! swiftResource() == "swift resource")
    precondition(String(cString: clangResource()) == "clang resource")
    print("Swift and Objective-C resources read")
}
`);
    const args = ['swift', 'build', '--build-system', 'native', '--disable-build-manifest-caching', '-c', 'release'];
    const bin = run('xcrun', [...args, '--show-bin-path']);
    run('xcrun', [...args, '--print-manifest-job-graph']);
    run('xcrun', [...args, ...swiftResourceOverlay(bin)]);
    const bundle = join(dir, 'Relocated.app'), contents = join(bundle, 'Contents');
    mkdirSync(join(contents, 'MacOS'), { recursive: true });
    mkdirSync(join(contents, 'Resources'));
    const library = join(contents, 'MacOS/libSharedResources.dylib');
    copyFileSync(join(bin, 'libSharedResources.dylib'), library);
    expect(() => writePrivacyManifest(join(contents, 'Resources'), [library], { mac: true, modules: true })).toThrow('must declare its UserDefaults reason');
    const manifests = swiftPrivacyManifests(readdirSync(bin).filter(name => name.endsWith('.bundle')).map(name => join(bin, name)));
    expect(manifests).toHaveLength(1);
    writePrivacyManifest(join(contents, 'Resources'), [library], { mac: true, modules: true, manifests });
    expect(readPrivacyManifest(join(contents, 'Resources/PrivacyInfo.xcprivacy'))).toEqual(sdkPrivacy);
    write('main.swift', `import Foundation
let path = Bundle.main.executableURL!.deletingLastPathComponent().appendingPathComponent("libSharedResources.dylib").path
let library = dlopen(path, RTLD_NOW | RTLD_LOCAL)!
let read = unsafeBitCast(dlsym(library, "read_resources")!, to: (@convention(c) () -> Void).self)
read()
`);
    run('xcrun', ['swiftc', 'main.swift', '-o', join(contents, 'MacOS/ReadResources')]);
    run('codesign', ['--force', '--sign', '-', '--timestamp=none', library]);
    write('Relocated.app/Contents/Info.plist', plistFile({ CFBundleIdentifier: 'dev.exact.resources', CFBundleExecutable: 'ReadResources', CFBundlePackageType: 'APPL' }));
    for (const name of readdirSync(bin).filter(name => name.endsWith('.bundle'))) cpSync(join(bin, name), join(contents, 'Resources', name), { recursive: true });
    run('codesign', ['--force', '--sign', '-', '--timestamp=none', bundle]);
    run('codesign', ['--verify', '--deep', '--strict', bundle]);
    renameSync(join(dir, '.build'), join(dir, 'build-unavailable'));
    expect(run(join(contents, 'MacOS/ReadResources'), [])).toBe('Swift and Objective-C resources read');
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60_000);

test('a static SwiftPM framework or archive is force-loaded ahead of the copy SwiftPM links', () => {
  const dir = mkdtempSync(join(tmpdir(), 'swift-force-load-'));
  let swift;
  try {
    const bin = join(dir, 'bin'), artifacts = join(dir, 'vendor\\artifacts'), destination = join(dir, 'modules');
    mkdirSync(bin); mkdirSync(destination);
    const machO = type => { const head = Buffer.alloc(16); head.writeUInt32LE(0xfeedfacf); head.writeUInt32LE(type, 12); return head; };
    const archive = Buffer.from('!<arch>\n'.padEnd(16));
    for (const [name, contents] of [['Static', archive], ['Dynamic', machO(6)]]) {
      mkdirSync(join(artifacts, `${name}.framework`), { recursive: true });
      writeFileSync(join(artifacts, `${name}.framework`, name), contents);
    }
    writeFileSync(join(artifacts, 'libPlain.a'), archive);
    writeFileSync(join(bin, 'libExactSwiftDependencies.dylib'), '');
    // SwiftPM copies each binary target's product into `bin` and links that copy; `Other` is no input of this product.
    const copies = ['Static.framework', 'Dynamic.framework', 'libPlain.a'];
    const node = (root, name) => ({ kind: 'directory', name: join(root, name) });
    writeFileSync(join(bin, 'description.json'), JSON.stringify({
      swiftCommands: { dependencies: { moduleName: 'ExactSwiftDependencies', otherArguments: [], inputs: copies.map(name => node(bin, name)) } },
      copyCommands: Object.fromEntries([...copies, 'Other.framework'].map(name => [name, { inputs: [node(artifacts, name)], outputs: [node(bin, name)] }])),
    }));
    const swiftPackages = [{ url: 'https://fixture.invalid/vendor.git', exactVersion: '1.0.0', products: ['Vendor'] }];
    writeFileSync(join(dir, 'Package.resolved'), JSON.stringify({ version: 3, pins: [{ identity: 'vendor', location: swiftPackages[0].url,
      state: { version: '1.0.0', revision: 'a'.repeat(40) } }] }));
    let manifest;
    swift = spyOn(appleToolchain, 'spawnSync').mockImplementation((command, args, opts) => {
      if (command === 'swift') manifest = readFileSync(join(opts.cwd, 'Package.swift'), 'utf8');
      return { status: 0, stdout: args.includes('--show-bin-path') ? `${bin}\n` : '', stderr: '' };
    });
    packageCompiler({ app: { dir, target: dir, manifest: { host: { ios: { swiftPackages } } } }, packages: [], extensions: {},
      packageDir: destination, packageTargets: new Map([[destination, ['iphonesimulator', ['-target', 'arm64-apple-ios17.0-simulator']]]]),
      packageCopies: [], packagesRoot: dir, swiftc: 'fixture', ios: true }).place(destination);
    const [, encodedFlags] = manifest.match(/linkerSettings: \[\.unsafeFlags\((.*)\)\]\)\]\)$/m);
    const flags = JSON.parse(encodedFlags);
    const loaded = flags.flatMap((flag, index) => flag === '-force_load' ? [flags.slice(index - 1, index + 3)] : []);
    expect(loaded).toEqual(['Static.framework/Static', 'libPlain.a'].map(path =>
      ['-Xclang-linker', '-force_load', '-Xclang-linker', join(artifacts, path)]));
  } finally { swift?.mockRestore(); rmSync(dir, { recursive: true, force: true }); }
});

test('replacing package and service dylibs preserves the old open inode', () => {
  const root = mkdtempSync(join(tmpdir(), 'exact-dylibs-')), descriptors = [];
  try {
    const built = join(root, 'built.dylib');
    const libraries = ['libPackage.dylib', 'libService.dylib'].map(load => ({ built, load }));
    writeFileSync(built, 'first build');
    placeMacDylibs(root, libraries);
    for (const library of libraries) descriptors.push(openSync(join(root, library.load), 'r'));
    writeFileSync(built, 'second build');
    placeMacDylibs(root, libraries);
    for (const [i, library] of libraries.entries()) {
      const destination = join(root, library.load);
      expect(statSync(destination).ino).not.toBe(fstatSync(descriptors[i]).ino);
      expect(readFileSync(descriptors[i], 'utf8')).toBe('first build');
      expect(readFileSync(destination, 'utf8')).toBe('second build');
    }
  } finally {
    for (const descriptor of descriptors) closeSync(descriptor);
    rmSync(root, { recursive: true, force: true });
  }
});

// The Mac app and its sample host (LLP 1031 D10) carry the same prebuilt frameworks: the app's library and its
// native modules' dynamic xcframeworks, which module code loads from `@loader_path/../Frameworks`.
test('places and signs the app library and every module library', () => {
  const root = mkdtempSync(join(tmpdir(), 'exact-bundle-'));
  try {
    const lib = (name) => { const placed = join(root, 'built', `${name}.framework`); mkdirSync(placed, { recursive: true }); return { placed, link: name }; };
    const frameworks = join(root, 'Host.app/Contents/Frameworks'), signed = [];
    const placed = placeMacLibraries(frameworks, { library: lib('App'), moduleLibraries: [lib('Vision'), lib('Sync')] }, (p) => signed.push(p));
    expect(placed).toEqual(['App', 'Vision', 'Sync'].map((n) => join(frameworks, `${n}.framework`)));
    expect(signed).toEqual(placed);
    for (const p of placed) expect(existsSync(p)).toBe(true);
    expect(placeMacLibraries(frameworks, { library: null }, () => {})).toEqual([]);
  } finally { rmSync(root, { recursive: true, force: true }); }
});
