# Recipe: tests

`app.test.contract` drives `testId`s (`bun exact.mjs test web`, `test ios`, `verify`).
Each test clears storage and awaits data. [Groceries](../start-here.md).

```contract-test
test "a duplicate stays a draft and the list survives reload"
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
| `tap "id"` (`… contextmenu`) | Press or long-press. `tap "Milk"` matches text or label. Disabled controls fail. |
| `type "id" "text"`, `type "id" key "Enter"` | Set a select, date (`"2026-10-10"`) or checkbox (`"true"`), or press a key. |
| `expect text "id" == "…"` | Text, then value, then descendants' text. |
| `expect tree has "id"` (`missing`) · `expect disabled "id"` (`enabled`) · `expect focused "id"` · `expect state draft == ""` | In tree · disabled · focused · a state reply value. |
| `clock data` · `clock settle` · `clock +5000` · `reload` | Lands replies · ends transitions · moves virtual time · reloads on the same storage and date. |
| `tap "intro" drag 0 -170 over 400 hold 200` | Android scrolls from the hit and releases at 600 ms on the agent clock. |
| `rotate <orientation>` | Wait for an iOS simulator scene to turn. See [driving iOS](ios-driving.md). |
| `back` | Platform Back cancels a dialog, dismisses a sheet or pops a route. |
| `presence allow` (`cancel`, `fail`, `lockout`, `invalidated`) | Answers the user-presence prompt. |
| `fail fetch`, `pass fetch`, `drop stream "<prefix>"` | Fails; clears; ends streams. |
| `answer fetch "<prefix>" with 200 {…}`, `answer source s with {…}` | HTTP, or source answers in `verify`. |
| `request POST "/items" {…} as grace expect 409` | Writes as another person through [the app's server](rest-backend.md#where-the-server-lives), empty per test; expects 2xx, or exactly 409 with `expect 409`. |
| `size 390x844`, `at "/item/7#moves"`, `epoch`, `time-zone`, `locale`, `seed`, `before data` | Start a test or file. |

[Grammar and limits](../contract-grammar.md#authored-tests).

`expect state` takes `==`, `!=`, `>=`, `<=` or `in 0 1`, against literals or
`state <path>` (`in` needs a list). Use `clock data` before reading an input's reply
(iOS needs it); `clock settle` after a sheet, alert or push. Test refusals. Pin `epoch`, `time-zone`. Name screens. Open a tab before reading it. Avoid real-time waits.

## Before you call it done

`verify` checks launch, test ends and relaunch in light and dark on web and iOS.
Answer alerts to check beneath at the largest text size.
A `squeezed` scroller reports port and content extents. Web reloads flag changed
saved values even under alerts. An `aria-invalid` field is a draft.
End a test per screen. Shots go in `.exact/verify/`.
In app.json: `"verify": {"accept":
[{"finding": "contrast", "node": "<testId>", "why": "…"}]}`.
`agent web --storage s1 …` keeps its store.
