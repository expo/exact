# Apple's compact number formatter rereads options after construction

**Status:** Closed
**Resolution:** The compact-number fallback copies the options it reads at format time when it is constructed, so changing the caller's options object afterwards changes neither its output nor its resolved options.
**Systems:** js/src/intl.js
**Severity:** P3
**Author:** Codex, repository review
**Date:** 2026-11-12

## Failure

At `f6be99aff76bc8bd881b0ec8daaa2c10ace0b9a2`, the compact-number fallback used by Apple's Hermes retains the caller's options object (`js/src/intl.js:236–242`). Each `format()` rereads its digit and sign options (lines 252–263, 284–298). Updating a shared options object for another formatter therefore changes an existing formatter's output, while `resolvedOptions()` still describes its original configuration.

`Intl.NumberFormat` resolves its options during construction and stores the resulting values in the formatter's internal slots. Formatting uses those values. See [ECMA-402, NumberFormat objects](https://tc39.es/ecma402/#numberformat-objects).

## Reproduction

This valid data module formats the same value before and after changing the object passed to its constructor:

```js
function probe() {
 var options = {notation: "compact", maximumFractionDigits: 1};
 var format = new Intl.NumberFormat("en-US", options);
 var before = format.format(12345);
 options.maximumFractionDigits = 0;
 var after = format.format(12345);
 return JSON.stringify({before:before,after:after,resolved:format.resolvedOptions().maximumFractionDigits});
}
globalThis.exact={abi:1,appId:"review.intl",grants:"",answer:probe};

```

Save it as `/tmp/exact-review-20261112/intl-options-retained/app.js`. Compile it with the pinned Hermes compiler, using the ordinary bytecode flags:

```sh
/Users/ccheever/.cargo/hermes-lean-sys/hermes-vanilla-d412d3bd8512-v5/bc71c7b398f1d41a5390761b75ce72e7be234aa48cb1ddec947224188ed6a4f2/bin/hermesc \
  -O -emit-async-break-check -Xes6-block-scoping -emit-binary \
  -out /tmp/exact-review-20261112/intl-options-retained/app.hbc \
  /tmp/exact-review-20261112/intl-options-retained/app.js
```

Save this caller as `/tmp/exact-review-20261112/intl-options-retained/main.rs`:

```rust
use exact_js::Module;
use exact_runner::DataSource;
fn main() {
 let bytes=std::fs::read("/tmp/exact-review-20261112/intl-options-retained/app.hbc").unwrap();
 let mut m=Module::loaded(bytes,"review.intl","").unwrap();
 m.set_budget_ms(f64::INFINITY);
 m.bind(&contract::compile("component App\n  resource probe = probe() as shape string\n  view\n    text probe\n").unwrap());
 println!("{}",m.query("probe",&[]).unwrap().as_str().unwrap());
}

```

Run it against the existing review artifacts:

```sh
rustc --edition=2021 /tmp/exact-review-20261112/intl-options-retained/main.rs \
  -L dependency=target/debug/deps \
  --extern exact_js=target/debug/deps/libexact_js-13cc590fe137e0bc.rlib \
  --extern exact_runner=target/debug/deps/libexact_runner-faa2c5b2aa381b98.rlib \
  --extern contract=target/debug/deps/libcontract-6c4999c7ae79fb84.rlib \
  -o /tmp/exact-review-20261112/intl-options-retained/repro
/tmp/exact-review-20261112/intl-options-retained/repro
```

Actual macOS Hermes executor output:

```json
{"before":"12.3K","after":"12K","resolved":1}
```

The unchanged JavaScript run in pinned Bun 1.4.2 gives:

```json
{"before":"12.3K","after":"12.3K","resolved":1}
```

Both formatting calls should retain the constructor's one-fraction-digit configuration. No product code was changed and no Cargo rebuild was needed.

## Direction

Store normalized option values owned by the formatter at construction, and use those values for every later formatting call. Cover reuse and mutation of the original options object.

**Deduplication.** Checked open and closed filesystem issues and the all-state GitHub issue snapshot; no matching report was found. The closed `20261110-cached-date-formatter-keeps-old-time-zone.md` concerns reuse of date formatters across system-zone changes; `20261010-apple-intl-number-format-half-even.md` concerns the engine's rounding rule. Neither covers number formatters retaining the caller's mutable options.
