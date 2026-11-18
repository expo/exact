// Bun 1.4.2; glTF-Transform CLI 4.2.1 (SDK 4.5.1); meshoptimizer 0.25.0.
// Run: bun packages/model-viewer/test/fixture/generate.mjs
import { mkdirSync, writeFileSync } from 'node:fs';
import { join } from 'node:path';
import { spawnSync } from 'node:child_process';

const assets = join(import.meta.dir, 'assets');
mkdirSync(assets, { recursive: true });
const model = {
  asset: { version: '2.0', generator: 'Exact model-viewer assembly fixture' },
  scene: 0, scenes: [{ nodes: [0] }], nodes: [], meshes: [],
  materials: [
    { name: 'Housing', pbrMetallicRoughness: { baseColorFactor: [0.4, 0.5, 0.6, 0.2] } },
    { name: 'Impeller', pbrMetallicRoughness: { baseColorFactor: [0.8, 0.2, 0.1, 1] } },
    { name: 'Glass', alphaMode: 'BLEND', pbrMetallicRoughness: { baseColorFactor: [0.1, 0.4, 0.8, 0.35] } },
    { name: 'Cutout', alphaMode: 'MASK', alphaCutoff: 0.6, pbrMetallicRoughness: { baseColorFactor: [0.2, 0.8, 0.3, 0.4] } },
  ],
  buffers: [], bufferViews: [], accessors: [],
};
const chunks = [];
let length = 0;
function accessor(values, type, components) {
  const data = type === 5126 ? new Float32Array(values) : new Uint16Array(values);
  const view = model.bufferViews.push({ buffer: 0, byteOffset: length, byteLength: data.byteLength }) - 1;
  chunks.push(new Uint8Array(data.buffer));
  length += data.byteLength;
  const pad = (4 - length % 4) % 4;
  chunks.push(new Uint8Array(pad));
  length += pad;
  const a = { bufferView: view, componentType: type, count: values.length / components, type: components === 3 ? 'VEC3' : 'SCALAR' };
  if (components === 3) {
    a.min = [0, 1, 2].map(k => Math.min(...values.filter((_, i) => i % 3 === k)));
    a.max = [0, 1, 2].map(k => Math.max(...values.filter((_, i) => i % 3 === k)));
  }
  return model.accessors.push(a) - 1;
}
function box(lo, hi, material) {
  const positions = [], normals = [], indices = [];
  for (const [axis, sign, u, v] of [[0, -1, 2, 1], [0, 1, 1, 2], [1, -1, 0, 2], [1, 1, 2, 0], [2, -1, 1, 0], [2, 1, 0, 1]]) {
    const base = positions.length / 3;
    for (const [a, b] of [[0, 0], [1, 0], [1, 1], [0, 1]]) {
      const p = [0, 0, 0], n = [0, 0, 0];
      p[axis] = sign < 0 ? lo[axis] : hi[axis];
      p[u] = a ? hi[u] : lo[u];
      p[v] = b ? hi[v] : lo[v];
      n[axis] = sign;
      positions.push(...p);
      normals.push(...n);
    }
    indices.push(base, base + 1, base + 2, base, base + 2, base + 3);
  }
  const primitive = { attributes: { POSITION: accessor(positions, 5126, 3), NORMAL: accessor(normals, 5126, 3) }, indices: accessor(indices, 5123, 1) };
  if (material !== undefined) primitive.material = material;
  return primitive;
}
function mesh(name, primitives) { return model.meshes.push({ name, primitives }) - 1; }
model.nodes = [
  { name: 'Assembly', children: [1, 2, 3, 4, 5, 6, 7, 8, 9] },
  { name: 'Housing', mesh: mesh('Shell', [box([-1, -1, -0.7], [1, 1, 0.7], 0)]) },
  { name: 'Impeller', mesh: mesh('Rotor', [box([-0.7, -0.6, -0.15], [0.7, 0.6, 0.15], 1)]) },
  { name: 'Shaft', mesh: mesh('Axle', [box([-0.1, -0.1, -1], [0.1, 0.1, 1])]) },
  { mesh: mesh('Bracket mesh name is not the node name', [box([-0.3, -0.1, -0.2], [0.3, 0.1, 0.2])]), translation: [0, -1.2, 0] },
  { name: 'Plate', mesh: mesh('Plate', [box([-0.6, -0.3, 0.8], [-0.1, 0.3, 0.9], 0)]) },
  { name: 'Bolt A', mesh: mesh('Shared bolt', [box([-0.08, -0.08, -0.1], [0.08, 0.08, 0.1], 1)]), translation: [-0.8, 0.8, 0.8] },
  { name: 'Bolt B', mesh: 5, matrix: [1, 0, 0, 0, 0, 1, 0, 0, 0, 0, 1, 0, 0.8, 0.8, 0.8, 1] },
  { name: 'Two materials', mesh: mesh('Two materials', [box([1.2, 0, 0], [1.5, 0.3, 0.2], 2), box([1.2, -0.5, 0], [1.5, -0.2, 0.2], 3)]) },
  { name: 'Inlet', translation: [0, 0.8, 0.7] },
];
model.buffers = [{ byteLength: length }];
const json = new TextEncoder().encode(JSON.stringify(model));
const jlen = Math.ceil(json.length / 4) * 4;
const out = new Uint8Array(28 + jlen + length);
const header = new DataView(out.buffer);
header.setUint32(0, 0x46546c67, true);
header.setUint32(4, 2, true);
header.setUint32(8, out.length, true);
header.setUint32(12, jlen, true);
header.setUint32(16, 0x4e4f534a, true);
out.fill(32, 20, 20 + jlen);
out.set(json, 20);
header.setUint32(20 + jlen, length, true);
header.setUint32(24 + jlen, 0x004e4942, true);
let at = 28 + jlen;
for (const bytes of chunks) {
  out.set(bytes, at);
  at += bytes.length;
}
const plain = join(assets, 'assembly.glb'), optimized = join(assets, 'assembly-meshopt.glb');
writeFileSync(plain, out);
// Preserve parts, hierarchy, materials and triangles for the reader and later pick/pixel tests.
const result = spawnSync(process.execPath, ['x', '@gltf-transform/cli@4.2.1', 'optimize', plain, optimized,
  '--compress', 'meshopt', '--meshopt-level', 'medium', '--flatten', 'false', '--instance', 'false',
  '--join', 'false', '--palette', 'false', '--prune', 'false', '--simplify', 'false',
  '--weld', 'false', '--texture-compress', 'false'], { stdio: 'inherit' });
if (result.status !== 0) throw new Error(`glTF-Transform failed: ${result.error ?? result.status}`);
