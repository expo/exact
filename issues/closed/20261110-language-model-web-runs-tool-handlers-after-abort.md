# On the web, an aborted language-model response still runs its remaining tool handlers

**Status:** Closed
**Resolution:** An ended browser response ignores buffered stream output and starts no further tool handlers.
**Systems:** packages/language-model/web/index.js
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

At `dd6f5dcd`, the browser path's tool loop (`packages/language-model/web/index.js:342` to `351`) walks every `tool-call` item of a reply and awaits `toolCall` for each, with no look at `r.state` between calls. The check that the response is still `calling-tool` comes only after the whole list (line 352). So when a reply carries two or more calls and the response ends while the first handler runs (the app's `abortLanguageModelResponse`, a destroyed session, a `cancel` of a `prompt` let go), the loop goes on: it appends each remaining call to `r.calls` and `r.turn.calls` and runs its handler through `invoke`. The page runtime's `invoke` refuses only a handler the session did not list (`host/web-js/modules.js:745` to `752`), so the handler runs.

The agent path's loop does check, at the top of each iteration (`runCalls`, line 286), so a drive under the agent does not show it.

LLP 1133.016 D9 item 6 says the runtime "lets the run go when the response is aborted, its session destroyed or the app's session ends". A handler is the app's code and may write, send or pay; starting one for a response the person stopped is the opposite.

No shipping browser reaches this yet: none offers the Prompt API's tool calls (QUEUE, "The web's Prompt API tool loop is held only by a stand-in"), which `createLanguageModel` asks the browser for whenever a session lists tools (line 412). It becomes live when one does.

## The failure

A stand-in `LanguageModel` whose first reply is two `tool-call` items for a tool `charge`; the app aborts while the first handler runs:

```js
const s = await impl.createLanguageModel({ tools }, { origin: 'app' });
response = (await impl.promptStreaming({ session: s.id, input: 'go' }, {})).id;
// inside the first handler: await impl.abortLanguageModelResponse({ response })
```

Observed:

```
handlers run:
  charge({"n":1}) while the response is calling-tool
  charge({"n":2}) while the response is aborted
```

and the aborted response's `toolCalls` holds the second call, `"state":"running"`, for good. Expected: one handler run, and no call added to a response after its end.

## Verification

Ran the unmodified module under Bun with that stand-in, whose stream yields the explainer's `tool-call` items and whose `invoke` records the response's state at each handler; the output above is its. Not run against a browser, for the reason above.

## Direction

Leave the `for (const call of calls)` loop when `r.state !== 'calling-tool'`, before the call is recorded, as `runCalls` does.

## Related

QUEUE "The web's Prompt API tool loop is held only by a stand-in" asks for a run against a real browser and names no defect.
