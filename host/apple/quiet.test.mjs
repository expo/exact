// Exact's own Swift warnings leave an outside app's build output; the app's own, errors and
// everything else stay (quiet.mjs, LLP 1116.021 D3).
import { expect, test, spyOn } from 'bun:test';
import { quiet, quietFor } from './quiet.mjs';
import { copyFileSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { packageShareExtension } from './extensions.mjs';
import { plistFile } from './assets.mjs';

const ROOT = '/x/exact2', C = (code) => `\x1b[${code}m`;
const block = (file, kind, text, cite = 'NoUsage') => [
  `${file}:72:91: ${C('1;33')}${kind}: ${C('1;39')}${text}${C('0;0')}`,
  `${C('0;36')}71 |${C('0;0')}         }`,
  `   ${C('0;36')}|${C('0;0')}    \`- ${C('1;33')}${kind}: ${C('1;39')}${text}${C('0;0')} [#\x1b]8;;https://docs.swift.org/x\x1b\\${cite}\x1b]8;;\x1b\\]`,
  `${C('0;36')}72 |${C('0;0')}         f.go()`,
  '',
];

test('an Exact warning, its blank line and an uncited note go; the app\'s warning and an error stay', () => {
  const output = [
    '[129 / 137] ExactKit',
    ...block(`${ROOT}/host/apple/Sources/ExactKit/A.swift`, 'warning', 'result unused'),
    ...block(`${ROOT}/host/apple/Sources/ExactKit/B.swift`, 'warning', 'deprecated', 'DeprecatedDeclaration'),
    '[#DeprecatedDeclaration]: <https://docs.swift.org/compiler/documentation/diagnostics/deprecated-declaration>',
    ...block('/apps/mine/apple/Mine.swift', 'warning', 'mine unused'),
    ...block(`${ROOT}/host/apple/Sources/ExactKit/C.swift`, 'error', 'cannot find x'),
    '[#NoUsage]: <https://docs.swift.org/compiler/documentation/diagnostics/no-usage>',
    'Build complete! (5.53 sec)',
  ].join('\n');
  const out = quiet(output, `${ROOT}/`);
  expect(out).not.toContain('A.swift');
  expect(out).not.toContain('B.swift');
  expect(out).not.toContain('[#DeprecatedDeclaration]: <');
  expect(out).toContain('mine unused');
  expect(out).toContain('C.swift:72:91');
  expect(out).toContain('[#NoUsage]: <');
  expect(out.startsWith('[129 / 137] ExactKit\n/apps/mine/apple/Mine.swift')).toBe(true);
  expect(out.endsWith('Build complete! (5.53 sec)')).toBe(true);
});

test.skipIf(process.platform !== 'darwin')('share packaging reads the parent plist without joining JSON to the build summary', async () => {
  const { runApple } = await import('./build.mjs');
  const dir = mkdtempSync(resolve(tmpdir(), 'exact-share-quiet-'));
  const stdout = spyOn(process.stdout, 'write').mockImplementation(() => true);
  try {
    writeFileSync(resolve(dir, 'Info.plist'), plistFile({ CFBundleVersion: '42', UIDeviceFamily: [1, 2], CFBundleSupportedPlatforms: ['iPhoneSimulator'] }));
    const extension = packageShareExtension({
      app: { id: 'dev.exact.quiet', displayName: 'Quiet', manifest: { share_target: { action: '/share', params: { text: 'text' } } } },
      bundle: dir, directory: resolve(dir, 'build'), sdk: '/sdk', triple: 'fixture', group: 'group.dev.exact.quiet',
      entitlements: plistFile({}), sign() {},
      runApple: (command, args, options) => {
        if (command === 'plutil') return runApple(command, args, options);
        // Packaging inspects the compiler's output for required-reason API imports.
        if (args.includes('swiftc')) copyFileSync('/usr/bin/true', args[args.indexOf('-o') + 1]);
        return { status: 0 };
      },
    });
    expect(readFileSync(resolve(extension, 'Info.plist'), 'utf8')).toContain('<string>42</string>');
    process.stdout.write('host/apple: Quiet.app (--no-install)\n');
    expect(stdout.mock.calls.map(([text]) => text).join('')).toBe('host/apple: Quiet.app (--no-install)\n');
  } finally { stdout.mockRestore(); rmSync(dir, { recursive: true, force: true }); }
});

test('an app in the checkout keeps every warning', () => {
  const output = block(`${ROOT}/host/apple/Sources/ExactKit/A.swift`, 'warning', 'result unused').join('\n');
  quietFor({ dir: `${ROOT}/apps/caltrain` }, ROOT);
  expect(quiet(output)).toBe(output);
  quietFor({ dir: '/elsewhere/groceries' }, ROOT);
  expect(quiet(output)).toBe('');
});


test('captured Apple tool data leaves stdout clean without hiding diagnostics or refusals', async () => {
  const { runApple } = await import('./build.mjs');
  const stdout = spyOn(process.stdout, 'write').mockImplementation(() => true);
  const stderr = spyOn(process.stderr, 'write').mockImplementation(() => true);
  try {
    const output = () => stdout.mock.calls.map(([text]) => text).join('');
    const errors = () => stderr.mock.calls.map(([text]) => text).join('');
    const child = (source, options) => runApple(process.execPath, ['--eval', source], options);
    const result = child('process.stdout.write(JSON.stringify({group:"test"})); process.stderr.write("diagnostic\\n")', { read: true });
    process.stdout.write('host/apple: Fixture.app\n');
    expect(JSON.parse(result.stdout)).toEqual({ group: 'test' });
    expect(output()).toBe('host/apple: Fixture.app\n');
    expect(errors()).toBe('diagnostic\n');
    stdout.mockClear(); stderr.mockClear();
    child('console.log("build output")');
    expect(output()).toBe('build output\n');
    expect(() => child('console.error("tool failed"); process.exit(7)', { read: true })).toThrow('failed (7)');
    expect(errors()).toContain('tool failed');
    expect(() => child('console.log("object file fixture.o was built for newer macOS")', { read: true })).toThrow('Apple deployment target mismatch');
  } finally { stdout.mockRestore(); stderr.mockRestore(); }
});
