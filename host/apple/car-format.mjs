// BOM storage and CoreUI integer encodings shared by the asset writers.
const words = (values, width = 4, endian = 'LE') => {
  const buffer = Buffer.alloc(values.length * width);
  values.forEach((value, i) => buffer[`writeUInt${width * 8}${endian}`](value, i * width));
  return buffer;
};
export const le = (...values) => words(values), be = (...values) => words(values, 4, 'BE'), u16 = (...values) => words(values, 2);
export const fixed = (text, length) => { const bytes = Buffer.alloc(length); bytes.write(text, 0, length - 1); return bytes; };

export class BOM {
  blocks = [Buffer.alloc(0)]; variables = [];
  block(bytes) { this.blocks.push(bytes); return this.blocks.length - 1; }
  variable(name, bytes) { this.variables.push([name, this.block(bytes)]); }
  tree(name, entries, inline = false, blockSize = 4096) {
    entries.sort((a, b) => inline ? a[0] - b[0] : Buffer.compare(a[0], b[0]));
    const lengths = new Set(entries.map(([key]) => inline ? 0 : key.length));
    const keySize = inline ? 0 : lengths.size === 1 ? [...lengths][0] : 0xffffffff;
    const cached = !inline && keySize !== 0xffffffff;
    let level = entries.map(([key, value]) => ({ key, value: this.block(value), keyID: inline ? key : this.block(key) })), leaf = true;
    const capacity = Math.floor((blockSize - 16) / (8 + (cached ? keySize : 0)));
    if (capacity < 2) throw new Error(`host/apple: asset key is too large for ${name}`);
    do {
      const nodes = [];
      for (let offset = 0; offset < level.length || !nodes.length; offset += capacity) {
        const members = level.slice(offset, offset + capacity), bytes = Buffer.alloc(blockSize + (cached ? keySize : 0));
        bytes.writeUInt16BE(Number(leaf)); bytes.writeUInt16BE(members.length, 2);
        members.forEach((member, i) => be(member.value, member.keyID).copy(bytes, 12 + i * 8));
        // Fixed-width keys have a contiguous lookup cache after the entry table
        // and four reserved bytes. Enumeration alone never exercises it.
        if (cached) members.forEach((member, i) => member.key.copy(bytes, 16 + members.length * 8 + i * keySize));
        nodes.push({ bytes, value: this.block(bytes), key: members.at(-1)?.key, keyID: members.at(-1)?.keyID });
      }
      nodes.forEach((node, i) => be(nodes[i + 1]?.value ?? 0, nodes[i - 1]?.value ?? 0).copy(node.bytes, 4));
      level = nodes; leaf = false;
    } while (level.length > 1);
    this.variable(name, Buffer.concat([Buffer.from('tree'), be(1, level[0].value, blockSize, entries.length), Buffer.from([Number(inline)]), be(keySize, 0)]));
  }
  finish() {
    const header = Buffer.alloc(512); header.write('BOMStore'); be(1, this.blocks.length - 1).copy(header, 8);
    let offset = header.length;
    const index = [[0, 0], ...this.blocks.slice(1).map(bytes => { const entry = [offset, bytes.length]; offset += bytes.length; return entry; })];
    const capacity = Math.ceil(index.length / 256) * 256;
    while (index.length < capacity) index.push([0, 0]);
    const table = Buffer.concat([be(capacity), ...index.map(entry => be(...entry))]);
    const variables = Buffer.concat([be(this.variables.length), ...this.variables.flatMap(([name, id]) => [be(id), Buffer.from([Buffer.byteLength(name)]), Buffer.from(name)])]);
    be(offset, table.length, offset + table.length, variables.length).copy(header, 16);
    return Buffer.concat([header, ...this.blocks.slice(1), table, variables]);
  }
}
