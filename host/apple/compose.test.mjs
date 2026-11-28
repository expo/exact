import { test, expect, spyOn } from 'bun:test';
import assert from 'node:assert/strict';
import { execFileSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, realpathSync, rmSync, symlinkSync, writeFileSync } from 'node:fs';
import { resolve } from 'node:path';
import { packageCompiler } from './compose.mjs';
import * as appleToolchain from './toolchain.mjs';
import { useXcode } from './devices.mjs';

test.skipIf(process.platform !== 'darwin')('packageCompiler.place compiles packages concurrently, deduplicates in-flight calls, and hits packages-kept across fresh build dirs (#1612)', async () => {
  useXcode();
  const root = resolve(import.meta.dir, '../..');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/compose-parallel-'));
  const prevJobs = process.env.EXACT_SWIFT_JOBS;
  process.env.EXACT_SWIFT_JOBS = '2';
  let spy;
  try {
    const makePkg = (index) => {
      const name = `ExactPkg_par${index}`;
      const src = resolve(dir, `src/Par${index}.swift`);
      mkdirSync(resolve(dir, 'src'), { recursive: true });
      writeFileSync(src, `import ExactPackage
public final class Par${index}: ExactPackage {
    public init(context: ExactPackageContext) {}
    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) { reply([:]) }
    public func call(_ name: String, _ args: [String: Any]) throws -> [String: Any] { ["id": ${index}] }
}
`);
      return { extension: `par${index}`, module: name, className: `Par${index}`, sources: [src] };
    };
    const packages = [1, 2, 3, 4].map(makePkg);
    const dest1 = resolve(dir, 'run-1/packages'), dest2 = resolve(dir, 'run-2/packages');
    const cacheRoot = resolve(dir, 'shared-cache');
    const origSpawn = appleToolchain.spawn;
    let active = 0, peak = 0, spawns = 0;
    spy = spyOn(appleToolchain, 'spawn').mockImplementation((cmd, args, opts) => {
      const child = origSpawn(cmd, args, opts);
      if (cmd === 'xcrun') {
        spawns++;
        active++;
        if (active > peak) peak = active;
        child.once('close', () => { active--; });
      }
      return child;
    });

    const compiler1 = packageCompiler({
      packages, packageDir: dest1, packageTargets: new Map([[dest1, ['macosx', []]]]),
      packageCopies: packages.map(p => p.sources), seam: resolve(root, 'host/apple/modules/ExactPackage.swift'),
      packagesRoot: cacheRoot, swiftc: 'fixture', production: false, distribution: false, ios: false, tv: false,
    });
    // Concurrent calls for the same directory share one in-flight placement.
    await Promise.all([compiler1.place(dest1), compiler1.place(dest1)]);
    expect(peak).toBeGreaterThan(1);
    for (const p of packages) assert.ok(readFileSync(resolve(dest1, `lib${p.module}.dylib`)).length > 0);
    const coldSpawns = spawns;
    expect(coldSpawns).toBe(2 + packages.length * 2); // seam (object+stub) + 4*(object+link)

    // A second build in a fresh private directory hits packages-kept with zero xcrun spawns.
    const compiler2 = packageCompiler({
      packages, packageDir: dest2, packageTargets: new Map([[dest2, ['macosx', []]]]),
      packageCopies: packages.map(p => p.sources), seam: resolve(root, 'host/apple/modules/ExactPackage.swift'),
      packagesRoot: cacheRoot, swiftc: 'fixture', production: false, distribution: false, ios: false, tv: false,
    });
    await compiler2.place(dest2);
    expect(spawns).toBe(coldSpawns);
    for (const p of packages) assert.ok(readFileSync(resolve(dest2, `lib${p.module}.dylib`)).length > 0);
  } finally {
    if (prevJobs === undefined) delete process.env.EXACT_SWIFT_JOBS;
    else process.env.EXACT_SWIFT_JOBS = prevJobs;
    spy?.mockRestore();
    rmSync(dir, { recursive: true, force: true });
  }
}, 120_000);

test.skipIf(process.platform !== 'darwin')('app modules can import packages with ObjC modules, including @exact/media-capture and cache hits (#2090)', async () => {
  useXcode();
  const root = resolve(import.meta.dir, '../..');
  mkdirSync(resolve(root, 'target'), { recursive: true });
  const dir = mkdtempSync(resolve(root, 'target/compose-objc-2090-'));
  try {
    const mediaDir = resolve(root, 'packages/media-capture/apple');
    const mediaPkg = {
      extension: 'media-capture',
      module: 'ExactPkg_media_capture',
      className: 'MediaCaptureModule',
      sources: [resolve(mediaDir, 'MediaCapture.swift'), resolve(mediaDir, 'VideoCapture.swift')],
      objc: [resolve(mediaDir, 'MediaCaptureBridge.m')],
      objcModule: { name: 'MediaCaptureObjC', header: resolve(mediaDir, 'MediaCaptureBridge.h') },
    };

    const synthDir = resolve(dir, 'synth');
    const sharedDir = resolve(dir, 'Shared');
    mkdirSync(resolve(synthDir, 'include/Detail'), { recursive: true });
    mkdirSync(resolve(synthDir, 'src'), { recursive: true });
    mkdirSync(sharedDir, { recursive: true });
    const synthShared = resolve(sharedDir, 'Config.inc');
    writeFileSync(synthShared, '#define EXACT_SYNTH_SHARED 10\n');
    const synthLinkedTarget = resolve(dir, 'linked-extra.inc');
    writeFileSync(synthLinkedTarget, '#pragma once\n#define EXACT_SYNTH_LINKED 7\n');
    const synthChoiceA = resolve(synthDir, 'include/ChoiceA.h');
    const synthChoiceB = resolve(synthDir, 'include/ChoiceB.h');
    const synthChoiceLink = resolve(synthDir, 'include/ZChoice.h');
    writeFileSync(synthChoiceA, '#define EXACT_SYNTH_CHOICE 100\n');
    writeFileSync(synthChoiceB, '#define EXACT_SYNTH_CHOICE 200\n');
    symlinkSync(synthChoiceA, synthChoiceLink);
    const synthDetail = resolve(synthDir, 'include/Detail/Extra.inc');
    const synthAliasDir = resolve(synthDir, 'include/Alias');
    const synthLinked = resolve(synthDir, 'include/Linked.h');
    const synthLocalCfg = resolve(synthDir, 'include/Collision.inc');
    const synthHeader = resolve(synthDir, 'include/SynthBridge.h');
    const synthPrivate = resolve(synthDir, 'src/Private.h');
    const synthObjc = resolve(synthDir, 'src/SynthBridge.m');
    const synthSwift = resolve(synthDir, 'Synth.swift');
    writeFileSync(synthDetail, '#pragma once\n#define EXACT_SYNTH_EXTRA 1\n');
    writeFileSync(synthLocalCfg, '#define EXACT_PKG_A_ONLY 1\n');
    symlinkSync(resolve(synthDir, 'include/Detail'), synthAliasDir);
    symlinkSync(synthLinkedTarget, synthLinked);
    writeFileSync(synthHeader, '#import <Foundation/Foundation.h>\n#include "Detail/Extra.inc"\n#import "Alias/Extra.inc"\n#import "Linked.h"\n#include "Collision.inc"\n#include "ZChoice.h"\n@interface ExactSynthBox : NSObject\n+ (NSInteger)answer;\n+ (const char *)sourcePath;\n@end\n');
    writeFileSync(synthPrivate, '#define EXACT_SYNTH_PRIVATE 25\n');
    writeFileSync(synthObjc, '#import "SynthBridge.h"\n#import "Private.h"\n#include "../../Shared/Config.inc"\n@implementation ExactSynthBox\n+ (NSInteger)answer { return EXACT_SYNTH_PRIVATE + EXACT_SYNTH_SHARED + EXACT_SYNTH_LINKED + EXACT_SYNTH_CHOICE; }\n+ (const char *)sourcePath { return __FILE__; }\n@end\n');
    writeFileSync(synthSwift, `import ExactPackage
public import SynthObjC
public final class SynthModule: ExactPackage {
    public init(context: ExactPackageContext) {}
    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) { reply([:]) }
    public func call(_ name: String, _ args: [String: Any]) throws -> [String: Any] { [:] }
    public static func bridgeType() -> ExactSynthBox.Type { ExactSynthBox.self }
}
`);
    const synthPkg = {
      extension: 'synth',
      module: 'ExactPkg_synth',
      className: 'SynthModule',
      sources: [synthSwift],
      objc: [synthObjc],
      objcModule: { name: 'SynthObjC', header: synthHeader },
    };

    // Second package with an identically named Collision.inc header: downstream Swift must not shadow it.
    const peerDir = resolve(dir, 'peer');
    mkdirSync(resolve(peerDir, 'include'), { recursive: true });
    writeFileSync(resolve(peerDir, 'include/Collision.inc'), '#define EXACT_PKG_B_ONLY 2\n');
    const peerHeader = resolve(peerDir, 'include/PeerBridge.h');
    writeFileSync(peerHeader, '#import <Foundation/Foundation.h>\n#include "Collision.inc"\n#ifndef EXACT_PKG_B_ONLY\n#error shadowed by another package header dir\n#endif\n@interface ExactPeerBox : NSObject\n+ (NSInteger)tag;\n@end\n');
    const peerObjc = resolve(peerDir, 'PeerBridge.m');
    writeFileSync(peerObjc, '#import "PeerBridge.h"\n@implementation ExactPeerBox\n+ (NSInteger)tag { return EXACT_PKG_B_ONLY; }\n@end\n');
    const peerSwift = resolve(peerDir, 'Peer.swift');
    writeFileSync(peerSwift, `import ExactPackage
public import PeerObjC
public final class PeerModule: ExactPackage {
    public init(context: ExactPackageContext) {}
    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) { reply([:]) }
    public func call(_ name: String, _ args: [String: Any]) throws -> [String: Any] { [:] }
    public static func peerType() -> ExactPeerBox.Type { ExactPeerBox.self }
}
`);
    const peerPkg = {
      extension: 'peer',
      module: 'ExactPkg_peer',
      className: 'PeerModule',
      sources: [peerSwift],
      objc: [peerObjc],
      objcModule: { name: 'PeerObjC', header: peerHeader },
    };

    const packages = [mediaPkg, synthPkg, peerPkg];
    const dest1 = resolve(dir, 'run-1/packages'), dest2 = resolve(dir, 'run-2/packages');
    const cacheRoot = resolve(dir, 'shared-cache');
    const makeCompiler = (dest) => packageCompiler({
      packages, packageDir: dest, packageTargets: new Map([[dest, ['macosx', []]]]),
      packageCopies: packages.map(p => p.sources), seam: resolve(root, 'host/apple/modules/ExactPackage.swift'),
      packagesRoot: cacheRoot, swiftc: 'fixture', production: false, distribution: false, ios: false, tv: false,
    });

    const compiler1 = makeCompiler(dest1);
    await compiler1.place(dest1);
    // __FILE__ resolves to the authored path directly without staging or rewriting.
    const coldObjcObj = readFileSync(resolve(dest1, 'ExactPkg_synth_objc_0.o'));
    expect(coldObjcObj.includes(synthObjc) || coldObjcObj.includes(realpathSync(synthObjc))).toBe(true);
    expect(coldObjcObj.includes('run-1')).toBe(false);

    const mediaConsumer = resolve(dir, 'MediaConsumer.swift');
    writeFileSync(mediaConsumer, 'import ExactPkg_media_capture\npublic func mediaType() -> MediaCaptureModule.Type { MediaCaptureModule.self }\n');
    const synthConsumer = resolve(dir, 'SynthConsumer.swift');
    writeFileSync(synthConsumer, 'import ExactPkg_synth\nimport ExactPkg_peer\npublic func synthAnswer() -> Int { SynthModule.bridgeType().answer() + PeerModule.peerType().tag() }\n');

    const compileConsumer = (swiftFile, dest, extraImports, outName) => new Promise((ok, fail) => {
      const child = appleToolchain.spawn('xcrun', [
        '--sdk', 'macosx', 'swiftc', '-parse-as-library', '-emit-object', '-swift-version', '5',
        '-module-name', 'Consumer', '-I', dest, ...extraImports, swiftFile, '-o', resolve(dir, outName),
      ], { stdio: ['ignore', 'pipe', 'pipe'] });
      let err = '';
      child.stderr.on('data', d => { err += d; });
      child.on('close', code => code === 0 ? ok() : fail(new Error(err || `swiftc exited ${code}`)));
    });

    // Compiling a downstream app module against @exact/media-capture requires compiler.imports(dest)
    // to expose only each package's objc-<module> modulemap directory (never shared package header dirs).
    expect(compiler1.imports(dest1)).toEqual([
      '-I', resolve(dest1, 'objc-ExactPkg_media_capture'),
      '-I', resolve(dest1, 'objc-ExactPkg_synth'),
      '-I', resolve(dest1, 'objc-ExactPkg_peer'),
    ]);
    await compileConsumer(mediaConsumer, dest1, compiler1.imports(dest1), 'media-cold.o');
    await compileConsumer(synthConsumer, dest1, compiler1.imports(dest1), 'synth-cold.o');

    // Delete the cold build directory before exercising the warm packages-kept hit so a cached
    // Swift module cannot silently read module maps or headers left behind in dest1.
    const coldStamp = compiler1.stamp(dest1);
    const dylibBefore = readFileSync(resolve(dest1, 'ExactPkg_synth.dylib'));
    rmSync(dest1, { recursive: true, force: true });

    // A fresh build dir hitting packages-kept still writes objc-<module>/module.modulemap with zero xcrun spawns.
    const compiler2 = makeCompiler(dest2);
    let warmSpawns = 0;
    const origSpawn = appleToolchain.spawn;
    const spy = spyOn(appleToolchain, 'spawn').mockImplementation((cmd, args, opts) => {
      if (cmd === 'xcrun') warmSpawns++;
      return origSpawn(cmd, args, opts);
    });
    try {
      await compiler2.place(dest2);
      expect(warmSpawns).toBe(0);
    } finally {
      spy.mockRestore();
    }
    expect(compiler2.stamp(dest2)).toBe(coldStamp);
    await compileConsumer(mediaConsumer, dest2, compiler2.imports(dest2), 'media-warm.o');
    await compileConsumer(synthConsumer, dest2, compiler2.imports(dest2), 'synth-warm.o');

    // Retargeting a symlink (ZChoice.h -> ChoiceB.h) when both targets are already in the directory still invalidates the cache.
    rmSync(synthChoiceLink);
    symlinkSync(synthChoiceB, synthChoiceLink);
    const destRetarget = resolve(dir, 'run-retarget/packages');
    const compilerRetarget = makeCompiler(destRetarget);
    await compilerRetarget.place(destRetarget);
    expect(compilerRetarget.stamp(destRetarget)).not.toBe(coldStamp);

    // A header-only change to an unused ObjC declaration still invalidates compiler.stamp(dir) even when the dylib is unchanged.
    const dylibAfterRetarget = readFileSync(resolve(destRetarget, 'ExactPkg_synth.dylib'));
    writeFileSync(synthHeader, '#import <Foundation/Foundation.h>\n#include "Detail/Extra.inc"\n#import "Alias/Extra.inc"\n#import "Linked.h"\n#include "Collision.inc"\n#include "ZChoice.h"\n@interface ExactSynthBox : NSObject\n+ (NSInteger)answer;\n+ (const char *)sourcePath;\n+ (NSInteger)unused;\n@end\n');
    const dest3 = resolve(dir, 'run-3/packages');
    const compiler3 = makeCompiler(dest3);
    await compiler3.place(dest3);
    expect(readFileSync(resolve(dest3, 'ExactPkg_synth.dylib'))).toEqual(dylibAfterRetarget);
    expect(compiler3.stamp(dest3)).not.toBe(compilerRetarget.stamp(destRetarget));

    // A change inside a nested `.inc`, sibling `src/Private.h`, or parent `../Shared/Config.inc` invalidates packages-kept and compiler.stamp(dir).
    writeFileSync(synthDetail, '#pragma once\n#define EXACT_SYNTH_EXTRA 2\n');
    const dest4 = resolve(dir, 'run-4/packages');
    const compiler4 = makeCompiler(dest4);
    await compiler4.place(dest4);
    expect(compiler4.stamp(dest4)).not.toBe(compiler3.stamp(dest3));

    writeFileSync(synthPrivate, '#define EXACT_SYNTH_PRIVATE 26\n');
    const dest5 = resolve(dir, 'run-5/packages');
    const compiler5 = makeCompiler(dest5);
    await compiler5.place(dest5);
    expect(compiler5.stamp(dest5)).not.toBe(compiler4.stamp(dest4));

    writeFileSync(synthShared, '#define EXACT_SYNTH_SHARED 11\n');
    const dest6 = resolve(dir, 'run-6/packages');
    const compiler6 = makeCompiler(dest6);
    await compiler6.place(dest6);
    expect(compiler6.stamp(dest6)).not.toBe(compiler5.stamp(dest5));

    // A macro-computed external include outside scanned roots is never cached in packages-kept and never reuses stale dylibs.
    const macroShared = resolve(sharedDir, 'MacroOnly.inc');
    writeFileSync(macroShared, '#define EXACT_MACRO_VAL 1\n');
    const macroDir = resolve(dir, 'macro-pkg');
    mkdirSync(macroDir, { recursive: true });
    const macroHeader = resolve(macroDir, 'MacroBridge.h');
    writeFileSync(macroHeader, '#import <Foundation/Foundation.h>\n@interface ExactMacroBox : NSObject\n+ (NSInteger)val;\n@end\n');
    const macroObjc = resolve(macroDir, 'MacroBridge.m');
    writeFileSync(macroObjc, '#import "MacroBridge.h"\n#define MACRO_PATH "../Shared/MacroOnly.inc"\n#include MACRO_PATH\n@implementation ExactMacroBox\n+ (NSInteger)val { return EXACT_MACRO_VAL; }\n@end\n');
    const macroSwift = resolve(macroDir, 'Macro.swift');
    writeFileSync(macroSwift, 'import ExactPackage\ninternal import MacroObjC\npublic final class MacroModule: ExactPackage {\n  public init(context: ExactPackageContext) {}\n  public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) { reply([:]) }\n  public func call(_ name: String, _ args: [String: Any]) throws -> [String: Any] { ["v": ExactMacroBox.val()] }\n}\n');
    const macroPkg = { extension: 'macro', module: 'ExactPkg_macro', className: 'MacroModule', sources: [macroSwift], objc: [macroObjc], objcModule: { name: 'MacroObjC', header: macroHeader } };
    const macroDest1 = resolve(dir, 'run-macro-1/packages'), macroDest2 = resolve(dir, 'run-macro-2/packages');
    const makeMacro = d => packageCompiler({
      packages: [macroPkg], packageDir: d, packageTargets: new Map([[d, ['macosx', []]]]),
      packageCopies: [macroPkg.sources], seam: resolve(root, 'host/apple/modules/ExactPackage.swift'),
      packagesRoot: cacheRoot, swiftc: 'fixture', production: false, distribution: false, ios: false, tv: false,
    });
    const m1 = makeMacro(macroDest1);
    await m1.place(macroDest1);
    const m1Dylib = readFileSync(resolve(macroDest1, 'ExactPkg_macro.dylib'));
    const m1Stamp = m1.stamp(macroDest1);
    writeFileSync(macroShared, '#define EXACT_MACRO_VAL 9999\n');
    const m2 = makeMacro(macroDest2);
    await m2.place(macroDest2);
    expect(m2.stamp(macroDest2)).not.toBe(m1Stamp);
    expect(readFileSync(resolve(macroDest2, 'ExactPkg_macro.dylib'))).not.toEqual(m1Dylib);
    // Two uncacheable compilers in the same process never collide on stamp(dir) even with identical outputs.
    const macroDest3 = resolve(dir, 'run-macro-3/packages');
    const m3 = makeMacro(macroDest3);
    await m3.place(macroDest3);
    expect(m3.stamp(macroDest3)).not.toBe(m2.stamp(macroDest2));

    // `__has_include` pointing to an absent external file never caches a stale warm hit when that file appears.
    const lateFile = resolve(sharedDir, 'LateFeature.inc');
    const hasIncDir = resolve(dir, 'hasinc-pkg');
    mkdirSync(hasIncDir, { recursive: true });
    const hasIncHeader = resolve(hasIncDir, 'HasIncBridge.h');
    writeFileSync(hasIncHeader, '#import <Foundation/Foundation.h>\n@interface ExactHasIncBox : NSObject\n+ (NSInteger)val;\n@end\n');
    const hasIncObjc = resolve(hasIncDir, 'HasIncBridge.m');
    writeFileSync(hasIncObjc, '#import "HasIncBridge.h"\n#if __has_include("../Shared/LateFeature.inc")\n#include "../Shared/LateFeature.inc"\n#else\n#define EXACT_LATE_VAL 1\n#endif\n@implementation ExactHasIncBox\n+ (NSInteger)val { return EXACT_LATE_VAL; }\n@end\n');
    const hasIncSwift = resolve(hasIncDir, 'HasInc.swift');
    writeFileSync(hasIncSwift, 'import ExactPackage\ninternal import HasIncObjC\npublic final class HasIncModule: ExactPackage {\n  public init(context: ExactPackageContext) {}\n  public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) { reply([:]) }\n  public func call(_ name: String, _ args: [String: Any]) throws -> [String: Any] { ["v": ExactHasIncBox.val(), "p": #filePath] }\n}\n');
    const hasIncPkg = { extension: 'hasinc', module: 'ExactPkg_hasinc', className: 'HasIncModule', sources: [hasIncSwift], objc: [hasIncObjc], objcModule: { name: 'HasIncObjC', header: hasIncHeader } };
    const makeHasInc = d => packageCompiler({
      packages: [hasIncPkg], packageDir: d, packageTargets: new Map([[d, ['macosx', []]]]),
      packageCopies: [hasIncPkg.sources], seam: resolve(root, 'host/apple/modules/ExactPackage.swift'),
      packagesRoot: cacheRoot, swiftc: 'fixture', production: false, distribution: false, ios: false, tv: false,
    });
    const hiDest1 = resolve(dir, 'run-hasinc-1/packages'), hiDest2 = resolve(dir, 'run-hasinc-2/packages');
    const hi1 = makeHasInc(hiDest1);
    await hi1.place(hiDest1);
    const hi1Dylib = readFileSync(resolve(hiDest1, 'ExactPkg_hasinc.dylib'));
    writeFileSync(lateFile, '#define EXACT_LATE_VAL 7777\n');
    const hi2 = makeHasInc(hiDest2);
    await hi2.place(hiDest2);
    expect(readFileSync(resolve(hiDest2, 'ExactPkg_hasinc.dylib'))).not.toEqual(hi1Dylib);

    // Exercise the iOS simulator target directly (the exact platform in #2090) importing ExactPkg_media_capture.
    let iosSdk = '';
    try { iosSdk = execFileSync('xcrun', ['--sdk', 'iphonesimulator', '--show-sdk-path'], { encoding: 'utf8', stdio: ['ignore', 'pipe', 'ignore'] }).trim(); } catch {}
    if (iosSdk) {
      const iosArch = process.arch === 'arm64' ? 'arm64' : 'x86_64';
      const iosTarget = ['-target', `${iosArch}-apple-ios17.0-simulator`, '-sdk', iosSdk];
      const iosDest = resolve(dir, 'run-ios/packages');
      const iosCompiler = packageCompiler({
        packages: [mediaPkg], packageDir: iosDest, packageTargets: new Map([[iosDest, ['iphonesimulator', iosTarget]]]),
        packageCopies: [mediaPkg.sources], seam: resolve(root, 'host/apple/modules/ExactPackage.swift'),
        packagesRoot: cacheRoot, swiftc: 'fixture', production: false, distribution: false, ios: true, tv: false,
      });
      await iosCompiler.place(iosDest);
      await new Promise((ok, fail) => {
        const child = appleToolchain.spawn('xcrun', [
          '--sdk', 'iphonesimulator', 'swiftc', '-parse-as-library', '-emit-object', '-swift-version', '5',
          '-module-name', 'ExactAppModules', '-I', iosDest, ...iosCompiler.imports(iosDest), ...iosTarget,
          mediaConsumer, '-o', resolve(dir, 'media-ios.o'),
        ], { stdio: ['ignore', 'pipe', 'pipe'] });
        let err = '';
        child.stderr.on('data', d => { err += d; });
        child.on('close', code => code === 0 ? ok() : fail(new Error(err || `swiftc exited ${code}`)));
      });
    }
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
}, 120_000);
