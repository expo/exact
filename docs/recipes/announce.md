# Recipe: announce a result to a screen reader

There is no announce call (React Native's `AccessibilityInfo.announceForAccessibility`).
Put the words in a `text` that is a live region and change them: a screen reader reads a
`role="status"` politely, after what it is saying, and a `role="alert"` at once where the
platform allows. Keep the `text` mounted, empty until there is something to say: text it
already had is not read again, and one that arrives holding its text is not read on every
host.

```contract
component App
  state picked = 0
  state problem = ""
  action pick
    picked = picked + 1
  action save
    problem = picked > 2 ? "Pick at most two." : ""
  view
    column padding=16 gap=8
      button press=pick testId="pick"
        text "Pick one"
      button press=save testId="save"
        text "Save"
      text (picked == 0 ? "" : `${picked} selected`) role="status" testId="picked"
      text problem role="alert" testId="problem"
```

```contract-test
test "the counts and the error are the regions' text"
  tap "pick"
  expect text "picked" == "1 selected"
  tap "pick"
  tap "pick"
  tap "save"
  expect text "problem" == "Pick at most two."
```

- `aria-live="polite"` or `"assertive"` makes a `text` a live region without the role, and
  wins over the role's; `aria-live="off"` silences a `status`.
- Put the role on the `text`, and leave `aria-label` off it: Android and Windows read a
  live region by its accessible name, which a box has only from an `aria-label` (Windows
  says nothing when a child of a `row role="status"` changes), and an `aria-label` on the
  `text` is read in place of its text.
- iOS, iPadOS, tvOS and macOS post the platform's announcement, the web leaves it to the
  browser, Android and Windows mark a live region for TalkBack and Narrator, and the Linux
  host and watchOS say nothing yet: [Announcing a
  change](../contract-for-agents.md#announcing-a-change) has each host.
