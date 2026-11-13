# An agent drive uses whatever server already answers at the app's server URL without saying so

**Status:** Open
**Systems:** scripts/verify.mjs (`driveServer`)
**Severity:** P2
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13

## What is wrong

`driveServer` (`scripts/verify.mjs:282`) starts the app's own server for an `agent` drive. When
something already answers at the server's URL (`:340`), the drive uses it and returns. It refuses
only a fixture another checkout owns (`:341-344`), or any server when the drive starts its store
empty (`:336-339`), and it speaks only when another drive's lease names a different
database (`:345-346`). A server started by hand, by another worktree of the
app, or by any other program is used silently, with its data.

`test` and `verify` do not do this: `started` refuses a URL that already answers with
`heldLine` (`:242`, `:172`), naming the pid. So the same server is refused by `test` and taken
by `agent`.

## The failure

An admin app builder at `94b8671f7`: an `agent web` drive reused a server that another worktree
had started against real data. Nothing said it was not the drive's fixture server; the builder
noticed from the data the screens showed.

## Fix

When the drive did not start the server and no other drive's lease names it, say so in one line,
as `heldLine` does: the URL, the pid and command holding it, and that this drive uses that
server's data. Refusing, as `test` does, would be consistent; `--storage` drives that want to
share a server across drives already go through the lease records.
