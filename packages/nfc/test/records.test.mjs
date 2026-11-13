import { test } from 'bun:test';
import assert from 'node:assert/strict';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { join, resolve } from 'node:path';
import { spawnSync } from 'node:child_process';

const root = resolve(import.meta.dir, '../../..');

// The record encoder is the same on iOS and on the Mac, where Core NFC is absent and the module refuses every request.
test.skipIf(process.platform !== 'darwin')('Apple encodes a url or absolute-url record whose data is a string or bytes', () => {
  const target = join(root, 'target');
  mkdirSync(target, { recursive: true });
  const dir = mkdtempSync(join(target, 'nfc-records-'));
  const run = (command, args) => {
    const result = spawnSync(command, args, { cwd: root, encoding: 'utf8' });
    assert.equal(result.status, 0, result.stdout + result.stderr);
    return result.stdout;
  };
  try {
    run('xcrun', ['swiftc', '-parse-as-library', '-emit-library', '-static', '-module-name', 'ExactPackage',
      '-emit-module', '-emit-module-path', join(dir, 'ExactPackage.swiftmodule'),
      'host/apple/modules/ExactPackage.swift', '-o', join(dir, 'libExactPackage.a')]);
    const source = join(dir, 'Records.swift');
    writeFileSync(source, `import Foundation

@main struct Records {
    static func main() {
        let bytes = ["base64": Data("https://example.com/a".utf8).base64EncodedString()]
        for recordType in ["url", "absolute-url"] {
            for data in ["https://example.com/a", bytes] as [Any] {
                switch NfcRecords.encode(["recordType": recordType, "data": data], lang: "en") {
                case .success(let (tnf, type, _, payload)):
                    print("\\(recordType) \\(data is String ? "string" : "bytes"): \\(tnf) \\(String(decoding: type, as: UTF8.self)) \\(payload.count)")
                case .failure(let error):
                    print("\\(recordType) \\(data is String ? "string" : "bytes"): TypeError \\(error.message)")
                }
            }
        }
        let colonless = ["base64": Data("example".utf8).base64EncodedString()]
        if case .failure(let error) = NfcRecords.encode(["recordType": "absolute-url", "data": colonless], lang: "en") {
            print("absolute-url without a colon: TypeError \\(error.message)")
        }
    }
}
`);
    const binary = join(dir, 'records');
    run('xcrun', ['swiftc', '-parse-as-library', '-I', dir, '-L', dir, '-lExactPackage', 'packages/nfc/apple/Nfc.swift', 'packages/nfc/apple/NfcRegistry.swift', source, '-o', binary]);
    assert.deepEqual(run(binary, []).trim().split('\n'), [
      'url string: 1 U 14',
      'url bytes: 1 U 14',
      'absolute-url string: 3 https://example.com/a 0',
      'absolute-url bytes: 3 https://example.com/a 0',
      "absolute-url without a colon: TypeError an absolute-url record's data is a URL",
    ]);
  } finally { rmSync(dir, { recursive: true, force: true }); }
}, 120000);
