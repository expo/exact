// A copy of a ZIP archive with one path prefix renamed, every entry's compressed bytes kept as they are.
//
// repack.mjs uses it when the app's bundle is named differently from its base's (a shared template base: the bundle and
// executable are named after the app), so the base IPA's frameworks are not compressed again. Plain ZIPs only (an IPA
// under 4 GiB, as zip writes it); anything else throws, and the caller archives the bundle in full.
import { readFileSync, writeFileSync } from 'node:fs';

const EOCD = 0x06054b50, CENTRAL = 0x02014b50, LOCAL = 0x04034b50;

/** Writes `dst`: `src` with each entry whose name starts with `from` named `to` + the rest instead. */
export function renameZipPrefix(src, dst, from, to) {
  const zip = readFileSync(src);
  let end = -1;
  for (let at = zip.length - 22; at >= Math.max(0, zip.length - 22 - 0xffff); at--) if (zip.readUInt32LE(at) === EOCD) { end = at; break; }
  if (end < 0) throw new Error('not a ZIP archive (no end of central directory)');
  const count = zip.readUInt16LE(end + 10), size = zip.readUInt32LE(end + 12), offset = zip.readUInt32LE(end + 16);
  if (count === 0xffff || size === 0xffffffff || offset === 0xffffffff || zip.readUInt16LE(end + 4) !== 0) throw new Error('a ZIP64 or multi-disk archive');
  const locals = [], centrals = [];
  let written = 0;
  for (let at = offset, i = 0; i < count; i++) {
    if (zip.readUInt32LE(at) !== CENTRAL) throw new Error('a malformed central directory');
    const flags = zip.readUInt16LE(at + 8), csize = zip.readUInt32LE(at + 20), nameLength = zip.readUInt16LE(at + 28), extraLength = zip.readUInt16LE(at + 30), commentLength = zip.readUInt16LE(at + 32), local = zip.readUInt32LE(at + 42);
    if (csize === 0xffffffff || local === 0xffffffff) throw new Error('a ZIP64 entry');
    const name = zip.subarray(at + 46, at + 46 + nameLength), next = at + 46 + nameLength + extraLength + commentLength;
    if (zip.readUInt32LE(local) !== LOCAL) throw new Error('a malformed local header');
    const localName = zip.readUInt16LE(local + 26), localExtra = zip.readUInt16LE(local + 28), data = local + 30 + localName + localExtra;
    const text = name.toString('utf8'), renamed = Buffer.from(text.startsWith(from) ? to + text.slice(from.length) : text, 'utf8');
    // The local header with the central directory's sizes and CRC, so a data descriptor (flag bit 3) is not needed.
    const header = Buffer.alloc(30);
    zip.copy(header, 0, local, local + 30);
    header.writeUInt16LE(flags & ~0x0008, 6);
    header.writeUInt32LE(zip.readUInt32LE(at + 16), 14); header.writeUInt32LE(csize, 18); header.writeUInt32LE(zip.readUInt32LE(at + 24), 22);
    header.writeUInt16LE(renamed.length, 26); header.writeUInt16LE(localExtra, 28);
    const localExtraBytes = zip.subarray(local + 30 + localName, data);
    const central = Buffer.concat([zip.subarray(at, at + 46), renamed, zip.subarray(at + 46 + nameLength, next)]);
    central.writeUInt16LE(flags & ~0x0008, 8); central.writeUInt16LE(renamed.length, 28); central.writeUInt32LE(written, 42);
    locals.push(header, renamed, localExtraBytes, zip.subarray(data, data + csize));
    written += 30 + renamed.length + localExtraBytes.length + csize;
    if (written > 0xfffffff0) throw new Error('the archive is too large for a plain ZIP');
    centrals.push(central);
    at = next;
  }
  const directory = Buffer.concat(centrals), tail = Buffer.from(zip.subarray(end, end + 22 + zip.readUInt16LE(end + 20)));
  tail.writeUInt32LE(directory.length, 12); tail.writeUInt32LE(written, 16);
  writeFileSync(dst, Buffer.concat([...locals, directory, tail]));
}
