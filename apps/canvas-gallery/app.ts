// The Canvas 2D gallery's TypeScript module (LLP 1056 §4).
// Each `canvas surface=<name>(…)` in app.contract draws with the
// fixture of that name in fixtures.js, which direct.html also runs against
// Chrome's own context.
import type { Answer, Ctx2D, Frame } from './app.contract.d.ts';
import { fixtures, surfaces as roster, TILE } from './fixtures';

export const appId = 'com.exact.canvasgallery';
// Page 6's captures write here and nowhere else.
export const grants = 'fs.write app:/tmp';
export const answer: Answer = (async (source: string, [surface, path]: [string, string]) => {
  if (source !== 'capture') throw { kind: 'UnknownSource', message: source };
  try {
    const c = await captureCanvas(surface, path);
    return { record: `${c.path} ${c.width}x${c.height} @${c.scale}`, path: c.path };
  } catch (e: any) {
    return { record: `${e.name}: ${e.message}`, path: '' };
  }
}) as unknown as Answer;
// Page 6's canvases (not Chrome-parity fixtures): an annotation over an image
// with text that keeps every step's stroke, and one that draws nothing.
const captures: Record<string, (ctx: Ctx2D, frame: Frame, args: any[]) => void> = {
  annotate(ctx, frame, [step]) {
    if (frame.causes.some((c) => c === 'mount' || c === 'image' || c === 'font' || c === 'size')) {
      ctx.reset();
      ctx.drawImage(TILE, 0, 0, 150, 100);
      ctx.font = "700 18px 'Exposure Sans'"; ctx.fillStyle = '#0f172a';
      ctx.fillText('Exact', 10, 28);
    }
    // No clearing: each step adds its stroke to what the bitmap kept.
    ctx.strokeStyle = `hsl(${step * 70} 80% 40%)`; ctx.lineWidth = 4; ctx.lineCap = 'round';
    ctx.beginPath(); ctx.moveTo(12 + (step % 4) * 30, 85); ctx.lineTo(32 + (step % 4) * 30, 50); ctx.stroke();
    // The changing band makes a stale capture exceed the smoke's pixel tolerance.
    ctx.fillStyle = step % 2 ? '#ef4444' : '#2563eb'; ctx.fillRect(0, 92, 150, 8);
  },
  blank() {},
  captureOrder(ctx, frame, [step]) {
    ctx.fillStyle = step % 2 ? '#0000ff' : '#ff0000';
    ctx.fillRect(0, 0, frame.width, frame.height);
  },
};
export const surfaces: Record<string, number> = { ...roster, annotate: 1, blank: 0, captureOrder: 1 };
export function draw(surface: string, args: any[], ctx: Ctx2D, frame: Frame): boolean {
  if (captures[surface]) { captures[surface](ctx, frame, args); return false; }
  const fixture = fixtures[surface];
  if (!fixture) throw new Error(`no fixture ${surface}`);
  return fixture(ctx, frame, args) === true;
}
