# Parsing JSON objects in reverse key order takes quadratic time

**Status:** Closed
**Resolution:** A parsed or collected object gathers its members and sorts them once, keeping the last of a repeated key, so key order no longer makes parsing quadratic.
**Systems:** js/value/src/json.rs, js/value/src/parse.rs, js/src
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At audited commit `f6be99af`, `json::parse` inserts every object member into a
sorted vector (`js/value/src/parse.rs:132-136`). Each new key moves the vector's
tail (`js/value/src/json.rs:132-136`). An object whose distinct keys arrive in
descending order therefore moves N(N−1)/2 entries. JSON member order should not
turn a modest payload into a long stall.

This parser handles native module calls (`js/src/door.rs:138`,
`js/src/lib.rs:727`), every overlay reply (`js/src/lib.rs:1152`), and replies
with captured large strings (`js/src/reply.rs:15-22`). Ordinary shaped replies
use a different direct decoder; the finding does not claim that every reply
pays this cost.

## Verification

A temporary Rust probe parsed objects with keys `k000000` onward and numeric
zero values, in ascending and descending order. Input generation and dropping
the result were outside the timed interval; each result's member count was
checked. The unchanged `js/value/src/lib.rs` was compiled with `rustc -C
opt-level=3`, linking the existing checkout dependencies, without a Cargo
rebuild. Median of three parses on this Mac, milliseconds:

| Members | JSON bytes | Ascending | Descending |
| ---: | ---: | ---: | ---: |
| 10,000 | 120,001 | 1.246 | 42.626 |
| 20,000 | 240,001 | 2.192 | 171.178 |
| 40,000 | 480,001 | 4.606 | 696.908 |

The roughly fourfold cost per doubling matches repeated vector insertion.
These are isolated parser timings, not whole application latency measurements.
Probe source: `/tmp/exact-review-20261112/json-object.rs`; executable:
`/tmp/exact-review-20261112/json-object-opt`.

## Direction and validation

Build parsed objects with an ordered map or collect members and sort once.
Preserve sorted iteration and the last value for duplicate keys. Cover reverse
and mixed key order, duplicate keys, nested objects, and error positions, and
measure scaling after the change. Small fixed objects can retain a compact
representation if useful without imposing this cost on wide parsed objects.

## Duplicate check

Searched open and closed filesystem issues and all 275 cached GitHub issues.
Read the closed `20261110-overlay-and-large-reply-numbers-read-by-serde-json.md`:
it fixed numeric rounding by routing these paths to this parser, and did not
address object insertion. GitHub #211 concerns compiler scope and table
lookups. The separate selector destruction finding concerns kernel cleanup,
not JSON parsing. No existing issue covers this parser's quadratic insertion.
