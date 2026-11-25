// The web has no standing screen permission. Under the agent, write only
// the fixture displays (LLP 1178 D6), without calling getDisplayMedia.
const defaults = [{ left: 0, top: 0, width: 1440, height: 900,
  availLeft: 0, availTop: 0, availWidth: 1440, availHeight: 900,
  devicePixelRatio: 2, isPrimary: true, isInternal: true, label: 'Agent display',
  clientRect: { x: 0, y: 0, width: 1440, height: 900 } }];
const glyphs = [[7,5,5,5,7],[2,6,2,2,7],[7,1,7,4,7],[7,1,7,1,7],[5,5,7,1,1],
  [7,4,7,1,7],[7,4,7,5,7],[7,1,1,1,1],[7,5,7,5,7],[7,5,7,1,7],[0,5,2,5,0]];

export function card(index, width, height, pointer) {
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
  if (pointer) {
    for (let y = 0; y < 12; y++) ctx.fillRect(pointer.x, pointer.y + y, 1 + (y >> 1), 1);
  }
  return canvas;
}

export function implementation({ agent, files }) {
  let screens = null, queue = Promise.resolve(), ended = false;
  const pending = new Map();
  const status = () => ({ available: !!agent, state: agent ? 'granted' : 'denied' });
  const handle = Array.from(crypto.getRandomValues(new Uint8Array(16)), n => n.toString(16).padStart(2, '0')).join('');
  const snapshotOf = preferred => structuredClone(screens ?? preferred?.screens ?? defaults)
    .sort((a, b) => a.left - b.left || a.top - b.top);
  const primary = list => list.find(s => s.isPrimary) ?? list[0];
  const frameOf = s => ({ x: s.left, y: s.top, width: s.width, height: s.height });
  const contains = (r, x, y) => x >= r.x && y >= r.y && x < r.x + r.width && y < r.y + r.height;
  const lookup = (point, preferred) => {
    if (!agent) return { error: 'NotSupportedError' };
    if (point && ![point.x, point.y].every(Number.isFinite)) return { error: 'TypeError' };
    const screen = primary(snapshotOf(preferred));
    if (!screen || (point && !contains(frameOf(screen), point.x, point.y))) return { error: 'NotFoundError' };
    return { handle, title: 'Agent window', frame: frameOf(screen), app: { name: 'Agent', processName: 'Agent', pid: 1 } };
  };
  return {
    preferred(values) { screens = values.screens ?? null; },
    screenRecordingStatus: status,
    getWindowAtPoint(point, { preferred } = {}) { return lookup(point, preferred); },
    getFrontmostWindow(_, { preferred } = {}) { return lookup(null, preferred); },
    requestScreenRecording() { return agent ? status() : { error: 'NotSupportedError' }; },
    captureScreens(args, { preferred, request } = {}) {
      if (!agent) return { error: 'NotSupportedError' };
      const turn = { held: true };
      pending.set(request, turn);
      const snapshot = snapshotOf(preferred);
      const run = async () => {
        try {
          if (!turn.held || ended) return { error: 'AbortError' };
          let total = 0;
          const { screen: selected, rect, window, cursor = 'never' } = args;
          if ((rect != null && selected == null) || (window != null && selected != null) ||
              (cursor != null && !['always', 'never'].includes(cursor))) return { error: 'TypeError' };
          const home = primary(snapshot);
          let list = snapshot;
          if (window != null) {
            if (window !== handle || !home) return { error: 'NotFoundError' };
            list = [home];
          }
          if (selected != null) {
            const fields = ['left', 'top', 'width', 'height', 'devicePixelRatio', 'label'];
            list = list.filter(s => fields.every(k => s[k] === selected[k]));
            if (list.length !== 1) return { error: 'NotFoundError' };
          }
          const displays = list.map(screen => {
            const region = rect ?? { x: 0, y: 0, width: screen.width, height: screen.height };
            if (rect && (!Object.values(region).every(Number.isFinite) ||
                !(rect.x >= 0 && rect.y >= 0 && rect.width > 0 && rect.height > 0 &&
                  rect.x + rect.width <= screen.width && rect.y + rect.height <= screen.height)))
              throw new DOMException('rect is outside the display', 'TypeError');
            const width = Math.round(screen.width * screen.devicePixelRatio);
            const height = Math.round(screen.height * screen.devicePixelRatio);
            const captureWidth = rect ? Math.max(1, Math.round(rect.width * width / screen.width)) : width;
            const captureHeight = rect ? Math.max(1, Math.round(rect.height * height / screen.height)) : height;
            const pixels = captureWidth * captureHeight;
            total += pixels;
            if (!Number.isSafeInteger(pixels) || width <= 0 || height <= 0 || pixels > 2 ** 26 || total > 2 ** 27)
              throw new DOMException('capture exceeds the pixel limit', 'QuotaExceededError');
            const pointer = cursor === 'always' && screen === home && contains(region, screen.width / 2, screen.height / 2)
              ? { x: Math.floor((screen.width / 2 - region.x) * captureWidth / region.width),
                  y: Math.floor((screen.height / 2 - region.y) * captureHeight / region.height) } : null;
            return { index: snapshot.indexOf(screen), pointer, answer: { screen: { ...screen, clientRect: screen.clientRect ?? null },
              width: captureWidth, height: captureHeight, ...(window != null ? { frame: frameOf(screen) } : {}) } };
          });
          if (!displays.length) return { error: 'EncodingError', message: 'no display' };
          const directory = args.directory.replace(/\/+$/, '');
          const type = (args.type || 'image/png').toLowerCase();
          const quality = typeof args.quality === 'number' && args.quality >= 0 && args.quality <= 1 ? args.quality : 0.92;
          const captureId = Array.from(crypto.getRandomValues(new Uint8Array(16)), n => n.toString(16).padStart(2, '0')).join('');
          const encoded = await Promise.all(displays.map(async (display, index) => {
            const canvas = card(display.index, display.answer.width, display.answer.height, display.pointer);
            const blob = await canvas.convertToBlob({ type, quality });
            canvas.width = canvas.height = 1;
            const ext = { 'image/jpeg': 'jpg', 'image/webp': 'webp' }[blob.type] || 'png';
            return { blob, answer: { ...display.answer, path: `${directory}/screen-${captureId}-${index}.${ext}`, type: blob.type } };
          }));
          if (!turn.held || ended) return { error: 'AbortError' };
          await files.putAll(encoded.map(({ blob, answer }) => ({ path: answer.path, blob })),
            () => !turn.held || ended ? new DOMException('capture cancelled', 'AbortError') : undefined);
          return { captures: encoded.map(e => e.answer) };
        } catch (error) {
          return { error: ['TypeError', 'QuotaExceededError', 'AbortError'].includes(error.name) ? error.name : 'EncodingError', message: error.message };
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
