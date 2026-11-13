# A finite but oversized terminal timeout panics before boot

**Status:** Open
**Systems:** host/terminal
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P3

At `f6be99af`, `host/terminal/src/argv.rs:201-203` accepts every finite,
nonnegative floating-point value for `--timeout`. Print mode then converts it
with `Duration::from_secs_f64` and adds it to an `Instant` without checking
either operation (`host/terminal/src/ending.rs:108-119`). Some accepted values
cannot be represented as a duration or deadline, so malformed command-line
input panics instead of receiving the normal usage error.

The actual checkout binary reproduces this with any valid entry:

```sh
target/debug/exact-terminal entry.contract --json --timeout 1e300
```

It exits 101 before boot, writes no JSON result, and reports:

```text
cannot convert float seconds to Duration: value is either too big or NaN
```

Validate the conversion and deadline range while parsing the option, including
the shutdown grace added to the deadline. Return the existing command-line
error for out-of-range values. Add cases for an oversized finite duration and
a duration that is representable but cannot be added to the current instant.

Duplicate check: searched open and closed filesystem issues and all 275 cached
GitHub issue titles and bodies for terminal timeout overflow, panic and range
validation. No matching report was found.
