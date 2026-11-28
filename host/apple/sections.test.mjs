import { afterEach, expect, test } from 'bun:test';
import { mkdtempSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { tmpdir } from 'node:os';
import { resolve } from 'node:path';
import { ENTITLEMENTS_SECTION, fitEntitlements, fitLinkedSections, occurrencesOutsideLinked, readLinkedSections, reserveEntitlements, reserveLinkedPlan, reservedSize, textSections } from './sections.mjs';

const dirs = []; afterEach(() => { for (const d of dirs.splice(0)) rmSync(d, { recursive: true, force: true }); });
const temp = () => { const d = mkdtempSync(resolve(tmpdir(), 'sections-')); dirs.push(d); return d; };

/** A minimal thin arm64 Mach-O: __TEXT with __text and the three linked sections (each `room` bytes, `used` of them
 * its size), then __unwind_info (and, with `entitled`, a simulator's __entitlements after it); an LC_UUID. */
function macho(used = { plan: 10, compat: 4, bytecode: 8 }, room = 64, entitled = false) {
  const names = ['__text', '__exact_plan', '__exact_compat', '__exact_hbc', '__unwind_info', ...(entitled ? ['__entitlements'] : [])];
  const segment = 72 + names.length * 80, uuid = 24, commands = segment + uuid, start = 1024;
  const bytes = Buffer.alloc(start + names.length * room);
  bytes.writeUInt32LE(0xfeedfacf, 0); bytes.writeUInt32LE(0x0100000c, 4); bytes.writeUInt32LE(2, 12); bytes.writeUInt32LE(2, 16); bytes.writeUInt32LE(commands, 20);
  let at = 32;
  bytes.writeUInt32LE(0x19, at); bytes.writeUInt32LE(segment, at + 4); bytes.write('__TEXT', at + 8, 'latin1');
  bytes.writeBigUInt64LE(0n, at + 40); bytes.writeBigUInt64LE(BigInt(bytes.length), at + 48); bytes.writeUInt32LE(names.length, at + 64);
  names.forEach((name, i) => {
    const h = at + 72 + i * 80, offset = start + i * room, size = { __exact_plan: used.plan, __exact_compat: used.compat, __exact_hbc: used.bytecode, __entitlements: used.entitlements }[name] ?? room;
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

test('an app id counts only where a repack keeps the base\'s bytes: not in the linked sections, the code signature or the string table', () => {
  const id = 'com.example.base';
  // The fixture plus an LC_CODE_SIGNATURE whose blob, after __TEXT, names the id as a real signature does, and an
  // LC_SYMTAB whose string table names it in a debug-map object path, as a simulator build's does.
  const base = readFileSync(macho()), blob = Buffer.from(`\0\0${id}\0`), strings = Buffer.from(`\0/t/target/clients/k/${id}/ios-simulator/a.o\0`);
  const bytes = Buffer.concat([base, blob, strings]);
  const sizeofcmds = bytes.readUInt32LE(20), at = 32 + sizeofcmds;
  bytes.writeUInt32LE(0x1d, at); bytes.writeUInt32LE(16, at + 4); bytes.writeUInt32LE(base.length, at + 8); bytes.writeUInt32LE(blob.length, at + 12);
  bytes.writeUInt32LE(0x2, at + 16); bytes.writeUInt32LE(24, at + 20); bytes.writeUInt32LE(base.length + blob.length, at + 32); bytes.writeUInt32LE(strings.length, at + 36);
  bytes.writeUInt32LE(4, 16); bytes.writeUInt32LE(sizeofcmds + 40, 20);
  const sections = textSections(bytes), put = (name, skip = 0) => { const s = sections.find(x => x.name === name); bytes.write(id, s.offset + skip, 'utf8'); };
  put('__exact_plan'); put('__exact_compat', 20); put('__exact_hbc', 40); // the id in each linked section, even in its reserve
  const exe = resolve(temp(), 'App'); writeFileSync(exe, bytes);
  expect(occurrencesOutsideLinked(exe, id)).toBe(0);
  expect(occurrencesOutsideLinked(exe, 'com.example.other')).toBe(0);
  // Compiled in: a literal in __text (rust_entry's `.named("<app.id>")`, 6c2c8bda22), and one in __unwind_info.
  put('__text', 3); put('__unwind_info'); writeFileSync(exe, bytes);
  expect(occurrencesOutsideLinked(exe, id)).toBe(2);
  expect(occurrencesOutsideLinked(exe, '')).toBe(0);
});

// A simulator executable's entitlements (simulator Security reads them by the section's size): linked with room, then
// written for the app, so a repack onto another app's base gives this app's id, and the id there is not compiled code.
test('a simulator\'s entitlements are linked padded to their room, then fitted, and are no compiled-in app id', () => {
  const dir = temp(), plist = resolve(dir, 'ExactIOS-entitlements.plist');
  writeFileSync(plist, '<plist>SIMULATORX.com.example.base</plist>');
  const reserved = reserveEntitlements(plist);
  expect(reserved).toBe(`${plist}.reserved`);
  const padded = readFileSync(reserved);
  expect(padded.length).toBe(65536); expect(padded.subarray(0, 42).toString()).toBe('<plist>SIMULATORX.com.example.base</plist>'); expect(padded.subarray(42).every(b => b === 0)).toBe(true);
  const exe = macho({ plan: 10, compat: 4, bytecode: 8, entitlements: 30 }, 64, true), before = readFileSync(exe);
  const other = '<plist>SIMULATORX.com.example.app</plist>';
  expect(fitEntitlements(exe, other)).toEqual({ entitlements: { bytes: other.length, room: 64 } });
  expect(readLinkedSections(exe, ENTITLEMENTS_SECTION).entitlements.toString()).toBe(other);
  // The plan's sections are untouched.
  const plan = textSections(before).find(x => x.name === '__exact_plan');
  expect(readFileSync(exe).subarray(plan.offset, plan.offset + plan.room)).toEqual(before.subarray(plan.offset, plan.offset + plan.room));
  let error; try { fitEntitlements(exe, Buffer.alloc(65)); } catch (e) { error = e; }
  expect(error.code).toBe('EXACT_SECTION_ROOM');
  expect(occurrencesOutsideLinked(exe, 'com.example.app')).toBe(0);
});
