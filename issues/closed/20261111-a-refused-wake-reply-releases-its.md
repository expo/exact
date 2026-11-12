# A refused wake reply releases its request under the app’s origin

**Status:** Closed
**Resolution:** A one-shot reply, stream message or mapper turn whose commit is refused is released under its request's origin, invocation and causes (`release_failed_as_asked`), so what the release refreshes is asked as the wake's.
**Systems:** runner/src/runner/commit.rs, runner/src/runner/admission.rs
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `09cb236d64007f51a668e38926fda6f64dfb6fb5`, one-shot fulfillment runs `fulfill_inner` inside `as_invocation_caused` (`runner/src/runner/commit.rs:1198`), then leaves that context. If the commit is refused, it calls `release_failed` outside the invocation at `:1223`. That function ends a mutation's write and commits its resource refreshes (`runner/src/runner/admission.rs:335`, `:345`). Those refreshes inherit the ambient app origin, so a wake can reach a source declared app-only after its reply fails shape validation. The stream fallback at `commit.rs:1289` has the same context boundary.

## The failure

Use the existing `@fixture/tide` wake package, declaring `everyTides` with `"origin": "app"`. A wake handler sends one mutation `pulled`, declared as `{tag: string}`, with `refreshes sensitive`; `sensitive` reads `everyTides()`. Make the pending source's parser return `{tag: 7}`.

Expected: releasing the failed reply refreshes `sensitive` under wake origin and refuses it with `person-required (wake origin)`. Observed: the mutation fails shape validation, then `sensitive` is queried successfully and the wake completes.

## Verification

Ran `CARGO_BUILD_JOBS=5 cargo test -p contract --test review_findings -- --nocapture`. The temporary test reused the tide package fixture and its `Later` data source. The journal printed:

```
fulfil 1 (pulled) [HTTP 200, 2 bytes] refused: Shape { resource: "pulled", why: "Pulled.tag: a String was declared, and 7 answered" }
request 1 (pulled) failed and is no longer pending: it is not retried: `pulled` answered outside its shape: Pulled.tag: a String was declared, and 7 answered
query sensitive: tide.everyTides (substituted)
a failed request (0 asked again) → epoch 1 (+0 −0 ~0)
wake: completed
```

`failed_resources()` was empty. The assertion requiring `person-required (wake origin)` failed. The temporary test was removed. This proves admission through the declared substitute; no external module or device was invoked.

## Direction

Keep `release_failed`, and any failed-reply reconciliation it triggers, inside the ticket's origin, invocation id and causes. Apply the same rule to the stream fallback.

## Related

`issues/closed/20261102-kept-replies-lose-tool-origin.md` concerns retained requests losing origin. Here the ticket retains it, but the fallback commit runs after leaving its context. `contract/cli/tests/it/module_wakes.rs:506` covers origin during explicit expiry, not failed fulfillment. No matching open issue or QUEUE entry was found.
