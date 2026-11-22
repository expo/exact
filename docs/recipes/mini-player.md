# Recipe: a mini player above the tab bar

Name the element with `navigationTabAccessory` on the root tablist. On iOS 26 and
later it is UIKit's tab bar accessory, a glass capsule on every tab that UIKit places
and sizes: on an iPhone above the bar, and inline where the bar was while the bar
hides (48 points high on an iPhone 18 Pro with iOS 27); on an iPad at the bottom of
the page, beside a shown sidebar. The routes clear it. The web, the Mac and Android
show the element where it is written, above the tab strip, at the size it says.

```contract
routes nav
  tab library "/"
  tab search "/search"

component App
  state playing = true
  state paused = false
  action pick(name: string)
    nav = select(nav, name)
  action togglePlay
    paused = not paused
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" display="flex" flex-direction="column"
      column flex=1 min-height=0 position="relative"
        for t in nav.tabs key=t.name
          column role="tabpanel" id=`panel-${t.name}` position="absolute" inset=0
            for e in t.stack key=e.id
              column navigationKey=`${e.id}` position="absolute" inset=0
                header
                  text (t.name == "library" ? "Library" : "Search") role="heading" aria-level=1
      if playing
        row id="now-playing" display="flex" align-items="center" gap=12 padding="0 16px" height=56
          text "Song title" flex=1
          button press=togglePlay aria-label=(paused ? "Play" : "Pause") testId="toggle-play"
            image (paused ? "symbol:play" : "symbol:pause")
      row role="tablist" navigationTabAccessory=(playing ? "now-playing" : "")
        for t in nav.tabs key=t.name
          button role="tab" aria-controls=`panel-${t.name}` aria-selected=(nav.tab == t.name) press=pick(t.name)
            text (t.name == "library" ? "Library" : "Search")
```

- **Place.** Write the element after every tab panel the tablist links, usually just
  before the tablist: outside the tablist, the panels and their routes, the sidebar's
  `footer`, a popover, a `dialog` and a `list`, and as a box, not a run of text, a part
  of an `svg` or a `select`, or a radio or a tab of its group.
- **Size.** On iOS the element's box is the accessory's whatever `height` it says, and
  narrower inline: lay its content out to fit. A background you write paints over
  UIKit's glass there, so leave it unsaid unless the web needs one.
- **Show and hide** it with `if` or `display`, and name none (`""`) while it is absent,
  as the example does, so a development build notes nothing.
- **Errors.** Where the compiler can prove it, a literal id that names nothing, or an
  element in a place **Place** rules out, is an error (`lower-tab-accessory`) whose
  message says what to change. Otherwise, on iOS 26 and later while Exact's own tab
  container shows the tabs, a development build writes the reason to `logs`. iOS 17
  to 25 have no accessory: there the element stays where it is written, hidden under
  the shown tab bar (a known gap, QUEUE.md).
