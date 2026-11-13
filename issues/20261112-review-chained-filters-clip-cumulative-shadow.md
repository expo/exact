# Chained CSS filters clip the shadow added by a later filter

**Status:** Open
**Systems:** kernel/src/svg/scene/filter.rs, native filter rendering
**Severity:** P2
**Author:** Codex, repository review
**Date:** 2026-11-12

## Failure

At `f6be99aff76bc8bd881b0ec8daaa2c10ace0b9a2`, a 10px-wide box with
`filter="drop-shadow(10px 0 0 #000000) drop-shadow(10px 0 0 #000000)"`
gets a filter region only 20px wide. The first shadow extends the content to
20px; the second shadows that result and extends it to 30px. Native rendering
clips the last 10px.

[Filter Effects 1 §5](https://www.w3.org/TR/filter-effects-1/#FilterProperty)
requires each function to consume the previous function's output, and
[§13.2](https://www.w3.org/TR/filter-effects-1/)
requires the region to cover the expanded visible area.

`function` computes each expansion from the original `visual` box and unions
it with the prior region (`kernel/src/svg/scene/filter.rs:158–161`). Both
shadows therefore contribute the same region. The primitive inputs correctly
chain, so the pixels need more room than the region reserves. `box_filter`
passes the same zero-size box for every function (lines 308–317); SVG filter
resolution likewise retains the original visual bounds.

The Linux paint walk uses this region to allocate the filtered box's pixel
buffer (`host/linux/src/paint/box_filter.rs:68–92`). Its SVG path allocates
from the same region (`host/linux/src/paint/island.rs:115–144`). Apple's SVG
bridge also sends this region to its presenter (`host/apple/src/svg.rs:595–609`).

## Reproduction

The following standalone program calls the existing kernel parser and filter
resolver, then the existing native rasterizer. It paints a red 10×2 rectangle
and compares the resolver's allocation with a 30×2 allocation, using identical
filter primitives and source pixels.

```rust
use exact_kernel::svg::{filter::FilterList, scene::box_filter};
use exact_kernel::{Color, ColorValue};
fn main() {
    for css in ["drop-shadow(10px 0 0 #000000)", "drop-shadow(10px 0 0 #000000) drop-shadow(10px 0 0 #000000)"] {
        let list = FilterList::parse(css).unwrap();
        let f = box_filter(&list, ColorValue::Fixed(Color(0xff0000ff)), false).unwrap();
        println!("{css}: growth={:?}, primitives={}", f.region, f.primitives.len());
        for width in [(10.0 + f.region.2) as usize, 30] {
            let mut pixels = vec![0u8; width * 2 * 4];
            for y in 0..2 { for x in 0..10 {
                pixels[(y * width + x) * 4..(y * width + x + 1) * 4].copy_from_slice(&[255, 0, 0, 255]);
            }}
            exact_svg_raster::filter::run(&f, &mut pixels, width, 2,
                exact_svg_raster::filter::Space { origin: (0.0, 0.0), scale: (1.0, 1.0) });
            let painted = pixels.chunks_exact(4).filter(|p| p[3] != 0).count();
            println!("  allocated width={width}, painted pixels={painted}");
        }
    }
}
```

Compiled against the review checkout's existing artifacts:

```sh
rustc --edition=2021 /tmp/exact-review-20261112/filter-chain-bounds/main.rs \
  -L dependency=target/debug/deps \
  --extern exact_kernel=target/debug/deps/libexact_kernel-cd5dba534e1d4ea9.rlib \
  --extern exact_svg_raster=target/debug/deps/libexact_svg_raster.rlib \
  -o /tmp/exact-review-20261112/filter-chain-bounds/repro
/tmp/exact-review-20261112/filter-chain-bounds/repro
```

Observed:

```text
drop-shadow(10px 0 0 #000000): growth=(0.0, 0.0, 10.0, 0.0), primitives=1
  allocated width=20, painted pixels=40
  allocated width=30, painted pixels=40
drop-shadow(10px 0 0 #000000) drop-shadow(10px 0 0 #000000): growth=(0.0, 0.0, 10.0, 0.0), primitives=2
  allocated width=20, painted pixels=40
  allocated width=30, painted pixels=60
```

The one-filter control agrees at both allocations. With two filters, the
resolver's allocation loses 20 opaque pixels. This verifies the shared
resolver and rasterizer; no native window was launched.

## Direction

Propagate the current output bounds through each function before allocating
the chain's surface. Cover repeated shadows and a shadow followed by a blur,
on both box and SVG filter paths.

**Deduplication.** Checked open and closed filesystem issues and the all-state
GitHub issue snapshot. `issues/closed/20261110-svg-use-and-text-have-no-bounding-box.md`
fixed a missing initial object box. This report starts with a valid box and
concerns the cumulative expansion of a filter chain.
