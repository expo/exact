function readError(e) {
  if (e?.name === 'NotFoundError' || e?.code === 'ENOENT') return 'NotFoundError';
  if (['NotAllowedError', 'SecurityError'].includes(e?.name) || ['EACCES', 'EPERM'].includes(e?.code)) return 'NotAllowedError';
  if (!e?.code && /^(ENOENT|(?:file )?not found|no such file)\b/i.test(e?.message)) return 'NotFoundError';
  return 'OperationError';
}

export function implementation({ files }) {
  let detector, lastFast;
  return {
    async detectFaces({ image, maxDetectedFaces, fastMode }) {
      const limit = maxDetectedFaces ?? undefined, fast = fastMode ?? false;
      if (limit !== undefined && (!Number.isInteger(limit) || limit < 0 || limit > 65535)) return { error: 'TypeError' };
      if (typeof globalThis.FaceDetector !== 'function') return { error: 'NotSupportedError' };
      let bytes;
      try { bytes = await files.read(image); }
      catch (e) { return { error: readError(e), message: e?.message }; }

      let bitmap;
      try { bitmap = await createImageBitmap(new Blob([bytes])); }
      catch { return { error: 'InvalidStateError', message: 'decode' }; }
      try {
        const { width, height } = bitmap;
        const ratio = Math.min(1, 2048 / Math.max(width, height));
        // Chrome's detector reads an ImageBitmap's stored pixels. Drawing it applies EXIF first.
        const canvas = new OffscreenCanvas(Math.max(1, Math.round(width * ratio)), Math.max(1, Math.round(height * ratio)));
        canvas.getContext('2d').drawImage(bitmap, 0, 0, canvas.width, canvas.height);
        if (!detector || fast !== lastFast) {
          // Rank all candidates before applying the caller’s limit.
          detector = new FaceDetector({ maxDetectedFaces: 65535, fastMode: fast });
          lastFast = fast;
        }
        const sx = width / canvas.width, sy = height / canvas.height;
        const found = await detector.detect(canvas);
        const faces = found.map(({ boundingBox: b, landmarks }) => ({
          boundingBox: { x: b.x * sx, y: b.y * sy, width: b.width * sx, height: b.height * sy },
          landmarks: (landmarks ?? []).map(({ type, locations }) => ({
            ...(type ? { type } : {}), locations: locations.map(p => ({ x: p.x * sx, y: p.y * sy })),
          })),
        }));
        const area = f => f.boundingBox.width * f.boundingBox.height;
        faces.sort((a, b) => area(b) - area(a));
        return { width, height, faces: faces.slice(0, limit) };
      } catch (e) {
        return { error: e?.name === 'NotSupportedError' ? e.name : 'OperationError', message: e?.message };
      } finally { bitmap.close(); }
    },
  };
}
