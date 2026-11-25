// The tool runs inside the simulator. Each drive builds it in its own temporary directory.
import { spawnSync } from 'node:child_process';
import { resolve } from 'node:path';
import { fileURLToPath } from 'node:url';

export const hasSimulatorHinge = dev => dev.type === 'iPhone-Duo';

export function hingeHelper(udid, dir) {
  const helper = resolve(dir, 'hinge_helper');
  const source = fileURLToPath(new URL('./hinge_helper.m', import.meta.url));
  const run = (command, args, what) => {
    const r = spawnSync(command, args, { encoding: 'utf8', timeout: 60000 });
    if (r.status !== 0) throw new Error(`${what}: ${r.error?.message || r.stderr?.trim() || r.stdout?.trim() || r.signal || r.status}`);
    return r.stdout.trim();
  };
  run('xcrun', ['-sdk', 'iphonesimulator', 'clang', '-arch', process.arch === 'arm64' ? 'arm64' : 'x86_64',
    '-mios-simulator-version-min=17.0', '-O2', '-fobjc-arc', '-DEXACT_AGENT',
    '-framework', 'CoreMotion', '-framework', 'Foundation', '-framework', 'IOKit', '-framework', 'CoreFoundation',
    '-o', helper, source], 'compiling the hinge helper');
  run('codesign', ['-f', '-s', '-', helper], 'signing the hinge helper');
  return (command, ...args) => run('xcrun', ['simctl', 'spawn', udid, helper, command, ...args.map(String)], `hinge ${command} ${args.join(' ')}`);
}
