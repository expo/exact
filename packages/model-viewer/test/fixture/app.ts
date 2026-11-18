import type { Answer, Sources } from './app.contract.d.ts';

export const grants = 'fs.write app:/cache/model-viewer';

// Generate this model in data code so the drive cannot load a bundled copy by mistake.
function cachedTriangle(): Uint8Array {
  const positions = new Float32Array([-1, -1, 0, 1, -1, 0, 0, 1, 0]);
  const model = {
    asset: { version: '2.0' }, scene: 0, scenes: [{ nodes: [0] }],
    nodes: [{ name: 'Cached triangle', mesh: 0 }],
    meshes: [{ primitives: [{ attributes: { POSITION: 0 } }] }],
    buffers: [{ byteLength: positions.byteLength }],
    bufferViews: [{ buffer: 0, byteLength: positions.byteLength }],
    accessors: [{ bufferView: 0, componentType: 5126, count: 3, type: 'VEC3', min: [-1, -1, 0], max: [1, 1, 0] }],
  };
  const json = new TextEncoder().encode(JSON.stringify(model));
  const jsonLength = Math.ceil(json.length / 4) * 4;
  const bytes = new Uint8Array(28 + jsonLength + positions.byteLength);
  const header = new DataView(bytes.buffer);
  for (const [offset, value] of [[0, 0x46546c67], [4, 2], [8, bytes.length], [12, jsonLength], [16, 0x4e4f534a],
    [20 + jsonLength, positions.byteLength], [24 + jsonLength, 0x004e4942]]) header.setUint32(offset, value, true);
  bytes.fill(32, 20, 20 + jsonLength);
  bytes.set(json, 20);
  bytes.set(new Uint8Array(positions.buffer), 28 + jsonLength);
  return bytes;
}

const sources: Sources = {
  cacheModel: async (_args, _store, storage) => {
    const directory = 'app:/cache/model-viewer';
    await storage.fs.mkdir(directory).catch((e: { code?: string }) => { if (e.code !== 'EEXIST') throw e; });
    const path = `${directory}/triangle.glb`;
    await storage.fs.writeFile(path, cachedTriangle());
    return { path };
  },
};
export const answer: Answer = (source, args, store, storage) => sources[source](args, store, storage);
