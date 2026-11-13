# logic ABI's web wire test runs a script Bun refuses to parse

**Status:** Open
**Systems:** logic/abi
**Severity:** P3
**Author:** Claude (Opus 5.5), async lane triage
**Date:** 2026-11-13

`exact-logic-abi`'s `tests::web_wire_enum_strings_cross_calls_and_nested_overlay_replies`, an `async lane:` test, runs a script with `bun --eval` (`logic/abi/src/tests.rs`, near line 850), and Bun 1.4.2 refuses it before it runs:

```
SyntaxError: Unexpected token '{'. import call expects one or two arguments.
      at <parse> (6:1)
```

## How to reproduce

```
cargo test -p exact-logic-abi --lib -- --ignored web_wire_enum_strings_cross_calls_and_nested_overlay_replies
```

Fails at 69668888a on the async lane's Mac, at f932ba81a on a second Mac, and in the lane's run at afde7b25e.
