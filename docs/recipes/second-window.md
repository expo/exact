# Recipe: a second window

A `link` whose `target` names a window opens that window at the link's route,
as HTML's does (LLP 1030.004). On the Mac it is its own session, rooted at the
route (that screen alone, no sidebar), sharing the app's store. The next press
brings it forward, at the link's route. On the web it is the browser window or
tab of that name. Other hosts navigate in place.

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

Drive it on the Mac: `bun exact.mjs agent macos "tap new-message" state` opens
it and lists each window's `name`, `location` and whether it is rooted.

What to know:

- **Names.** Empty, `_self`, `_parent` and `_top`, in any case, navigate in
  place. `_blank` opens a new window each press, with no name. A literal name
  that starts with `_` and is no keyword, or is `window-<n>` (the agent's label
  for a window with no name), is refused; a bound one that starts with `_` acts
  as `_blank`.
- **Which window.** `exactPage().name` is `main` in the app's first window on
  every host, the `target` in a window a link opened, and empty in a `_blank`
  one. Every window runs the root's `task`s, so gate work the app does once:
  `task sync when page.name == "main"`.
- **One store.** A `persist` state or a store write in one window reaches the
  others' readers. So does an `app:/` file write, and a SQLite statement that
  changed rows (an `INSERT`, `UPDATE` or `DELETE`; a `CREATE TABLE` does not
  count): a resource that read that file or database is asked again. On the
  web, two tabs cannot open one SQLite database at once.
- **Closing.** Closing a window ends its session. `main` holds the app: under
  `keepRunning` or with a status item placed, closing it hides it, and a Dock
  click shows it again.
- For the Mac's Settings window, see [a Settings window](settings-window.md).
