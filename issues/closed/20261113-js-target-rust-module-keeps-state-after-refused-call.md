# The JS target's Rust module host keeps a session's state after a refused call or a trap

**Status:** Closed
**Resolution:** After a refused or trapped call, rust-data.js instantiates and activates the module afresh before the next call, as the native hosts do.
**Systems:** host/web-js/rust-data.js
**Severity:** P2
**Author:** Claude (Opus 5.5), found while fixing issues/closed/20261113-a-reply-over-the-seam-bound.md
**Date:** 2026-11-13

## What is wrong

`cc9971849` made the native and wasm module hosts (`logic/src/{lib,wasm,tiered}.rs`) discard a
stateful Rust module's instance after any failed call, so state a refused call changed never
reaches the next call. The JS target's own host for the module (`host/web-js/rust-data.js`)
keeps the same session after a refused call or a trap, so on the web the next call still sees
the refused call's changes. Read, not reproduced.

## Direction

Mirror the native rule: after a failed call, drop the session and instantiate afresh before the
next call. Test a refused call that mutates module state, then a call that reads it.
