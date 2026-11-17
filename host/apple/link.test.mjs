import { capturePrivacyManifest } from './privacy.mjs';
import { test, spyOn } from 'bun:test';
import * as appleToolchain from './toolchain.mjs';
import assert from 'node:assert/strict';
import {copyFileSync, cpSync, existsSync, mkdtempSync, mkdirSync, readdirSync, readFileSync, writeFileSync, rmSync} from 'node:fs';
import {tmpdir} from 'node:os';
import {join, resolve} from 'node:path';
import { encodePng } from '../../scripts/png.mjs';
import { simulator, useXcode } from './devices.mjs';
import { spawnSync } from 'node:child_process';
import { assertActivityRefusals } from './activities.mjs';
import { resolveApp } from '../../scripts/app.mjs';
import { receiptChanges } from '../../scripts/agent-launch.mjs';
import { need, notApplicable } from '../../scripts/coverage.mjs';
import { appleArtifacts } from './build.mjs';
import { appleComposition, assertCapabilityFrameworks, assertLinkedSdk, compiledTargets, deploymentTargets, iosSimulatorTarget, loadedDylibs, recordMinimumOS } from './link.mjs';
import { composedModule, moduleSeam, moduleLibraryLinkArgs, systemFrameworks, packageCompiler, rosterProbe, rosterProbeMain } from './compose.mjs';
import { placeMacDylibs } from './bundle.mjs';
import { keepRoster, keptRoster, probeDigest } from './arms.mjs';
import { writeDataKeys } from './data-keys.mjs';
import { appleImplementations } from '../../js/bake/src/typescript.mjs';

test.skipIf(process.platform !== 'darwin')('package images load on use, replay facts and keep credential members in their session', async () => {
  useXcode();
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/credential-members-'));
  const run = args => {
    const result = spawnSync('xcrun', ['--sdk', 'macosx', 'swiftc', '-swift-version', '5', ...args], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
  };
  const write = (name, text) => { const path = resolve(dir, name); writeFileSync(path, text); return path; };
  try {
    const packages = appleImplementations(resolve(root, 'apps/auth-fixture'), 'macos', { passkeys: '@exact/passkeys', 'sign-in': '@exact/sign-in' });
    assert.deepEqual(packages.find(p => p.extension === 'sign-in').credentialMembers, ['passkeys#get.identity']);
    const owner = write('Owner.swift', `import Foundation
import ExactPackage
public final class Owner: ExactPackage {
    let context: ExactPackageContext
    let initialForeground: Bool
    var values: [String: Any] = [:], states: [String: String] = [:]
    var angle = 0.0
    public init(context: ExactPackageContext) { self.context = context; initialForeground = context.foreground }
    public func preferred(_ values: [String: Any]) { self.values = values }
    public func permissions(_ states: [String: String]) { self.states = states }
    public func orientation(_ angle: Double) { self.angle = angle }
    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) {
        do { reply(try context.credentials.request("passkeys#get.identity", args: args).answer(NSObject())!) }
        catch { reply(["error": "missing-member"]) }
    }
    public func call(_ name: String, _ args: [String: Any]) throws -> [String: Any] { ["preferred": values, "permissions": states, "foreground": context.foreground, "initialForeground": initialForeground, "angle": angle] }
}`);
    const member = write('Member.swift', `import Foundation
import ExactPackage
public final class Member: ExactPackage, ExactCredentialMember {
    let context: ExactPackageContext
    public init(context: ExactPackageContext) {
        self.context = context
        context.credentials.register("passkeys#get.identity", member: self)
    }
    public func credentialRequest(_ args: [String: Any]) throws -> ExactCredentialRequest {
        ExactCredentialRequest(requests: [NSObject()]) { [context] _ in ["session": context.session, "identity": args["identity"]!] }
    }
    public func later(_ name: String, _ args: [String: Any], preferred: Any?, reply: @escaping ([String: Any]) -> Void) { reply([:]) }
    public func call(_ name: String, _ args: [String: Any]) throws -> [String: Any] { [:] }
}`);
    const implemented = [
      { extension: 'passkeys', module: 'Owner', className: 'Owner', sources: [owner] },
      { extension: 'sign-in', module: 'Member', className: 'Member', sources: [member], credentialMembers: packages.find(p => p.extension === 'sign-in').credentialMembers },
      { extension: 'starting', module: 'Starting', className: 'Owner', sources: [owner], start: true },
    ];
    const graph = [...implemented, { extension: 'missing', module: 'Missing', className: 'Missing' },
      { extension: 'broken', module: 'Broken', className: 'Broken' }];
    packageCompiler({ packages: implemented, packageDir: dir, packageTargets: new Map([[dir, ['macosx', []]]]),
      packageCopies: implemented.map(p => p.sources), seam: resolve(root, 'host/apple/modules/ExactPackage.swift'),
      packagesRoot: dir, swiftc: 'fixture', production: false, distribution: false, ios: false, tv: false }).place(dir);
    const broken = write('Broken.swift', `import Owner
@_cdecl("broken_probe") public func brokenProbe() { print(Owner.self) }
`);
    run(['-parse-as-library', '-emit-library', '-I', dir, '-L', dir, broken,
      '-Xlinker', '-rpath', '-Xlinker', '@loader_path', '-o', resolve(dir, 'libBroken.dylib')]);
    const composed = write('ExactComposed.swift', composedModule(graph, false));
    const keys = resolve(dir, 'ExactDataKeys.swift');
    writeDataKeys(resolveApp(resolve(root, 'apps/native-fixture')), keys, 'macos');
    const main = write('Fixture.swift', `import Foundation
import MachO
private var answer: [String: Any] = [:]
private var loadReports = 0
@_cdecl("fixture_run") public func fixtureRun() {
func loaded(_ name: String) -> Bool {
    (0..<_dyld_image_count()).contains { String(cString: _dyld_get_image_name($0)).hasSuffix("/lib\\(name).dylib") }
}
func session() -> ExactModule {
    ExactComposedModule(context: ExactModuleContext(json: [:], host: nil, changed: { _, _, _ in }, now: { _ in 0 }))
}
func ask(_ module: ExactModule, _ value: String) -> String {
    let reply = ExactReply(ctx: UnsafeMutableRawPointer(bitPattern: 1)) { _, status, bytes, count in
        precondition(status == 200)
        answer = try! JSONSerialization.jsonObject(with: Data(bytes: bytes!, count: count)) as! [String: Any]
    }
    precondition(ExactExtensions.later(module, ["extension": "passkeys", "send": "get", "args": ["publicKey": [:], "identity": value]], reply: reply))
    precondition(answer["identity"] as? String == value)
    return answer["session"] as! String
}
let first = session(), second = session()
ExactExtensions.created(first)
precondition(!loaded("Owner") && !loaded("Member") && !loaded("Starting"))
func tell(_ body: [String: Any]) {
    precondition(ExactExtensions.later(first, body, reply: ExactReply(ctx: nil) { _, status, _, _ in precondition(status == 200) }))
}
tell(["extension": "passkeys", "preferred": ["status": "old"], "permissions": ["access": "denied"]])
tell(["extension": "passkeys", "preferred": ["status": "latest"], "permissions": ["access": "granted"]])
tell(["extension": "passkeys", "foreground": false])
ExactExtensions.orientation(first, 90)
for key in ["release", "dispose", "cancel", "woke"] {
    tell(["extension": "passkeys", key: key == "dispose" ? [] : key == "cancel" ? 1 : "unused"])
}
precondition(!loaded("Owner") && !loaded("Member"))
typealias Call = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, Int, UnsafeMutableRawPointer?, ExactModuleReplyFn?) -> Void
let call = unsafeBitCast(exactNativeAbi().load(fromByteOffset: 96, as: UnsafeRawPointer.self), to: Call.self)
func sync() {
    let body = Array(#"{"extension":"passkeys","sync":"status"}"#.utf8)
    body.withUnsafeBufferPointer { call(Unmanaged.passUnretained(first).toOpaque(), $0.baseAddress, $0.count, nil) { _, status, bytes, count in
        let data = Data(bytes: bytes!, count: count)
        if status == 102 {
            precondition(Double(String(decoding: data, as: UTF8.self))! > 0)
            loadReports += 1
        } else {
            precondition(status == 200)
            answer = try! JSONSerialization.jsonObject(with: data) as! [String: Any]
        }
    } }
}
sync()
let status = answer
precondition(loadReports == 1)
sync()
precondition(loadReports == 1)
precondition(loaded("Owner") && !loaded("Member"))
precondition((status["preferred"] as? [String: String])?["status"] == "latest")
precondition((status["permissions"] as? [String: String])?["access"] == "granted")
precondition(status["foreground"] as? Bool == false && status["initialForeground"] as? Bool == false)
precondition(status["angle"] as? Double == 90)
let failed = try! ExactExtensions.call(first, ["extension": "missing", "sync": "status"])!
precondition(failed["error"] as? String == "NotSupportedError")
precondition((failed["message"] as? String)?.contains("libMissing.dylib") == true)
precondition(!loaded("Missing"))
let a = ask(first, "first"), b = ask(second, "second")
precondition(loaded("Owner") && loaded("Member"))
precondition(a != b)
precondition(ask(first, "again") == a)
var queued = false
DispatchQueue.main.async { queued = true }
let deadline = Date().addingTimeInterval(10)
while !queued && Date() < deadline { _ = RunLoop.main.run(mode: .default, before: deadline) }
precondition(queued && loaded("Starting"))
ExactExtensions.destroy(first)
precondition(ask(second, "survives") == b)
let broken = try! ExactExtensions.call(second, ["extension": "broken", "sync": "status"])!
precondition(broken["error"] as? String == "NotSupportedError")
precondition((broken["message"] as? String)?.contains("no exact_package_create entry") == true)
ExactExtensions.destroy(second)
print("lazy packages: 0 loaded after facts, 1 after sync, 2 after member use; replay, start and isolation passed")
}
`);
    const core = resolve(dir, 'libexact_modules.dylib');
    run(['-parse-as-library', '-emit-library', '-I', dir, '-L', dir, resolve(dir, 'ExactPackage.o'),
      ...moduleLibraryLinkArgs(),
      ...moduleSeam(resolve, root), keys, composed, main, '-o', core]);
    const launcher = write('main.swift', `import Foundation
guard let library = dlopen(CommandLine.arguments[1], RTLD_NOW | RTLD_LOCAL) else {
    fputs(String(cString: dlerror()), stderr)
    exit(1)
}
let entry = dlsym(library, "fixture_run")!
unsafeBitCast(entry, to: (@convention(c) () -> Void).self)()
`);
    const binary = resolve(dir, 'members');
    run([launcher, '-o', binary]);
    const result = spawnSync(binary, [core], { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    assert.match(result.stdout, /lazy packages: 0 loaded after facts, 1 after sync, 2 after member use/);
    const imported = write('Imported.swift', `import Foundation
import MachO
import Owner
final class ImportedModule: ExactModule {
    override class var views: [String: ExactNativeFactory] {
        precondition(String(describing: Owner.self) == "Owner")
        return ["exact-probe": ExactNativeFactory(firstFrame: true) { _, _ in throw ExactNativeRefusal("probe only") }]
    }
}
let exactModule: ExactModule.Type = ImportedModule.self
@_cdecl("fixture_run") public func fixtureRun() {
    let roster = String(cString: exactNativeAbi().load(fromByteOffset: 8, as: UnsafePointer<CChar>.self))
    let tags = try! JSONSerialization.jsonObject(with: Data(roster.utf8)) as! [String: Any]
    precondition((tags["exact-probe"] as? [String: Any])?["firstFrame"] as? Bool == true)
    precondition((0..<_dyld_image_count()).contains { String(cString: _dyld_get_image_name($0)).hasSuffix("/libOwner.dylib") })
}
`);
    writeFileSync(composed, composedModule(graph, true));
    const artifact = resolve(dir, 'artifact'), importedCore = resolve(artifact, 'libexact_modules.dylib');
    mkdirSync(artifact);
    run(['-parse-as-library', '-emit-library', '-I', dir, '-L', dir, resolve(dir, 'ExactPackage.o'),
      ...moduleLibraryLinkArgs(), ...moduleSeam(resolve, root), keys, composed, imported, '-o', importedCore]);
    const unstaged = spawnSync(binary, [importedCore], { encoding: 'utf8' });
    assert.equal(unstaged.status, 1);
    assert.match(unstaged.stderr, /libOwner.dylib/);
    placeMacDylibs(artifact, implemented.map(p => ({ built: resolve(dir, `${p.module}.dylib`), load: `lib${p.module}.dylib` })));
    const direct = spawnSync(binary, [importedCore], { encoding: 'utf8' });
    assert.equal(direct.status, 0, direct.stderr);
    const arms = resolve(dir, 'arms');
    mkdirSync(arms);
    const cached = resolve(arms, 'cached.dylib'), cachedLauncher = resolve(arms, 'cached-launcher');
    copyFileSync(importedCore, cached);
    const probeMain = write('ProbeMain.swift', rosterProbeMain);
    run(['-parse-as-library', probeMain, '-o', cachedLauncher]);
    let placements = 0;
    const probe = async (build) => {
      mkdirSync(build);
      const built = resolve(build, 'probe'), packagesIn = resolve(build, 'packages');
      copyFileSync(cachedLauncher, built);
      const compiler = packageCompiler({ packages: implemented, packageDir: packagesIn,
        packageTargets: new Map([[packagesIn, ['macosx', []]]]), packageCopies: implemented.map(p => p.sources),
        seam: resolve(root, 'host/apple/modules/ExactPackage.swift'), packagesRoot: dir,
        swiftc: 'fixture', production: false, distribution: false, ios: false, tv: false });
      const library = Object.assign(async () => { copyFileSync(cached, `${built}.dylib`); return true; }, { cached });
      const placed = rosterProbe({ library, launcher: async () => true, built, packagesIn, packages: implemented,
        placePackages: where => { placements++; compiler.place(where); }, frameworks: [] });
      assert.equal(await placed(), true);
      rmSync(build, { recursive: true });
      return placed;
    };
    const firstProbe = await probe(resolve(dir, 'first-probe'));
    // No roster was kept: the previous build compiled its probe but could not run it.
    assert.equal(keptRoster(firstProbe.cached, {}), undefined);
    const retry = await probe(resolve(dir, 'retry-probe'));
    assert.equal(placements, 2);
    assert.equal(firstProbe.cached, retry.cached);
    const moved = resolve(dir, 'relocated-probe');
    cpSync(retry.cached, moved, { recursive: true });
    assert.equal(probeDigest(moved), probeDigest(retry.cached));
    rmSync(artifact, { recursive: true });
    for (const p of implemented) rmSync(resolve(dir, `lib${p.module}.dylib`));
    const resultProbe = spawnSync(resolve(moved, 'ExactRosterProbe'), [], { encoding: 'utf8' });
    assert.equal(resultProbe.status, 0, resultProbe.stderr);
    assert.equal(JSON.parse(resultProbe.stdout)['exact-probe'].firstFrame, true);
    keepRoster(moved, resultProbe.stdout);
    assert.equal(keptRoster(moved, {}), resultProbe.stdout);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 180_000);

test.skipIf(process.platform !== 'darwin' || !process.env.EXACT_LANE_SIM)('the browser framework loads on its first simulator call', async () => {
  useXcode();
  const root = resolve(import.meta.dir, '../..'), dir = mkdtempSync(resolve(root, 'target/browser-load-'));
  const sim = process.env.EXACT_LANE_SIM;
  const run = (cmd, args) => {
    const result = spawnSync(cmd, args, { encoding: 'utf8' });
    assert.equal(result.status, 0, `${result.stdout}\n${result.stderr}`);
    return result.stdout;
  };
  const write = (name, text) => { const path = resolve(dir, name); writeFileSync(path, text); return path; };
  try {
    const sdk = run('xcrun', ['--sdk', 'iphonesimulator', '--show-sdk-path']).trim();
    const target = ['-target', `${process.arch === 'arm64' ? 'arm64' : 'x86_64'}-apple-ios17.0-simulator`, '-sdk', sdk];
    const p = { extension: 'in-app-browser', module: 'ExactPkg_in_app_browser', className: 'InAppBrowserModule',
      sources: [resolve(root, 'packages/in-app-browser/apple/InAppBrowser.swift')] };
    packageCompiler({ packages: [p], packageDir: dir, packageTargets: new Map([[dir, ['iphonesimulator', target]]]),
      packageCopies: [p.sources], seam: resolve(root, 'host/apple/modules/ExactPackage.swift'), packagesRoot: dir,
      swiftc: 'fixture', production: true, distribution: false, ios: true, tv: false }).place(dir);
    const composed = write('ExactComposed.swift', composedModule([p], false)), keys = resolve(dir, 'ExactDataKeys.swift');
    writeDataKeys(resolveApp(resolve(root, 'apps/native-fixture')), keys, 'ios');
    const fixture = write('Fixture.swift', `import Foundation
import MachO
private var result: [String: Any] = [:]
private var loading = 0.0
@_cdecl("fixture_run") public func fixtureRun() {
    func loaded() -> Bool {
        (0..<_dyld_image_count()).contains { String(cString: _dyld_get_image_name($0)).hasSuffix("/ExactPkg_in_app_browser.framework/ExactPkg_in_app_browser") }
    }
    let module = ExactComposedModule(context: ExactModuleContext(json: ["agent": true], host: nil, changed: { _, _, _ in }, now: { _ in 0 }))
    ExactExtensions.created(module)
    precondition(!loaded())
    _ = ExactExtensions.later(module, ["extension": "in-app-browser", "preferred": [:], "permissions": [:]], reply: ExactReply(ctx: nil) { _, _, _, _ in })
    precondition(!loaded())
    typealias Call = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, Int, UnsafeMutableRawPointer?, ExactModuleReplyFn?) -> Void
    let call = unsafeBitCast(exactNativeAbi().load(fromByteOffset: 96, as: UnsafeRawPointer.self), to: Call.self)
    let body = Array(#"{"extension":"in-app-browser","sync":"inAppBrowserStatus"}"#.utf8)
    body.withUnsafeBufferPointer { call(Unmanaged.passUnretained(module).toOpaque(), $0.baseAddress, $0.count, nil) { _, status, bytes, count in
        let data = Data(bytes: bytes!, count: count)
        if status == 102 { loading += Double(String(decoding: data, as: UTF8.self))! }
        else {
            precondition(status == 200)
            result = try! JSONSerialization.jsonObject(with: data) as! [String: Any]
        }
    } }
    precondition(loaded() && loading > 0)
    precondition(result["available"] as? Bool == true && result["overApp"] as? Bool == true)
    ExactExtensions.destroy(module)
    print("browser images: 0 before first use, 1 after; loader seconds: \\(loading)")
}
`);
    const frameworks = resolve(dir, 'Frameworks'), core = resolve(frameworks, 'ExactModules.framework/ExactModules');
    const browser = resolve(frameworks, `${p.module}.framework/${p.module}`);
    mkdirSync(resolve(frameworks, 'ExactModules.framework'), { recursive: true });
    mkdirSync(resolve(frameworks, `${p.module}.framework`));
    copyFileSync(resolve(dir, `${p.module}.dylib`), browser);
    run('xcrun', ['--sdk', 'iphonesimulator', 'swiftc', ...target, '-parse-as-library', '-emit-library', '-swift-version', '5',
      '-I', dir, '-L', dir, resolve(dir, 'ExactPackage.o'), ...moduleLibraryLinkArgs({ wrapped: true }),
      ...moduleSeam(resolve, root), keys, composed, fixture, '-o', core]);
    const main = write('main.swift', `import Foundation
let library = dlopen(CommandLine.arguments[1], RTLD_NOW | RTLD_LOCAL)!
unsafeBitCast(dlsym(library, "fixture_run")!, to: (@convention(c) () -> Void).self)()
`), binary = resolve(dir, 'browser');
    run('xcrun', ['--sdk', 'iphonesimulator', 'swiftc', ...target, main, '-o', binary]);
    for (const file of [core, browser, binary]) run('codesign', ['--force', '--sign', '-', '--timestamp=none', file]);
    const output = run('xcrun', ['simctl', 'spawn', simulator(sim).udid, binary, core]);
    assert.match(output, /browser images: 0 before first use, 1 after/);
    console.log(output.trim());
    const imported = write('Imported.swift', `import Foundation
import MachO
import ExactPkg_in_app_browser
@_cdecl("fixture_run") public func fixtureRun() {
    precondition(String(describing: InAppBrowserModule.self) == "InAppBrowserModule")
    let count = (0..<_dyld_image_count()).filter { String(cString: _dyld_get_image_name($0)).hasSuffix("/ExactPkg_in_app_browser.framework/ExactPkg_in_app_browser") }.count
    precondition(count == 1)
    print("direct import: 1 browser image loaded with the module")
}
`);
    run('xcrun', ['--sdk', 'iphonesimulator', 'swiftc', ...target, '-parse-as-library', '-emit-library', '-swift-version', '5',
      '-I', dir, '-L', dir, resolve(dir, 'ExactPackage.o'), ...moduleLibraryLinkArgs({ wrapped: true }),
      ...moduleSeam(resolve, root), keys, composed, imported, '-o', core]);
    run('codesign', ['--force', '--sign', '-', '--timestamp=none', core]);
    assert.match(run('xcrun', ['simctl', 'spawn', simulator(sim).udid, binary, core]), /direct import: 1 browser image loaded with the module/);

    const packagesIn = resolve(dir, 'probe-packages'), built = resolve(dir, 'probe-roster-simulator');
    const compiler = packageCompiler({ packages: [p], packageDir: dir,
      packageTargets: new Map([[packagesIn, ['iphonesimulator', target]]]), packageCopies: [p.sources],
      seam: resolve(root, 'host/apple/modules/ExactPackage.swift'), packagesRoot: dir,
      swiftc: 'fixture', production: true, distribution: false, ios: true, tv: false });
    compiler.place(packagesIn);
    const framework = resolve(dir, 'RosterDependency.framework');
    mkdirSync(framework);
    const dependency = write('Dependency.swift', '@_cdecl("roster_dependency") public func dependency() -> Int32 { 42 }');
    run('xcrun', ['--sdk', 'iphonesimulator', 'swiftc', ...target, '-emit-library', dependency,
      '-Xlinker', '-install_name', '-Xlinker', '@rpath/RosterDependency.framework/RosterDependency', '-o', resolve(framework, 'RosterDependency')]);
    const module = write('ProbeModule.swift', `import ExactPkg_in_app_browser
@_silgen_name("roster_dependency") private func dependency() -> Int32
final class ProbeModule: ExactModule {
    override class var views: [String: ExactNativeFactory] {
        precondition(dependency() == 42 && String(describing: InAppBrowserModule.self) == "InAppBrowserModule")
        return ["exact-probe": ExactNativeFactory(firstFrame: true) { _, _ in throw ExactNativeRefusal("probe only") }]
    }
}
let exactModule: ExactModule.Type = ProbeModule.self
`);
    writeFileSync(composed, composedModule([p], true));
    run('xcrun', ['--sdk', 'iphonesimulator', 'swiftc', ...target, '-parse-as-library', '-emit-library', '-swift-version', '5',
      '-I', packagesIn, '-L', packagesIn, resolve(packagesIn, 'ExactPackage.o'), ...moduleLibraryLinkArgs(),
      '-Xlinker', '-rpath', '-Xlinker', '@loader_path/Frameworks', resolve(framework, 'RosterDependency'),
      ...moduleSeam(resolve, root), keys, composed, module, '-o', `${built}.dylib`]);
    const probeMain = write('ProbeMain.swift', rosterProbeMain);
    run('xcrun', ['--sdk', 'iphonesimulator', 'swiftc', ...target, '-parse-as-library', probeMain, '-o', built]);
    const cached = resolve(dir, 'arms/cached.dylib');
    mkdirSync(resolve(dir, 'arms'));
    copyFileSync(`${built}.dylib`, cached);
    const placed = rosterProbe({ library: Object.assign(async () => true, { cached }), launcher: async () => true,
      built, packagesIn, packages: [p], placePackages: compiler.place, frameworks: [{ placed: framework }] });
    assert.equal(await placed(), true);
    const moved = resolve(dir, 'relocated-probe');
    cpSync(placed.cached, moved, { recursive: true });
    for (const path of [built, `${built}.dylib`, packagesIn, framework, frameworks, placed.cached]) rmSync(path, { recursive: true });
    const roster = run('xcrun', ['simctl', 'spawn', simulator(sim).udid, resolve(moved, 'ExactRosterProbe')]);
    assert.equal(JSON.parse(roster)['exact-probe'].firstFrame, true);
    console.log('relocated simulator probe: launcher, module, package and framework loaded; first-frame tag preserved');
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 120_000);

for (const [sdk, ios, tv] of [['appletvsimulator', true, true], ['iphonesimulator', true, false], ['macosx', false, false]]) {
  test.skipIf(process.platform !== 'darwin')(`native-fixture's Apple module typechecks for ${sdk}`, async () => {
    const { moduleTripleFor } = await import('./build.mjs');
    const { useXcode } = await import('./devices.mjs');
    useXcode();
    const root = join(import.meta.dir, '../..'), app = resolveApp(join(root, 'apps/native-fixture'));
    mkdirSync(join(root, 'target'), { recursive: true });
    const dir = mkdtempSync(join(root, 'target/native-fixture-module-'));
    try {
      const keys = join(dir, 'ExactDataKeys.swift'), composed = join(dir, 'ExactComposed.swift');
      writeDataKeys(app, keys, ios ? 'ios' : 'macos');
      writeFileSync(composed, composedModule([], true));
      const built = spawnSync('xcrun', ['--sdk', sdk, 'swiftc', '-typecheck', '-parse-as-library', '-swift-version', '5',
        '-target', moduleTripleFor(app, ios, false, tv), ...moduleSeam(join, root), keys, composed, ...app.modules.apple],
      { encoding: 'utf8', maxBuffer: 8 << 20 });
      assert.equal(built.status, 0, built.stderr);
    } finally { rmSync(dir, { recursive: true, force: true }); }
  }, 60_000);
}

test('a live watch entry in an app with TypeScript reaches the bake', async () => {
  const { buildWatch } = await import('./watch.mjs');
  const dir = mkdtempSync(join(tmpdir(), 'exact-watch-live-')), events = [];
  try {
    writeFileSync(join(dir, 'app.ts'), '');
    const app = { dir, id: 'test.watch', displayName: 'Watch', manifest: { watch: { source: 'watch.contract', component: 'Watch' } } };
    await assert.rejects(buildWatch(app, ['--watchos', '--no-install'], {
      appleBuildLock: () => { events.push('lock'); return () => events.push('release'); },
      buildParent: async () => events.push('parent'),
      appleArtifacts: () => ({ bundle: join(dir, 'Parent.app'), namespace: dir }),
      runApple: () => { throw new Error('watch bake reached'); },
    }), /watch bake reached/);
    assert.deepEqual(events, ['lock', 'parent', 'release']);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test.skipIf(process.platform !== 'darwin')('production share extensions keep source dSYMs outside their signed bundles', async () => {
  const { packageShareExtension } = await import('./extensions.mjs');
  const { plistFile } = await import('./assets.mjs');
  const { useXcode } = await import('./devices.mjs');
  useXcode();
  const dir = mkdtempSync(join(tmpdir(), 'exact-share-symbols-'));
  const run = (command, args) => {
    const r = spawnSync(command, args, { encoding: 'utf8', maxBuffer: 32 << 20 });
    assert.equal(r.status, 0, `${command}: ${r.stderr}`);
    return r;
  };
  const uuid = path => /UUID: ([A-F0-9-]+)/.exec(run('dwarfdump', ['--uuid', path]).stdout)?.[1];
  try {
    for (const platform of ['ios', 'macos']) for (const production of [false, true]) {
      const directory = join(dir, `${platform}-${production}`), bundle = join(directory, 'Fixture.app');
      const contents = platform === 'macos' ? join(bundle, 'Contents') : bundle;
      mkdirSync(contents, { recursive: true });
      writeFileSync(join(contents, 'Info.plist'), plistFile({ CFBundleVersion: '1', CFBundleShortVersionString: '1.0',
        MinimumOSVersion: '17.0', LSMinimumSystemVersion: '14.0', UIDeviceFamily: [1, 2], CFBundleSupportedPlatforms: ['iPhoneSimulator'] }));
      const sdk = run('xcrun', ['--sdk', platform === 'macos' ? 'macosx' : 'iphonesimulator', '--show-sdk-path']).stdout.trim();
      const arch = process.arch === 'x64' ? 'x86_64' : process.arch;
      const triple = platform === 'macos' ? `${arch}-apple-macosx14.0` : 'arm64-apple-ios17.0-simulator';
      const symbols = join(directory, 'Fixture.dSYMs');
      const app = { id: 'test.symbols', displayName: 'Fixture', manifest: { share_target: { action: '/share', params: { text: 'text' } } } };
      const extension = packageShareExtension({ app,
        bundle, directory, sdk, triple, platform, production, symbols, group: 'group.test.symbols', entitlements: plistFile({}), runApple: run,
        sign(path) { run('codesign', ['--force', '--sign', '-', '--timestamp=none', path]); } });
      const binary = join(extension, platform === 'macos' ? 'Contents/MacOS/ExactShare' : 'ExactShare');
      const names = run('nm', ['-a', binary]).stdout;
      assert.ok(!existsSync(`${binary}.dSYM`));
      assert.match(names, /U _NSExtensionMain$/m);
      assert.match(names, /_OBJC_CLASS_\$_ExactShareViewController$/m);
      if (production) {
        assert.doesNotMatch(names, /^[0-9a-f]+ [a-z] /m, 'local symbols are stripped');
        const dsym = join(symbols, 'ExactShare.appex.dSYM');
        assert.ok(uuid(binary));
        assert.equal(uuid(dsym), uuid(binary));
        assert.match(run('dwarfdump', ['--debug-info', dsym]).stdout, /ShareInbox.swift/);
      } else {
        assert.match(names, /^[0-9a-f]+ [a-z] /m);
        assert.ok(!existsSync(symbols));
      }
      run('plutil', ['-lint', join(extension, platform === 'macos' ? 'Contents/Resources/PrivacyInfo.xcprivacy' : 'PrivacyInfo.xcprivacy')]);
      run('codesign', ['--verify', '--strict', extension]);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60_000);

test.skipIf(process.platform !== 'darwin')('widget and watch packaging strips production copies before signing and keeps matching dSYMs', async () => {
  const { packageWidgets, packageAlarmExtension } = await import('./widgets.mjs');
  const { packageWatch } = await import('./watch.mjs');
  const { plistFile, wrapFramework } = await import('./assets.mjs');
  const { useXcode } = await import('./devices.mjs');
  useXcode();
  const dir = mkdtempSync(join(tmpdir(), 'exact-widget-symbols-'));
  const run = (command, args) => {
    const r = spawnSync(command, args, { encoding: 'utf8' });
    assert.equal(r.status, 0, `${command}: ${r.stderr}`);
    return r;
  };
  const uuid = path => /UUID: ([A-F0-9-]+)/.exec(run('dwarfdump', ['--uuid', path]).stdout)?.[1];
  try {
    const source = join(dir, 'fixture.c');
    writeFileSync(source, 'static int local_value = 42; int answer(void) { return local_value; } int main(void) { return answer() != 42; }');
    for (const [platform, sdkName, triple] of [['ios', 'iphonesimulator', 'arm64-apple-ios17.0-simulator'],
      ['watchos', 'watchsimulator', 'arm64-apple-watchos10.0-simulator'], ['macos', 'macosx', 'arm64-apple-macosx14.0']]) {
      const sdk = run('xcrun', ['--sdk', sdkName, '--show-sdk-path']).stdout.trim();
      for (const production of [false, true]) {
        const diskSpace = platform === 'watchos' && production;
        writeFileSync(source, 'static int local_value = 42; int answer(void) { return local_value; } int main(void) { return answer() != 42; }');
        const directory = join(dir, `${platform}-${production}`), bundle = join(directory, 'Fixture.app');
        const contents = platform === 'macos' ? join(bundle, 'Contents') : bundle;
        const resources = platform === 'macos' ? join(contents, 'Resources') : contents;
        const frameworks = join(contents, 'Frameworks'), symbols = join(directory, 'Fixture.dSYMs');
        mkdirSync(join(resources, 'plans'), { recursive: true });
        mkdirSync(frameworks, { recursive: true });
        const app = { dir, id: 'test.symbols', displayName: 'Fixture', manifest: {} };
        const info = plistFile({ CFBundleIdentifier: app.id, CFBundleExecutable: 'ExactWatch', CFBundleVersion: '1', CFBundleShortVersionString: '1.0',
          MinimumOSVersion: '10.0', LSMinimumSystemVersion: '14.0', UIDeviceFamily: [1, 2], CFBundleSupportedPlatforms: ['iPhoneSimulator'], ExactWidgetGeneration: 'fixture' });
        writeFileSync(join(contents, 'Info.plist'), info);
        const object = join(directory, 'fixture.o'), original = join(directory, 'Original');
        run('xcrun', ['--sdk', sdkName, 'clang', '-g', '-target', triple, '-c', source, '-o', object]);
        run('xcrun', ['--sdk', sdkName, 'clang', '-target', triple, object, '-o', original]);
        run('xcrun', ['--sdk', sdkName, 'clang', '-target', triple, '-dynamiclib', object, '-o', join(frameworks, 'libcore.dylib')]);
        wrapFramework(frameworks, 'libcore.dylib', 'ExactCore', app, platform === 'macos' ? 'MacOSX' : 'WatchSimulator', { apiManifests: [capturePrivacyManifest(join(dir, 'PrivacyInfo.xcprivacy'))] });
        const core = join(frameworks, 'ExactCore.framework'), entries = [{ kind: 'fixture', families: ['accessoryInline'] }];
        const runApple = (command, args) => {
          if (!args.includes('swiftc')) return run(command, args);
          // The fixture replaces the runtime link; the packager still owns symbols and signing.
          const output = args[args.indexOf('-o') + 1];
          if (args.includes('-emit-library')) copyFileSync(join(core, 'ExactCore'), output);
          else {
            assert.equal(args.includes('-g'), production);
            assert.ok(args.includes('_main'), 'WidgetKit keeps its @main entry');
            copyFileSync(original, output);
            if (production) run('dsymutil', [output, '-o', `${output}.dSYM`]);
          }
          return { stdout: '' };
        };
        const sign = path => run('codesign', ['--force', '--sign', '-', '--timestamp=none', path]);
        const widgets = { app, bundle, shared: { framework: core }, directory, sdk, triple, device: false,
          entitlements: plistFile({}), sign, runApple, production, symbols, platform, entries };
        packageWidgets(widgets);
        const extension = join(contents, 'PlugIns/ExactWidgets.appex');
        const binary = join(extension, platform === 'macos' ? 'Contents/MacOS/ExactWidgets' : 'ExactWidgets');
        const check = (binary, dsym, required) => {
          const names = run('nm', ['-a', binary]).stdout;
          assert.match(names, new RegExp(` T _${required}$`, 'm'));
          assert.equal(/ _local_value$/m.test(names), !production);
          assert.equal(existsSync(dsym), production);
          if (production) { assert.ok(uuid(binary)); assert.equal(uuid(dsym), uuid(binary)); }
          assert.ok(!existsSync(`${binary}.dSYM`));
        };
        check(binary, join(symbols, 'ExactWidgets.appex.dSYM'), 'main');
        run('plutil', ['-lint', join(extension, platform === 'macos' ? 'Contents/Resources/PrivacyInfo.xcprivacy' : 'PrivacyInfo.xcprivacy')]);
        run('codesign', ['--verify', '--strict', extension]);
        if (platform === 'ios') {
          packageAlarmExtension({ ...widgets, alarm: 'FixtureAlarm' });
          check(binary, join(symbols, 'ExactWidgets.appex.dSYM'), 'main');
          run('plutil', ['-lint', join(extension, 'PrivacyInfo.xcprivacy')]);
          run('codesign', ['--verify', '--strict', extension]);
        }
        if (platform === 'watchos') {
          writeFileSync(join(dir, 'PrivacyInfo.xcprivacy'), plistFile({ NSPrivacyAccessedAPITypes: [{ NSPrivacyAccessedAPIType: 'NSPrivacyAccessedAPICategoryDiskSpace', NSPrivacyAccessedAPITypeReasons: ['E174.1'] }],
            NSPrivacyTracking: true, NSPrivacyTrackingDomains: ['ads.example.com'], NSPrivacyCollectedDataTypes: [{ NSPrivacyCollectedDataType: 'NSPrivacyCollectedDataTypeEmailAddress',
              NSPrivacyCollectedDataTypeLinked: true, NSPrivacyCollectedDataTypeTracking: false, NSPrivacyCollectedDataTypePurposes: ['NSPrivacyCollectedDataTypePurposeAppFunctionality'] }] }));
          if (diskSpace) {
            writeFileSync(source, '#include <sys/mount.h>\nstatic int local_value = 42; int answer(void) { struct statfs s; statfs(".", &s); return local_value; } int main(void) { return answer() != 42; }');
            run('xcrun', ['--sdk', sdkName, 'clang', '-g', '-target', triple, '-c', source, '-o', object]);
            run('xcrun', ['--sdk', sdkName, 'clang', '-target', triple, object, '-o', original]);
            run('xcrun', ['--sdk', sdkName, 'clang', '-target', triple, '-dynamiclib', object, '-o', join(core, 'ExactCore')]);
          }
          copyFileSync(original, join(directory, 'ExactWatch'));
          packageWatch({ ...widgets, privacyManifests: [capturePrivacyManifest(join(dir, 'PrivacyInfo.xcprivacy'))], bin: directory, parent: directory, entries: [], sign: path => { if (path !== directory) sign(path); } });
          run('plutil', ['-lint', join(bundle, 'PrivacyInfo.xcprivacy')]);
          run('plutil', ['-lint', join(core, 'PrivacyInfo.xcprivacy')]);
          for (const owner of [bundle, core]) assert.deepEqual(JSON.parse(run('plutil', ['-convert', 'json', '-o', '-', join(owner, 'PrivacyInfo.xcprivacy')]).stdout), {
            NSPrivacyAccessedAPITypes: diskSpace ? [{ NSPrivacyAccessedAPIType: 'NSPrivacyAccessedAPICategoryDiskSpace', NSPrivacyAccessedAPITypeReasons: ['E174.1'] }] : [],
          });
          check(join(bundle, 'ExactWatch'), join(symbols, 'ExactWatch.app.dSYM'), 'main');
          check(join(core, 'ExactCore'), join(symbols, 'ExactCore.framework.dSYM'), 'answer');
          run('codesign', ['--verify', '--deep', '--strict', bundle]);
        }
        assert.match(run('nm', ['-a', original]).stdout, / _local_value$/m, 'the cached link stays unstripped');
      }
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60_000);

test.skipIf(process.platform !== 'darwin')('production Mac bundles keep Rust images loadable and preserve their dSYMs before signing', async () => {
  const { assembleMac } = await import('./bundle.mjs');
  const { codeDigest, plistFile, wrapFramework } = await import('./assets.mjs');
  const { placeAppleArtifact } = await import('./build.mjs');
  const { useXcode } = await import('./devices.mjs');
  const { createHash } = await import('node:crypto');
  useXcode();
  const dir = mkdtempSync(join(tmpdir(), 'exact-mac-symbols-'));
  const run = (command, args) => {
    const r = spawnSync(command, args, { encoding: 'utf8' });
    assert.equal(r.status, 0, `${command}: ${r.stderr}`);
    return r;
  };
  const uuid = path => /UUID: ([A-F0-9-]+)/.exec(run('dwarfdump', ['--uuid', path]).stdout)?.[1];
  try {
    const source = join(dir, 'fixture.c'), object = join(dir, 'fixture.o'), dylib = join(dir, 'fixture.dylib');
    writeFileSync(source, 'static int local_value = 42; int answer(void) { return local_value; } int main(void) { return answer() != 42; }');
    run('clang', ['-g', '-c', source, '-o', object]);
    run('clang', ['-dynamiclib', object, '-o', dylib]);
    const rustSource = join(dir, 'fixture.rs'), rustDylib = join(dir, 'rust.dylib');
    writeFileSync(rustSource, '#[no_mangle] pub extern "C" fn answer() -> i32 { 42 }');
    run('env', ['MACOSX_DEPLOYMENT_TARGET=14.0', 'rustc', '--crate-type', 'cdylib', '-g', '-C', 'strip=none', rustSource, '-o', rustDylib]);
    const loader = join(dir, 'loader.c');
    writeFileSync(loader, `#include <dlfcn.h>
#include <stdio.h>
extern int answer(void);
int main(int argc, char **argv) {
  if (answer() != 42) return 1;
  for (int i = 1; i < argc; i++) {
    void *image = dlopen(argv[i], RTLD_NOW);
    if (!image) { fprintf(stderr, "%s\\n", dlerror()); return 2; }
    int (*value)(void) = dlsym(image, "answer");
    if (!value || value() != 42) return 3;
    dlclose(image);
  }
  return 0;
}`);
    const app = { dir, id: 'test.symbols', displayName: 'Fixture', manifest: {} };
    const libraries = join(dir, 'libraries'); mkdirSync(libraries);
    copyFileSync(rustDylib, join(libraries, 'libfixture.dylib'));
    wrapFramework(libraries, 'libfixture.dylib', 'FixtureLibrary', app, 'MacOSX');
    const library = { placed: join(libraries, 'FixtureLibrary.framework'), link: 'FixtureLibrary' };
    const libraryBytes = readFileSync(join(library.placed, library.link)), rustBytes = readFileSync(rustDylib);
    const plan = Buffer.from('fixture'), compat = { embedded: { plan: { sha256: createHash('sha256').update(plan).digest('hex'), bytes: plan.length }, assets: [] } };
    for (const production of [false, true]) {
      const binDir = join(dir, String(production)), bundle = join(binDir, 'Fixture.app');
      mkdirSync(binDir);
      const bin = join(binDir, 'Fixture');
      run('clang', [loader, '-F', libraries, '-framework', library.link, '-Wl,-rpath,@executable_path/../Frameworks', '-o', bin]);
      const modules = 'libmodules.dylib', web = 'libweb.dylib', service = 'libservice.dylib';
      const rustModules = ['libsvg.dylib', 'libcanvas.dylib', 'libexact_listen.dylib'];
      copyFileSync(dylib, join(binDir, web));
      assembleMac({ app, paths: { executable: 'Fixture', capture: binDir }, binDir, bin, libDir: dir, library,
        serviceDylibs: [{ built: dylib, load: service }], hasWeb: true, webLoadName: web, args: ['--bundle'],
        loadName: 'libgpu.dylib', hasGpu: false, moduleDylibs: [], looseArms: [[true, dylib, web, 'ExactWeb'], [true, rustDylib, rustModules[2], null]],
        hasSvg: true, svgBuilt: rustDylib, svgLoadName: rustModules[0],
        modulesLoadName: modules, modulesBuilt: dylib, canvasGpuBuilt: rustDylib, canvasGpuLoadName: rustModules[1], linkRoot: binDir,
        bakedCompat: compat, bakedPlan: plan, buildReceipt: { graph: { extensions: {}, contractSources: [] }, binary: { metadata: {} } }, production, products: ['Fixture'], cleanup: [],
      }, { runApple: run, appleArtifacts: () => ({ bundle }), placeAppleArtifact,
        macInfoPlist: () => plistFile({ CFBundleExecutable: 'Fixture', CFBundleIdentifier: app.id, CFBundlePackageType: 'APPL' }),
        copyAppleStaticTrees() {}, writeUsageStrings() {}, receipt: () => '{}', assertAppleIdentity() {},
      });
      const symbols = join(binDir, 'Fixture.dSYMs');
      const rustImages = [join(bundle, 'Contents/Frameworks/FixtureLibrary.framework/FixtureLibrary'),
        ...rustModules.map(name => join(bundle, 'Contents/MacOS', name))];
      run(join(bundle, 'Contents/MacOS/Fixture'), rustImages.slice(1));
      for (const [binary, name] of [[join(bundle, 'Contents/Frameworks/FixtureLibrary.framework/FixtureLibrary'), 'FixtureLibrary.framework'],
        ...[modules, web, service, ...rustModules].map(name => [join(bundle, 'Contents/MacOS', name), name])]) {
        const names = run('nm', ['-a', binary]).stdout;
        assert.match(names, / T _answer$/m);
        if (rustImages.includes(binary)) {
          const original = name === 'FixtureLibrary.framework' ? join(library.placed, library.link) : rustDylib;
          assert.equal(codeDigest(readFileSync(binary)), codeDigest(readFileSync(original)), 'Rust images keep their linker output');
        } else assert.equal(/ _local_value$/m.test(names), !production);
        const dsym = join(symbols, `${name}.dSYM`);
        assert.equal(existsSync(dsym), production);
        if (production) { assert.ok(uuid(binary)); assert.equal(uuid(dsym), uuid(binary)); }
      }
      run('codesign', ['--verify', '--deep', '--strict', bundle]);
    }
    assert.deepEqual(readFileSync(join(library.placed, library.link)), libraryBytes, 'the cached framework stays intact');
    assert.deepEqual(readFileSync(rustDylib), rustBytes, 'the cached Rust module stays intact');
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60_000);

test('an x64 Linux host can select the matching ARM simulator Rust and Swift targets', () => {
  assert.deepEqual(iosSimulatorTarget('arm64'), { target: 'aarch64-apple-ios-sim', triple: 'arm64-apple-ios17.0-simulator' });
  for (const arch of ['x64', 'x86_64']) assert.deepEqual(iosSimulatorTarget(arch), { target: 'x86_64-apple-ios', triple: 'x86_64-apple-ios17.0-simulator' });
  for (const arch of ['ia32', '__proto__', 'toString']) assert.throws(() => iosSimulatorTarget(arch), /unsupported iOS simulator architecture/);
  const environment = { ...process.env }; delete environment.EXACT_IOS_SIM_ARCH;
  const source = `Object.defineProperty(process,'platform',{value:'linux'}); const {iosSimulatorTarget}=await import('./host/apple/link.mjs'); console.log(JSON.stringify(iosSimulatorTarget()));`;
  const result = spawnSync(process.execPath, ['-e', source], { cwd: join(import.meta.dir, '../..'), encoding: 'utf8', env: environment });
  assert.equal(result.status, 0, result.stderr);
  assert.equal(JSON.parse(result.stdout).target, 'aarch64-apple-ios-sim');
});

test('the complete builder uses the selected simulator CPU with an app minimum OS, including native modules', () => {
  for (const [arch, target, cpu] of [['arm64', 'aarch64-apple-ios-sim', 'arm64'], ['x86_64', 'x86_64-apple-ios', 'x86_64']]) {
    const script = `import {iosTarget,iosTripleFor,moduleTripleFor} from './host/apple/build.mjs';
      const app={manifest:{host:{ios:{minimumOS:'18.2'}}}};
      console.log(JSON.stringify([iosTarget,iosTripleFor(app,false),moduleTripleFor(app,true),iosTripleFor(app,true)]));`;
    const r = spawnSync(process.execPath, ['-e', script], { cwd: join(import.meta.dir, '../..'), encoding: 'utf8', env: { ...process.env, EXACT_IOS_SIM_ARCH: arch } });
    assert.equal(r.status, 0, r.stderr);
    assert.deepEqual(JSON.parse(r.stdout), [target, `${cpu}-apple-ios18.2-simulator`, `${cpu}-apple-ios18.2-simulator`, 'arm64-apple-ios18.2']);
  }
});

test('package frameworks retain category-only dependencies and weak-link APIs newer than the deployment target', () => {
  const packages = [
    { frameworks: { PhotosUI: { ios: '14.0' } } },
    { frameworks: { FoundationModels: { ios: '26.0', macos: '26.0' } } },
    { frameworks: { PhotosUI: { ios: '14.0' } } },
  ];
  assert.deepEqual(systemFrameworks(packages, 'ios', '17.0'), [
    '-Xlinker', '-weak_framework', '-Xlinker', 'FoundationModels', '-framework', 'PhotosUI',
  ]);
  assert.deepEqual(systemFrameworks(packages, 'ios', '26.0'), ['-framework', 'FoundationModels', '-framework', 'PhotosUI']);
  assert.deepEqual(systemFrameworks(packages, 'macos', '14.0'), ['-Xlinker', '-weak_framework', '-Xlinker', 'FoundationModels']);
});

test('production links the baked set; development links costly frameworks by use (LLP 1047.001.000 E, F)', () => {
  const graph = { uses: ['markdown', 'picker'] };
  const baked = linked => ({ inputs: { linked } });
  const auth = { inputs: { linked: ['auth_session', 'media_picker', 'numerals', 'tips'] }, reach: { auth: { sessions: true } } };
  assert.deepEqual(appleComposition(graph, true, [], baked(graph.uses)), { link: 'markdown,picker', products: ['ExactMarkdown'] });
  assert.deepEqual(appleComposition({ uses: [] }, true, [], baked([])), { link: 'none', products: [] });
  assert.deepEqual(appleComposition(graph, true, [], baked([...graph.uses, 'auth_session'])).products, ['ExactMarkdown', 'ExactAuthSession']);
  assert.deepEqual(appleComposition(graph, false).products, ['ExactGroupedLists', 'ExactMarkdown', 'ExactSurfaces', 'ExactDrag', 'ExactPresence']);
  assert.deepEqual(appleComposition(graph, false, [], auth).products,
    ['ExactGroupedLists', 'ExactMarkdown', 'ExactSurfaces', 'ExactDrag', 'ExactAuthSession', 'ExactPresence']);
  assert.deepEqual(appleComposition({ uses: ['picker', 'media_picker'] }, false, [], auth).products,
    ['ExactGroupedLists', 'ExactMarkdown', 'ExactSurfaces', 'ExactDrag', 'ExactMediaPicker', 'ExactAuthSession', 'ExactPresence']);
  assert.deepEqual(appleComposition(graph, false, ['auth_session']).products.includes('ExactAuthSession'), true);
  assert.deepEqual(appleComposition({ uses: ['numerals'] }, false).products.includes('ExactNumerals'), true);
  assert.ok(!appleComposition(graph, false).products.includes('ExactNumerals'));
  // TipKit and SwiftUI cost every launch: a tip links them in development too only by use.
  assert.deepEqual(appleComposition({ uses: ['tips'] }, false).products.includes('ExactTips'), true);
  assert.ok(!appleComposition(graph, false).products.includes('ExactTips'));
  assert.equal(appleComposition(undefined, false).link, 'all');
  assert.equal(appleComposition(graph, true).link, 'all');
});

// The production hello bake that `metrics.mjs --long` measures (`IOS_HELLO_BYTES`), when one is on disk:
// `EXACT_UPDATE_TRUST=production EXACT_UPDATE_GENESIS=1 bun host/apple/build.mjs hello-apple --ios`.
const helloBake = () => {
  const clients = resolve(import.meta.dir, '../../target/clients');
  if (!existsSync(clients)) return [];
  return readdirSync(clients).map((d) => join(clients, d, 'com.exact.hello/ios-simulator/aarch64-apple-ios-sim/embedded/production/standalone/Hello.dSYM/Contents/Resources/DWARF/Hello')).filter(existsSync);
};
test('presence links its Swift backend by grants or ahead in production and stays available in development', () => {
  const baked = linked => ({ inputs: { linked } });
  assert.deepEqual(appleComposition({ uses: ['io', 'presence'] }, true, [], baked(['io', 'presence'])),
    { link: 'io,presence', products: ['ExactPresence'] });
  assert.deepEqual(appleComposition({ uses: ['io'] }, true, [], baked(['io'])).products, []);
  assert.deepEqual(appleComposition({ uses: [] }, true, ['presence'], baked(['presence'])).products, ['ExactPresence']);
  assert.ok(appleComposition({ uses: [] }, false).products.includes('ExactPresence'));
});

test('production Swift follows the baked linked set when the grant ceiling is unknown', () => {
  const compat = { inputs: { grantCeiling: null, linked: ['io', 'presence', 'auth_session'] } };
  assert.deepEqual(appleComposition({ uses: [] }, true, [], compat),
    { link: 'io,presence,auth_session', products: ['ExactAuthSession', 'ExactPresence'] });
});

test.skipIf(process.platform !== 'darwin' || helloBake().length === 0)('hello links no presence backend without a presence grant', () => {
  for (const symbols of helloBake()) {
    const names = spawnSync('nm', [symbols], { encoding: 'utf8', maxBuffer: 1 << 28 }).stdout;
    assert.deepEqual(names.split('\n').filter(s => /PresenceKeychain|PresenceCheck|presenceKeychainHook|presence_secrets|exact_on_presence_keychain/.test(s)), []);
  }
});

const agentHelloBakes = () => {
  const clients = resolve(import.meta.dir, '../../target/clients');
  if (!existsSync(clients)) return [];
  return readdirSync(clients).flatMap((client) => {
    const owner = join(clients, client, 'com.exact.hello');
    return [
      ['iOS', 'production', join(owner, 'ios-simulator/aarch64-apple-ios-sim/embedded/production/standalone/Hello.dSYM/Contents/Resources/DWARF/Hello'), join(owner, 'ios-simulator/aarch64-apple-ios-sim/embedded/production/standalone/Hello.receipt.json')],
      ['iOS', 'development', join(owner, 'ios-simulator/aarch64-apple-ios-sim/embedded/development/standalone/Hello.app/Hello'), join(owner, 'ios-simulator/aarch64-apple-ios-sim/embedded/development/standalone/Hello.app/receipt.json')],
      ['macOS', 'production', join(owner, 'macos/aarch64-apple-darwin/embedded/production/standalone/Hello.dSYM/Contents/Resources/DWARF/Hello'), join(owner, 'macos/aarch64-apple-darwin/embedded/production/standalone/receipt.json')],
      ['macOS', 'development', join(owner, 'macos/aarch64-apple-darwin/embedded/development/standalone/Hello'), join(owner, 'macos/aarch64-apple-darwin/embedded/development/standalone/receipt.json')],
    ].map(([platform, trust, symbols, receipt]) => ({ platform, trust, symbols, receipt }))
      .filter(({ symbols, receipt }) => existsSync(symbols) || existsSync(receipt));
  });
};
const expectedAgentHelloBakes = () => {
  const app = resolveApp('hello');
  const iosProduction = appleArtifacts(app, { destination: 'ios-simulator', trust: 'production' });
  const iosDevelopment = appleArtifacts(app, { destination: 'ios-simulator', trust: 'development' });
  const macProduction = appleArtifacts(app, { trust: 'production' });
  const macDevelopment = appleArtifacts(app, { trust: 'development' });
  return [
    { platform: 'iOS', trust: 'production', symbols: join(`${iosProduction.binary}.dSYM`, 'Contents/Resources/DWARF', iosProduction.executable), receipt: join(iosProduction.products, `${iosProduction.executable}.receipt.json`) },
    { platform: 'iOS', trust: 'development', symbols: join(iosDevelopment.bundle, iosDevelopment.executable), receipt: join(iosDevelopment.bundle, 'receipt.json') },
    { platform: 'macOS', trust: 'production', symbols: join(`${macProduction.binary}.dSYM`, 'Contents/Resources/DWARF', macProduction.executable), receipt: join(macProduction.products, 'receipt.json') },
    { platform: 'macOS', trust: 'development', symbols: macDevelopment.binary, receipt: join(macDevelopment.products, 'receipt.json') },
  ];
};
const requiredAgentHelloBakes = () => {
  const bySymbols = new Map([...agentHelloBakes(), ...expectedAgentHelloBakes()].map(bake => [bake.symbols, bake]));
  return [...bySymbols.values()];
};
const missingAgentHelloBakes = () => {
  const app = resolveApp('hello'), missing = [];
  for (const bake of requiredAgentHelloBakes()) {
    const label = `${bake.platform} ${bake.trust}`;
    if (!existsSync(bake.symbols)) missing.push(`${label} symbols at ${bake.symbols}`);
    if (!existsSync(bake.receipt)) missing.push(`${label} receipt at ${bake.receipt}`);
    else {
      const changed = receiptChanges(bake.receipt, app);
      if (changed.length) missing.push(`${label} is stale (${changed.slice(0, 3).join(', ')}${changed.length > 3 ? ` and ${changed.length - 3} more` : ''})`);
    }
  }
  return missing.length ? missing.join('; ') : null;
};
// The executor's cache, which parses URLs and grants, and the process's launch work stay out of it.
test.skipIf(process.platform !== 'darwin' || helloBake().length === 0)('an app whose plan and grants open no I/O links no launch preloads (LLP 1047.001)', () => {
  for (const symbols of helloBake()) {
    const names = spawnSync('nm', [symbols], { encoding: 'utf8', maxBuffer: 1 << 28 }).stdout;
    assert.deepEqual(names.split('\n').filter((s) => /14exact_executor5cache|15launch_preloads6Launch/.test(s)), []);
  }
});

const agentSymbolCheck = process.platform !== 'darwin' ? notApplicable('Darwin only')
  : process.env.EXACT_GLUE_FAST === '1' ? notApplicable('the async glue step builds no Apple apps; metrics --long builds and checks these artifacts')
  : need('Apple Hello builds', missingAgentHelloBakes());
agentSymbolCheck('production omits the agent while development links it (LLP 1047.001)', () => {
  const names = (binary) => {
    const raw = spawnSync('xcrun', ['llvm-nm', '--demangle', binary], { encoding: 'utf8', maxBuffer: 1 << 28 });
    assert.equal(raw.status, 0, raw.stderr);
    const demangled = spawnSync('xcrun', ['swift-demangle'], { input: raw.stdout, encoding: 'utf8', maxBuffer: 1 << 28 });
    assert.equal(demangled.status, 0, demangled.stderr);
    return demangled.stdout;
  };
  // llvm-nm prefixes a symbol with its address and type; this positive control
  // proves the matcher is looking through that prefix before absence is useful.
  const line = (symbol) => new RegExp(`^[0-9a-fA-F]+\\s+\\S\\s+.*${symbol}`, 'm');
  const app = resolveApp('hello'), bakes = requiredAgentHelloBakes();
  assert.ok(bakes.some(bake => bake.platform === 'iOS' && bake.trust === 'production'));
  assert.ok(bakes.some(bake => bake.platform === 'macOS' && bake.trust === 'production'));
  for (const bake of bakes) {
    assert.deepEqual(receiptChanges(bake.receipt, app), [], `${bake.platform} ${bake.trust} build is stale`);
    const receipt = JSON.parse(readFileSync(bake.receipt, 'utf8'));
    assert.equal(receipt.build?.trust, bake.trust, `${bake.platform} receipt has another trust`);
    const symbols = names(bake.symbols);
    if (bake.trust === 'production') {
      assert.match(symbols, line('ExactKit\\.FrameHealth\\.reportSource\\.getter'), `${bake.platform} positive control`);
      assert.doesNotMatch(symbols, line('ExactKit\\.Agent(?:[.( ]|$)'), `${bake.platform} production agent`);
    } else {
      assert.match(symbols, line(`static ExactKit\\.Agent\\.${bake.platform === 'iOS' ? 'startSocket' : 'startStdio'}`), `${bake.platform} carrier`);
      assert.match(symbols, line('ExactKit\\.Agent\\.accessibilityElementsTree'), `${bake.platform} accessibility`);
      assert.match(symbols, line('ExactKit\\.Agent\\.layout\\(\\)'), `${bake.platform} layout`);
    }
  }
}, 60_000);

test('the load commands are read from the file, each slice of a universal one; a binary with no capability framework passes the check', () => {
  assert.ok(loadedDylibs('/bin/ls').includes('/usr/lib/libSystem.B.dylib'));
  assert.doesNotThrow(() => assertCapabilityFrameworks('/bin/ls', []));
});

test('SDK checks read thin, universal and fat64 executables on any build host, including every slice', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-sdk-'));
  const thin = (major, minor, patch = 0) => {
    const b = Buffer.alloc(56);
    b.writeUInt32LE(0xfeedfacf, 0);
    b.writeUInt32LE(1, 16); b.writeUInt32LE(24, 20);
    b.writeUInt32LE(0x32, 32); b.writeUInt32LE(24, 36);
    b.writeUInt32LE(7, 40); b.writeUInt32LE((major << 16) | (minor << 8) | patch, 48);
    return b;
  };
  const fat = (slices, wide) => {
    const width = wide ? 32 : 20, header = Buffer.alloc(8 + slices.length * width);
    header.writeUInt32BE(wide ? 0xcafebabf : 0xcafebabe, 0); header.writeUInt32BE(slices.length, 4);
    let offset = header.length;
    slices.forEach((slice, i) => {
      const at = 8 + i * width;
      if (wide) { header.writeBigUInt64BE(BigInt(offset), at + 8); header.writeBigUInt64BE(BigInt(slice.length), at + 16); }
      else { header.writeUInt32BE(offset, at + 8); header.writeUInt32BE(slice.length, at + 12); }
      offset += slice.length;
    });
    return Buffer.concat([header, ...slices]);
  };
  const path = join(dir, 'App (Exact)');
  const check = (bytes, passes) => {
    writeFileSync(path, bytes);
    if (passes) assert.doesNotThrow(() => assertLinkedSdk(path, '26.4'));
    else assert.throws(() => assertLinkedSdk(path, '26.4'), /records SDK .*not 26\.4/);
  };
  try {
    check(thin(26, 4, 1), true);
    check(thin(26, 5), false);
    for (const wide of [false, true]) {
      check(fat([thin(26, 4), thin(26, 4, 2)], wide), true);
      check(fat([thin(26, 4), thin(26, 5)], wide), false);
      const truncatedSlice = fat([thin(26, 4), thin(26, 4)], wide).subarray(0, -1);
      writeFileSync(path, truncatedSlice);
      assert.throws(() => assertLinkedSdk(path, '26.4'), /invalid 64-bit Mach-O/);
    }
    const missing = thin(26, 4); missing.writeUInt32LE(0x1b, 32); check(missing, false);
    const oversized = thin(26, 4); oversized.writeUInt32LE(64, 36);
    writeFileSync(path, oversized);
    assert.throws(() => assertLinkedSdk(path, '26.4'), /invalid 64-bit Mach-O/);
    for (const bytes of [Buffer.alloc(0), Buffer.alloc(4), thin(26, 4).subarray(0, 40)]) {
      writeFileSync(path, bytes);
      assert.throws(() => assertLinkedSdk(path, '26.4'), /invalid 64-bit Mach-O/);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});


test('ordinary app executables refuse WidgetKit, ActivityKit and SwiftUI, including versioned Mac frameworks (LLP 1126.001 WP16)', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-activity-links-'));
  try {
    for (const framework of ['WidgetKit', 'ActivityKit', 'SwiftUI']) for (const version of ['', 'Versions/A/']) {
      const name = Buffer.from(`/System/Library/Frameworks/${framework}.framework/${version}${framework}\0`);
      const size = (24 + name.length + 7) & ~7, bytes = Buffer.alloc(32 + size);
      bytes.writeUInt32LE(0xfeedfacf, 0); bytes.writeUInt32LE(1, 16); bytes.writeUInt32LE(size, 20);
      bytes.writeUInt32LE(0xc, 32); bytes.writeUInt32LE(size, 36); bytes.writeUInt32LE(24, 40); name.copy(bytes, 56);
      const executable = join(dir, 'App'); writeFileSync(executable, bytes);
      assert.throws(() => assertCapabilityFrameworks(executable, []), new RegExp(framework));
      assert.doesNotThrow(() => assertCapabilityFrameworks(executable, framework === 'SwiftUI' ? [] : ['ExactSnapshot'], framework === 'SwiftUI' ? ['SwiftUI'] : []));
    }
  } finally { rmSync(dir, {recursive:true,force:true}); }
});

test('a framework two capabilities may bring passes when either is linked, and a stray one names them both (#364)', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-shared-framework-'));
  try {
    const name = Buffer.from('/System/Library/Frameworks/SwiftUI.framework/SwiftUI\0');
    const size = (24 + name.length + 7) & ~7, bytes = Buffer.alloc(32 + size);
    bytes.writeUInt32LE(0xfeedfacf, 0); bytes.writeUInt32LE(1, 16); bytes.writeUInt32LE(size, 20);
    bytes.writeUInt32LE(0xc, 32); bytes.writeUInt32LE(size, 36); bytes.writeUInt32LE(24, 40); name.copy(bytes, 56);
    const executable = join(dir, 'App'); writeFileSync(executable, bytes);
    const frameworks = [['SwiftUI', ['ExactNumerals', 'ExactOther']]];
    assert.doesNotThrow(() => assertCapabilityFrameworks(executable, ['ExactOther'], [], frameworks));
    assert.doesNotThrow(() => assertCapabilityFrameworks(executable, ['ExactNumerals'], [], frameworks));
    assert.throws(() => assertCapabilityFrameworks(executable, [], [], frameworks), /SwiftUI \(only ExactNumerals or ExactOther may\)/);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test("the ActivityKit arm compiles the checkout's sources when the build runs from an app's own directory", () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-activity-arm-'));
  try {
    // A child process, so the working directory is the app's and not this process's.
    const script = `const { packageActivityArm } = await import(${JSON.stringify(new URL('./activities.mjs', import.meta.url).href)});
try { packageActivityArm({ shared: { base: process.cwd() }, device: false, triple: 'arm64-apple-ios17.0-simulator', sdk: '/sdk', runApple: (cmd, args) => { console.log(JSON.stringify(args)); throw new Error('stop'); } }); } catch {}`;
    const r = spawnSync(process.execPath, ['-e', script], { cwd: dir, encoding: 'utf8' });
    const sources = JSON.parse(r.stdout || '[]').filter(arg => arg.endsWith('.swift'));
    assert.equal(sources.length, 2, r.stderr);
    for (const source of sources) assert.ok(existsSync(source), source);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('activity refusals allow native APNs without activities and still scan every activity bundle binary', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-activity-scope-'));
  const plain = { manifest: {} }, empty = { manifest: { live_activities: [] } };
  const activity = resolveApp('caltrain');
  const granted = 'net.fetch https://x/\ndevice.notifications purpose.notifications\ndevice.push';
  const runApple = (cmd, args) => {
    const result = spawnSync(cmd, args, { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return result;
  };
  const bytes = readFileSync('/bin/ls');
  const directories = [dir, join(dir, 'PlugIns', 'Widget.appex'), join(dir, 'Frameworks', 'Arm.framework')];
  try {
    for (const directory of directories) {
      mkdirSync(directory, { recursive: true });
      writeFileSync(join(directory, 'Info.plist'), JSON.stringify({ CFBundleExecutable: 'Binary' }));
      writeFileSync(join(directory, 'Binary'), bytes);
    }
    const binaries = [...directories.map(directory => join(directory, 'Binary')), join(dir, 'Frameworks', 'Loose.dylib')];
    writeFileSync(binaries.at(-1), bytes);
    assert.equal(assertActivityRefusals(dir, runApple, activity), binaries.length);
    for (const binary of binaries) {
      for (const symbol of ['registerForRemoteNotifications', 'pushToStartTokenUpdates']) {
        writeFileSync(binary, Buffer.concat([bytes, Buffer.from(`\0${symbol}\0`)]));
        if (symbol === 'registerForRemoteNotifications') for (const app of [plain, empty]) {
          assert.equal(assertActivityRefusals(dir, runApple, app), 0);
        }
        if (symbol === 'registerForRemoteNotifications') {
          assert.throws(() => assertActivityRefusals(dir, runApple, activity), new RegExp(symbol));
          // With `device.push`, the app registers beside its activities.
          assert.equal(assertActivityRefusals(dir, runApple, activity, granted), binaries.length);
        } else {
          // Push-to-start is admitted (LLP 1126 D9, amended 2026-11-17).
          assert.equal(assertActivityRefusals(dir, runApple, activity), binaries.length);
          assert.equal(assertActivityRefusals(dir, runApple, activity, granted), binaries.length);
        }
      }
      writeFileSync(binary, bytes);
    }
    for (const directory of directories) {
      const plist = join(directory, 'Info.plist');
      writeFileSync(plist, JSON.stringify({ CFBundleExecutable: 'Binary', UIBackgroundModes: ['remote-notification'] }));
      for (const app of [plain, empty]) assert.equal(assertActivityRefusals(dir, runApple, app), 0);
      assert.throws(() => assertActivityRefusals(dir, runApple, activity), /remote-notification/);
      assert.throws(() => assertActivityRefusals(dir, runApple, activity, granted), /remote-notification/);
      writeFileSync(plist, JSON.stringify({ CFBundleExecutable: 'Binary' }));
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

// The async iOS step supplies its just-built bundle. Glue has no assembled iOS fixture yet.
test.skipIf(!process.env.EXACT_ACTIVITY_APP)('the assembled app and extension admit push-to-start symbols, refuse remote registration without device.push, and refuse its background mode (LLP 1126.001 WP9; LLP 1133.004 §9 P8)', async () => {
  const { cpSync, readFileSync, readdirSync } = await import('node:fs');
  const { spawnSync } = await import('node:child_process');
  const { resolve } = await import('node:path');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-activity-refusals-'));
  const runApple = (cmd, args) => {
    const result = spawnSync(cmd, args, { encoding: 'utf8' });
    assert.equal(result.status, 0, result.stderr);
    return result;
  };
  try {
    const app = resolve(process.env.EXACT_ACTIVITY_APP), declaration = resolveApp('caltrain');
    assert.ok(assertActivityRefusals(app, runApple, declaration) >= 3, 'app, extension and ActivityKit arm are scanned');
    cpSync(app, dir, { recursive: true });
    const extension = resolve(dir, 'PlugIns', readdirSync(resolve(dir, 'PlugIns')).find(n => n.endsWith('.appex')));
    for (const directory of [dir, extension]) {
      const plist = resolve(directory, 'Info.plist'), original = readFileSync(plist);
      const info = JSON.parse(runApple('plutil', ['-convert', 'json', '-o', '-', plist]).stdout);
      const binary = resolve(directory, info.CFBundleExecutable), bytes = readFileSync(binary);
      runApple('plutil', ['-replace', 'UIBackgroundModes', '-json', '["remote-notification"]', plist]);
      assert.throws(() => assertActivityRefusals(dir, runApple, declaration), /remote-notification/);
      writeFileSync(plist, original);
      const granted = 'device.notifications purpose.notifications\ndevice.push';
      for (const symbol of ['pushToStartTokenUpdates', 'registerForRemoteNotifications']) {
        writeFileSync(binary, Buffer.concat([bytes, Buffer.from(`\0${symbol}\0`)]));
        if (symbol === 'pushToStartTokenUpdates') {
          assert.ok(assertActivityRefusals(dir, runApple, declaration) >= 3);
          assert.ok(assertActivityRefusals(dir, runApple, declaration, granted) >= 3);
        } else {
          assert.throws(() => assertActivityRefusals(dir, runApple, declaration), new RegExp(symbol));
          assert.ok(assertActivityRefusals(dir, runApple, declaration, granted) >= 3);
        }
      }
      writeFileSync(binary, bytes);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
});

test('outside production trust the SDK compiles for iOS 17.0, and the app keeps its minimumOS', () => {
  const app = { manifest: { host: { ios: { minimumOS: '18.0' }, macos: { minimumOS: '15.0' } } } };
  assert.deepEqual(deploymentTargets(app), { ios: '18.0', macos: '15.0' });
  assert.deepEqual(compiledTargets(app, 'development'), { ios: '17.0', macos: '15.0' });
  assert.deepEqual(compiledTargets(app, 'production'), { ios: '18.0', macos: '15.0' });
  assert.deepEqual(compiledTargets({ manifest: {} }, 'development'), deploymentTargets({ manifest: {} }));
  const launching = { manifest: { launch: ['player'], host: { ios: { minimumOS: '18.0' } } } };
  assert.deepEqual(compiledTargets(launching, 'development', 'darwin'), deploymentTargets(launching), 'on a Mac, launch Swift compiles in the host package at the app minimum');
  // The native build system (the Linux cross build) gives the launch modules and the executable that imports them
  // the app's own target, so the host keeps the floor.
  assert.deepEqual(compiledTargets(launching, 'development', 'linux'), { ...deploymentTargets(launching), ios: '17.0' });
  const files = { manifest: { ...app.manifest, intents: { tools: { save: { doors: ['apple'], parameters: { image: { type: 'file', accept: ['image/png'] } } } } } } };
  assert.deepEqual(compiledTargets(files, 'development'), deploymentTargets(files), 'file content types need the iOS 18 initializer');

  // One spelling per version, so apps that write it differently share one Swift scratch (expo/exact-build#94).
  for (const minimumOS of ['18', '18.0', '18.0.0', 18]) assert.equal(deploymentTargets({ manifest: { host: { ios: { minimumOS } } } }).ios, '18.0', String(minimumOS));
  assert.equal(deploymentTargets({ manifest: { host: { ios: { minimumOS: '18.4.1' } } } }).ios, '18.4.1');
  assert.equal(deploymentTargets({ manifest: { host: { macos: { minimumOS: '15' } } } }).macos, '15.0');
});

test.skipIf(process.platform !== 'darwin' || !['clang', 'codesign', 'sips', 'dsymutil', 'nm'].every(tool => Bun.which(tool))).each([false, true])('production iOS bundles keep symbols and owning privacy declarations (disk-space service: %s)', async diskService => {
  useXcode();
  const { createHash } = await import('node:crypto');
  const { assembleIOS } = await import('./bundle.mjs');
  const { stripProductionExecutable } = await import('./link.mjs');
  const { shippedReceipt, copyAppleGpuProducts } = await import('./build.mjs');
  const { entitlements } = await import('./build.mjs');
  const { wrapFramework, signedGpuProduct, codeDigest, plistFile } = await import('./assets.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-ios-package-'));
  const run = (cmd, args) => {
    const r = spawnSync(cmd, args, { encoding: 'utf8' });
    assert.equal(r.status, 0, `${cmd}: ${r.stderr}`);
    return r;
  };
  try {
    const source = resolve(dir, 'module.c'), executable = resolve(dir, 'Fixture'), dylib = resolve(dir, 'module.dylib');
    writeFileSync(source, '#include <sys/stat.h>\nstatic int local_value = 42; int answer(void){struct stat s; stat(".", &s); return local_value;} int main(void){return answer()!=42;}\n');
    run('clang', ['-g', source, '-o', executable]);
    run('clang', ['-g', '-dynamiclib', source, '-o', dylib]);
    assert.match(run('nm', ['-a', dylib]).stdout, / _local_value$/m);
    const icon = resolve(dir, 'icon.png');
    writeFileSync(icon, encodePng({ width: 1024, height: 1024, data: new Uint8Array(1024 * 1024 * 4).fill(255) }));
    const app = { dir, id: 'com.exact.fixture', name: 'Fixture', manifest: {} };
    const api = (name, reasons) => ({ NSPrivacyAccessedAPIType: `NSPrivacyAccessedAPICategory${name}`, NSPrivacyAccessedAPITypeReasons: reasons });
    const declarations = { NSPrivacyAccessedAPITypes: [api('DiskSpace', ['E174.1'])], NSPrivacyTracking: true,
      NSPrivacyTrackingDomains: ['ads.example.com'], NSPrivacyCollectedDataTypes: [{ NSPrivacyCollectedDataType: 'NSPrivacyCollectedDataTypeEmailAddress',
        NSPrivacyCollectedDataTypeLinked: true, NSPrivacyCollectedDataTypeTracking: false, NSPrivacyCollectedDataTypePurposes: ['NSPrivacyCollectedDataTypePurposeAppFunctionality'] }] };
    writeFileSync(resolve(dir, 'PrivacyInfo.xcprivacy'), plistFile(declarations));
    const service = resolve(dir, 'service.dylib');
    writeFileSync(source, diskService ? '#include <sys/mount.h>\nstatic int local_value = 42; int answer(void){struct statfs s; statfs(".", &s); return local_value;}\n' : readFileSync(source));
    run('clang', ['-g', '-dynamiclib', source, '-o', service]);
    const libraries = resolve(dir, 'libraries'); mkdirSync(libraries);
    copyFileSync(dylib, resolve(libraries, 'library.dylib'));
    wrapFramework(libraries, 'library.dylib', 'FixtureLibrary', app);
    const library = { placed: resolve(libraries, 'FixtureLibrary.framework'), link: 'FixtureLibrary' };
    const plan = Buffer.from('fixture'), compat = { embedded: { plan: { sha256: createHash('sha256').update(plan).digest('hex'), bytes: plan.length }, assets: [] } };
    const pairs = [['libweb.dylib', 'ExactWeb'], ['libmodules.dylib', 'ExactModules'], ['libsvg.dylib', 'ExactSvg'], ['libcanvas.dylib', 'ExactCanvasGpu'], ['libservice.dylib', 'ExactService']];
    const cloudkitEnvironment = 'com.apple.developer.icloud-container-environment';
    const appleReach = { entitlements: ['com.apple.developer.icloud-services', 'com.apple.developer.icloud-container-identifiers', cloudkitEnvironment],
      entitlementValues: { 'com.apple.developer.icloud-services': ['CloudKit'] } };
    const build = { trust: 'production', linkedPlan: '/private/plan', binary: { sha256: 'binary', inputs: [{ path: '/private/source' }], metadata: {}, privacyManifests: [capturePrivacyManifest(resolve(dir, 'PrivacyInfo.xcprivacy'))] }, products: [{ path: '/private/product' }], graph: { artifacts: [], extensions: {}, contractSources: [] } };
    writeFileSync(resolve(dir, 'PrivacyInfo.xcprivacy'), plistFile({ NSPrivacyTracking: false }));
    for (const [production, device, archived] of [[true, true, false], [true, true, true], [true, false, false], [false, true, false]]) {
      app.manifest = archived ? { icons: [{ src: 'icon.png', sizes: '1024x1024' }] } : {};
      const binDir = resolve(dir, `${production}-${device}-${archived}`); mkdirSync(binDir);
      const bin = resolve(binDir, 'ExactClip');
      copyFileSync(executable, bin);
      const capture = resolve(binDir, 'capture'); mkdirSync(capture);
      const gpu = { built: 'gpu.dylib', load: 'libgpu.dylib', framework: 'ExactGpu' };
      copyFileSync(dylib, resolve(capture, gpu.built));
      if (production) {
        const signed = signedGpuProduct(resolve(capture, gpu.built), { framework: gpu.framework, app, privacyManifest: build.binary.privacyManifests[0], platform: device ? 'iPhoneOS' : 'iPhoneSimulator' });
        cpSync(resolve(signed, '..'), resolve(capture, 'ExactGpu.framework'), { recursive: true });
      }
      const gpuSource = resolve(capture, production ? 'ExactGpu.framework/ExactGpu' : gpu.built);
      run('codesign', ['--force', '--sign', '-', '--timestamp=none', production ? resolve(capture, 'ExactGpu.framework') : gpuSource]);
      const ipa = archived ? resolve(binDir, 'Archive.ipa') : null;
      const symbols = stripProductionExecutable(bin, { production, ipa });
      const bundle = resolve(binDir, 'Fixture.app');
      const whole = JSON.stringify({ build, target: 'aarch64-apple-ios', development: production ? null : { token: 'kept' } });
      const clipBin = resolve(binDir, 'clip', 'Clip');
      mkdirSync(resolve(binDir, 'clip'));
      copyFileSync(bin, clipBin);
      const options = { app, library, paths: { bundle, executable: 'ExactClip', capture }, binDir, bin, device,
        clipBin, production, distribution: archived, ipa, bakedCompat: compat, bakedPlan: plan, buildReceipt: build,
        libDir: capture, gpuArtifacts: [gpu], looseArms: [[true, dylib, ...pairs[0]]],
        modulesBuilt: dylib, modulesLoadName: pairs[1][0], hasSvg: true, svgBuilt: dylib, svgLoadName: pairs[2][0],
        canvasGpuBuilt: dylib, canvasGpuLoadName: pairs[3][0], serviceDylibs: [{ built: service, load: pairs[4][0], framework: pairs[4][1] }],
        args: [], unsigned: true, sha1: '-', linkRoot: binDir, sdkName: 'iphoneos', appleReach };
      const helpers = {
        runApple: run, infoPlist: () => plistFile({ CFBundleIdentifier: app.id, CFBundleExecutable: 'ExactClip', CFBundlePackageType: 'APPL' }),
        copyAppleStaticTrees() {}, writeUsageStrings() {}, assertAppleIdentity() {}, distributionKeys: () => ({}),
        receipt: () => whole, shippedReceipt, copyAppleGpuProducts, entitlements, appClip: () => ({ id: app.id + '.Clip', name: 'Clip', executable: 'Clip' }),
      };
      if (!production) {
        // Inspect the app after AlarmKit's plist writes, before compiling its separate extension.
        const { appleMetadataProblem } = await import('./plist.mjs');
        const metadataWritten = new Error('alarm metadata inspected');
        const execute = appleToolchain.spawnSync;
        const probe = spyOn(appleToolchain, 'spawnSync').mockImplementation((command, args, opts) => {
          const result = execute(command, args, opts);
          if (command === 'plutil' && args[0] === '-replace' && args[1] === 'ExactAlarmCountdown') {
            probe.mockRestore();
            const metadata = JSON.parse(run('plutil', ['-convert', 'json', '-o', '-', resolve(bundle, 'Info.plist')]).stdout);
            for (const key of ['NSSupportsLiveActivities', 'ExactAlarmCountdown']) {
              assert.equal(metadata[key], true);
              assert.match(appleMetadataProblem(`host.ios.infoPlist.${key}: not a known key`), /Exact writes this key itself from its declarations.*cannot be set here/);
            }
            throw metadataWritten;
          }
          return result;
        });
        try { assert.throws(() => assembleIOS({ ...options, alarm: {} }, helpers), error => error === metadataWritten); }
        finally { probe.mockRestore(); }
      }
      const result = assembleIOS(options, helpers);
      const signed = device ? run('codesign', ['-d', '--entitlements', '-', '--xml', bundle]).stdout : readFileSync(resolve(binDir, 'entitlements.plist'), 'utf8');
      assert.match(signed, new RegExp(`<key>${cloudkitEnvironment}</key>\\s*<string>Production</string>`),
        'a distribution archive needs Production, including when a recipient will re-sign it (ITMS-90046)');
      assert.match(signed, /<key>com.apple.developer.icloud-services<\/key>\s*<array>\s*<string>CloudKit<\/string>/);
      assert.match(signed, new RegExp(`<string>iCloud\\.${app.id}</string>`));
      assert.doesNotMatch(signed, /aps-environment|ubiquity-/);
      assert.doesNotMatch(run('codesign', ['-d', '--entitlements', '-', '--xml', resolve(bundle, 'AppClips/Clip.app')]).stdout, /com.apple.developer.icloud-/);
      for (const owner of [bundle, resolve(bundle, 'AppClips/Clip.app'), ...(production ? [...pairs, [null, 'ExactGpu']].map(([, name]) => resolve(bundle, 'Frameworks', `${name}.framework`)) : [])]) {
        const privacy = resolve(owner, 'PrivacyInfo.xcprivacy'); assert.equal(run('plutil', ['-lint', privacy]).status, 0);
        const usesDisk = diskService && (owner.endsWith('/ExactService.framework') || (!production && owner.endsWith('.app')));
        const reasons = [...(usesDisk ? [api('DiskSpace', ['E174.1'])] : []), ...(diskService && owner.endsWith('/ExactService.framework') ? [] : [api('FileTimestamp', ['3B52.1', 'C617.1'])])];
        assert.deepEqual(JSON.parse(run('plutil', ['-convert', 'json', '-o', '-', privacy]).stdout), {
          ...(owner.endsWith('.app') ? declarations : {}), NSPrivacyAccessedAPITypes: reasons,
        }, owner);
      }
      const clipBytes = readFileSync(resolve(bundle, 'AppClips/Clip.app/Clip'));
      assert.equal(codeDigest(clipBytes), codeDigest(readFileSync(device ? bin : clipBin)), 'the clip uses its platform executable, including when the parent is named ExactClip');
      assert.equal(codeDigest(readFileSync(resolve(bundle, 'ExactClip'))), codeDigest(readFileSync(bin)));
      if (symbols) {
        const uuid = path => /UUID: ([A-F0-9-]+)/.exec(run('dwarfdump', ['--uuid', path]).stdout)?.[1];
        assert.ok(uuid(symbols.dsym));
        assert.equal(uuid(symbols.dsym), uuid(resolve(bundle, 'ExactClip')));
        assert.equal(uuid(symbols.dsym), uuid(resolve(bundle, 'AppClips/Clip.app/Clip')));
      }
      const bundled = readFileSync(resolve(bundle, 'receipt.json'), 'utf8');
      assert.equal(bundled, production ? shippedReceipt(whole) : whole);
      if (production) {
        const sidecar = resolve(binDir, ipa ? 'Archive.receipt.json' : 'ExactClip.receipt.json');
        assert.equal(readFileSync(sidecar, 'utf8'), whole);
        assert.equal(result.frameworkSymbols.count, pairs.length + 2);
        for (const [loose, name] of [...pairs, [null, 'FixtureLibrary']]) {
          const binary = resolve(bundle, 'Frameworks', `${name}.framework`, name);
          if (loose) assert.equal(existsSync(resolve(bundle, 'Frameworks', loose)), false);
          const preserved = ['ExactSvg', 'ExactCanvasGpu', 'FixtureLibrary'].includes(name), names = run('nm', ['-a', binary]).stdout;
          assert.match(names, / T _answer$/m);
          assert.equal(/ _local_value$/m.test(names), preserved, `${name} symbol policy`);
          const symbols = resolve(result.frameworkSymbols.dir, `${name}.framework.dSYM`);
          assert.equal(/UUID: ([A-F0-9-]+)/.exec(run('dwarfdump', ['--uuid', symbols]).stdout)?.[1], /UUID: ([A-F0-9-]+)/.exec(run('dwarfdump', ['--uuid', binary]).stdout)?.[1]);
        }
        const packageBinary = resolve(bundle, 'Frameworks/FixtureLibrary.framework/FixtureLibrary');
        assert.equal(codeDigest(readFileSync(packageBinary)), codeDigest(readFileSync(resolve(library.placed, library.link))), 'the package library keeps its linker output');
        assert.equal(codeDigest(readFileSync(resolve(bundle, 'Frameworks/ExactGpu.framework/ExactGpu'))), codeDigest(readFileSync(gpuSource)), 'the baked GPU digest is preserved');
        assert.ok(existsSync(resolve(result.frameworkSymbols.dir, 'ExactGpu.framework.dSYM')));
      } else {
        assert.equal(result.frameworkSymbols.count, 0);
        for (const [loose] of pairs) assert.match(run('nm', ['-a', resolve(bundle, 'Frameworks', loose)]).stdout, / _local_value$/m);
        assert.equal(existsSync(resolve(binDir, 'ExactClip.receipt.json')), false);
      }
      run('codesign', ['--verify', '--deep', '--strict', bundle]);
    }
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60_000);

test('the app minimum is written into every slice after a link for the floor, leaving the SDK and the rest as linked (expo/exact-build#176)', () => {
  const dir = mkdtempSync(join(tmpdir(), 'exact-minos-'));
  const thin = (minos, sdk) => {
    const b = Buffer.alloc(56);
    b.writeUInt32LE(0xfeedfacf, 0); b.writeUInt32LE(1, 16); b.writeUInt32LE(24, 20);
    b.writeUInt32LE(0x32, 32); b.writeUInt32LE(24, 36); b.writeUInt32LE(2, 40);
    b.writeUInt32LE(minos, 44); b.writeUInt32LE(sdk, 48);
    return b;
  };
  const path = join(dir, 'App');
  const v = (major, minor, patch = 0) => (major << 16) | (minor << 8) | patch;
  try {
    writeFileSync(path, thin(v(17, 0), v(26, 4)));
    recordMinimumOS(path, '18');
    const thinOut = readFileSync(path);
    assert.equal(thinOut.readUInt32LE(44), v(18, 0));
    assert.equal(thinOut.readUInt32LE(48), v(26, 4));
    assert.equal(thinOut.readUInt32LE(40), 2);
    assert.doesNotThrow(() => assertLinkedSdk(path, '26.4'));
    recordMinimumOS(path, '18.2.1');
    assert.equal(readFileSync(path).readUInt32LE(44), v(18, 2, 1));
    // Every slice of a universal file.
    const header = Buffer.alloc(8 + 2 * 20); header.writeUInt32BE(0xcafebabe, 0); header.writeUInt32BE(2, 4);
    header.writeUInt32BE(48, 16); header.writeUInt32BE(56, 20); header.writeUInt32BE(104, 36); header.writeUInt32BE(56, 40);
    writeFileSync(path, Buffer.concat([header, thin(v(17, 0), v(26, 4)), thin(v(17, 0), v(26, 4))]));
    recordMinimumOS(path, '19.1');
    const fatOut = readFileSync(path);
    assert.equal(fatOut.readUInt32LE(48 + 44), v(19, 1)); assert.equal(fatOut.readUInt32LE(104 + 44), v(19, 1));
    // No LC_BUILD_VERSION, or not a version: refused, the file unchanged.
    const none = thin(v(17, 0), v(26, 4)); none.writeUInt32LE(0x1b, 32); writeFileSync(path, none);
    assert.throws(() => recordMinimumOS(path, '18'), /no single LC_BUILD_VERSION/);
    assert.throws(() => recordMinimumOS(path, 'eighteen'), /not a minimum OS version/);
    assert.deepEqual(readFileSync(path), none);
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
