import { expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const root = resolve(import.meta.dir, '../../..');

test.skipIf(process.platform !== 'darwin')('Apple BackgroundModule uses live wall time across sleep-paused monotonic clocks', () => {
  mkdirSync(join(root, 'target'), { recursive: true });
  const out = mkdtempSync(join(root, 'target/background-apple-sleep-'));
  const run = (cmd, args) => {
    const r = spawnSync(cmd, args, { cwd: root, encoding: 'utf8', timeout: 120_000 });
    expect(r.status, `${cmd}: ${r.stdout}\n${r.stderr}`).toBe(0);
    return r.stdout;
  };
  try {
    const source = join(out, 'Check.swift');
    writeFileSync(source, String.raw`import ExactPackage
import Foundation

@main struct Check {
    static func main() {
        let defaults = UserDefaults.standard
        let previous = defaults.data(forKey: "exact.background")
        defaults.removeObject(forKey: "exact.background")
        defer { defaults.set(previous, forKey: "exact.background") }
        func wait(_ ready: () -> Bool) {
            let limit = Date().addingTimeInterval(5)
            while !ready() && Date() < limit { _ = RunLoop.main.run(mode: .default, before: limit) }
            precondition(ready(), "callback did not arrive")
        }
        var wall = 1_800_000_000_000.0
        var sessionClock = 10_000.0
        var requestedAt: [String: Double] = [:]
        let wakes = ExactWakes { key, request in requestedAt[key] = request?["at"] as? Double }
        var attempt1: ((String) -> Void)?
        var raised = 0, done: [Bool] = []
        let mod = BackgroundModule(context: ExactPackageContext(agent: false, session: "sleep", changed: { _ in }, log: { _ in },
                                                                now: { sessionClock },
                                                                wake: { _, _, _, cb in raised += 1; attempt1 = cb },
                                                                wakes: wakes), wallClock: { wall })
        var registered = false
        mod.later("registerSync", ["tag": "outbox"], preferred: nil) { _ in registered = true }
        wait { registered }
        var periodic = false
        mod.later("registerPeriodicSync", ["tag": "digest", "minInterval": 900_000.0], preferred: nil) { _ in periodic = true }
        wait { periodic }
        let initialPeriodicWall = requestedAt["periodicsync digest"]! - sessionClock + wall
        precondition(abs(initialPeriodicWall - (wall + 900_000.0)) < 1)

        // First attempt (via NWPathMonitor if online, or via woke if offline) fails with re-registration -> 5-minute backoff.
        mod.woke(ExactWake(kind: "processing", keys: ["sync"], deadline: nil, platform: nil,
                           raise: { _, _, cb in raised += 1; attempt1 = cb }, done: { done.append($0) }))
        wait { attempt1 != nil }
        var again = false
        mod.later("registerSync", ["tag": "outbox"], preferred: nil) { _ in again = true }
        wait { again }
        attempt1?("expired")
        wait { done.count == 1 }
        precondition(raised == 1, "expected 1 initial attempt, got \(raised)")

        // Sleep 6 minutes: live wall advances while monotonic sessionClock stays frozen.
        wall += 360_000.0
        var cb2: ((String) -> Void)?
        mod.woke(ExactWake(kind: "processing", keys: ["sync"], deadline: nil, platform: nil,
                           raise: { _, _, cb in raised += 1; cb2 = cb }, done: { done.append($0) }))
        wait { cb2 != nil }
        cb2?("completed")
        wait { done.count == 2 }
        precondition(raised == 2 && done == [false, true], "unexpected wake outcomes: raised=\(raised) done=\(done)")
        let afterSleepPeriodicWall = requestedAt["periodicsync digest"]! - sessionClock + wall
        precondition(abs(afterSleepPeriodicWall - initialPeriodicWall) < 1, "periodic wake drifted: \(afterSleepPeriodicWall) vs \(initialPeriodicWall)")
        print("PASS Apple BackgroundModule sleep clock")
    }
}
`);
    run('xcrun', ['swiftc', '-parse-as-library', '-emit-library', '-static', '-module-name', 'ExactPackage', '-emit-module',
      '-emit-module-path', join(out, 'ExactPackage.swiftmodule'), 'host/apple/modules/ExactPackage.swift', '-o', join(out, 'libExactPackage.a')]);
    run('xcrun', ['swiftc', '-parse-as-library', '-swift-version', '5', '-I', out, '-L', out, '-lExactPackage',
      'packages/background/apple/Background.swift', source, '-o', join(out, 'check')]);
    const stdout = run(join(out, 'check'), []);
    expect(stdout).toContain('PASS Apple BackgroundModule sleep clock');
  } finally {
    rmSync(out, { recursive: true, force: true });
  }
}, 120_000);
