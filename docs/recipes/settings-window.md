# Recipe: a Settings window

A `link` to a route whose shortcut is `Meta+,` is the app menu's Settings…
(LLP 1030.004 D5). On the Mac it opens the window named `settings` at that
route, as `target="settings"` would, without your writing `target`: the route
alone, no minimize or zoom, never a tab, its title the shown pane's heading,
its frame kept. It reopens at the pane last shown, and ⌘, brings it forward as
it stands. ⌘W closes it; Escape does not. On the web and other hosts the link
navigates in place, as any link does.

```contract
routes nav
  home "/"
    settings "/settings"

component App
  state units = "C" persist
  action follow(url: string)
    nav = go(nav, url)
  action unit(u: string)
    units = u
  view
    main testId="root" navigationKey=`${top(nav).id}` navigate=follow width="100%" height="100%"
      link href="/settings" aria-keyshortcuts="Meta+," aria-label="Settings" testId="open-settings"
        image "symbol:settings"
      for e in stack(nav) key=e.id
        if e.name == "home"
          column navigationKey=`${e.id}` position="absolute" inset=0 padding=16
            text (units == "C" ? "21 °C" : "70 °F") testId="reading"
        if e.name == "settings"
          column navigationKey=`${e.id}` position="absolute" inset=0 padding=16 gap=12
            header display="flex" align-items="center"
              text "General" role="heading" aria-level=1 flex=1
            row role="radiogroup" aria-label="Temperature"
              button role="radio" aria-checked=(units == "C") press=unit("C") testId="unit-c" flex=1
                text "°C"
              button role="radio" aria-checked=(units == "F") press=unit("F") testId="unit-f" flex=1
                text "°F"
```

A choice made in Settings reaches `main` at once: the windows share one store.
Drive it with `bun exact.mjs agent macos "type root key Meta+," state`, which
lists the `settings` window rooted at `/settings`.

What to know:

- **An authored `target` wins.** `target="_self"` on the link keeps Settings in
  the window it was chosen from.
- **A command that runs an action** (`button press=… aria-keyshortcuts="Meta+,"`)
  is still Settings… and runs its action; no window opens.
- **Panes.** Give each pane a route under the Settings route
  (`/settings/:pane`) and a heading; a link to one with `target="settings"`
  from elsewhere navigates the open Settings window there.
