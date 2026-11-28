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
| `tap "id"` (`… contextmenu`) | Press or long-press. `tap "Milk"` matches text/label. Disabled controls fail. |
| `type "id" "text"`, `type "id" key "Enter"` | Set a select, date (`"2026-10-10"`) or checkbox (`"true"`), or press a key. |
| `tap "@browser" dismissed`, `type "@date" "…"` | Answer a held device/package ticket by capability or target. |
| `expect text "id" == "…"` | Text, then value, then descendants' text. |
| `expect tree has "id"` (`missing`) · `expect disabled "id"` (`enabled`) · `expect focused "id"` · `expect state draft == ""` | In tree · disabled · focused · state value. |
| `clock data` · `clock settle` · `clock +5000` · `reload` | Land replies · end transitions · step virtual time · reload on same store and date. |
| `tap "intro" drag 0 -170 over 400 hold 200` | Android drags from hit, releasing at 600 ms on agent clock. |
| `rotate <orientation>` | Wait for iOS simulator turn ([driving iOS](ios-driving.md)). |
| `back` | Platform Back cancels a dialog, dismisses a sheet or pops a route. |
| `presence allow` (`cancel`, `fail`, `lockout`, `invalidated`) | Answer the user-presence prompt. |
| `fail fetch`, `pass fetch`, `drop stream "<prefix>"` | Fail · clear · end open streams. |
| `answer fetch`, `answer stream`, `pass stream`, `answer source` | Buffered HTTP (`with 200 {…}`), HTTP streams (`event`/`bytes`/`end`/`fail`), or source answers. |
| `request POST "/items" {…} as grace expect 409` | Write via [app server](rest-backend.md#where-the-server-lives) (empty per test; 2xx or `expect N`). |
| `size 390x844`, `at "/item/7#moves"`, `epoch`, `time-zone`, `locale`, `seed`, `before data` | Start a test or file. |

[Grammar and limits](../contract-grammar.md#authored-tests).

`expect state` takes `==`, `!=`, `>=`, `<=` or `in 0 1` against literals or `state <path>`. Use `clock data` before reading a reply (iOS needs it); `clock settle` after a sheet, alert or push. Test refusals. Pin `epoch`, `time-zone`. Name screens. Open a tab first. Avoid real-time waits.

## Before you call it done

`verify` checks launch, test ends, relaunch light/dark on web & iOS (`verify ios <glob>`, `--duo none`; end full).
Check alerts at largest text; `squeezed` scrollers report port/content extents.
Web reloads flag changed saved values; `aria-invalid` marks drafts.
End a test per screen. Shots: `.exact/verify/`.
`app.json`: `"verify": {"accept": [{"finding": "contrast", "node": "<testId>", "why": "…"}]}`.
`agent web --storage s1 …` keeps its store.
