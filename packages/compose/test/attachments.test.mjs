import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { resolve, join } from 'node:path';
import { spawnSync } from 'node:child_process';

const root = resolve(import.meta.dir, '../../..');

test.skipIf(process.platform !== 'darwin')('Apple composers require existing file attachments before presenting', () => {
  const target = join(root, 'target');
  mkdirSync(target, { recursive: true });
  const dir = mkdtempSync(join(target, 'compose-attachments-'));
  const run = (command, args) => {
    const result = spawnSync(command, args, { cwd: root, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stdout + result.stderr);
  };
  try {
    run('xcrun', ['swiftc', '-parse-as-library', '-emit-library', '-static', '-module-name', 'ExactPackage',
      '-emit-module', '-emit-module-path', join(dir, 'ExactPackage.swiftmodule'),
      'host/apple/modules/ExactPackage.swift', '-o', join(dir, 'libExactPackage.a')]);
    const source = join(dir, 'Attachments.swift');
    writeFileSync(source, `import ExactPackage
import Foundation

@main struct Attachments {
    static func main() throws {
        let root = URL(fileURLWithPath: CommandLine.arguments[1])
        let file = root.appendingPathComponent("invite.ics")
        try Data("invitation".utf8).write(to: file)
        let directory = root.appendingPathComponent("album.jpg")
        try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: true)
        try Data("photo".utf8).write(to: directory.appendingPathComponent("a.jpg"))
        let context = ExactPackageContext(agent: true, session: "attachments", changed: { _ in },
            log: { _ in }, now: { 0 }, file: { path in
                guard path.hasPrefix("app:/data/") else { return nil }
                return AppFileWalk.destination(root, path.dropFirst(10).split(separator: "/", omittingEmptySubsequences: false).map(String.init))
            })
        let module = ComposeModule(context: context)
        var failures = 0
        for name in ["composeEmail", "composeMessage"] {
            for path in ["app:/data/missing.jpg", "app:/data/new/missing.jpg", "app:/data/album.jpg"] {
                var result: [String: Any]?
                module.later(name, ["attachments": [path]], preferred: ["tap": "sent"]) { result = $0 }
                if result?["error"] as? String != "NotFoundError" {
                    print("FAIL \\(name) \\(path): \\(String(describing: result))")
                    failures += 1
                }
            }
        }
        var result: [String: Any]?
        module.later("composeEmail", ["attachments": ["app:/data/invite.ics"]], preferred: ["tap": "sent"]) { result = $0 }
        if result?["action"] as? String != "sent" {
            print("FAIL existing attachment: \\(String(describing: result))")
            failures += 1
        }
        exit(failures == 0 ? 0 : 1)
    }
}
`);
    const binary = join(dir, 'attachments');
    run('xcrun', ['swiftc', '-parse-as-library', '-I', dir, '-L', dir, '-lExactPackage',
      'host/apple/Sources/ExactCore/AppFileWalk.swift', 'packages/compose/apple/Compose.swift', source, '-o', binary]);
    run(binary, [dir]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 60000);
