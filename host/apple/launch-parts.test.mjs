import { expect, test } from 'bun:test';
import { mkdirSync, mkdtempSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { auditLaunchParts, launchPartsObjects } from './launch-parts.mjs';

const object = import.meta.filename;
const modules = [{ name: 'observe' }];
const size = { status: 0, stdout: '__TEXT __DATA\n21083 936\n' };
const commands = { status: 0, stdout: 'cmd LC_SEGMENT_64\nsectname __text\n' };

test('launch-part inspection refuses failed or unreadable tool results', () => {
  for (const bad of [
    { status: 1, stdout: size.stdout },
    { status: null, stdout: '' },
    { status: 0, stdout: 'unreadable' },
    { status: 0, stdout: '__TEXT __DATA\nNaN 936\n' },
    { status: 0, stdout: '__TEXT __DATA\n-1 936\n' },
  ]) {
    expect(() => auditLaunchParts(object, modules, tool => tool === 'xcrun' ? bad : commands)).toThrow('size inspection failed');
  }
  for (const bad of [{ status: 1, stdout: commands.stdout }, { status: 0, stdout: '' }]) {
    expect(() => auditLaunchParts(object, modules, tool => tool === 'xcrun' ? size : bad)).toThrow('load-command inspection failed');
  }
});

test('launch-part inspection accepts the byte limit and reports all forbidden code', () => {
  expect(() => auditLaunchParts(object, modules, tool => tool === 'xcrun'
    ? { status: 0, stdout: '__TEXT __DATA\n32768 0\n' } : commands)).not.toThrow();
  const inspect = tool => tool === 'xcrun'
    ? { status: 0, stdout: '__TEXT __DATA\n32768 1\n' }
    : { status: 0, stdout: `${commands.stdout}sectname __mod_init_func\nsectname __objc_nlclslist\n` };
  for (const problem of ['32 KiB budget', 'static initializer', 'Objective-C +load']) {
    expect(() => auditLaunchParts(object, modules, inspect)).toThrow(problem);
  }
  expect(() => auditLaunchParts(object, modules, tool => tool === 'xcrun' ? size
    : { status: 0, stdout: `${commands.stdout}sectname __init_offsets\n` })).toThrow('static initializer');
});

test('the launch parts are found in either SwiftPM layout, and missing ones cannot be audited', () => {
  const bin = mkdtempSync(resolve(tmpdir(), 'exact-launch-parts-'));
  try {
    expect(launchPartsObjects(bin)).toEqual([]);
    expect(() => auditLaunchParts(launchPartsObjects(bin), modules, () => size)).toThrow('cannot be audited: SwiftPM left no ExactLaunchParts object');
    // The native build system (a Linux cross build): one object per source. A removed or renamed
    // source's object, or another app's, stays in the directory; only the output-file map's count.
    const dir = resolve(bin, 'ExactLaunchParts.build');
    mkdirSync(dir);
    for (const f of ['Registry.swift.o', 'EighteenLaunch.swift.o', 'Removed.swift.o', 'ExactLaunchParts.swiftmodule']) writeFileSync(resolve(dir, f), '');
    expect(launchPartsObjects(bin)).toEqual([]);
    writeFileSync(resolve(dir, 'output-file-map.json'), JSON.stringify({
      '': { 'swift-dependencies': resolve(dir, 'primary.swiftdeps') },
      '/pkg/.launch-parts/app/Registry.swift': { object: resolve(dir, 'Registry.swift.o'), dependencies: resolve(dir, 'Registry.d') },
      '/pkg/.launch-parts/app/EighteenLaunch.swift': { object: resolve(dir, 'EighteenLaunch.swift.o') },
    }));
    expect(launchPartsObjects(bin)).toEqual(['EighteenLaunch.swift.o', 'Registry.swift.o'].map(f => resolve(dir, f)));
    // A whole-module build in one job writes the '' entry's object instead.
    writeFileSync(resolve(dir, 'output-file-map.json'), JSON.stringify({
      '': { object: resolve(dir, 'ExactLaunchParts.o') }, '/pkg/.launch-parts/app/Renamed.swift': { object: resolve(dir, 'Renamed.swift.o') },
    }));
    expect(launchPartsObjects(bin)).toEqual([resolve(dir, 'Renamed.swift.o')]);
    writeFileSync(resolve(dir, 'ExactLaunchParts.o'), '');
    expect(launchPartsObjects(bin)).toEqual([resolve(dir, 'ExactLaunchParts.o')]);
    // Swift Build: the target linked into one object, which wins.
    writeFileSync(resolve(bin, 'ExactLaunchParts.o'), '');
    expect(launchPartsObjects(bin)).toEqual([resolve(bin, 'ExactLaunchParts.o')]);
  } finally { rmSync(bin, { recursive: true, force: true }); }
  expect(() => auditLaunchParts([object, `${object}.missing`], modules, () => size)).toThrow(`${object}.missing is missing`);
});

test('the budget and the forbidden code count across every launch-parts object', () => {
  const half = { status: 0, stdout: '__TEXT __DATA\n16384 0\n' };
  expect(() => auditLaunchParts([object, object], modules, tool => tool === 'xcrun' ? half : commands)).not.toThrow();
  const over = { status: 0, stdout: '__TEXT __DATA\n16384 1\n' };
  expect(() => auditLaunchParts([object, object], modules, tool => tool === 'xcrun' ? over : commands)).toThrow('32770 bytes');
  let n = 0;
  const second = tool => tool === 'xcrun' ? size : (n++ % 2 ? { status: 0, stdout: `${commands.stdout}sectname __mod_init_func\n` } : commands);
  expect(() => auditLaunchParts([object, object], modules, second)).toThrow('static initializer');
  let m = 0;
  expect(() => auditLaunchParts([object, object], modules, tool => tool === 'xcrun' ? (m++ ? { status: 1, stdout: '' } : size) : commands)).toThrow('size inspection failed');
});
