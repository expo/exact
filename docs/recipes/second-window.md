# Recipe: a second window

A `link` with a named `target` opens that route in its own Mac window or
browser tab. Other hosts navigate in place. On macOS it opens the route alone,
without a sidebar. A second press brings the same window to the link's route (LLP 1030.004).

```contract
routes nav
  inbox "/"
    compose "/compose"

shape Page
  name: string

component App
  state draft = "" persist
  resource page = exactPage() as shape Page
  action follow(url: string)
    nav = go(nav, url)
  action edit(v: string)
    draft = v
  view
    main navigationKey=`${top(nav).id}` navigate=follow width="100%" height="100%"
      for e in stack(nav) key=e.id
        if e.name == "inbox"
          column navigationKey=`${e.id}` position="absolute" inset=0
            header display="flex" align-items="center"
              text "Inbox" role="heading" aria-level=1 flex=1
              link href="/compose" target="compose" aria-label="New Message" testId="new-message"
                image "symbol:compose"
            text (draft == "" ? "No draft" : `Draft: ${draft}`) testId="draft-line"
        if e.name == "compose"
          column navigationKey=`${e.id}` position="absolute" inset=0 padding=16 gap=12
            header display="flex" align-items="center"
              text "New Message" role="heading" aria-level=1 flex=1
            textarea value=draft input=edit aria-label="Message" flex=1 testId="draft"
            text `This window is ${page.name}` testId="window-name"
```

`bun exact.mjs agent macos "tap new-message" state` lists each window’s `name`, `location` and whether it is rooted.

What to know:

- **Names.** Empty, `_self`, `_parent` and `_top`, in any case, navigate in
  place. `_blank` opens a new window each press, with no name. A literal name
  that starts with `_` and is no keyword, or is `window-<n>` (the agent's label
  for a window with no name), is refused; a bound one that starts with `_` acts
  as `_blank`.
- **HUD.** Add `windowKind="panel"` to the opening link to float above
  full-screen Mac apps without activating. Other hosts report `panel`
  unsupported. Escape closes the HUD when it receives the key.
- **Which window.** `exactPage().name` is `main` in the app's first window on
  every host, the `target` in a window a link opened, and empty in a `_blank`
  one. Every window runs the root's `task`s, so gate work the app does once:
  `task sync when page.name == "main"`.
- **One store.** Persisted state, store and `app:/` writes reach each window’s readers.
  A SQLite statement refreshes readers only if rows or schema change. A no-op
  `DELETE` or `CREATE TABLE IF NOT EXISTS` leaves readers alone.
  On the web, two tabs cannot open the same SQLite database at once.
- **Closing.** Closing a window ends its session. `main` holds the app: under
  `keepRunning` or with a status item placed, closing it hides it, and a Dock
  click shows it again.
- For the Mac's Settings window, see [a Settings window](settings-window.md).
