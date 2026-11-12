# The painting host refuses every dev envelope that offers a `tiered` Rust variant

**Status:** Closed
**Resolution:** `fetch_changed` accepts `tiered` beside `native` and `wasm`, takes the variant its `rustMode` names, compares the target for every variant but `wasm`, and still refuses an unknown key.
**Systems:** host/linux/src/fetch.rs, scripts/rust.mjs, host/apple/Sources/ExactKit/PlanURL.swift
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`fetch_changed` treats any key of the envelope's `rust` object other than
`native` or `wasm` as a malformed manifest (`host/linux/src/fetch.rs:454`):

```rust
if variants.keys().any(|k| k != "native" && k != "wasm") {
    return Err("invalid Rust executor manifest".into());
}
```

The dev producer publishes a third key. When the app opts any desktop platform
into `tiered` (LLP 1029.000 §4.1: "Development publishes a `tiered` variant
beside native/Wasm"), `buildRust` adds `variants.tiered`
(`scripts/rust.mjs:363-366`) and `rustCards` (`scripts/rust.mjs:394-406`)
writes it into the one envelope every client reads (`host/web/dev.mjs:462`,
`497`). The Apple loader accepts the three keys and takes the one matching
its mode and target (`host/apple/Sources/ExactKit/PlanURL.swift:650`, `674`).

The Linux display, the Android reader, the stdio agent and the Windows host
all take a dev URL through `fetch_changed`, so each refuses the whole
generation, plan included, even when the variant it wants is there. A binary
baked `tiered` (`contract/cli/src/manifest.rs:75`) loads its pair from a
local `EXACT_PLAN` directory (`host/linux/src/delivery.rs:77`) and never
from a URL.

## The failure

An app with a Rust module and
`"rust": {"platforms": {"macos": {"dev": "tiered"}}}`, as Update Lab has
(`apps/update-lab/app.json:19`), started on the painting host with
`EXACT_DEV_PLAN=http://…`, prints
`exact url: invalid Rust executor manifest; booting the baked plan`, then
`exact url: invalid Rust executor manifest; keeping the running app` on every
poll, and never takes an edit.

Expected: the host selects the variant matching its own `rustMode` and target
and ignores the others, as the Apple loader does.

No app in the repository reaches this today: Update Lab has no Linux, Android
or Windows host, and Caltrain, which has them, does not opt into `tiered`.

## Verification

A test appended to `fetch.rs` served an envelope whose `rust` object had the
given keys (`native` and `tiered` with target `aarch64-apple-darwin`) and
called `fetch_changed(page, compat, None)`:

```
keys wasm, native          binary wasm                        -> Ok
keys wasm, native          binary native aarch64-apple-darwin -> Ok
keys wasm, native, tiered  binary wasm                        -> Err("invalid Rust executor manifest")
keys wasm, native, tiered  binary native aarch64-apple-darwin -> Err("invalid Rust executor manifest")
keys wasm, native, tiered  binary tiered aarch64-apple-darwin -> Err("invalid Rust executor manifest")
```

A presenter polling the three-key envelope printed the `keeping the running
app` line six times in 3.2 s.

The producer was read, not run: `rustPolicy` on Update Lab's manifest answers
`tiered` for macOS, the condition at `scripts/rust.mjs:363`, and
`bun test scripts/rust.test.mjs -t tiered` passes.

## Direction

Accept `tiered` among the known keys, select by `mode`, and compare the target
for every variant but `wasm` (`fetch.rs:460` compares it only for `native`).
An unknown key should still be refused.

## Related

"A dev generation the painting host refuses is downloaded and tried again
twice a second" is why this refusal repeats, with a plan download, on every
poll. Neither fix covers the other. QUEUE's "Rust replacement latency
follow-up" covers tiered load timing on macOS, not the envelope's keys.
