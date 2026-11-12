# The web language model does not rebuild the browser's session after the app aborts a reply

**Status:** Closed
**Resolution:** An active response's abort invalidates the browser session before the next queued prompt rebuilds it from the committed transcript.
**Systems:** packages/language-model/web/index.js
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

LLP 1133.016 D6 keeps a session's transcript in the module and says how the platform's own session follows it: "the web and Apple implementations rebuild after an abort, since neither platform keeps a cancelled reply in its own session." A turn that ends with no text still commits its input, so after an abort the module's transcript holds a question the browser's session never saw.

The rebuild is asked for by `s.stale = true` in the reply loop's `catch` (`packages/language-model/web/index.js:364`), and that line is not reached when the app itself aborts. `end` sets the response's state first and aborts the controller second (lines 176-185):

```js
r.state = state;            // 'aborted'
…
r.controller?.abort();
```

The browser's `prompt` then rejects with `AbortError`, and the `catch` leaves at its first line because the state is no longer `generating` or `calling-tool`:

```js
} catch (e) {
  if (r.state !== 'generating' && r.state !== 'calling-tool') return;   // line 360
  …
  s.stale = true;                                                       // line 364
```

`s.stale` stays false, so the next prompt reuses the browser's session (`index.js:132`).

## The failure

Prompt a session, abort the reply while it streams, then prompt again. Expected: the second prompt is answered by a session rebuilt from the transcript, which includes the aborted turn's input. Observed: no rebuild; the model answers without the question the transcript shows before it.

## Verification

Ran the web module in Bun over a stand-in `LanguageModel` that counts `create` calls: an abort mid-stream followed by a prompt made 0 calls of `LanguageModel.create`. A failure that is not the app's abort (the browser's own error) reaches line 364 and rebuilds. Not run in a browser with the Prompt API.

## Direction

Mark the session stale where the app's abort ends the response (in `end`, for a response that had begun), so that both ways out of a cancelled reply rebuild.

## Related

`issues/20261110-language-model-web-abort-before-admission-hangs-session.md` and `issues/20261110-language-model-web-runs-tool-handlers-after-abort.md` are the other two abort paths of this module.
