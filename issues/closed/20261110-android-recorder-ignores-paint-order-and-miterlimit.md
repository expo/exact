# The Android recorder paints an SVG shape's fill before its stroke whatever `paint-order` says, and drops `stroke-miterlimit`

**Status:** Closed
**Resolution:** The recorder writes a shape's fill and stroke in its `paint-order`, and `STROKE` carries the miter limit after the join, which the reader sets as `Paint.strokeMiter` (and counts in a stroke's bounds and its icon cache key).
**Systems:** host/linux/src/canvas.rs, host/android/canvas (Player.kt)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

The paint walk hands a backend one `SvgPaint` per shape with its `order`
(`paint-order`) and `miter` (`stroke-miterlimit`) (`host/linux/src/paint/svg.rs:502-520`).
When markers come first or last, the fill and the stroke arrive in one call and the
backend orders them (`paint/svg.rs:616-637`). LLP 1055.000 says of the painting hosts:
"`paint-order` orders the fill and stroke calls".

The CPU painter (`raster.rs:684`), the vello painter (`gpu.rs:593`) and the instanced
painter (`ink/svg.rs:23`) loop over `s.order`. The Android recorder's `svg_path`
(`host/linux/src/canvas.rs:707-746`) never reads it: it writes `PATH` for the fill,
then `STROKE`. It never reads `s.miter` either: the stream's `STROKE` op is
`color width cap join n …` (`canvas.rs:36`, `741-744`), and the reader leaves
`Paint.strokeMiter` at Android's default of 4
(`host/android/canvas/src/main/java/dev/exact/canvas/Player.kt:830-835`).

## The failure

```
svg viewBox="0 0 60 60" width=60 height=60
  path d="M 0 0 L 40 0 L 40 40 Z" fill="#f00" stroke="#00f" stroke-width=8 paint-order="stroke"
```

At (38, 20), inside the triangle and inside the stroke's inner half, the fill is
over the stroke (SVG 2 §13.8, `paint-order`): red. The recorder's stream is fill
then stroke, so Android shows blue there. On Android the fill of any such shape
loses half the stroke width along every edge, and an outline meant to sit behind a
shape covers it.

SVG's initial `stroke-miterlimit` is 4 (SVG 2 §13.5.5), so only an authored value
differs. With `stroke-miterlimit="10"`, which many exported icons carry, a corner
between about 11.5° and 29° is mitered on the other painters and bevelled on Android.
With a value below 4, Android miters corners the others bevel.

## Verification

A temporary test in `canvas.rs` (the module compiles under `cfg(test)` on macOS)
painted one `SvgPaint` for that path through `raster::Raster`, `gpu::Gpu`, `ink::Ink`
and `Recorder`, and read the pixel at (38, 20):

| `order` | CPU | vello | instanced | recorder's stream |
| --- | --- | --- | --- | --- |
| `[0, 1, 2]` (normal) | blue | blue | blue | `PATH`, `STROKE` |
| `[1, 0, 2]` (`stroke`) | red | red | red | `PATH`, `STROKE` |

The recorder's `STROKE` op for the path was 16 words: the op, colour, width, cap,
join, the count and ten words of path, with no miter limit.

Not run: the reader on a device. That it leaves `strokeMiter` alone is by reading.

## Direction

Loop over `s.order` in `Recorder::svg_path` as the other three backends do. Carry the
miter limit in `STROKE` (or a prefix op, as `DASH` is) and set `strokeMiter` in the
reader.

## Related

`canvas/gradient.rs:100-103` already says once that an SVG pattern is skipped on this
host; nothing says this. `rules/DEFERRED.md` lists neither property as deferred on
Android. No issue or `QUEUE.md` entry names `paint-order`.
