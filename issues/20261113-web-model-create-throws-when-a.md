# Web model create throws when a download becomes unavailable

**Status:** Open
**Systems:** packages/language-model
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

An agent `createLanguageModel` that is waiting on a download stores a promise. `preferred` rejects that promise with a thrown `Error` named `NotSupportedError` when availability becomes `unavailable`. The `await` is outside the `try` that returns `{error}`. `createLanguageModel` therefore rejects.

Apple and Android answer `{error: NotSupportedError}` on the same transition. The model becoming unavailable while create is waiting is the state the waiter exists for. The send fails as an exception, so the app's typed failure path does not run.

## Where

`packages/language-model/web/index.js`, `createLanguageModel` and `preferred`.

## How to reproduce

Start `createLanguageModel` while the agent availability is `downloadable`. Change availability to `unavailable` before the download finishes. The call rejects instead of returning `{error: 'NotSupportedError'}`.

## Direction

Resolve that waiter with `{error: 'NotSupportedError'}`, or catch the rejection at the await and return that object.
