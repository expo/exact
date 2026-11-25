# Unbounded recursion (stack overflow) and O(N²) allocation in `roles::role` and `Paint::parse`

**Status:** Closed
**Resolution:** Make roles::role and Paint::parse non-recursive and allocation-free on nested var(--exact-accent, ...) and repeated url(...) inputs.
**Systems:** kernel CSS color role parser & SVG paint parser
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Quadratic work, stack depth and allocation)

Verification: verified by reading.

At `9c0431fd5`, two CSS/SVG value parsers recurse without a depth bound on attacker- or data-controlled strings well below `MAX_STRING_BYTES` (`1 << 24` = 16 MiB at `kernel/src/wire/codec.rs:20` and `kernel/src/style.rs:534`):

1. **`roles::role(text)` (`kernel/src/style/roles.rs:18-28`):** allocates `let lower = text.to_ascii_lowercase();` on every call and unconditionally recurses `role(name)` whenever `lower` starts with `"var(--exact-accent,"` and ends with `')'`. Each wrapper is 20 bytes (`"var(--exact-accent,"` + `")"`), so `N` nested wrappers (~`20*N` bytes; 1 MB at `N = 50,000`) recurse `N + 1` frames deep while holding `N` live `String` copies simultaneously totaling `10 * N * (N + 1)` bytes (~25 GB at `N = 50,000`, ~1 GB at `N = 10,000`).
2. **`Paint::parse(css)` (`kernel/src/svg/mod.rs:294-308`):** matches `url(...)` and unconditionally evaluates `Paint::parse(&t[close + 1..])` *before* evaluating the match guard `_ if t[close + 1..].trim().is_empty()`, with no depth limit, even though any nested `Some(Paint::Url(..))` is always rejected by `_ => return None`. Input `"url(#a)".repeat(50_000)` (~350 KB) recurses 50,000 stack frames deep before unwinding.

### Reachable trigger
Both functions are directly reachable from real untrusted and runtime entry points:
- **SVG image files (`host/svg-file/src/lib.rs:222, 243` -> `host/svg-file/src/lower.rs:487` -> `StyleProps::set_dynamic`):** a single-element SVG image such as `<svg><rect fill="url(#a)url(#a)..."/></svg>` (~350 KB) or `<svg><rect color="var(--exact-accent,var(--exact-accent,..."/></svg>` (~200 KB–1 MB) bypasses `host/svg-file`'s `MAX_ELEMENTS`, `MAX_DEPTH`, and scene `Budget` limits and hits `Paint::parse` / `ColorValue::parse_light_dark` (`kernel/src/style.rs:849`) -> `roles::role`.
- **Dynamic style bindings (`runner/src/bridge.rs` -> `StyleProps::set_dynamic`):** any dynamic color, shadow, gradient, filter, `fill`, `stroke`, or `clip-path` (`kernel/src/clip.rs:36`, which delegates to `Paint::parse`) value from app data.
- **EXWF wire / snapshot decoding (`Kernel::apply_frame` -> `StyleProps::decode_patch`, `kernel/build/codec.rs:477`):** `CssValue` rows (`Paint`, `ClipPath`, `BoxShadow`, `TextShadow`, `Filter`, `BackgroundImage`, `MaskImage`, `SymbolPalette`) decode a string up to `MAX_STRING_BYTES` and pass it directly to `Paint::parse` / `ClipPath::parse` / `ColorValue::parse_light_dark` -> `roles::role`. (Note: `Reader::color_value()` at `kernel/src/wire/codec.rs:248-260` uses binary tags rather than `parse_light_dark`, so wire reachability goes through `CssValue` rows.)

### Actual vs. expected
- **Actual:** Crafted ~200 KB–1 MB strings abort the host process via stack overflow or O(N²) OOM in `roles::role`, and ~350 KB strings abort via stack overflow in `Paint::parse` (and transitively `ClipPath::parse`).
- **Expected:** `reference_css` (`kernel/src/style/roles.rs:752-763`) never nests `var(--exact-accent, ...)` inside itself, and SVG 2 §13.2 paint syntax allows at most one `url()` paint server followed by a non-`url()` fallback. Both parsers should reject invalid nested forms cleanly (`None`) in O(N) time and O(1) stack space, matching the hardening already applied to `host/svg-file` and `kernel/src/style/calc/parse.rs`.

### Verification & siblings checked
Verified by reading `kernel/src/style/roles.rs:18-53, 752-763`, `kernel/src/style.rs:534, 833-862`, `kernel/src/svg/mod.rs:294-321`, `kernel/src/clip.rs:31-41`, `kernel/build/codec.rs:477`, and `host/svg-file/src/lower.rs:487`.
Siblings checked:
- `kernel/src/clip.rs:36` (`ClipPath::grammar`): delegates directly to `Paint::parse`, so fixing `Paint::parse` fixes `clip-path` as well.
- `kernel/src/style/roles.rs:30-32`: calls `role("AccentColorText")` with a fixed literal (depth 1).
- `kernel/src/style/roles.rs:643` (`parse_platform`): iterative, non-recursive.
- `kernel/src/style/theme.rs:222, 436`: `measure_at` caps `depth < 8`; `resolved` uses the `INSIDE` thread-local guard (depth 1).
- `kernel/src/style/calc/parse.rs:235-259, 500-537`: `calc`/`min`/`max`/`clamp`/`(...)` bound depth via `self.deeper()`; `var()` bounds nesting via `self.count(p)`.
- `kernel/src/svg/mod.rs:389, 438, 478` (`PaintOrder::parse`, `MarkerRef::parse`, `DashArray::parse`) and `kernel/src/svg/filter.rs:96` (`FilterList::grammar`): all iterative.

### Suggested fix & regression test
- In `roles::role` (`kernel/src/style/roles.rs:18-53`), split out a non-recursive inner lookup helper `role_flat(text: &str) -> Option<u8>` (or check `lower.strip_prefix("var(--exact-accent,")` once and look up the trimmed inner name directly without recursing or reallocating).
- In `Paint::parse` (`kernel/src/svg/mod.rs:294-321`), split out a non-recursive `parse_non_url(t: &str) -> Option<Paint>` helper; in the `url(` branch, check `let rest = t[close + 1..].trim();` first, return `PaintFallback::Default` immediately when `rest.is_empty()`, and otherwise match `parse_non_url(rest)` so a second `url(` is rejected in O(1) without recursion.
- Add unit tests in `kernel/src/style/roles.rs` and `kernel/src/svg/tests.rs` passing `"var(--exact-accent,".repeat(50_000) + &")".repeat(50_000)` to `roles::role` (and `ColorValue::parse_light_dark`) and `"url(#a)".repeat(50_000)` to `Paint::parse` and `ClipPath::parse`, asserting `None` promptly without overflowing the stack.

### Siblings checked

ClipPath::grammar (kernel/src/clip.rs:36, inherits Paint::parse recursion), roles::role accent-text arm (kernel/src/style/roles.rs:30-32, fixed literal), roles::parse_platform (kernel/src/style/roles.rs:643, iterative), theme::measure_at & resolved (kernel/src/style/theme.rs:222, 436, depth < 8 and INSIDE guard), style::calc::parse (kernel/src/style/calc/parse.rs:235-259, bounded by deeper/count), PaintOrder/MarkerRef/DashArray/FilterList::grammar (kernel/src/svg/mod.rs:389-496, kernel/src/svg/filter.rs:96, iterative).
