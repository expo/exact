# The `lower-symbol-tint` warning tells the author to write a param by its hashed token name

**Status:** Closed
**Resolution:** Symbol tint repairs preserve local param names, warnings hide internal tokens, and authored internal param tokens are refused at their rows.
**Systems:** contract/lower/src/lib.rs, contract/lower/src/symbols/tint.rs, contract/lower/src/params/rewrite.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

Before anything reads a component's rows, `param.<name>` is rewritten to the param's token
name, `param.f<file hash>.<Component>.<name>` (`contract/lower/src/params.rs:1-10`). An author
is never meant to read the hash: lowering's refusals pass through `params::shown`, which
writes each token name back as the param's full name (`contract/lower/src/theme.rs:596-612`,
called at `contract/lower/src/lib.rs:337` and `:372`). Warnings do not: `lower_all` appends
them as built (`lib.rs:369-371`). `lower-symbol-tint` quotes the tint's value in its repair
(`contract/lower/src/symbols/tint.rs:201`, `:225`), so a tint that names a param is printed by
its hash.

## The failure

```
component App
  params
    ink: color = "#333333"
  view
    column
      image "symbol:add" -exact-tint-color="param.ink" aria-label="a"
```

Observed:

```
app.contract:6:26 warning [lower-symbol-tint] `-exact-tint-color` on a `symbol:` image does nothing: … write `color="param.fbc96c5e4.App.ink"` instead
```

Expected: `write `color="param.ink"` instead`, the value as the row wrote it.

The repair as printed builds with no refusal and no warning:

```
      image "symbol:add" color="param.fbc96c5e4.App.ink" aria-label="a"
```

`rewrite::found` leaves the word alone because a `.` follows its first segment
(`contract/lower/src/params/rewrite.rs:153`), the kernel reads it as the token it is, and
`read` counts it as a read of the param (`rewrite.rs:752-765`). The hash is of the file's
path, so the row breaks when the file is renamed, and the refusal then points at the param,
not the row: in a file named `main.contract` the source with that row is `3:5
[forma-param-unused] param `main.contract#App.ink` is read by no row of `App``. A second
component that writes the hashed name reads `App`'s param too, which a param's scope is meant
to prevent.

## Verification

Ran `target/debug/contract build` on the source above (the warning, also in `--json`), on the
same source with the repair as printed and with `color="param.ink"` (both built clean), on the
printed form in a file named `main.contract` (the refusal above), and on a source where a
second component `Chip` writes `color="param.fbc96c5e4.App.ink"` (built clean). A refusal that
quotes a row shows the full name: `translate="param.gap 0"` is refused as
`translate="app.contract#App.gap 0"`.

## Direction

Have the tint's repair print the value as the row wrote it, and pass warnings through
`params::shown` as refusals are. Refuse a row that itself writes a `param.f<hash>.…` name, as
a design token named `param.…` is refused (`theme.rs:262-268`).

## Related

`docs/agent-pitfalls.md` names `lower-symbol-tint` as the diagnosis of a tint on a symbol.
Nothing in `issues/` or `QUEUE.md` is about a hashed param name in a message.
