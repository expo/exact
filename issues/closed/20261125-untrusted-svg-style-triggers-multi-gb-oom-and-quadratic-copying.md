# Untrusted SVG <style> triggers multi-GB OOM and quadratic copying in css::rules

**Status:** Closed
**Resolution:** Replaced quadratic string copying and cloned declarations in exact-svg-file with a cursor-based indexed Sheet, shared declaration blocks, O(1) attribute merging, and hard stylesheet/matching/warning bounds.
**Systems:** exact-svg-file (Apple, Linux, Windows, and Android SVG image loaders; contract-lower bundled check)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P1
**Related:** LLP 1184 (Quadratic work, stack depth and allocation)

Verification: verified by reading.

At `9c0431fd5`, `exact_svg_file`'s stylesheet parser (`host/svg-file/src/css.rs:93-145`) and lowerer (`host/svg-file/src/lower.rs:166-186,435-437`) have three unbounded amplification / quadratic-work flaws reachable from untrusted remote and `data:` SVG images before `exact_kernel::svg::scene::Budget` ever runs:

1. **Multiplicative OOM in `css::rules` (`host/svg-file/src/css.rs:131-142`)**: `declarations(&body)` is parsed once into a `Vec<(String, String)>`, then cloned (`declarations: declarations.clone()`, line 136) for every comma-separated selector in `selectors.split(',')`, with no limit on rules, selectors, cloned declarations, or `dropped` warnings (`line 139`, copied into `self.warnings` at `lower.rs:179-181`).
2. **Quadratic string copying in `css::rules` (`host/svg-file/src/css.rs:106,123`)**: `rest` is a `String` (`strip_comments(text)` at line 95), and every `;`-terminated at-rule (line 106: `rest = lead[end + 1..].to_string()`) or `{...}` rule/block (line 123: `rest = rest[open + close + 1..].to_string()`) reallocates and copies the entire remaining stylesheet.
3. **Unbounded per-element rule scan and merge (`host/svg-file/src/css.rs:162-174`, `host/svg-file/src/lower.rs:398-405,435-437`)**: `Lowerer::attributes` calls `css::matching(&self.rules, ...)` for every element (up to `MAX_ELEMENTS = 50_000`, `lower.rs:16`), linearly scanning and sorting all `self.rules`, and linearly scanning `merged` for each declaration.

### Trigger
Load any remote or `data:` SVG image (well within `MAX_DATA_URL_BYTES = 1 MiB` and `MAX_ENCODED_BYTES = 64 MiB`, `host/raster/src/types.rs:14,20`) through:
- Painted hosts (Linux, Windows, Android): `host/painted/src/image/svg.rs:207` (`decode` -> `exact_svg_file::load(text)`).
- Apple hosts (iOS, macOS, tvOS): `host/apple/src/svg_image.rs:22,129` (`exact_svg_image_scene` -> `exact_svg_file::load(text)`, called by `host/apple/Sources/ExactImages/RasterImage.swift:499`).
- Compiler build check for bundled assets: `contract/lower/src/bundled.rs:158` (`exact_svg_file::parse`).

Example ~100 KB payload (only 2 XML elements, depth 1 — passes `xml::tokenize`, `MAX_ELEMENTS = 50_000`, and `MAX_DEPTH = 256` in `lower.rs:16,18`):
`<svg><style>a,a,...(25,000 times){f:1;f:1;...(12,500 times)}</style></svg>`
Or a ~1 MiB stylesheet of `a{}` repeated ~349,000 times.

### Actual vs. expected
- **Actual:** The ~100 KB selector×declaration payload allocates 25,000 × 12,500 = 312.5 million `(String, String)` pairs (>15 GB of heap memory, aborting the process with OOM) inside `Lowerer::read_sheets` (`lower.rs:34,178`) before `Kernel::apply` or `exact_kernel::svg::scene::Budget` runs. A ~1 MiB `a{}a{}...` stylesheet copies ~183 GB of strings in `css::rules` (`css.rs:123`).
- **Expected:** Untrusted SVG images are strictly bounded so a small file cannot exhaust memory or CPU (LLP 1011.001 D2: *"Bounds, since a remote file is untrusted"*, matching `MAX_ELEMENTS = 50_000` / `MAX_DEPTH = 256` in `host/svg-file/src/lower.rs:15-18`, entity-declaration refusal in `host/svg-file/src/xml.rs:159-170`, and `kernel/src/svg/scene/budget.rs:1-54`).

### Verification & sibling sweep
Verified by reading `host/svg-file/src/css.rs`, `host/svg-file/src/lower.rs`, `host/svg-file/src/xml.rs`, `host/painted/src/image/svg.rs`, and `host/apple/src/svg_image.rs`. Sibling parsers checked:
- `host/svg-file/src/css.rs:176-188` (`strip_comments`) and `host/svg-file/src/xml.rs:105-112,335-378` (`Tokenizer`, `decode`): advance `&str` slices in O(N) without reallocating the remainder per iteration.
- `host/svg-file/src/css.rs:149-158` (`declarations`): splits `&str` in O(N), though `lower.rs:398-405,438-442` still merges inline `style=""` declarations via O(M²) linear scan on `merged`.
- `contract/lower/src/icon_svg.rs:177`: uses `exact_svg_file::xml::tokenize` only and does not invoke `css::rules`. `host/svg-file` is the sole runtime untrusted SVG stylesheet parser (Web delegates to the browser).

### Suggested fix & regression test
In `host/svg-file/src/css.rs`: (1) iterate over a `&str` cursor on the comment-stripped buffer instead of reallocating `rest` with `.to_string()` on every rule; (2) store parsed declaration blocks once (e.g. `Rc<[(String, String)]>` or indexed into a `Vec<Vec<(String, String)>>`) so comma-separated selectors share declarations without cloning; (3) enforce hard caps in `read_sheets` / `css::rules` (total `<style>` bytes, rules/selectors, total declarations, and warnings — returning an `Error` when exceeded) and index rules by tag/class/id or bound total rule-matching work in `lower.rs` (using a `HashMap` or bounded merge for `merged`). Add regression tests in `host/svg-file/src/css.rs` and `host/svg-file/src/lib.rs` feeding a ~100 KB selector×declaration fan-out and a large `a{}` stylesheet to `load()`, asserting fast bounded completion/refusal.

### Siblings checked

Checked css::strip_comments (css.rs:176-188), xml::Tokenizer & xml::decode (xml.rs:105-112,335-378) which all use O(N) &str cursors; contract/lower/src/icon_svg.rs:177 (does not parse CSS); noted lower.rs:398-405 linear merge on inline style="". host/svg-file is shared across Apple (host/apple/src/svg_image.rs:22) and painted hosts (Linux, Windows, Android via host/painted/src/image/svg.rs:207).
