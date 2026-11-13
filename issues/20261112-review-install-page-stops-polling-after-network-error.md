# A failed status poll leaves the local installer stuck in its busy state

**Status:** Open
**Systems:** scripts/install-page.mjs, local iOS installation
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

## Problem

The local installation page disables both Build and Refresh while the server reports
`building`. It schedules the next status poll only in `paint`. If that poll fails,
`load` displays the error but schedules no retry and leaves both buttons disabled
(`scripts/install-page.mjs:194–205`). A temporary connection failure therefore leaves
the page unable to observe completion or request another status, even after the
server recovers. Reloading the page is the only available recovery.

## Reproduction

1. Open a local installation page while its build is running.
2. Let its first status response report `building`.
3. Make the next GET fail once, then let the server answer normally again.
4. Observe that no further GET occurs and Refresh remains disabled.

Verified at `f6be99af` by executing the unchanged script emitted by
`developmentInstallPage` with controlled fetch, timer, and DOM adapters. After the
failed poll it had made two requests, had no scheduled poll, and left both buttons
disabled. A subsequent request would have returned `installed`.

## Fix

Keep a bounded retry path after status failures, or enable Refresh so the person
can recover. Keep starting a second build separate from refreshing the current one.
Cover a transient failure followed by successful completion.

## Existing reports

No matching filesystem issue or GitHub issue was found in the review's all-state
snapshot of `expo/exact` issues.
