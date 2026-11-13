# Native URLSearchParams removes a question mark that belongs to the parameter name

**Status:** Open
**Systems:** vendored Ibex URLSearchParams, native JavaScript data sources
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At `f6be99af`, `new URLSearchParams("??a=1")` produces the name `a` on
native data sources, where the web produces `?a`. A URL's live
`searchParams` similarly misreads `https://example.test/??a=1&b=2`.
Reading or updating these params can address the wrong name and rewrite
the query with data missing.

The JavaScript binding removes one leading delimiter in the constructor
and in `query()` (`vendor/ibex/crates/ibex2/src/bindings/url.js:147,176`).
The Rust boundary then calls `SearchParams::parse`, which removes another
leading question mark (`stdlib/url.rs:208-210`). That Rust helper is also
used for getters and mutations, although their input is already the query
data, without its delimiter.

The [URL Standard's constructor](https://url.spec.whatwg.org/#dom-urlsearchparams-urlsearchparams)
removes one leading question mark before form parsing. Subsequent question
marks are data. A URL's query also already excludes its delimiter.

Reproduced by evaluating the unchanged Ibex URL binding and Exact's
`pure.js` in Bun 1.4.2, with boundary calls forwarded to a small Rust
executable linked against the checkout's actual Ibex library. No URL
parsing algorithm was substituted. Bun's built-in URL API was the control:

| Input | Actual native binding pairs | Expected pairs |
| --- | --- | --- |
| `new URLSearchParams("?a=1")` | `[["a","1"]]` | same |
| `new URLSearchParams("??a=1")` | `[["a","1"]]` | `[["?a","1"]]` |
| `new URLSearchParams("?%3Fa=1")` | `[["?a","1"]]` | same |
| `new URL("https://example.test/??a=1&b=2").searchParams` | `[["a","1"],["b","2"]]` | `[["?a","1"],["b","2"]]` |

The probe is `/tmp/exact-review-20261112/url-leading-question.mjs`, with
`url-native-adapter.rs` beside it. This exercises the production JavaScript
and Rust implementations across a process adapter, without launching a
Hermes host or making network requests.

Distinguish constructor input from raw query data, and remove a delimiter
only at the appropriate boundary. Cover construction, live URL views,
lookup and mutation with a literal leading question mark in the first
name, plus its percent-encoded equivalent.

Checked open and closed filesystem issues and all 275 cached GitHub
issues. The closed `20261110-searchparam-cuts-value-at-question-mark.md`
concerns the Contract router's former web-target split at a second `?`;
it does not cover this native URLSearchParams parser boundary.
