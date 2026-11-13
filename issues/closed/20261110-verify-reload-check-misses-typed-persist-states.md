# verify's reload check does not see a `persist` state declared with its type

**Status:** Closed
**Resolution:** `persistedStates` reads a `persist` state declared with its type (`state note: string = "" persist`) as it reads one without, so verify's reload check compares it.
**Systems:** scripts/verify.mjs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

After each web test, `verify` reloads the end screen and reports a finding when a saved value came back different (LLP 1116.011 D5). The saved values it knows are the root's `persist` states, which `persistedStates` reads from `app.contract` by a regular expression (`scripts/verify.mjs:547-554` at `dd6f5dcd`):

```js
const m = line.replace(/\s\/\/.*$/, '').match(/^\s*state\s+([A-Za-z_][\w-]*)\s*=.*\spersist\s*$/);
```

The pattern requires `=` directly after the name. The grammar allows a type between them (`docs/contract-grammar.md:280`: `"state" IDENT [ ":" type ] "=" expr [ "persist" ]`), and a state that nothing writes needs one (`state saved = [] persist` alone is `type-cannot-infer`). A state written with its type is never in the list `savedChanges` checks (`verify.mjs:560-565`, called at line 648 with the list from line 832).

## The failure

In an app `exact new` made, a note that doubles on each tap until it passes the 16 KB a persisted value may hold, so the last doubling is not kept:

```
shape Greeting
  text: string

component Keeper
  resource greeting = greeting("Keeper") as shape Greeting
  state note: string = "" persist
  action grow
    note = note.length > 30000 ? note : `${note}${note}0123456789abcdef`
  view
    column padding=16 gap=12
      text greeting.text testId="greeting"
      button press=grow testId="grow"
        text "Grow"
      text `Length ${note.length}` testId="size"
```

with one test, `test "a note past 16 KB"`: eleven `tap "grow"` lines, then `expect text "size" == "Length 32752"`. `bun exact.mjs verify` reports:

```
reload 1 test end: no findings
reload note: 1 screen reloads into another state (not counted)
  note     / reloads into another state: "Length 32752" → "Length 16368" ("a note past 16 KB")
verify: clean in 5.7 s
```

With `state note = "" persist`, the same app and test report:

```
reload 1 test end: 1 finding
  reload   / a saved value came back different: the saved state `note` was "0123456789abcdef0123456…" and is "0123456789abcdef0123456…"; a reload must bring back what was saved ("a note past 16 KB")
verify: 1 problem (1 finding; the tests passed) in 5.7 s
```

Expected: the finding in both.

## Verification

Ran `bun exact.mjs verify` on both spellings at `dd6f5dcd`; the lines above are from its reports (the last line of each goes on to say `contract verify` failed, which it does on this app either way). Also called the two functions on a root with `state goal = 8 persist`, `state typed-goal: number = 8 persist` and `state saved: string[] = [] persist`, which `contract build` accepts: `persistedStates` returned `["goal"]`, and `savedChanges`, given a reload that brought all three back as their initializers, reported `goal` only. No app in this repository declares a persisted state with a type.

## Direction

Allow the optional `: type` in the pattern (`\s*(?::[^=]*)?=`) and add a typed state to the test at `scripts/verify.test.mjs:313-317`. Better, take the names from the compiler, which already decides which states persist, so the rule is not read from the source a second time.

## Related

No issue or QUEUE entry names `persistedStates` or `savedChanges`.
