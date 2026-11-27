import { test, expect, spyOn } from 'bun:test';
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
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
