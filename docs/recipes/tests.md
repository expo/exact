# Recipe: tests

`app.test.contract` drives `testId`s (`bun exact.mjs test web`, `test ios`, `verify`).
Each test starts with empty storage and waits for its first data.
Example from [Groceries](../start-here.md).

```contract-test
test "a repeat is refused and keeps its text; the list survives a relaunch"
  type "draft" "Eggs"
  type "draft" key "Enter"
  clock data
  type "draft" "eggs"
  tap "add"
  clock data
  expect text "add-error" == "Eggs is already on the list."
  expect state draft == "eggs"
  reload
  expect text "left" == "1 item left"
```

| Step | What it does |
| --- | --- |
| `tap "id"` (`… contextmenu`) | Presses (long-presses) it; `tap "Milk"`: by text or label. A disabled control fails the step. |
| `type "id" "text"`, `type "id" key "Enter"` | Sets a value (a `select`'s, a date `"2026-10-10"`, a checkbox `"true"`); presses a key. |
| `expect text "id" == "…"` | Its text, else its value, else its descendants' text. |
| `expect tree has "id"` (`missing`) · `expect disabled "id"` (`enabled`) · `expect focused "id"` · `expect state draft == ""` | In tree · disabled · focused · a state reply value. |
| `clock data` · `clock settle` · `clock +5000` · `reload` | Lands replies · ends transitions · moves virtual time · reloads on the same storage and date. |
| `back` | The platform's Back: cancels a dialog, else dismisses a sheet, else pops the route. |
| `presence allow` (`cancel`, `fail`, `lockout`, `invalidated`) | Answers the user-presence prompt. |
| `fail fetch`, `pass fetch`, `drop stream "<prefix>"` | Fails; clears; ends streams. |
| `answer fetch "<prefix>" with 200 {…}`, `answer source s with {…}` | HTTP, or source answers in `verify`. |
| `request POST "/items" {…} as grace expect 409` | Writes as another person through [the app's server](rest-backend.md#where-the-server-lives), empty per test; a status not 2xx fails, or with `expect 409` any other. |
| `size 390x844`, `at "/item/7#moves"`, `epoch`, `time-zone`, `locale`, `seed`, `before data` | Launch lines, first in a test or atop the file. |

[Grammar](../contract-grammar.md#authored-tests).

`expect state` takes `==`, `!=`, `>=`, `<=` or `in 0 1`, against literals or
`state <path>` (`in` needs a list). Use `clock data` before reading an input's reply
(iOS needs it); `clock settle` after a sheet, alert or push. Test refusals; never
wait in real time (pin `epoch`, `time-zone`). Give screens ids; tap a tab before
reading in it.

## Before you call it done

`verify` (`verify ios`) checks the first screen, test ends and a relaunch, light and dark.
A modal alert at a test's end gets one finding. Answer it to check the page beneath,
including at the largest text size. Web reloads check saved
values even with an alert open; changed values are findings, and an `aria-invalid`
field is a draft. Screenshots: `.exact/verify/`. End a test on each screen to check it.
Accept a design finding in app.json: `"verify": {"accept":
[{"finding": "contrast", "node": "<testId>", "why": "…"}]}`.
Drive: `agent web --storage s1 …`.
