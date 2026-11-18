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
  (transitions). Layout: `-exact-layout-transition`. Native `button`: opacity,
  transforms.
- **Spring:** `-exact-spring(stiffness, damping, mass)`: transition timing, no duration;
  not `animation`.
- **Hover scale:** `:hover`, `:focus-visible`: `scale=1.08` with
  `transition="scale 150ms ease"`; paint and hits grow, layout stays.
- **Keyframes:** literals or app `fn` on constants.
- **Exit:** an `if` arm or `for` row leaves layout at once, inert, and must end; siblings use
  layout transitions. Not in a popped screen or on Linux, Windows or Android.
- **Routes:** `::view-transition-new` / `::view-transition-old` take `animation=`:
  opacity, translate, scale, rotate keyframes. `none` cuts; unset is the platform's.
  Not on sheets or zooms.
- **Layout:** surfaces move/resize; children jump, unscaled. `sharedElement=` flies
  except on Linux/Windows; Android: in-page only, no native dialogs.
- **Zoom:** `navigationSource=` names an underlying `id`: UIKit's zoom on iOS 18;
  Android's whole-route container transform on push/fullscreen, back to a visible
  source. Missing/hidden/clipped/off-screen: push uses shared axis, fullscreen cuts.
  No sheets.
- **Expand/collapse:** as above; an `if` row appears full size, siblings slide over.
- **JS target:** bound exit/layout values must be binding literals.
- **Stagger:** delays; `after(ms, action)`.
- **Reduced motion:** gate on `exactViewport()`; drive `prefer prefers-reduced-motion reduce`.
  Flights land; routes cut.
- **Check:** `clock settle`; `screenshot .exact/m.png over 600 every 50` films motion.
