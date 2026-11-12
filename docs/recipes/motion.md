# Recipe: motion

Use `transition` for changes, `keyframes` and `animation` for entrances or loops,
`-exact-exit-animation` for removal, `-exact-layout-transition` for layout.

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
      text "Saving…" animation=(still ? "none" : "breathe 1.6s ease-in-out infinite")
      column height=(open ? "auto" : "0px") overflow="hidden" inert=(not open)
        -exact-layout-transition=(still ? "none" : "320ms ease")
        text "Shown" opacity=(open ? 1 : 0) transition=(still ? "none" : "opacity 200ms ease")
      for name, i in items key=name
        row gap=8 animation=(still ? "none" : `rise 300ms ${i * 60}ms both`)
          -exact-layout-transition=(still ? "none" : "320ms ease")
          -exact-exit-animation=(still ? "none" : "leave 200ms ease-in both")
          text name flex=1
          button press=remove(name) testId=`remove-${name}`
            text "Remove"
```

- **What animates:** translate, scale, rotate, opacity, colours, `box-shadow`, SVG geometry,
  numeric `height`, `font-weight`, `font-stretch`, `font-variation-settings`. Other layout
  properties are refused; use `-exact-layout-transition`. A native `button` animates
  only opacity and transforms.
- **Spring:** `-exact-spring(stiffness, damping, mass)` is transition timing with no
  duration; `animation` refuses it.
- **Hover/focus scale:** `:hover` and `:focus-visible` accept `scale=1.08`. Set
  `transition="scale 150ms ease"` on the node. Paint and hits grow; layout stays fixed.
  Leaving restores the base.
- **Keyframes:** literals or app `fn` on constants.
- **Exit:** an `if` arm or `for` row leaves layout at once, inert; siblings use their layout
  transitions. The exit must end. Unsupported in a screen being popped, and on Linux.
- **Layout transition:** moves and resizes the surface; children jump, unscaled.
  `sharedElement=` flies between boxes on iOS, macOS and web, not Linux.
- **Expand/collapse:** keep content mounted (`Shown`) in a clipped wrapper with `height`
  changing between `"0px"` and `"auto"`. An `if` row appears at full size; siblings slide over it.
- **JS target:** bound exit/layout values must be binding literals.
- **Stagger:** use delays, as above. State changes after motion use a gated `after(ms, action)`.
- **Reduced motion:** gate on `exactViewport()`; drive `prefer prefers-reduced-motion reduce`.
  Apple shared-element flights land at once.
- **Check:** `clock settle` ends motion; `screenshot .exact/m.png over 600 every 50` films it.
