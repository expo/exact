// LLP 1056 D1 on the JS target: a Rust `draw`'s arguments are encoded by the declared types the build emits for every
// surface argument (ts-data.js's field-named form), as rust-data.js's type codes; one without a type by its shape.
import { test, expect, mock } from "bun:test";

mock.module("./canvas2d-glue.js", () => ({}));
mock.module("./draw.js", () => ({ drawer: null }));

test("a Rust draw's arguments are encoded by their declared types", async () => {
  const kept = ["getComputedStyle", "matchMedia", "document", "exact"].map((name) => [name, Object.getOwnPropertyDescriptor(globalThis, name)]);
  try {
    globalThis.getComputedStyle = () => ({ color: "rgb(0, 0, 0)", direction: "ltr" });
    globalThis.matchMedia = () => new EventTarget();
    globalThis.document = { documentElement: {}, addEventListener() {} };
    globalThis.exact = { canvas2dGlue: () => ({ op() {}, geometry() {} }) };
    const encoded = [], memory = { buffer: new ArrayBuffer(64) };
    const writer = () => ({ u8() {}, u32() {}, f64() {}, str() {}, done: () => new Uint8Array(1) });
    const reply = [7, 5, 0, 0, 0];
    const reader = () => ({ u32: () => reply.shift(), u8: () => reply.shift(), bytes() {}, str() {} });
    const exports = { memory, exact_logic_draw: () => 0, exact_logic_alloc: () => 0, exact_logic_dealloc() {}, exact_logic_output: () => 0, exact_logic_output_len: () => 0 };
    const logic = { writer, reader, ABI: 7, exports, session: 1, encode: (w, v, t) => encoded.push([v, t]) };
    const { engine } = await import("./canvas2d.js");
    const canvases = engine({ data: { logic, ready() {} }, clock: { now: 0 }, inflight: { n: 0 }, journal: [], After: [], views: new Map(), viewId: () => 4 });
    const node = { id: "s", x: "n", at: ["?", "n"] };
    canvases.args({ e: { isConnected: true, dataset: {} }, name: "map", args: [[["a", 1, null]], "on", 3], types: [["[", node], "s", 0], names: [] });
    globalThis.exact.wasm.exact_canvas_geometry(4, 10, 10, 1);
    expect(encoded).toEqual([[[["a", 1, null]], "[{sn?n}"], ["on", "s"], [3, "n"]]);
  } finally { for (const [name, d] of kept) d ? Object.defineProperty(globalThis, name, d) : delete globalThis[name]; }
});
