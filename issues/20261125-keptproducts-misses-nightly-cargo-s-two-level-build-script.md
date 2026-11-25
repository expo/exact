# `keptProducts` misses nightly Cargo's two-level build-script output path, recording `env: []` for all web wasm leaf modules and failing open on cache lookups

**Status:** Open
**Systems:** Cross-worktree web wasm build cache
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Fail-open defaults)

Verification: verified by running.

At `9c0431fd5`, `asked(spec)` in `scripts/kept.mjs:131-137` only checks single-level `<buildDir>/<dir>/output` files:

```js
const asked = (spec) => {
  const names = new Set();
  for (const builds of spec.buildDirs ?? []) for (const dir of existsSync(builds) ? readdirSync(builds) : []) {
    for (const m of (text(resolve(builds, dir, 'output')) ?? '').matchAll(/^cargo::?rerun-if-env-changed=(.+)$/gm)) names.add(m[1]);
  }
  return [...names].sort();
};
```

### Triggering sequence
1. Populate `~/.cache/exact/web-modules` for any of the four web wasm leaf modules (`markdown.wasm`, `motion.wasm`, `markup-editor.wasm`, `textflow.wasm`) via `bun host/web/build.mjs <app>` or `bun host/web-js/module.mjs --prebuild`.
2. Those four leaves compile with `WEB_TOOLCHAIN = 'nightly-2026-08-21'` (`scripts/cargo-check.mjs:45`, `host/web-js/module.mjs:120, 522-523, 537, 622-623, 636`), whose Cargo build-dir layout writes build-script stdout two levels deep at `<buildDir>/<pkg>/<hash>/run/stdout` rather than `<buildDir>/<pkg>-<hash>/output`.
3. Build in another checkout or environment where a build-script `cargo:rerun-if-env-changed` variable differs (for example, `ICU4X_DATA_DIR` emitted by `icu_properties_data` in `markdown.wasm`, or `LIBM_BUILD_VERBOSE` / `LIBC_BUILD_VERBOSE` / `RUST_LIBC_UNSTABLE_FREEBSD_VERSION` emitted by `compiler_builtins` and `libc` in `markdown.wasm` and `motion.wasm`).

### Actual vs. expected
- **Actual:** `asked(spec)` finds no `output` files under the nightly build directories and returns `[]`. `keep()` (`scripts/kept.mjs:178`) records `"env": []` in `~/.cache/exact/web-modules/<group>/<entry>/inputs.json`, and `find()` (`scripts/kept.mjs:155`) evaluates `kept.env.every(([k, v]) => (env[k] ?? null) === v)` vacuously to `true` — returning a cached wasm binary even when a watched build-script env var changed.
- **Expected:** `asked(spec)` records every `cargo:rerun-if-env-changed` variable emitted by the product's build scripts in both stable (`<buildDir>/<pkg>-<hash>/output`) and nightly (`<buildDir>/<pkg>/<hash>/run/stdout`) Cargo layouts, as required by `scripts/kept.mjs:17, 87-88, 130` and already implemented in siblings `scripts/tools.mjs:326-335`, `scripts/cargo-dep-info.mjs:382, 533, 661-665`, `scripts/app.mjs:1053-1054`, and `host/web/dev.mjs:170-176`.

### Verification & siblings checked
- **Verified by running:**
  1. Inspected real nightly build outputs in this checkout: `target/web-js-modules/target-markdown/wasm32-unknown-unknown/release/build/icu_properties_data/09c45dcdb6c411ee/run/stdout` emits `ICU4X_DATA_DIR`, alongside `compiler_builtins/.../run/stdout` (`LIBM_BUILD_VERBOSE`) and `libc/.../run/stdout` (`LIBC_BUILD_VERBOSE`, `RUST_LIBC_UNSTABLE_FREEBSD_VERSION`); `target-motion` has the same `compiler_builtins` and `libc` build outputs.
  2. Inspected `~/.cache/exact/web-modules/*/*/inputs.json` on this machine: every stable `exact-web-js` entry captures 28–110 env vars, while every `markdown.wasm`, `motion.wasm`, `markup-editor.wasm`, and `textflow.wasm` entry has `env: []`.
  3. Ran a standalone Bun probe invoking `keptProducts` from `scripts/kept.mjs` on a temporary repo with `build/icu_properties_data/09c45dcdb6c411ee/run/stdout` declaring `cargo:rerun-if-env-changed=ICU4X_DATA_DIR`: `keep()` succeeded with `recordedEnv: []`, and `find()` in a second checkout with a different `ICU4X_DATA_DIR` returned a cache hit (`hitDifferentEnv: true`).
- **Siblings checked:** `scripts/tools.mjs:326-335`, `scripts/cargo-dep-info.mjs:382, 533, 661-665`, `scripts/app.mjs:1053-1054`, and `host/web/dev.mjs:170-176` all handle both layouts. `host/apple/modules.mjs:32-38` is the other caller of `keptProducts` (currently using stable Cargo's single-level layout; fixing `scripts/kept.mjs` future-proofs it too). `host/apple/crates.mjs` clones Cargo unit directories/fingerprints directly without parsing `output` for env vars.

### Suggested fix & regression test
Update `asked(spec)` in `scripts/kept.mjs:131-137` to read both `<buildDir>/<unit>/output` (stable layout) and `<buildDir>/<pkg>/<hash>/run/stdout` (nightly layout), reusing `buildScriptOutput` from `scripts/cargo-dep-info.mjs:533` or scanning nested hash directories when `output` is absent. Add a unit test in `scripts/kept.test.mjs` creating a nightly `<buildDir>/<pkg>/<hash>/run/stdout` with `cargo:rerun-if-env-changed=TEST_VAR` and asserting that `keep()` records `TEST_VAR` and `find()` misses when `TEST_VAR` changes.

### Siblings checked

scripts/tools.mjs:326-335, scripts/cargo-dep-info.mjs:382,533,661-665, scripts/app.mjs:1053-1054, host/web/dev.mjs:170-176 (all handle both layouts); host/apple/modules.mjs:32-38 (other keptProducts caller, stable Cargo); host/apple/crates.mjs (clones fingerprints directly)
