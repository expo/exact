# Recipe: a tip that teaches a control

A tip names a control's use the first time the person sees it, and goes away for good once
they have used it: TipKit's popover on iOS, iPadOS and macOS. Write what the tip says and
when its lesson is done; the platform decides when it shows, draws it, and remembers.

```contract
routes nav
  stations "/"

component App
  state filtered = false
  state saved = 0
  action filter
    filtered = not filtered
  action save
    saved = saved + 1
  view
    main navigationKey=`${top(nav).id}` width="100%" height="100%"
      for e in stack(nav) key=e.id
        column navigationKey=`${e.id}` position="absolute" inset=0
          header padding="8px 16px"
            text "Stations" role="heading" aria-level=1
            button id="filter-tip" press=filter aria-label="Filter" tip="Filter stations" tipMessage="Show only the stations near you." tipImage="symbol:filter" tipDone=filtered testId="filter"
              image "symbol:filter"
          column padding=16 gap=12 align-items="flex-start"
            button id="save-tip" press=save tip="Save a station" tipDone=(saved > 0) testId="save"
              text "Save"
```

- **`tip` is the title, and the element's `id` names it.** TipKit remembers the tip by the id
  across launches and languages, so give each tipped element one that stays the same;
  the build refuses a `tip` without one. `tipMessage` is the line under the title and
  `tipImage` a symbol beside it (`symbol:<role>` or `symbol:sf/<name>`). Write the words with
  `t()` to translate them, as any other text.
- **`tipDone` is the lesson done.** Here pressing Filter is what its tip teaches, so the
  pressed state is its `tipDone`. Once true, the tip is retired for good, even when the
  value turns false again or the app relaunches. Without it, the tip stays until the person
  closes it.
- **When it shows is the platform's.** A tip shows while its control is on screen and
  nothing covers it, one at a time in the order they are written, and not again after the
  person closes it. Bind `tip` to `""` until it should be offered at all (after the third
  launch, once a list has rows): an empty title is no tip.
- **A header's button keeps its tip.** Where the navigation bar (iOS) or the window's
  toolbar (macOS) draws the button, the tip points at that item.
- Tips carry nothing the person needs (HIG, Offering help): the web, Android, Linux and
  Windows have no tip system and show none, so put anything essential in the screen itself.
- The driver's `state` lists each tip under `tips`: its TipKit status, whether it shows,
  and whether it points at the element or a bar item. TipKit decides on its own clock, not
  the drive's: just after launch a tip may still be `pending`. What TipKit remembers is the drive's
  store's: a drive without `--storage` starts having seen no tip, one with a named store
  starts with what that store remembers, and `--reset-storage` forgets it.
