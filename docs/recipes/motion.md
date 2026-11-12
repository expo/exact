# Recipe: motion

Use `transition` for a changing value, `keyframes` and `animation` for entrances or loops,
`-exact-exit-animation` for removal, and `-exact-layout-transition` for layout changes.

```contract
keyframes rise
  from opacity=0 translate="0px 8px"
keyframes leave
  to opacity=0 scale=0.96 animation-timing-function="ease-in"
keyframes breathe
  50% opacity=0.4

shape Media
  prefersReducedMotion: bool

component App
  resource media = exactViewport() as shape Media
  derive still = media.prefersReducedMotion
  state items = ["Kick", "Snare", "Hat"]
  state open = false
  action toggle
    open = not open
  action remove(name: string)
    items = items.filter(i => i != name)
  view
    column padding=16 gap=8
      button press=toggle testId="toggle"
        text "Details"
      text "Shown" opacity=(open ? 1 : 0) translate=(open ? "0px 0px" : "0px -8px")
        transition=(still ? "none" : "opacity 200ms ease, translate -exact-spring(240, 26, 1)")
      for name, i in items key=name
        row gap=8 animation=(still ? "none" : `rise 300ms ${i * 60}ms both`)
          -exact-layout-transition=(still ? "none" : "320ms ease")
          -exact-exit-animation=(still ? "none" : "leave 200ms ease-in both")
          text name flex=1
          button press=remove(name) testId=`remove-${name}`
            text "Remove"
      text "Saving…" animation=(still ? "none" : "breathe 1.6s ease-in-out infinite")
```

- **What animates:** translate, scale, rotate, opacity, colours, `box-shadow`, SVG geometry,
  a numeric `height`, `font-weight`, `font-stretch`, `font-variation-settings`. Other
  layout properties (`width`, `top`) are refused, naming `-exact-layout-transition`. A
  native `button` animates only opacity and transforms.
- **`-exact-spring(stiffness, damping, mass)`** is a transition's timing, with no duration;
  an `animation` refuses it.
- **Keyframes** take literals or the app's own `fn` on constants.
- **Exit:** plays when an `if` arm or `for` row leaves, and must end (`infinite` and
  `paused` are refused). The node leaves layout at once, inert; others move by their layout
  transition. Not inside a screen being popped, nor on Linux.
- **Layout:** `-exact-layout-transition` moves and resizes a box; children jump,
  unscaled. `sharedElement=` flies on iOS, macOS, Android and web, not desktop Linux.
  Android excludes native dialogs and presentation zooms; flights stay in-page.
- **JS target:** a bound exit or layout value must be one of the binding's literals; one
  built at runtime clears the row with a diagnostic.
- **Stagger** with delays (`"a 200ms both, b 300ms 200ms both"`). A state change when a
  motion ends is a gated task's `after(ms, action)`; a `sequence` is sound and haptics, not
  motion.
- **Reduced motion:** gate transitions and keyframes with `exactViewport()`, as above.
  Shared flights on iOS, macOS and Android snap. Drive with
  `prefer prefers-reduced-motion reduce`.
- **Check it:** `clock settle` ends the motion; `screenshot .exact/m.png over 600 every 50`
  films it.
