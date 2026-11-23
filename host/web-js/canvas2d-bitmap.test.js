// LLP 1056 D6 on the JS target: a computed `bitmap-width`, `bitmap-height`, `color-space` or `color-type` (pieces.js
// `cb`) is read at the canvas's box, as the runner reads the props at each layout. A change is a new size generation,
// cause "size", and a commit that also changes the arguments draws once; the same value draws nothing, and a bitmap
// past the runner's limits is refused.
import { test, expect, mock } from "bun:test";

const draws = [], ops = [];
mock.module("./canvas2d-glue.js", () => ({}));
mock.module("./draw.js", () => ({ drawer: { draw: (q) => { draws.push(q); return { lists: [], frame: false }; }, retire() {} } }));

test("a changed bitmap attribute is a new generation, drawn at the new size", async () => {
  const kept = ["getComputedStyle", "matchMedia", "document", "exact"].map((name) => [name, Object.getOwnPropertyDescriptor(globalThis, name)]);
  try {
    globalThis.getComputedStyle = () => ({ color: "rgb(0, 0, 0)", direction: "ltr" });
    globalThis.matchMedia = () => new EventTarget();
    globalThis.document = { documentElement: {}, addEventListener() {} };
    globalThis.exact = { canvas2dGlue: () => ({ op: (op) => ops.push(op), geometry() {} }) };
    const { engine } = await import("./canvas2d.js");
    const el = { isConnected: true, dataset: { bitmapWidth: "640", bitmapHeight: "480" } };
    const canvases = engine({ data: { logic: null, ready() {} }, clock: { now: 0 }, inflight: { n: 0 }, journal: [], After: [], views: new Map(), viewId: () => 3 });
    await Promise.resolve();
    const c = { e: el, name: "pad", args: [640], types: [], names: [], mounted: 0 };
    canvases.args(c);
    globalThis.exact.wasm.exact_canvas_geometry(3, 320, 240, 2);
    const seen = () => draws.map((q) => [q.generation, q.frame[2], q.frame[5], q.frame[6]]);
    expect(seen()).toEqual([[0, 1, 640, 480]]);
    // One commit: the arguments' effect, then the size's (pieces.js `c2`, then `cb`), each a callback on the loaded
    // module.
    c.args = [1280]; el.dataset.bitmapWidth = "1280";
    const loaded = Promise.resolve(canvases);
    loaded.then((m) => m.args(c)); loaded.then((m) => m.resized(el));
    await new Promise((done) => setTimeout(done));
    expect(seen().slice(1)).toEqual([[1, 2 | 4, 1280, 480]]);
    expect(draws.at(-1).args).toEqual([1280]);
    // The other order, as a commit that writes the size's signal first runs its effects.
    c.args = [640]; el.dataset.bitmapWidth = "640";
    loaded.then((m) => m.resized(el)); loaded.then((m) => m.args(c));
    await new Promise((done) => setTimeout(done));
    expect(seen().slice(2)).toEqual([[2, 2 | 4, 640, 480]]);
    expect(ops.at(-2)).toMatchObject({ fresh: true, generation: 2, w: 640, h: 480, stretch: true });
    // The same value again draws nothing; a settings change is a new generation.
    const flush = () => new Promise((done) => setTimeout(done));
    canvases.resized(el); await flush();
    expect(draws.length).toBe(3);
    el.dataset.colorSpace = "display-p3";
    canvases.resized(el); await flush();
    expect(seen().at(-1)).toEqual([3, 4, 640, 480]);
    expect(draws.at(-1).p3).toBe(true);
    // With neither size, the box at its scale.
    delete el.dataset.bitmapWidth; delete el.dataset.bitmapHeight;
    canvases.resized(el); await flush();
    expect(seen().at(-1)).toEqual([4, 4, 640, 480]);
    expect(draws.at(-1).frame[3]).toBe(320);
    el.dataset.bitmapWidth = "65536";
    canvases.resized(el); await flush();
    expect(draws.length).toBe(5);
    expect(globalThis.exact.canvas2dState()[0].refused).toBe("65536 × 150 px is over the 65535 px side limit");
  } finally { for (const [name, d] of kept) d ? Object.defineProperty(globalThis, name, d) : delete globalThis[name]; }
});
