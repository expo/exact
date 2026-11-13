import { afterEach, expect, test } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { fitLinkedSections, readLinkedSections, reserveLinkedPlan, reservedSize, textSections } from './sections.mjs';

const dirs = []; afterEach(() => { for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true }); });
const temp = () => { const d = mkdtempSync(resolve(tmpdir(), 'sections-')); dirs.push(d); return d; };

/** A minimal thin arm64 Mach-O: __TEXT with __text and the three linked sections (each `room` bytes, `used` of them
 * its size), then __unwind_info; an LC_UUID. */
function macho(used = { plan: 10, compat: 4, bytecode: 8 }, room = 64) {
  const names = ['__text', '__exact_plan', '__exact_compat', '__exact_hbc', '__unwind_info'];
  const segment = 72 + names.length * 80, uuid = 24, commands = segment + uuid, start = 1024;
  const bytes = Buffer.alloc(start + names.length * room);
  bytes.writeUInt32LE(0xfeedfacf, 0); bytes.writeUInt32LE(0x0100000c, 4); bytes.writeUInt32LE(2, 12); bytes.writeUInt32LE(2, 16); bytes.writeUInt32LE(commands, 20);
  let at = 32;
  bytes.writeUInt32LE(0x19, at); bytes.writeUInt32LE(segment, at + 4); bytes.write('__TEXT', at + 8, 'latin1');
  bytes.writeBigUInt64LE(0n, at + 40); bytes.writeBigUInt64LE(BigInt(bytes.length), at + 48); bytes.writeUInt32LE(names.length, at + 64);
  names.forEach((name, i) => {
    const h = at + 72 + i * 80, offset = start + i * room, size = { __exact_plan: used.plan, __exact_compat: used.compat, __exact_hbc: used.bytecode }[name] ?? room;
    bytes.write(name, h, 'latin1'); bytes.write('__TEXT', h + 16, 'latin1'); bytes.writeBigUInt64LE(BigInt(size), h + 40); bytes.writeUInt32LE(offset, h + 48);
    bytes.fill(i + 1, offset, offset + size);
  });
  at += segment;
  bytes.writeUInt32LE(0x1b, at); bytes.writeUInt32LE(uuid, at + 4); bytes.fill(0xab, at + 8, at + 24);
  const path = resolve(temp(), 'App'); writeFileSync(path, bytes); return path;
}

test('the linked sections take new bytes up to their room, and their sizes become those bytes', () => {
  const exe = macho(), before = readFileSync(exe);
  expect(textSections(before).find(s => s.name === '__exact_plan')).toMatchObject({ size: 10, room: 64 });
  const next = { plan: Buffer.from('a new, longer plan'), compat: Buffer.from('{"id":"x"}'), bytecode: Buffer.alloc(64, 7) };
  expect(fitLinkedSections(exe, next)).toEqual({ plan: { bytes: 18, room: 64 }, compat: { bytes: 10, room: 64 }, bytecode: { bytes: 64, room: 64 } });
  expect(readLinkedSections(exe)).toEqual(next);
  const after = readFileSync(exe);
  // Only the three sections' bytes and size fields moved: __text, __unwind_info and the UUID are as they were.
  for (const name of ['__text', '__unwind_info']) { const s = textSections(before).find(x => x.name === name); expect(after.subarray(s.offset, s.offset + s.room)).toEqual(before.subarray(s.offset, s.offset + s.room)); }
  expect(after.subarray(after.length - 64)).toEqual(before.subarray(before.length - 64));
  const plan = textSections(after).find(s => s.name === '__exact_plan');
  expect(after.subarray(plan.offset + 18, plan.offset + plan.room).every(b => b === 0)).toBe(true);
});

test('bytes past a section\'s room are refused, and nothing is written', () => {
  const exe = macho(), before = readFileSync(exe);
  let error; try { fitLinkedSections(exe, { plan: Buffer.alloc(1), compat: Buffer.alloc(1), bytecode: Buffer.alloc(65) }); } catch (e) { error = e; }
  expect(error.code).toBe('EXACT_SECTION_ROOM'); expect(error.message).toContain('__exact_hbc needs 65 bytes');
  expect(readFileSync(exe)).toEqual(before);
});

test('renewing the UUID derives it from the old one and the new sections, the same way every time', () => {
  const contents = { plan: Buffer.from('p'), compat: Buffer.from('c'), bytecode: Buffer.from('b') };
  const uuid = (exe) => { const b = readFileSync(exe), at = 32 + 72 + 5 * 80; return b.subarray(at + 8, at + 24).toString('hex'); };
  const one = macho(), two = macho(), other = macho();
  fitLinkedSections(one, contents, { renewUuid: true }); fitLinkedSections(two, contents, { renewUuid: true }); fitLinkedSections(other, { ...contents, plan: Buffer.from('q') }, { renewUuid: true });
  expect(uuid(one)).toBe(uuid(two)); expect(uuid(one)).not.toBe(uuid(other)); expect(uuid(one)).not.toBe('ab'.repeat(16));
  expect(uuid(one)[12]).toBe('5'); // a name-based UUID
});

test('a link takes each file padded to its reserved room, rewritten only when it changes', () => {
  const dir = temp();
  writeFileSync(resolve(dir, 'app.plan'), Buffer.alloc(300_000, 1)); writeFileSync(resolve(dir, 'compat.json'), '{"id":"x"}'); writeFileSync(resolve(dir, 'app.hbc'), Buffer.alloc(10, 2));
  const out = reserveLinkedPlan(dir);
  expect(readFileSync(resolve(out, 'app.plan')).length).toBe(reservedSize(300_000));
  expect(reservedSize(300_000)).toBeGreaterThanOrEqual(300_000 + (1 << 20));
  // One room for every bake up to the minimum: one layout for apps whose plans differ.
  expect(reservedSize(10)).toBe(reservedSize(300_000)); expect(reservedSize(3 << 20)).toBe(6 << 20);
  expect(readFileSync(resolve(out, 'compat.json')).length).toBe(reservedSize(10, 65536));
  expect(readFileSync(resolve(out, 'app.plan')).subarray(0, 300_000).every(b => b === 1)).toBe(true);
});

