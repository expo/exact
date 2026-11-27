import { expect, test } from 'bun:test';
import { mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { spawnSync } from 'node:child_process';
import { need } from '../../../scripts/coverage.mjs';

const root = resolve(import.meta.dir, '../../..');
const swift = process.platform === 'darwin'
  ? spawnSync('xcrun', ['swiftc', '--version'], { encoding: 'utf8' })
  : null;
const check = need('swift', swift && swift.status !== 0
  ? (swift.stderr || swift.error?.message || `exit ${swift.status}`).trim()
  : null, { test: test.skipIf(process.platform !== 'darwin') });

function run(command, args) {
  const result = spawnSync(command, args, { cwd: root, encoding: 'utf8' });
  expect(result.status, `${result.stdout}${result.stderr}`).toBe(0);
  return result;
}

const STUB = `
import Foundation
public typealias Done = @convention(c) (UnsafeMutableRawPointer?, UnsafePointer<UInt8>?, UInt32) -> Void
@_cdecl("exact_web_automation")
public func exact_web_automation(_ bytes: UnsafePointer<UInt8>?, _ length: UInt32, _ context: UnsafeMutableRawPointer?, _ done: Done?) {
    guard let bytes, length > 1, let done else { return }
    let data = Data(UnsafeBufferPointer(start: bytes.advanced(by: 1), count: Int(length - 1)))
    let request = (try? JSONSerialization.jsonObject(with: data)) as? [String: Any] ?? [:]
    var reply: [String: Any] = ["ok": true]
    if let p = request["pathFile"] { reply["pathFile"] = p }
    if let d = request["destinationFolderFile"] { reply["destinationFolderFile"] = d }
    let out = try! JSONSerialization.data(withJSONObject: reply)
    out.withUnsafeBytes { done(context, $0.bindMemory(to: UInt8.self).baseAddress, UInt32(out.count)) }
}
`;

check('setDownloadBehavior resolves destinationFolder with context.directory', () => {
  const build = mkdtempSync(resolve(tmpdir(), 'exact-browser-auto-'));
  try {
    const stubSwift = resolve(build, 'stub.swift');
    writeFileSync(stubSwift, STUB);
    run('xcrun', ['swiftc', '-emit-library', '-parse-as-library', stubSwift, '-o', resolve(build, 'libexact_web.dylib')]);
    run('xcrun', ['swiftc', '-emit-library', '-emit-module', '-parse-as-library', '-module-name', 'ExactPackage',
      resolve(root, 'host/apple/modules/ExactPackage.swift'), '-o', resolve(build, 'libExactPackage.dylib')]);
    run('xcrun', ['swiftc', '-I', build, '-L', build, '-lExactPackage',
      resolve(root, 'host/apple/Sources/ExactCore/AppFileWalk.swift'),
      resolve(root, 'packages/browser-automation/apple/BrowserAutomation.swift'),
      resolve(import.meta.dir, 'apple-destination-folder.swift'), '-o', resolve(build, 'destination-folder-test')]);
    const result = spawnSync(resolve(build, 'destination-folder-test'), [], {
      cwd: root, encoding: 'utf8', env: { ...process.env, DYLD_LIBRARY_PATH: build },
    });
    expect(`${result.stdout}${result.stderr}`).toBe('destinationFolder ok\n');
    expect(result.status).toBe(0);
  } finally {
    rmSync(build, { recursive: true, force: true });
  }
}, 30000);
