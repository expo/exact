# Refreshing a live Convex resource leaves its pending mutations and actions unresolved

**Status:** Open
**Systems:** packages/convex/index.ts, packages/convex/sync.ts
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

## Failure

At `f6be99af`, calling `live()` again increments `epoch`, replaces `socket`
and starts a new connection (`packages/convex/index.ts:86-98`). Every later
event from the previous connection is ignored. The old connection's
pending requests are neither settled by `sync.closed()` nor retried.
`Sync.connecting()` and `opened()` preserve the request map.

A mutation or action sent before a user refresh therefore remains pending
after the new connection opens. Its response and even its close event are
discarded by the epoch guard. A healthy new socket cannot settle it because
the request was never sent on that socket; a later unrelated disconnect
finally rejects it. This leaves progress indicators stuck and retains the
request and its promise indefinitely. Retrying a possibly completed action
can also duplicate its external effects.

## Reproduction

Ran the real public client with Bun 1.4.2 and a stand-in fetch carrier that
exposes each call's `exactStream` mapper:

```js
await client.live();                    // mapper 0 receives `open`
let settled = false;
client.action('billing:charge', {}).then(
  () => { settled = true; }, () => { settled = true; });
await client.live();                    // mapper 1 receives `open`
// Deliver mapper 0's `close`, then its successful ActionResponse for id 0.
await Promise.resolve();
await Promise.resolve();
console.log(client.inspect().requests, settled); // 1, false
```

The new connection sent only `Connect` and `ModifyQuerySet`. The old close
and successful response changed nothing. No real Convex backend was used;
the failure is in retiring one carrier generation while preserving its
pending requests. Existing reconnect tests explicitly call `closed()`
first, which the active-refresh path omits.

## Direction and acceptance

Retire the previous connection's pending work before starting the next
epoch. Use the existing close semantics: keep acknowledged results and
reject unanswered requests with wording that says they may have run. Do not
silently replay mutations or actions. Test refreshing an open connection
with an unanswered action, an unanswered mutation, and an acknowledged
mutation awaiting its visibility transition; none should depend on the new
socket eventually closing.

## Duplicate check

Checked open and closed filesystem issues and the GitHub export on
2026-11-12 for Convex, reconnect, refresh and pending requests. GitHub #54
tracks the completed Convex integration and #82 tracks transport reconnect
and idle timeout; neither reports this package-level generation transition.
