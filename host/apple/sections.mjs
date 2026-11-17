// The linked plan's sections, written after the link.
//
// The link carries the plan, its compat.json and the module's bytecode in __TEXT (scripts/linked-plan.mjs;
// host/apple/src/baked.rs reads them with getsectiondata). __TEXT's later sections (__unwind_info, __eh_frame) and
// the segments after it cannot move without a link, so a section can take new bytes only where it already has room.
// A product linked with reserved room (`reserveLinkedPlan`) takes the bake's bytes afterwards (`fitLinkedSections`):
// the section's size is the bytes, and the rest of its room is zeros no reader sees. A link whose inputs are then
// the same for every plan of a size (the shared widget core) need not run again when only the plan changes.
import { createHash } from 'node:crypto';
import { closeSync, mkdirSync, openSync, readFileSync, readSync, renameSync, statSync, writeFileSync, writeSync } from 'node:fs';
import { resolve } from 'node:path';

export const LINKED_SECTIONS = { plan: '__exact_plan', compat: '__exact_compat', bytecode: '__exact_hbc' };
const FILES = { plan: 'app.plan', compat: 'compat.json', bytecode: 'app.hbc' };
const MH_MAGIC_64 = 0xfeedfacf, LC_SEGMENT_64 = 0x19, LC_UUID = 0x1b;

/** The room a section is linked with: twice its bytes, and never less than twice `minimum`, in 16 KiB pages. Every
 * bake up to `minimum` gets the same room, so apps that differ only in their plans link with one layout (a repack onto
 * another app's base is then byte for byte what that app's full build links). */
export const reservedSize = (bytes, minimum = 1 << 20) => Math.ceil(Math.max(2 * bytes, 2 * minimum) / 16384) * 16384;

/** Copies of a linked bake directory's three files, each padded with zeros to its reserved room, in `dir/reserved`; the
 * directory to link from (linkedPlanArgs). A file is rewritten only when its bytes change, so an unchanged plan keeps
 * the link's inputs as they were. The room depends only on the size, rounded, so a small edit keeps the same layout. */
export function reserveLinkedPlan(dir) {
  const out = resolve(dir, 'reserved');
  mkdirSync(out, { recursive: true });
  for (const [kind, file] of Object.entries(FILES)) {
    const bytes = readFileSync(resolve(dir, file)), padded = Buffer.alloc(reservedSize(bytes.length, kind === 'compat' ? 65536 : 1 << 20));
    bytes.copy(padded);
    const path = resolve(out, file);
    try { if (statSync(path).size === padded.length && readFileSync(path).equals(padded)) continue; } catch { /* none yet */ }
    writeFileSync(`${path}.${process.pid}.tmp`, padded);
    renameSync(`${path}.${process.pid}.tmp`, path);
  }
  return out;
}

/** The __TEXT sections of a thin 64-bit Mach-O, in file order: name, file offset, size, and where its header's size
 * field is. A universal (fat) file is refused: an iOS device archive has one arm64 slice. */
export function textSections(bytes) {
  if (bytes.length < 32 || bytes.readUInt32LE(0) !== MH_MAGIC_64) throw new Error('not a thin 64-bit Mach-O');
  const ncmds = bytes.readUInt32LE(16), sizeofcmds = bytes.readUInt32LE(20), sections = [];
  let at = 32; const end = 32 + sizeofcmds;
  if (end > bytes.length) throw new Error('Mach-O load commands run past the file');
  let text = null;
  for (let i = 0; i < ncmds; i++) {
    if (at + 8 > end) throw new Error('Mach-O load command runs past its table');
    const cmd = bytes.readUInt32LE(at), size = bytes.readUInt32LE(at + 4);
    if (size < 8 || at + size > end) throw new Error('invalid Mach-O load command size');
    if (cmd === LC_SEGMENT_64 && bytes.toString('latin1', at + 8, at + 24).replace(/\0+$/, '') === '__TEXT') {
      const fileoff = Number(bytes.readBigUInt64LE(at + 40)), filesize = Number(bytes.readBigUInt64LE(at + 48)), nsects = bytes.readUInt32LE(at + 64);
      if (72 + nsects * 80 > size) throw new Error('__TEXT names more sections than its command holds');
      text = { fileoff, filesize };
      for (let s = 0; s < nsects; s++) {
        const h = at + 72 + s * 80;
        sections.push({ name: bytes.toString('latin1', h, h + 16).replace(/\0+$/, ''), offset: bytes.readUInt32LE(h + 48), size: Number(bytes.readBigUInt64LE(h + 40)), header: h, flags: bytes.readUInt32LE(h + 64) });
      }
    }
    at += size;
  }
  if (!text) throw new Error('Mach-O has no __TEXT segment');
  sections.sort((a, b) => a.offset - b.offset);
  // Each section's room: up to the next section, or to the end of __TEXT for the last.
  sections.forEach((s, i) => { s.room = (sections[i + 1]?.offset ?? text.fileoff + text.filesize) - s.offset; });
  return sections;
}

/** Each linked section's current bytes. */
export function readLinkedSections(executable) {
  // The load commands, then each section's bytes: never the whole (tens of megabytes) executable.
  const fd = openSync(executable, 'r');
  try {
    const head = Buffer.alloc(32); readSync(fd, head, 0, 32, 0);
    if (head.readUInt32LE(0) !== MH_MAGIC_64) throw new Error(`${executable} is not a thin 64-bit Mach-O`);
    const commands = Buffer.alloc(32 + head.readUInt32LE(20)); readSync(fd, commands, 0, commands.length, 0);
    const sections = textSections(commands), out = {};
    for (const [kind, name] of Object.entries(LINKED_SECTIONS)) {
      const s = sections.find(item => item.name === name);
      if (!s) throw new Error(`${executable} has no __TEXT,${name}`);
      out[kind] = Buffer.alloc(s.size); readSync(fd, out[kind], 0, s.size, s.offset);
    }
    return out;
  } finally { closeSync(fd); }
}

/** Write `contents` ({plan, compat, bytecode}: Buffers) into the executable's linked sections, in place: each section's
 * size becomes its new bytes, and the rest of its room is zeroed. Throws, changing nothing, when one does not fit.
 * The executable's code signature no longer matches; the caller signs it again. Returns each section's bytes and room. */
export function fitLinkedSections(executable, contents, { renewUuid = false } = {}) {
  const fd = openSync(executable, 'r+');
  try {
    const head = Buffer.alloc(32); readSync(fd, head, 0, 32, 0);
    if (head.readUInt32LE(0) !== MH_MAGIC_64) throw new Error(`${executable} is not a thin 64-bit Mach-O`);
    const commands = Buffer.alloc(32 + head.readUInt32LE(20)); readSync(fd, commands, 0, commands.length, 0);
    const sections = textSections(commands);
    const plan = Object.entries(LINKED_SECTIONS).map(([kind, name]) => {
      const s = sections.find(item => item.name === name), bytes = contents[kind];
      if (!s) throw new Error(`${executable} has no __TEXT,${name}`);
      if (!Buffer.isBuffer(bytes)) throw new Error(`no new bytes for __TEXT,${name}`);
      // Every byte from the section's start to the next section's belongs to it alone (its reserve, then alignment).
      const room = s.room;
      if (bytes.length > room) throw Object.assign(new Error(`__TEXT,${name} needs ${bytes.length} bytes; the base executable has room for ${room}`), { code: 'EXACT_SECTION_ROOM' });
      return { s, bytes, room, kind };
    });
    for (const { s, bytes, room } of plan) {
      const filled = Buffer.alloc(room); bytes.copy(filled);
      writeSync(fd, filled, 0, room, s.offset);
      const size = Buffer.alloc(8); size.writeBigUInt64LE(BigInt(bytes.length));
      writeSync(fd, size, 0, 8, s.header + 40);
    }
    // A repack's executable is a new binary: its own UUID (what symbolication and the system's caches key by), derived
    // from the base's and the new sections, never the base's. The linker's own derivation needs the whole link.
    if (renewUuid) {
      const at = loadCommand(commands, LC_UUID);
      if (at === null) throw new Error(`${executable} has no LC_UUID`);
      const hash = createHash('sha256').update(commands.subarray(at + 8, at + 24));
      for (const { bytes } of plan) hash.update(createHash('sha256').update(bytes).digest());
      const uuid = hash.digest().subarray(0, 16);
      uuid[6] = (uuid[6] & 0x0f) | 0x50; uuid[8] = (uuid[8] & 0x3f) | 0x80; // an RFC 4122 name-based UUID's version and variant
      writeSync(fd, uuid, 0, 16, at + 8);
    }
    return Object.fromEntries(plan.map(({ kind, bytes, room }) => [kind, { bytes: bytes.length, room }]));
  } finally { closeSync(fd); }
}


/** The offset of the first load command `cmd` in a Mach-O's header bytes, or null. */
function loadCommand(bytes, cmd) {
  let at = 32;
  for (let i = 0, n = bytes.readUInt32LE(16); i < n; i++) { if (bytes.readUInt32LE(at) === cmd) return at; at += bytes.readUInt32LE(at + 4); }
  return null;
}
