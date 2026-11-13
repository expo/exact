# Native call and later arguments round JavaScript numbers to a neighboring double

**Status:** Closed
**Resolution:** `native.call` and `native.later` arguments are read with exact-js-value's correctly rounding JSON reader, as replies and overlays are, so each number arrives with the bits JavaScript held.
**Systems:** js/src/door.rs, js/src/lib.rs
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `09cb236d64007f51a668e38926fda6f64dfb6fb5`, the Rust native-module adapter parses a synchronous `native.call` argument with `serde_json::from_str` (`js/src/door.rs:138`) and an asynchronous `native.later` argument with `serde_json::from_slice` (`js/src/lib.rs:706`). The configured reader does not correctly round every JSON number. A number JavaScript serializes can therefore arrive at the native module as a different double.

## The failure

A data source calls each of:

```js
native.call({n: 985.6906946328695});
native.later({n: 985.6906946328695});
```

The native implementation reads `request["n"].as_f64()`. Expected: `985.6906946328695`, bits `0x408ecd868ae85d94`. Both handlers receive `985.6906946328696`, bits `0x408ecd868ae85d95`, one ulp higher. This occurs before the native module does any work.

## Verification

Compiled a temporary JavaScript fixture beside `js/tests/fixtures` with the checkout's pinned `hermesc`, then ran:

```sh
CARGO_BUILD_JOBS=5 cargo test -p exact-js --test review_findings -- --nocapture
```

The temporary test configured and activated a real `Module` with a `NativeModule` whose synchronous and asynchronous handlers print and echo the request. It used the native request handler and `Module::parse` to complete the asynchronous call. It printed:

```
call received 985.6906946328696 bits=408ecd868ae85d95
call answer: Now(Str("985.6906946328696"))
later received 985.6906946328696 bits=408ecd868ae85d95
later answer: Now(Str("985.6906946328696"))
```

The equality check counted two wrong results and failed. The test and JavaScript fixture were removed. This exercises the Rust native-module adapter; the hosted Swift callback path passes bytes directly and was not tested.

## Direction

Parse both argument paths with correctly rounded numbers, preserving their bits when constructing the JSON value handed to `NativeModule`. Cover both handlers with the same round-trip values used by the overlay tests.

## Related

`issues/closed/20261110-overlay-and-large-reply-numbers-read-by-serde-json.md` was fixed by `d072224ece90e35779edbe23f5821ed787a66822` ("An overlay's value and a reply holding a long string keep their numbers exactly"). That commit changes overlay and reply decoding, not native-call arguments. No matching open issue or QUEUE entry was found.
