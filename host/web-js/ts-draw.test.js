// LLP 1056 D1 on the JS target: a TypeScript `draw` receives records as objects by field name, each argument the
// runtime keeps converted once (a commit that changes another argument reuses it), and frozen, so no draw changes what
// the next one reads.
import { test, expect, mock } from "bun:test";
import { Recorder, Path2D, parseColor } from "../../canvas/recorder.js";

const seen = [];
let conversions = 0;
mock.module("./path2d.js", () => ({ browserPath2D: globalThis.Path2D }));
mock.module("__RECORDER__", () => ({ Recorder, Path2D, parseColor }));
mock.module("__APP_TS__", () => ({ draw: (surface, args) => { seen.push(args); try { args[0][0].x = 99; } catch {} return false; } }));
// ts-data.js's converter for a record type `{field: T}` and a list `["[", T]`.
const convert = (v, t) => Array.isArray(t) ? v.map((x) => convert(x, t[1])) : typeof t === "object" ? Object.fromEntries(Object.keys(t).map((k, i) => [k, convert(v[i], t[k])])) : v;
mock.module("./ts-data.js", () => ({ named: (v, t) => { conversions++; return convert(v, t); } }));

test("records reach draw by field name, converted once and read-only", async () => {
  const { drawer } = await import("./ts-draw.js");
  const nodes = [[1, 2], [3, 4]], type = ["[", { x: "n", y: "n" }];
  const q = (args, seq) => ({ canvas: 1, generation: 0, seq, surface: "board", args, types: [type, 0], names: [], frame: [0, 0, 2, 10, 10, 10, 10, 1] });
  const first = [nodes, 1];
  drawer.draw(q(first, 1));
  drawer.draw(q(first, 2));
  drawer.draw(q([nodes, 2], 3));
  expect(seen.map((a) => [a[0][0].x, a[0][1].y, a[1]])).toEqual([[1, 4, 1], [1, 4, 1], [1, 4, 2]]);
  expect(conversions).toBe(1);
  expect(Object.isFrozen(seen[0][0][0]) && Object.isFrozen(seen[0])).toBe(true);
  expect(nodes[0]).toEqual([1, 2]);
});
