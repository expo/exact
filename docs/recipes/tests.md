# Recipe: tests

`app.test.contract` drives `testId`s (`bun exact.mjs test web`, `test ios`, `verify`).
Each starts with empty storage after its first data lands. For
[Groceries](../start-here.md):

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
| `expect tree has "id"` (`missing`) · `expect disabled "id"` (`enabled`) · `expect focused "id"` · `expect state draft == ""` | In the tree or not · its `disabled` · focused · a state, derive or resource. |
| `clock data` · `clock settle` · `clock +5000` · `reload` | Lands replies · ends transitions · moves virtual time · reloads on the same storage and date: web at its URL, native at a path or scheme `--url`, else its cold launch location. |
| `back` | The platform's Back: cancels a dialog, else dismisses a sheet, else pops the route. |
| `fail fetch "<prefix>"`, `pass fetch …` | Fails fetches, then clears. |
| `answer fetch "<prefix>" with 200 {"text":"Hello"}` | Supplies a response; leading the test, before its first load. |
| `request POST "/items" {…} as grace expect 409` | Writes as another person through [the app's server](rest-backend.md#where-the-server-lives), empty per test; a status not 2xx fails, or with `expect 409` any other. |
| `size 390x844`, `at "/item/7#moves"`, `epoch`, `time-zone`, `locale`, `seed`, `before data` | Launch lines, first in a test or atop the file. |

More: [the grammar](../contract-grammar.md#authored-tests).

`expect state` takes `==`, `>=`, `<=` or `in 0 1`. `clock data` after an
input whose reply the next `expect` reads (iOS needs it); `clock settle` after a sheet, an
alert or a push. Test the refusal too; never wait in real time (pin `epoch`, `time-zone`); give screens ids; tap a tab before reading in it.

## Before you call it done

`verify` (`verify ios`) runs the tests, then `check`s the first screen and test ends,
light and dark at two widths. On the web it reloads each end: a changed saved value is a
finding; a field marked invalid by `aria-invalid` is a draft. It also checks a relaunch on that storage. Screenshots: `.exact/verify/`. A clean `verify` is
done; a screen no test ends on is not checked, so end a test there. A finding that is the design
is listed, not counted, with `"verify": {"accept": [{"finding": "contrast", "node": "<testId>",
"why": "…"}]}` in app.json. Investigate with `agent web --storage s1 …`.
