# A failed share delivery leaves file tokens pending

**Status:** Closed
**Resolution:** The painted host's `receive` lands a claimed share's files whether or not setting its tokens or delivering its location failed, `land` sets every token even after one fails, and the first error is still the answer, as on Apple.
**Systems:** host/linux
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

## What is wrong

An in-app share claims its files as pending, then sets each token, then delivers the location. `land`, which copies the files and sets the tokens again as they finish, runs only after delivery succeeds. If `deliver` returns an error, the function returns and the claimed value is dropped.

`sharedFiles` stays pending. Nothing later lands those tokens. A route miss, a navigate error, or a refusal to type the root is enough. The app can show a share that never finishes.

## Where

`host/linux/src/presenter/picker/share.rs`, `receive`. `runner/src/runner/picker/shared.rs`, `claim_share` and `set_shared`.

## How to reproduce

Share files into a running app whose delivery of the location fails (the route cannot be opened). Read `sharedFiles`. The files stay pending, and no completion follows.

## Direction

On delivery failure, clear the tokens or still run `land`. Do not leave a committed pending count with no completion.
