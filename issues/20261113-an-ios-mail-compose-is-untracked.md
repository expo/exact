# An iOS mail compose is untracked while attachments are read

**Status:** Open
**Systems:** packages/compose
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

When `canSendMail()` is true, `email` returns before setting `shown`, after hopping to a background queue to read attachments. The hop happens even when there are no attachments. `shown` is the only in-flight slot, and it is set on the main queue after the read.

A second `composeEmail` or `composeMessage` in that window also passes `shown == nil` and presents another controller, overwriting `shown`. `finish` ignores a controller that is not `shown`, so the first reply never runs. `cancel` and `destroy` only see `shown`, so they neither abort the read nor stop the later present.

On macOS, `destroy` does not clear `sharing` or reply to it.

## Where

`packages/compose/apple/Compose.swift`, `email`, `finish`, `cancel`, and `destroy`.

## How to reproduce

Call `composeEmail` twice before the first present. Two composers appear. Dismiss the first. Its call stays open. Destroy the session during the attachment read. The read still presents, and the call stays open.

## Direction

Set the in-flight record before the hop. Reject a second compose. On cancel or destroy, drop the read and reply `AbortError`. On macOS, abort `sharing` the same way.
