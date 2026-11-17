// The web has no standing screen permission. Under the agent, write only
// the fixture displays (LLP 1178 D6), without calling getDisplayMedia.
const defaults = [{ left: 0, top: 0, width: 1440, height: 900,
  availLeft: 0, availTop: 0, availWidth: 1440, availHeight: 900,
  devicePixelRatio: 2, isPrimary: true, isInternal: true, label: 'Agent display',
  clientRect: { x: 0, y: 0, width: 1440, height: 900 } }];
const glyphs = [[7,5,5,5,7],[2,6,2,2,7],[7,1,7,4,7],[7,1,7,1,7],[5,5,7,1,1],
  [7,4,7,1,7],[7,4,7,5,7],[7,1,1,1,1],[7,5,7,5,7],[7,5,7,1,7],[0,5,2,5,0]];

export function card(index, width, height) {
  const canvas = new OffscreenCanvas(width, height), ctx = canvas.getContext('2d');
  ctx.fillStyle = `rgb(${32 + index % 6 * 30},64,128)`;
  ctx.fillRect(0, 0, width, height);
  ctx.fillStyle = '#fff';
  [...`${index} ${width}x${height}`].forEach((ch, at) => {
    if (ch === ' ') return;
    const glyph = glyphs[ch === 'x' ? 10 : Number(ch)];
    glyph.forEach((row, y) => {
      for (let x = 0; x < 3; x++) if (row & (4 >> x)) ctx.fillRect(8 + at * 16 + x * 4, 8 + y * 4, 4, 4);
    });
  });
  return canvas;
}

export function implementation({ agent, files }) {
  let screens = null, queue = Promise.resolve(), ended = false;
  const pending = new Map();
  const status = () => ({ available: !!agent, state: agent ? 'granted' : 'denied' });
  return {
    preferred(values) { screens = values.screens ?? null; },
    screenRecordingStatus: status,
    requestScreenRecording() { return agent ? status() : { error: 'NotSupportedError' }; },
    captureScreens(args, { preferred, request } = {}) {
      if (!agent) return { error: 'NotSupportedError' };
      const turn = { held: true };
      pending.set(request, turn);
      const snapshot = structuredClone(screens ?? preferred?.screens ?? defaults).sort((a, b) => a.left - b.left || a.top - b.top);
      const run = async () => {
        try {
          if (!turn.held || ended) return { error: 'AbortError' };
          let total = 0;
          const displays = snapshot.map(screen => {
            const width = Math.round(screen.width * screen.devicePixelRatio);
            const height = Math.round(screen.height * screen.devicePixelRatio);
            const pixels = width * height;
            total += pixels;
            if (!Number.isSafeInteger(pixels) || width <= 0 || height <= 0 || pixels > 2 ** 26 || total > 2 ** 27)
              throw new DOMException('capture exceeds the pixel limit', 'QuotaExceededError');
            return { screen: { ...screen, clientRect: screen.clientRect ?? null }, width, height };
          });
          if (!displays.length) return { error: 'EncodingError', message: 'no display' };
          const directory = args.directory.replace(/\/+$/, '');
          const type = (args.type || 'image/png').toLowerCase();
          const quality = typeof args.quality === 'number' && args.quality >= 0 && args.quality <= 1 ? args.quality : 0.92;
          const encoded = await Promise.all(displays.map(async (display, index) => {
            const canvas = card(index, display.width, display.height);
            const blob = await canvas.convertToBlob({ type, quality });
            canvas.width = canvas.height = 1;
            const ext = { 'image/jpeg': 'jpg', 'image/webp': 'webp' }[blob.type] || 'png';
            return { blob, answer: { ...display, path: `${directory}/screen-${index}.${ext}`, type: blob.type } };
          }));
          if (!turn.held || ended) return { error: 'AbortError' };
          await files.putAll(encoded.map(({ blob, answer }) => ({ path: answer.path, blob })),
            () => !turn.held || ended ? new DOMException('capture cancelled', 'AbortError') : undefined);
          return { captures: encoded.map(e => e.answer) };
        } catch (error) {
          return { error: ['QuotaExceededError', 'AbortError'].includes(error.name) ? error.name : 'EncodingError', message: error.message };
        } finally { pending.delete(request); }
      };
      const result = queue.then(run);
      queue = result.then(() => {});
      return result;
    },
    cancel(request) { const turn = pending.get(request); if (turn) turn.held = false; },
    destroy() { ended = true; for (const turn of pending.values()) turn.held = false; },
  };
}
