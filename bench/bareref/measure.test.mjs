import { expect, test } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';
import { exactPush } from './push.mjs';

test('Exact sends no landing or settlement command before the first commit acknowledgement', async () => {
  const commands = [];
  let acknowledge, waiting;
  const commit = new Promise(resolve => { acknowledge = resolve; });
  const reached = new Promise(resolve => { waiting = resolve; });
  const session = {
    find: async target => { expect(target).toBe('change-station'); return { id: 42 }; },
    op: async req => { commands.push(req); return { tapped: 42 }; },
    tap: async () => {
      await session.op({ op: 'tap', id: 42 });
      await session.op({ op: 'clock', land: true });
    },
    clock: async value => { commands.push({ op: 'clock', value }); },
  };
  const push = exactPush(session, async () => { waiting(); await commit; });
  await reached;
  const beforeCommit = [...commands];
  acknowledge();
  await push;
  expect(beforeCommit).toEqual([{ op: 'tap', id: 42 }]);
  expect(commands).toEqual([{ op: 'tap', id: 42 }, { op: 'clock', value: 'settle' }]);
});

test.skipIf(process.platform !== 'darwin')('publishing records preserves a polling reader already holding the previous file', () => {
  const dir = mkdtempSync(join(tmpdir(), 'bareref-publication-'));
  try {
    // Exercise the probe's Foundation write without compiling its UIKit observer and hierarchy walker.
    const probe = readFileSync(new URL('./PushProbe.swift', import.meta.url), 'utf8');
    const statement = probe.match(/try! JSONSerialization\.data\(withJSONObject: records\)\.write\(to: url[^\n]*/)?.[0];
    expect(statement).toBeDefined();
    const source = join(dir, 'main.swift'), path = join(dir, 'bareref.json');
    writeFileSync(source, `import Foundation
let url = URL(fileURLWithPath: CommandLine.arguments[1])
let previous = Data(#"[{"serial":1}]"#.utf8)
try previous.write(to: url)
let reader = try FileHandle(forReadingFrom: url)
let records: [[String: Any]] = [["serial": 1], ["serial": 2]]
${statement}
let held = try reader.readToEnd()!
guard held == previous else { fatalError("publication rewrote the polling reader's open file") }
let current = try JSONSerialization.jsonObject(with: Data(contentsOf: url)) as! [[String: Int]]
precondition(current == [["serial": 1], ["serial": 2]])
try reader.close()
`);
    const binary = join(dir, 'publish');
    const built = spawnSync('xcrun', ['swiftc', source, '-o', binary], { encoding: 'utf8' });
    expect(built.status, built.stderr).toBe(0);
    const run = spawnSync(binary, [path], { encoding: 'utf8' });
    expect(run.status, run.stderr).toBe(0);
  } finally {
    rmSync(dir, { recursive: true, force: true });
  }
});
