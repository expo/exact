# Recipe: tabs

`bun exact.mjs pattern tabs` gives native tabs with separate stacks: iOS's bar, a sidebar on iPad and
wide Mac/web windows.

For an **app-drawn bar**, use `appearance="none"`: iOS lays routes full-height.
Overlay the bar and pad **every route** by its height plus the bottom safe inset.
Root `viewport-fit="cover"` exposes the insets. Label the badge for VoiceOver.

```contract
routes nav
  tab today "/"
  tab history "/history"
    day "/day/:key"

component App
  state count = 1
  action pick(name: string)
    nav = select(nav, name)
  action follow(url: string)
    nav = go(nav, url)
  view
    main viewport-fit="cover" navigationKey=`${top(nav).id}` navigate=follow
      width="100%" height="100%" position="relative"
      column position="absolute" inset=0
        for t in nav.tabs key=t.name
          column role="tabpanel" id=`panel-${t.name}` position="absolute" inset=0
            for e in t.stack key=e.id
              column navigationKey=`${e.id}` navigationScroll=`content-${e.id}` position="absolute" inset=0
                box-sizing="border-box" display="flex" flex-direction="column"
                padding-bottom="calc(49px + env(safe-area-inset-bottom))"
                header
                  text (e.name == "today" ? "Today" : e.name == "history" ? "History" : e.params.key)
                    role="heading" aria-level=(e.name == "day" ? 2 : 1)
                scroll id=`content-${e.id}` flex=1 min-height=0
                  if e.name == "history"
                    link href=path("day", "2026-10-09") testId="day-2026-10-09"
                      text "Yesterday"
                  else if e.name == "day"
                    text `Log for ${e.params.key}`
                  else
                    text "Nothing logged yet."
      row role="tablist" appearance="none" position="absolute" left=0 right=0 bottom=0
        height="calc(49px + env(safe-area-inset-bottom))" box-sizing="border-box"
        padding-bottom="env(safe-area-inset-bottom)" background-color="-exact-background" testId="tabs"
        for t in nav.tabs key=t.name
          button role="tab" aria-controls=`panel-${t.name}` aria-selected=(nav.tab == t.name) press=pick(t.name)
            testId=`tab-${t.name}` flex=1 min-height=0 padding=0 position="relative" appearance="none"
            display="flex" flex-direction="column" align-items="center" justify-content="center"
            image (t.name == "today" ? "symbol:calendar" : "symbol:chart")
            text (t.name == "today" ? "Today" : "History")
            if t.name == "history"
              row position="absolute" top=2 right=16 aria-label=`${count}` testId="badge"
                background-color="-exact-system-red" border-radius=12
                text `${count}` font-variant-numeric="tabular-nums"
```

Tabs mount on selection and keep stacks, drafts and scroll. Reselecting pops.
Hide a tab with `display="none"`, never `if`; while hidden, drop route padding and gate its
resources ([how](../contract-for-agents.md#a-features-requests)).
