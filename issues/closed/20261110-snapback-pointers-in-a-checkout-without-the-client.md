# A checkout without the Snapback client still tells every new app to wire Snapback

**Status:** Closed
**Resolution:** A new app's `AGENTS.md` offers Snapback only when the checkout has the client, `backend` and `snapback` have examples in their help, and the README and the recipe say the client is not in the repository (the recipe's link was already fixed by `ecc96fbe`).
**Systems:** game/new.mjs (`snapbackNotes`), scripts/help.mjs, docs/recipes/snapback.md, README.md
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

Line references are at `dd6f5dcd`.

`AGENTS.md:265` says the Snapback client (`snapback4/`) is not in this repository, and that
an app asking for Snapback is told so. The refusal works. What a new app is told before it
asks does not follow.

## What is wrong

1. **Every new app's instructions.** `snapbackNotes` (`game/new.mjs:839`, the last branch
   at `:855`) writes two lines into each app's `AGENTS.md`, which a builder carries on
   every step, without looking for the client:

   ```
   Shared data: `exact new <path> --snapback`, or `bun exact.mjs snapback` here.
   Install `snapback4@0.4.33` above; <exact2>/snapback4/README.md has setup and server commands.
   ```

   In this checkout both commands refuse and the file does not exist. The module already
   knows whether the client is present (`snapbackDevice`, `game/new.mjs:12`).
2. **The two verbs' help ends `Example  undefined`.** `backend` and `snapback` have no
   `example` (`scripts/help.mjs:64-78`), and `printHelp` prints the field unguarded
   (`scripts/help.mjs:570`). This does not depend on the client.
3. **One link is wrong with the client too.** `docs/recipes/snapback.md:17` links
   `../snapback4/README.md`, which is `docs/snapback4/README.md`; `snapback-auth.md` beside
   it links `../../snapback4/README.md`.
4. **Two documents describe the adapter without saying it is elsewhere.** `README.md:775-777`
   (the adapter's version, and "See the adapter guide") and `docs/recipes/snapback.md:3-5`
   (`exact new app --snapback` "writes a guestbook and wires its driver") name commands and
   a guide that only a checkout with the client has.

## The failure

```sh
bun <exact2>/scripts/exact.mjs new app1 && cd app1
grep -n "Shared data" -A1 AGENTS.md     # the two lines above
bun exact.mjs snapback
# exact: this checkout has no Snapback client (snapback4/), so it cannot wire an app to Snapback   (exit 1)
bun exact.mjs snapback --help | tail -1
# Example  undefined
bun exact.mjs backend --help | tail -1
# Example  undefined
```

Expected: an app made from a checkout without the client is not told to wire Snapback, and
each verb's help ends with an example or without the line.

## Verification

Ran the commands above in a new app outside the repository. `ls <exact2>/snapback4` reports
no such directory, `git ls-files snapback4` prints nothing, and `.gitignore:72` ignores it.

## Direction

Have `snapbackNotes` write its last paragraph only when the client is present. Give the two
help entries an example, or print the line only when there is one. Fix `snapback.md:17`'s
path, and say in the README and the recipe where the client lives, as `AGENTS.md:265` does.

## Related

`exact new <path> --snapback` writing the app before it refuses is a separate issue
(`issues/20261110-new-snapback-writes-the-app-before-finding-no-client.md`). `QUEUE.md:7` records the
generated `AGENTS.md` at 1,600 bytes against a 1,536-byte limit; the two lines of item 1
are about 160 of them. `QUEUE.md:54` (LLP 1154) plans to delete `snapback4/` once Snapback
publishes a package.
