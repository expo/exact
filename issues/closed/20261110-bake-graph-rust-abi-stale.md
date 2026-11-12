# The bake writes Rust ABI 2 into the graph of a binary that embeds a Rust pair; its cohort and the publisher carry 4

**Status:** Closed
**Resolution:** The graph's `rustAbi` was already `crate::compat::RUST_ABI` (f281b62c); the bake's `RUST_ABI` is now `exact_logic_abi::ABI` and the publisher reads the same declaration from `logic/abi/src/lib.rs`, so the number is written once.
**Systems:** bake/src/receipt.rs (artifact_graph), bake/src/compat.rs, scripts/rust.mjs, scripts/app.mjs (classifyArtifacts)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

The Rust module seam's version is written in three places, and one is stale:

- `bake/src/compat.rs:37`: `RUST_ABI = 4`. It becomes the cohort's `inputs.rustAbi` (`compat.rs:552`) and is what `stage_rust_bundle` demands of a supplied receipt (`bake/src/receipt.rs:362`).
- `scripts/rust.mjs:422`: `rustAbi:4`, in the requirements `rustBundle()` attaches at deploy and push.
- `bake/src/receipt.rs:585`: the literal `"rustAbi":2`, in the requirements `artifact_graph` writes for `app.plan` and both `rust/` rows when the bake embeds a Rust pair. That is a bake with `EXACT_RUST_BUNDLE`, which `prepareRustBundle` (`scripts/rust.mjs:35`) sets for a build from an `EXACT_UPDATE_RECEIPT` whose head carries `rust/` assets. The test at `receipt.rs:1144` pins the 2.

`classifyArtifacts` compares such a requirement with the cohort's input by equality (`scripts/app.mjs:1379`), so it refuses that graph against the cohort of the same bake.

Nothing classifies such a graph today, so nothing is refused yet. Deploy and push overwrite `app.plan`'s Rust requirements and add their own `rust/` rows with 4 (`scripts/rust.mjs:423-428`, `scripts/push.mjs:232-234`), and the dev loop classifies the web build's graph (`host/web/dev.mjs:1125`). The wrong number is in `artifacts.json` and the build receipt of the binary, and takes effect when a script first classifies that graph.

## The failure

A test beside `supplemental_rust_is_real_paired_bytes_with_executor_requirements`: stage the fixture pair, give the inputs what `compat.rs` gives a Rust cohort (`inputs["rustAbi"] = json!(crate::compat::RUST_ABI)`), build the graph, and compare `graph["artifacts"][0]["requires"]["rustAbi"]` with `inputs["rustAbi"]`:

```
assertion `left == right` failed
  left: Number(2)
 right: Number(4)
```

`classifyArtifacts` on that graph as the candidate, against the cohort receipt made from the same inputs:

```
"bundle": false,
"missing": [
  "app.plan: rustAbi (requires 2)",
  "rust/app.module.json: rustAbi (requires 2)",
  "rust/app.module.wasm: rustAbi (requires 2)" ]
```

Expected: `rustAbi` 4 in the graph, and nothing missing.

## Verification

Ran the test with `cargo test -p exact-bake --lib` (it failed as above) and called `classifyArtifacts` from `scripts/app.mjs` on the graph it wrote (output above). The callers of `classifyArtifacts` were read. Not run: a production build from a release receipt.

## Direction

Write `crate::compat::RUST_ABI` at `receipt.rs:585` and in the test, so the bake has one declaration. `scripts/rust.mjs:422` can read `build.compat.inputs.rustAbi` instead of its own literal.

## Related

No issue or QUEUE entry names `rustAbi`. `issues/closed/20260924-grant-readers-disagree.md` is about grant parsers.
