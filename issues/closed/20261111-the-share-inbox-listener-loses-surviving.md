# The share inbox listener loses surviving sessions when its last session ends

**Status:** Closed
**Resolution:** `ShareInboxPing` keeps a weak reference to every session that listens and hands each ping to the newest one not destroyed, so a session ending leaves the ping with the one before it.
**Systems:** host/apple/Sources/ExactKit/ShareReceive.swift
**Severity:** P3
**Author:** Codex (GPT-6 Astra), from the fix lanes of the 2026-11-10 project review
**Date:** 2026-11-11

## What is wrong

At `ec183f2a1cfc80caeb8259b4c1c37bbcec4de730`.

`ShareInboxPing` holds one weak session. Each `listen` replaces it, even when the Darwin observer is already installed (`host/apple/Sources/ExactKit/ShareReceive.swift:64`, `:67`). The callback calls only that session (`:75`). Ending the last registered session neither selects another live session nor restores the earlier target. `claimInbox` refuses a destroyed session (`:144`).

## The failure

Activate share-target session A, then B. Destroy B while A stays active. An inbox ping still targets B, or nil after B is released, so A does not claim the new share. A's foreground observer can claim it on the next application foreground notification. Expected: an inbox ping reaches a surviving eligible session.

## Verification

A temporary Swift script beside the Apple tests extracted the unchanged `ShareInboxPing` class from the production file and supplied a minimal session double that counts `claimInbox` calls. It registered A and B, released B, posted the Darwin notification, and printed:

```
FIX08 share: listener has surviving A = false, listener nil = true
FIX08 share: A claims = 0
```

Run with `swift host/apple/tests/ExactKitTests/Fix08ShareProbe.swift`; the script was removed. This verifies the listener's ownership, not a full share-extension delivery. Device check: keep A foreground, activate and destroy B, then share a URL into the app and confirm A receives it without a foreground transition.

## Direction

Keep weak references to the eligible sessions and choose a live recipient when the ping arrives, removing destroyed sessions. Preserve one claim per inbox entry.

## Related

`issues/closed/20261110-session-registries-keyed-by-released-address.md` fixes stale per-session registration identities. It does not restore the singleton listener's recipient.
