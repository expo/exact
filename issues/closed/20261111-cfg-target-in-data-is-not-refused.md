# The bake does not refuse `cfg(target_*)` in `data/`, and three in-tree data crates use it

**Status:** Closed
**Resolution:** Ruled (Charlie, 2026-11-12; LLP 1030 D3a, §5 ruling 7 and LLP 1029 D4 amended in llp `93aa557e`): the bake does not refuse target branches or native dependencies in `data/`, since the compatibility id and module routing are both source digests that an edit to any arm moves; a test now pins that, and the docs and `docs/agent-pitfalls.md` say a replaceable module branches only on `target_arch = "wasm32"`.
**Systems:** bake/src/compat.rs (data_crate), apps/markdown/data, apps/exact-live/data, apps/update-lab/data
**Severity:** P2
**Author:** Claude (Opus 5.5), split from issues/closed/20261110-compat-id-misses-data-crate-path-dependencies.md
**Date:** 2026-11-11

## What is wrong

LLP 1030 §5 item 7 (ruled 2026-09-03) makes the data crate target-neutral by construction: "bake refuses `cfg(target_*)` and native dependencies in `data/`". Nothing in the bake refuses either. Appending this to `apps/markdown/data/src/welcome.rs` is accepted:

```
#[cfg(target_os = "macos")]      pub fn only_on_mac() -> u32 { 7 }
#[cfg(not(target_os = "macos"))] pub fn only_on_mac() -> u32 { 8 }
```

`exact-bake compat apps/markdown --platform macos --json` exits 0 with a new id.

## Why it is not simply implemented

Refusing as ruled would refuse three in-tree apps. Each data crate branches on `target_arch = "wasm32"`, because the browser has no host worker or threads:

- `apps/markdown/data/src/lib.rs:250`: `cfg!(target_arch = "wasm32")` parses in place where no worker runs.
- `apps/exact-live/data/src/runbook.rs:5,7`: picks `MarkdownStress` on wasm and the threaded `NativeMarkdownStress` elsewhere.
- `apps/update-lab/data/src/lib.rs:31`: `rustExecutor` reports `Wasm` or `Native`.

The `harness` data crate also depends on `rustix`, a native dependency the ruling refuses.

Where the data crate's source digest enters the id (`rust.mode` off), a target branch is already covered: the digest reads every branch, so an edit to one moves every platform's id. The hazard D3a names is the other case, where a wasm module's digest stands for native code, and there a `wasm32` branch is exactly the divergence it describes.

## Decision needed

Either amend the ruling (for example, refuse only where the wasm digest stands for native code, or admit `target_arch = "wasm32"`), or keep it and give a data crate a runtime way to learn it has no worker, then rewrite the three crates and implement the refusal with a message naming the file and line.
