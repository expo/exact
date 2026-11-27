# Recipe: motion

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
  (transitions). Native `button`: opacity and transforms.
- **Spring:** `-exact-spring(stiffness, damping, mass)`: transition timing, no duration;
  not `animation`.
- **Hover scale:** `scale=1.08 transition="scale 150ms ease"` on `:hover` or
  `:focus-visible` grows paint and hits.
- **Keyframes:** literals or app `fn` on constants.
- **Exit:** an `if` arm or `for` row leaves layout and becomes inert at once.
  Give the exit an end. Siblings use layout transitions. Refused on route elements.
- **Routes:** `::view-transition-new` / `::view-transition-old` animate opacity,
  translate, scale and rotate. `animation="none"` cuts. Modal sheets and routes
  with a literal nonempty `navigationSource` reject these parts.
- **Layout:** surfaces move/resize; children jump, unscaled. `sharedElement=` flies
  (needs `-exact-layout-transition` on both endpoints) except Linux/Windows; Android: in-page only.
- **Zoom:** `navigationSource=` names a source `id` for iOS 18 zoom or Android's
  container transform. Without a visible source, push uses shared axis and
  fullscreen cuts. Sheets have their own motion.
- **Expand/collapse:** an `if` row appears full size and siblings slide over.
- **JS target:** exit/layout bindings must be literals.
- **Stagger:** delays; `after(ms, action)`.
- **Reduced motion:** `exactViewport()` and `prefer prefers-reduced-motion reduce`.
  Flights land and routes cut.
- **Check:** `clock settle` or `screenshot .exact/m.png over 600 every 50`.
  Android drives refuse film; use `--live` and `adb shell screenrecord`.
