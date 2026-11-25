import { afterEach, expect, test } from 'bun:test';
import { implementation } from '../web/index.js';

const saved = new Map(['FaceDetector', 'createImageBitmap', 'OffscreenCanvas'].map(k => [k, Object.getOwnPropertyDescriptor(globalThis, k)]));
afterEach(() => {
  for (const [k, descriptor] of saved) {
    if (descriptor) Object.defineProperty(globalThis, k, descriptor);
    else delete globalThis[k];
  }
});
const args = { image: 'app:/tmp/picked/photo.jpg' };

test('an unavailable detector and an invalid face limit fail before the file read', async () => {
  delete globalThis.FaceDetector;
  const module = implementation({});
  expect(await module.detectFaces(args)).toEqual({ error: 'NotSupportedError' });
  for (const maxDetectedFaces of [-1, 0.5, 65536]) {
    expect(await module.detectFaces({ ...args, maxDetectedFaces })).toEqual({ error: 'TypeError' });
  }
});

test('the upright canvas, options and coordinates survive the browser boundary', async () => {
  const sizes = [], hints = [];
  imageBitmap(4096, 1536);
  globalThis.FaceDetector = class {
    constructor(options) { hints.push(options); this.limit = options.maxDetectedFaces; }
    async detect(canvas) {
      sizes.push([canvas.width, canvas.height]);
      return [
        { boundingBox: { x: 50, y: 20, width: 10, height: 15 }, landmarks: null },
        { boundingBox: { x: -5, y: 12, width: 32, height: 44 }, landmarks: [{ type: 'eye', locations: [{ x: 2, y: 14 }] }] },
      ].slice(0, this.limit);
    }
  };
  const module = implementation({ files: { read: () => new Uint8Array() } });
  const expected = {
    width: 4096, height: 1536, faces: [{
      boundingBox: { x: -10, y: 24, width: 64, height: 88 }, landmarks: [{ type: 'eye', locations: [{ x: 4, y: 28 }] }],
    }],
  };
  for (let i = 0; i < 2; i++) expect(await module.detectFaces({ ...args, maxDetectedFaces: 1, fastMode: true })).toEqual(expected);
  expect(hints).toEqual([{ maxDetectedFaces: 65535, fastMode: true }]);
  expect(sizes).toEqual([[2048, 768], [2048, 768]]);
  const all = await module.detectFaces(args);
  expect(all.faces[1].landmarks).toEqual([]);
  expect(all.faces).toHaveLength(2);
  expect((await module.detectFaces({ ...args, maxDetectedFaces: 0 })).faces).toEqual([]);
});

test('read, decode and detector errors reach the caller', async () => {
  globalThis.FaceDetector = class { detect() { throw new DOMException('no backend', 'NotSupportedError'); } };
  const missing = implementation({ files: { read() { throw Object.assign(new Error(), { code: 'ENOENT' }); } } });
  expect((await missing.detectFaces(args)).error).toBe('NotFoundError');
  const absent = implementation({ files: { read() { throw new DOMException('', 'NotFoundError'); } } });
  expect((await absent.detectFaces(args)).error).toBe('NotFoundError');
  const module = implementation({ files: { read: () => new Uint8Array() } });
  for (const [code, expected] of [['EIO', 'OperationError'], ['EACCES', 'NotAllowedError']]) {
    const failing = implementation({ files: { read() { throw Object.assign(new Error('not found.jpg'), { code }); } } });
    expect((await failing.detectFaces(args)).error).toBe(expected);
  }
  const message = 'file not found';
  const missingByMessage = implementation({ files: { read() { throw new Error(message); } } });
  expect(await missingByMessage.detectFaces(args)).toEqual({ error: 'NotFoundError', message });
  imageBitmap(0, 0);
  expect((await module.detectFaces(args)).error).toBe('InvalidStateError');
  imageBitmap(1, 1);
  expect((await module.detectFaces(args)).error).toBe('NotSupportedError');
  expect((await module.detectFaces({ ...args, maxDetectedFaces: 0 })).error).toBe('NotSupportedError');
});

function imageBitmap(width, height) {
  globalThis.createImageBitmap = async () => {
    if (!width || !height) throw new Error('decode');
    return { width, height, close() {} };
  };
  globalThis.OffscreenCanvas = class {
    constructor(w, h) { this.width = w; this.height = h; }
    getContext() { return { drawImage() {} }; }
  };
}
