# Aborting a background transfer also cancels a registration whose id begins with the aborted id and a space

**Status:** Closed
**Resolution:** `abort` cancels a task only when the id its description parses to (`which`, by the last space) is the aborted registration's, so aborting `album` leaves `album 2`'s tasks running.
**Systems:** host/apple/Sources/ExactKit/Transfers.swift
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

The transfer carrier names each task `"\(id) \(index)"` (`host/apple/Sources/ExactKit/Transfers.swift:187`) and reads the name back by its last space (`which`, `Transfers.swift:316-319`), so an id may hold spaces. `abort` selects its tasks by prefix instead (`Transfers.swift:145-147`):

```swift
for t in tasks where t.taskDescription?.hasPrefix(id + " ") == true { t.cancel() }
```

A task of registration `"album 2"` is named `"album 2 0"`, which begins `"album "`, so aborting `"album"` cancels it. `completed` (`Transfers.swift:270-276`) reads the cancellation as the app's own and records `failureReason = "aborted"` for a registration nobody aborted.

An id is any non-empty string: `packages/background-transfer/exact.json` types it `string`, and `packages/background-transfer/apple/Transfers.swift:68` checks only `!id.isEmpty`.

## The failure

Data code using `@exact/background-transfer`, with both registrations in flight:

```ts
await backgroundFetch({ id: 'album', requests: first });
await backgroundFetch({ id: 'album 2', requests: second });
await abortBackgroundFetch({ id: 'album' });
```

Observed: both registrations' requests are cancelled, and both end `failure` with `aborted`. Expected: only `album`'s. The Background Fetch API's `abort()` ends the registration it is called on.

## Verification

Ran the selecting line (146) and `which` as the file declares them over stand-in tasks named as `start` names them, aborting `album`:

```
task "album 0" is registration "album" request 0: cancelled
task "album 1" is registration "album" request 1: cancelled
task "album 2 0" is registration "album 2" request 0: cancelled
task "album 2 1" is registration "album 2" request 1: cancelled
```

Not run: a background `URLSession`. The stand-in has the two members the line touches, `taskDescription` and `cancel()`.

## Direction

Select by the parsed id, as every delegate callback does: `where which(t)?.0 == id`.

## Related

No issue or QUEUE entry names the transfer carrier's `abort`. QUEUE.md "Background work: what is left" lists the Android transfer and the runner's substitute.
