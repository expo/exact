# The web build accepts `crypto.getRandomValues()` at a data module's top level, which every native bake refuses

**Status:** Open
**Systems:** host/web-js/ts-fetch.js, js/src/prelude.js (`answerOnly`), js/bake
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

Natively a data module may draw entropy only inside an answer: the prelude's `answerOnly` throws `crypto.getRandomValues() is unavailable during module initialization; call it inside an answer` (`js/src/prelude.js:161`), and the wasm target's browser realms do the same (`js/web/tests/it/browser.rs:432`). The bake evaluates the module, so a native build fails on it.

The web JS target binds `crypto` for the module too, but its `getRandomValues` and `randomUUID` refuse only inside an overlay (`host/web-js/ts-fetch.js:423`, `:430`). At module initialization they return the browser's values. So `web-build`, `test web`, `agent web` and the type check every host's build runs all accept a module that no native build will.

## The failure

An agent building an app for web, macOS and iOS (exact2 `9e53578f2`, macOS) put `crypto.getRandomValues()` at its data module's top level. The web build and the page ran; the first macOS build refused the module with the message above. The agent asked that the web build, or the shared type check, say the same thing.

Checked by reading both bindings; not rebuilt here.

## Fix

Make the web JS target's `getRandomValues` and `randomUUID` refuse outside an answer with the native message, as its timers already do (`ts-fetch.js:195`). The web build then fails where the native bake does, at the first build on any host.
