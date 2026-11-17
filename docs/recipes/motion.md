# Recipe: motion

`transition` for changes; `keyframes` and `animation` for entrances or loops;
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

- **Animates:** translate, scale, rotate, opacity, colours, `box-shadow`, SVG geometry,
  numeric `height`, `font-weight`, `font-stretch`, `font-variation-settings`, `filter`
  (transitions). Layout: `-exact-layout-transition`. Native `button`: opacity,
  transforms.
- **Spring:** `-exact-spring(stiffness, damping, mass)`: transition timing, no duration;
  not `animation`.
- **Hover scale:** `:hover`, `:focus-visible` take `scale=1.08` with
  `transition="scale 150ms ease"`; paint and hits grow, layout stays.
- **Keyframes:** literals or app `fn` on constants.
- **Exit:** an `if` arm or `for` row leaves layout at once, inert, and must end; siblings use
  layout transitions. Not in a popped screen or on Linux, Windows or Android.
- **Routes:** a route takes `::view-transition-new` and `::view-transition-old`, its
  entrance and exit: `animation=` alone, keyframes of opacity, translate, scale, rotate.
  `none` cuts; unset is the platform's. Not on a sheet or zoom route.
- **Layout:** moves/resizes the surface; children jump, unscaled.
  `sharedElement=` flies, not on Linux or Windows.
  Android: in-page only; no native dialogs or zooms.
- **Expand/collapse:** as above; an `if` row appears full size, siblings slide over.
- **JS target:** bound exit/layout values must be binding literals.
- **Stagger:** delays; after motion, a gated `after(ms, action)`.
- **Reduced motion:** gate on `exactViewport()`; drive `prefer prefers-reduced-motion reduce`.
  Native flights land; route parts cut.
- **Check:** `clock settle` ends it; `screenshot .exact/m.png over 600 every 50` films it.
