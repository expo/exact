# An asset index at a root URL with a query panics during update checking

**Status:** Open
**Systems:** update/src/envelope.rs, update/src/store.rs
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At audited commit `f6be99af`, a valid asset-index URL such as
`https://updates.example?index=1`, containing a relative asset card, panics in
`resolve_url` instead of producing a check outcome. `parse_http_url` retains
`?index=1` as the base path (`update/src/envelope.rs:789-794`). Removing its
query leaves an empty string; the relative-card branch slices through byte
zero of that string (`:684-691`). The index has already passed its declared
length and digest checks before the store validates its asset URLs
(`update/src/store.rs:1453-1457`).

This is a valid spelling of the same URL as
`https://updates.example/?index=1`, which stages successfully. The normal
publisher's immutable blob paths avoid this case, but the envelope format
accepts absolute same-origin index URLs. A custom update endpoint can therefore
break checking through a valid URL spelling. The panic's host consequence
depends on its panic and worker policy; it is not a normal refused check.

## Verification

A temporary Rust probe used this checkout's `Client::open_at` and `Client::check`
with a temporary development store and an in-memory fetch closure. It supplied
a hash-checked index with one relative `asset.bin` card. No network was used.

```
index URL https://o.example?index=1  -> panic at envelope.rs:690
index URL https://o.example/?index=1 -> Staged { seq: 1, ... }
```

The probe caught the panic outside `Client::check` so both cases could run.
Source: `/tmp/exact-review-20261112/update-index-url.rs`; executable:
`/tmp/exact-review-20261112/update-index-url`.

Direct checks of the same resolver found other URL-resolution differences:
an empty card drops a base query, `../assets/` loses its trailing slash, and
`./a//b.bin` collapses an empty path segment. Bun 1.4.2's `new URL(card, base)`
preserves those components. These differences can request different bytes from
an endpoint, contrary to this function's documented browser-resolution rule.
The direct probe is `/tmp/exact-review-20261112/update-url.rs`.

## Direction and validation

Resolve references using a complete URL implementation, or complete the
existing resolver's path/query handling, while retaining same-origin and
credential checks. An absent HTTP path must normalize to `/` before slicing.
Keep query inheritance and meaningful empty path segments. Add check-level
coverage for an index at an authority-only query URL and table coverage against
a URL implementation for empty, query-only, fragment-only and relative cards.
Invalid input must return an error without panicking.

## Duplicate check

Searched open and closed filesystem issues and all 275 cached GitHub issues.
The closed `20260904-stream-files-overwritten-before-head.md` concerns mutable
publisher URLs, not resolving valid references. GitHub #91 introduced the
separate asset index to avoid envelope-size limits; its full report does not
cover index-base resolution. No matching panic or URL-normalization issue was
found.
