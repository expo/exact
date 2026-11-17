# History and revert do not read a design's `params` and `component` blocks

**Status:** Closed
**Resolution:** History includes design params and components; reverts restore param values and refuse component changes or conditional restorations missing their default.
**Systems:** forma/src/history.rs, contract/cli/src/forma_history.rs, scripts/forma-server.mjs (`history`, `revert`), docs/forma-protocol.md
**Severity:** P2
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

`exact_forma::history::edits` (`forma/src/history.rs:62-154`, at `dd6f5dcd`) compares two versions of a `.forma` file by name, axes, dials, tokens, exports and icons. It never reads `Design::params`, the block `set-param` and `unset-param` write, or `Design::components`, the `component Name` blocks. `history::revert` plans from those edits (line 222), and the design server's `compare` and `revert-plan` call both (`contract/cli/src/forma_history.rs:579-685`).

So a commit's change to either block is missing from `history`, and `revert` drops it without saying so. LLP 1122 §10 has a revert refuse, by name, each edit with no inverse operation; `What::Other` exists for that (`history.rs:24-26`).

## The failure

An app whose `post.contract` declares `PostButton` with a `radius` param, and three commits of `app.forma`. The first:

```
forma t

theme
  ink = #000000

params
  PostButton.radius = 8px
```

The second changes `8px` to `20px`. The third changes `#000000` to `#111111` and `20px` to `30px`.

Through the dev loop's wire (`formaServer`):

- `history` lists the second commit with `edits: []`, and the third with one edit, `ink`.
- `revert` of the second answers 400, "commit b817741b5991 changed no design file (only the app's code in )" (`scripts/forma-server.mjs:1159`). The commit changed only the design file.
- `revert` of the third holds one operation, `set-token ink #000000`, and its review says nothing of the param. Accepting it leaves `PostButton.radius = 30px`.

Expected: each `params` line and `when` line that differs is an edit, reverted with `set-param` or `unset-param` (`forma/src/edit.rs:89`, `:104`), or refused by name as a kind change is. A `component` block's change is at least an `other` edit.

## Verification

A temporary Bun test drove `history`, `revert` and `accept` through `formaServer` on a git repository holding the three commits. The replies are as quoted. A temporary Rust test called `history::edits` on two designs differing only inside a `component MapCard` block and got `[]`.

## Direction

Have `edits` walk `params` as it walks tokens (a `What::Param`, by the line's name, its default and each `when`), and have `revert` emit `set-param` and `unset-param` for them. Report a `component` block's difference as `What::Other`. Add the new `what` to the `history` section of `docs/forma-protocol.md`.

## Related

QUEUE's "History of a template row: a key a node keeps across versions" and LLP 1121 §10 (stage 9 D3, "a `params` line is code") are about a component's `params` section in a `.contract` file, not the design file's own `params` block.
