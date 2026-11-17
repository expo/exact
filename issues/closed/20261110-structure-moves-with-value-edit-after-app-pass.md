# A value-only design edit changes `Plan::structure` when the app pass interned a string after the design's values

**Status:** Closed
**Resolution:** Plan structure sorts retained strings and remaps table and bytecode references, so value-only edits remain live batches when the app pass adds or shares strings.
**Systems:** plan/build.rs (`Plan::structure`), plan/src/builder.rs, plan/src/lib.rs (`Plan::intern`), contract/cli/src/reach.rs, host/web-js/dev.mjs
**Severity:** P2
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`Plan::structure` leaves the design's values out of the string pool and writes every other `str` field as its raw string id (`plan/build.rs:995`). Its comment says why that is safe: "the builder interns the values after every other string, so none moves another's id". `PlanBuilder::finish` does intern them last (`plan/src/builder.rs:1037`), but the app pass runs after `finish` and interns more through `Plan::intern` (`plan/src/lib.rs:1033`): a hatch word (`contract/cli/src/reach.rs:50`), a module substitute carrying the manifest's `shortcuts` or purchases catalogue (`reach.rs:83`, `:98`), a tool's face (`reach.rs:209`), a package view's names (`contract/cli/src/lib.rs:996`). A string the pool does not already hold lands after the values, so its id moves whenever an edit changes how many strings the values add: a token that stops sharing its value with another string, or comes to share one.

The dev loop compares structures to decide whether an edit is value-only (`host/web-js/dev.mjs:440`, `:579`), so such an edit takes the full build and reloads the page instead of going out as a batch.

## The failure

An app with `@exact/quick-actions` installed:

```
app.json      { "name": "Notes", "id": "com.example.notes",
                "app": { "id": "com.example.notes", "name": "Notes" },
                "shortcuts": [{ "name": "New note", "url": "/" }] }

app.forma    forma t

              theme
                page = #fafafa
                ink = #111111
                edge = #fafafa

app.contract  use setAppShortcuts, ShortcutItem from "@exact/quick-actions"

              routes nav
                home "/"

              component App
                derive current = top(nav)
                action shortcut(url: string)
                  setAppShortcuts(shortcuts=[ShortcutItem(name="Open", short_name=none, description=none, url=url, icons=none, apple=none, android=none)])
                action follow(url: string)
                  nav = go(nav, url)
                view
                  main navigationKey=`${current.id}` navigate=follow background-color="page" border-color="edge"
                    text "a" color="ink"
```

Under `bun host/web/dev.mjs --app <dir>`, three saves of `app.forma`:

| `edge` | the loop says |
|---|---|
| `#fafafa` to `#abcdef` | `edit → building`, then `edit → plan ready 662 ms (rebuilt in 656 ms) · 0 pages reloading` |
| `#abcdef` to `#abcde0` | `edit → batch ready 69 ms (design read in 63 ms, no build) · 1 token to 0 pages, no reload` |
| `#abcde0` to `#fafafa` | `edit → building`, then `edit → plan ready 360 ms (rebuilt in 353 ms) · 0 pages reloading` |

Expected: a batch each time. LLP 1121 D13: "saving a value change in `app.forma` is a live-table edit too". Without `shortcuts` in `app.json`, the first save is a batch.

## Verification

Ran the dev loop as above, with and without `shortcuts`. Its design reader alone (`exact-web-js design app.contract -`, the design on stdin) gives `structure` `dc6e88b6…` for `edge = #fafafa` and `#111111`, and `7e7d469d…` for `#abcdef` and `#abcde0`; without `shortcuts`, one value for all four.

A hatch word in `app.json` that the Contract does not name moves the structure the same way; one the Contract names does not, since the pool holds it before the values. So `apps/forma-host`, the one app in the tree with design values and a hatch word, is unaffected (run: its structure holds when `page` takes `swatch.fill`'s value).

Not run: a purchases catalogue, a tool's face, a package view, and a scrub. By reading, the compile that verifies a scrub's patched frame compares the same field (`dev.mjs:833`) and ends the scrub as a mismatch.

## Direction

Make `structure` independent of the ids: write each `str` field as its rank among the strings it keeps, or have the app pass intern before the design's values. Add a manifest string to `a_value_only_edit_keeps_the_plans_structure`'s fixture.

## Related

No issue, `QUEUE.md` entry or pitfall names `structure`. `contract/cli/tests/it/forma.rs:1167` covers the same edit for an app with no `app.json`.
