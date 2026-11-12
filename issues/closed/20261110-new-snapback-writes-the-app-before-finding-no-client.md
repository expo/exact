# `exact new --snapback` writes the app and a backend schema, then fails for want of the client

**Status:** Closed
**Resolution:** `createApp` refuses `--snapback` in a checkout without the client before it makes the directory, with the refusal `wireSnapback` gives, so nothing is written.
**Systems:** game/new.mjs (createApp, wireSnapback), scripts/exact.mjs (new)
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

The Snapback client (`snapback4/`) is not in the repository, so in a plain checkout
`wireSnapback` throws at its first line (`game/new.mjs:536`). `createApp` calls it
(line 186) only after it has written the app and, with no backend above it, the guestbook
starter and `../snapback/schema.q` (line 175). Its cleanup (`unwrite`, line 164, "A failure
leaves nothing of ours behind") runs for the steps before that call and the step after it,
not for the call.

So the verb the usage text and every generated `AGENTS.md` name ("Shared data: `exact new
<path> --snapback`") exits 1 and leaves an app that does not build, with a `snapback/`
directory beside it.

## The failure

In a checkout without `snapback4/`:

```sh
bun scripts/exact.mjs new /tmp/s/guest --snapback
```

Observed, exit 1:

```
exact: this checkout has no Snapback client (snapback4/), so it cannot wire an app to Snapback
```

Afterwards `/tmp/s/guest/` holds the ten scaffold entries, with the guestbook `app.ts`
(`import … from './snapback4/snapback.ts'`), and `/tmp/s/snapback/schema.q` exists. The
same command again prints `… already exists and is not empty`. A second app made beside it
finds the leftover schema, takes the existing-backend branch and is left behind the same
way.

Expected: the refusal, and nothing written.

## Verification

Ran the command in a new directory, then again, then for a second app beside the first;
the outputs and leftovers are as above. In the first app
`bun exact.mjs contract build app.contract` exits 1 with `Could not resolve
'./snapback4/snapback.ts'` and `Could not resolve './backend/api.ts'`.
`bun exact.mjs snapback` in an existing app prints the same refusal and writes nothing.

## Direction

Refuse in `createApp` before `mkdirSync` when `options.snapback` is set and the client is
absent (the test `wireSnapback` makes), or call `wireSnapback` inside a `try` that
`unwrite`s and removes a `../snapback/` this run created.

## Related

QUEUE's "LLP 1154 steps 2 and 3" tracks deleting `snapback4/` and does not mention this
path. `README.md:63`, `README.md:777` and `docs/start-here.md:69` link to
`snapback4/README.md`, and the generated `AGENTS.md` names `<exact2>/snapback4/README.md`;
a checkout without the client has no such file.
