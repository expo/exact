# A deploy's source capture refuses a checkout whose `llp` is a symbolic link, the arrangement AGENTS.md gives a worktree

**Status:** Closed
**Resolution:** Already fixed by 13ca8570, which excludes the Exact checkout's `llp` from the captured tree in `sourcePathspec`, so neither the link nor a real corpus directory is captured or published; after it, `bun test scripts/deploy.test.mjs` passes both named cases in a worktree whose `llp` is an absolute link.
**Systems:** scripts/deploy.mjs (validateCapturedTree), scripts/deploy.test.mjs, AGENTS.md
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

AGENTS.md tells a checkout to hold the design record at `llp/`, which the repository ignores, and a worktree to link it: `ln -s <checkout>/llp llp`.

At `fe4632ed` a deploy's source capture inventories the captured tree and, for each symbolic link, resolves its target against the captured repositories; a link that leaves them is refused (`scripts/deploy.mjs:276-287`). The captured tree includes `llp` although Git ignores it, and its target is outside every captured repository. So the capture refuses in any checkout arranged as AGENTS.md says, and in one whose `llp` is a relative link to a sibling clone.

## The failure

In a checkout whose `llp` is a link (`llp -> ../exact-llp`):

```sh
bun test scripts/deploy.test.mjs
```

Observed: 23 pass, 2 fail. Both failing cases ("diagnostic Git commits do not start automatic maintenance", "diagnostic source and outputs are isolated and cleaned on failure") stop at

```
error: <checkout>: captured symlink llp escapes the captured source repositories
```

With an absolute link the message reads "has an absolute target that escapes". Expected: a path Git ignores is not part of the captured source, or AGENTS.md prescribes an arrangement the capture accepts.

## Verification

Ran the command above at `fe4632ed` in a checkout with the relative link: the result shown. At `dd6f5dcd`, in a worktree with the absolute link, the same two cases failed, and passed once the link was removed. Not run: a deploy of an app from such a checkout outside the tests.

## Direction

Leave ignored paths out of the captured tree, as a clone of the repository would. If the capture must include them, say in AGENTS.md how a worktree should hold `llp`.

## Related

`issues/20261110-script-tests-run-by-no-check-or-lane.md` mentions these two cases among the script tests no lane runs.
