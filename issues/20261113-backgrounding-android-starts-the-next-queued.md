# Backgrounding Android starts the next queued model prompt

**Status:** Open
**Systems:** packages/language-model
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

Leaving the foreground fails only the response that is `session.responding`. `end` then calls `next`, which admits and starts the next queued prompt while the app is still in the background. A new `prompt` is refused with `NotAllowedError` `background`. This path is not.

`responses` is a `HashMap`, so the newly started response is failed only if the loop has not visited it yet. The agent model can then complete it. On device, generation is posted after foreground has returned.

## Where

`packages/language-model/android/LanguageModel.kt`, `foreground`, `end`, and `next`. The prompt arm refuses a background origin.

## How to reproduce

Queue a second prompt while one is generating, then leave the app. The queued prompt starts instead of failing with `NotAllowedError` `background`.

## Direction

On leaving the foreground, fail every live response, including those still queued, and do not call `next` until the app is in front again.
