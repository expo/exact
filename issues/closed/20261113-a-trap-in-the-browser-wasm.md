# A trap in the browser Wasm executor does not rebuild the data module

**Status:** Closed
**Resolution:** Already fixed by cc9971849: Swappable::run discards the executor and activates the module again after every failed call, which includes the browser glue's trap result.
**Systems:** logic, host/web
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

A trap in the browser Wasm executor does not rebuild the data module. `Browser::call` treats a return length above `MAX_MESSAGE` as an error and never records a trap. The glue returns `0xffffffff` for a wasm trap and for a clean refusal. `Executor::trapped` defaults to false, and the browser executor does not override it. `run` discards the instance only when `trapped` is true.

The wasmi executor sets `trapped` on a trap, so the next call builds a fresh instance. On the wasm web host a panic leaves the same session in place. Its memory is undefined, and the next call uses it.

## Where

`logic/src/browser.rs`, `call`. `logic/src/lib.rs`, `trapped` and `run`. `host/web/rust-glue.js`, the `call` catch that returns `0xffffffff`. `logic/src/wasm.rs`, `trap`.

## How to reproduce

On the wasm web host, panic inside a Rust data-module call. The call returns an error. The next call runs on the same instance. The same panic under wasmi activates the module again before the error is returned.

## Direction

Have the glue report a trap apart from a refusal, and have the browser executor return `trapped` for that result so `run` rebuilds the instance.
