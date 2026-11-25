# SVG filter chains hit `u16` underflow panic past 65,535 primitives and `feConvolveMatrix` wire desync

**Status:** Closed
**Resolution:** Export and enforce shared SVG filter limits (MAX_PRIMITIVES = 4096) with checked u16 indices, validate feConvolveMatrix and sibling op limits against Filter::decode, and guard Filter::encode and light::convolve.
**Systems:** kernel SVG filter resolver & `svg-filter` wire codec
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Numeric narrowing, Unicode slicing and escaping)

Verification: verified by reading.

At `9c0431fd5`, SVG filter chain resolution and wire encoding have two defects:

1. **Unchecked `u16` narrowing and underflow past 65,535 primitives:**
   - `Chain::source` (`kernel/src/svg/scene/filter.rs:128-133`), `Chain::push` (`kernel/src/svg/scene/filter.rs:135-143`), and `Resolver::filter_element` (`kernel/src/svg/scene/filter.rs:458, 491-492, 701, 760, 763`) cast `chain.primitives.len() as u16` and subtract `1`.
   - `FilterList::grammar` (`kernel/src/svg/filter.rs:96-180`) and `box_filter` (`kernel/src/svg/scene/filter.rs:297-322`) enforce no primitive count cap at all. `Resolver::filter` (`kernel/src/svg/scene/filter.rs:356-387`) only charges `Budget::primitives` (`kernel/src/svg/scene/budget.rs:21, 27, 211`), where `MAX_COST = 1_000_000` and `ITEM = 8` admit up to 125,000 primitives.
   - At `primitives.len() == 65_536`, `as u16` wraps to `0`, so `0u16 - 1` panics with integer underflow in debug builds (and wraps to `Input::Result(65535)` in release builds, corrupting the filter DAG; at line 763 a 65,536-primitive `<filter>` also compares `chain.primitives.len() as u16 == base` and is falsely treated as empty). Meanwhile `Filter::decode` (`svg-filter/src/lib.rs:370`) hard-caps primitive count at `4096`, silently dropping 4,097..65,535-primitive filters across the Apple raster ABI.

2. **`feConvolveMatrix` encode/decode wire desync:**
   - `Resolver::filter_element` (`kernel/src/svg/scene/filter.rs:635-661`) constructs `Op::Convolve` with `order = (ox.max(1.0) as u32, oy.max(1.0) as u32)`, raw `kernel = numbers(prop(PropId::KernelMatrix))`, and unchecked `target`, without verifying `kernel.len() == ox * oy`, `ox <= 64`, `oy <= 64`, or `target < order`.
   - `Filter::encode` (`svg-filter/src/lib.rs:331-342`) writes `[order.0, order.1, ...c.kernel, divisor, bias, target.0, target.1, edge, preserve_alpha]` with **no length prefix** for `c.kernel`, whereas `Filter::decode` (`svg-filter/src/lib.rs:450-465`) always reads `order.0 * order.1` floats for `kernel` and requires `order <= 64` and `target <= 63`.

### Reachable trigger
- **Underflow:** Pass a CSS `filter` string of 65,536 `blur()` calls (~393 KB, well below `MAX_STRING_BYTES = 16 MiB`) on an HTML box (`box_filter`, called via `host/painted/src/paint/box_filter.rs:36` and `host/apple/src/style.rs:468-471`) or an SVG element (`Resolver::filter`, reachable from inline `<svg>` or `host/svg-file`), OR reference a 64-primitive `<filter id="f">` 1,024 times via `filter="url(#f) ..."` (~8 KB SVG file).
- **Wire desync:** Render any inline `<svg>` or SVG image file containing `<feConvolveMatrix>` whose `kernelMatrix` length differs from `orderX * orderY` (for example, default `order="3 3"` with 4 entries, followed by another primitive like `<feFlood flood-color="red"/>`), or with `order > 64` / `targetX >= 64`, on Apple (`host/apple/src/svg.rs:658` -> `host/apple/Sources/ExactSvg/SvgIsland.swift:227-230` -> `host/svg-raster/src/abi.rs:51` -> `Filter::decode`).

### Actual vs. expected
- **Actual:**
  1. Chains reaching 65,536 primitives panic on `u16` underflow in debug builds or silently wrap/corrupt inputs in release builds; chains of 4,097..65,535 primitives decode to `None` on Apple and drop the filtered element.
  2. A malformed `<feConvolveMatrix>` desynchronizes `Filter::encode`'s float stream: `Filter::decode` misreads `divisor`/`bias`/`target` and trailing primitives as kernel entries or returns `None`, causing `exact_svg_raster_filter` (`host/svg-raster/src/abi.rs:51`) to return `1` and `SvgIsland.swift:230` to drop the entire filter chain and element—diverging from Linux's direct `light::convolve` (`host/svg-raster/src/filter/light.rs:13`), which clears only that primitive. (Note also that `order="1e20 1e20"` overflows `(ox * oy)` as `isize` in `light.rs:11-13`.)
- **Expected:** Filter chains enforce a shared primitive limit compatible with `Input::Result(u16)` and `Filter::decode`'s cap (4,096), and `Filter::decode(&filter.encode())` is a faithful inverse (`svg-filter/src/lib.rs:366`) for every filter the kernel resolves, matching Filter Effects 1 `feConvolveMatrix` validation.

### Verification & siblings checked
Verified by reading `kernel/src/svg/scene/filter.rs:128-143, 297-322, 356-387, 458-496, 635-661, 700-767`, `kernel/src/svg/filter.rs:96-180`, `kernel/src/svg/scene/budget.rs:21, 27, 211`, `svg-filter/src/lib.rs:331-342, 370, 450-465`, `host/apple/src/svg.rs:658`, `host/apple/src/style.rs:468-473`, `host/apple/Sources/ExactSvg/SvgIsland.swift:227-230`, `host/svg-raster/src/abi.rs:51`, and `host/svg-raster/src/filter/light.rs:11-15`.
Siblings checked:
- `svg-filter/src/lib.rs:292-295, 394` (`Op::Merge`), `307-315, 422` (`Op::ComponentTransfer`), and `322-329, 444` (`Op::Turbulence`): `Merge` and `ComponentTransfer` do encode length prefixes (unlike `Convolve`), but `kernel/src/svg/scene/filter.rs` does not clamp `feTurbulence` `numOctaves` (`filter.rs:613` vs `r.n(64)` in decode), `feMerge` input count (`filter.rs:543` vs `r.n(4096)`), or `feComponentTransfer` `tableValues` length (`filter.rs:591-592` vs `r.n(4096)`), so those can also fail `Filter::decode` on Apple.
- `host/apple/Sources/ExactEffects/SvgFilterMetal.swift:94` (`n < 64`) and `SvgFilterGPU.swift:96` (`count < 4096`): reject chains over their GPU limits and fall back to `SvgRasterModule` (`exact_svg_raster_filter`).

### Suggested fix & regression test
- Export the shared primitive cap (`pub const MAX_PRIMITIVES: usize = 4096;`) from `exact-svg-filter`, enforce it in `FilterList::grammar`, `box_filter`, `Chain::push`, and `Resolver::filter` / `Resolver::filter_element` (stopping/refusing once reached so `primitives.len()` never approaches `u16::MAX` or diverges from `Filter::decode`), and use `u16::try_from` instead of `as u16 - 1`.
- In `Resolver::filter_element`'s `"feConvolveMatrix"` arm (`kernel/src/svg/scene/filter.rs:635-661`), validate `1 <= ox <= 64`, `1 <= oy <= 64`, `kernel.len() == (ox * oy) as usize`, and `target.0 < ox && target.1 < oy` (replacing invalid primitives with transparent `Op::Flood([0.0; 4])` per Filter Effects 1 / `light.rs:13`), and/or guard `Filter::encode` (and align `feTurbulence` octaves / `feMerge` / `feComponentTransfer` bounds with `Filter::decode`).
- Add regression tests: (1) parsing/resolving 65,536 `blur()` calls via `box_filter` and `Resolver::filter` (and repeated `url(#f)`) never panics and stays within `MAX_PRIMITIVES`; (2) resolving `<feConvolveMatrix>` with mismatched `kernelMatrix` length, `order > 64`, and out-of-range `targetX`/`targetY` round-trips cleanly through `Filter::decode(&filter.encode()) == Some(filter)`.

### Siblings checked

Checked sibling filter ops in svg-filter/src/lib.rs vs kernel/src/svg/scene/filter.rs: Op::Merge (lines 292-295, 394) and Op::ComponentTransfer (307-315, 422) encode length prefixes unlike Convolve, though filter.rs does not clamp Merge inputs (4096), Transfer tableValues (4096), or feTurbulence numOctaves (64 at lib.rs:444 vs uncapped filter.rs:613). Also checked host/svg-raster/src/filter/light.rs:11-15 (isize overflow on huge ox*oy), host/apple/src/style.rs:468-473, host/painted/src/paint/box_filter.rs:36, and SvgFilterMetal.swift:94 / SvgFilterGPU.swift:96.
