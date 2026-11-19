import { expect, test } from 'bun:test';
import { spawnSync } from 'node:child_process';
import { mkdirSync, mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { renameZipPrefix } from './zip-rename.mjs';

test('a prefix is renamed with every entry\'s compressed bytes and contents kept, and the copy is a valid ZIP', () => {
  const dir = mkdtempSync(resolve(tmpdir(), 'zip-rename-'));
  try {
    mkdirSync(resolve(dir, 'Payload/Old.app/Frameworks'), { recursive: true });
    writeFileSync(resolve(dir, 'Payload/Old.app/Old'), Buffer.alloc(200_000, 7));
    writeFileSync(resolve(dir, 'Payload/Old.app/Frameworks/a.txt'), 'framework');
    writeFileSync(resolve(dir, 'other.txt'), 'outside');
    expect(spawnSync('zip', ['-q', '-r', '-X', resolve(dir, 'in.ipa'), 'Payload', 'other.txt'], { cwd: dir }).status).toBe(0);
    renameZipPrefix(resolve(dir, 'in.ipa'), resolve(dir, 'out.ipa'), 'Payload/Old.app/', 'Payload/New Name.app/');
    expect(spawnSync('unzip', ['-tq', resolve(dir, 'out.ipa')]).status).toBe(0);
    const listed = spawnSync('unzip', ['-Z1', resolve(dir, 'out.ipa')], { encoding: 'utf8' }).stdout.trim().split('\n').sort();
    expect(listed).toEqual(['Payload/', 'Payload/New Name.app/', 'Payload/New Name.app/Frameworks/', 'Payload/New Name.app/Frameworks/a.txt', 'Payload/New Name.app/Old', 'other.txt']);
    expect(spawnSync('unzip', ['-q', resolve(dir, 'out.ipa'), '-d', resolve(dir, 'x')]).status).toBe(0);
    expect(readFileSync(resolve(dir, 'x/Payload/New Name.app/Old')).equals(Buffer.alloc(200_000, 7))).toBe(true);
    expect(readFileSync(resolve(dir, 'x/Payload/New Name.app/Frameworks/a.txt'), 'utf8')).toBe('framework');
    // Not a ZIP: refused, so the caller archives in full.
    writeFileSync(resolve(dir, 'bad.ipa'), 'not a zip');
    expect(() => renameZipPrefix(resolve(dir, 'bad.ipa'), resolve(dir, 'o2.ipa'), 'a', 'b')).toThrow();
  } finally { rmSync(dir, { recursive: true, force: true }); }
});
