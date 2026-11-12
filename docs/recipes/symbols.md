# Recipe: symbols

`image "symbol:<role>"` is an icon each host draws from its own set (SF Symbols on Apple,
matching outlines elsewhere). `image "symbol:sf/<name>"` is any SF Symbol by Apple's name.

```contract
component Toolbar
  state starred = false
  action star
    starred = not starred
  view
    row gap=16 padding=16 align-items="center"
      button aria-label="Add" testId="add"
        image "symbol:add"
      button press=star aria-label=(starred ? "Unstar" : "Star") testId="star"
        image (starred ? "symbol:star-fill" : "symbol:star")
      button aria-label="Share" testId="share"
        image "symbol:share"
      image "symbol:sf/cloud.sun.rain" aria-label="Showers later" font="-exact-title1"
      row gap=6 align-items="center"
        image "symbol:calendar" aria-hidden=true
        text "Tomorrow"
```

Roles include `add`, `close`, `back`, `search`, `share`, `calendar`, `heart` and `star`.
`contract vocab symbol` lists every role. Unknown literals fail the build; computed
ones are checked at runtime. Role aliases (`gearshape`, `trash`, `chevron-right`)
build with a `lower-symbol-alias` warning naming the role.

- A role draws everywhere; `sf/<name>` on Apple only. Else add an icon token to
  `app.tamber`'s `icons`, `icon.rain = icon(sf "cloud.rain", svg "icons/rain.svg")` (a
  one-colour SVG), and write `symbol:icon.rain`.
- A symbol-only button needs an `aria-label`. A tab is a symbol over its text. A symbol beside
  text that says the same is decoration: `aria-hidden=true`, so VoiceOver reads it once.
- `activity` is an ECG trace (heart, workouts). Done or not done is `DoneMark` (`exact:content`).
- Leave a symbol's colour unsaid: in a button it is the tint, and it follows dark mode.
  A `color` written on the symbol is its colour, as `currentcolor` paints an icon on the
  web; a row's `forward-chevron` is already grey.
  Emoji and characters (▦ ☏) aren't icons: they ignore the tint and differ by browser.
- A symbol's font sets its natural size. Use `font="-exact-title1"` to follow the
  reader's text size. On a page or inside `button appearance="none"`, `width` and
  `height` size its box, and the glyph scales to fit it, keeping its aspect under
  both the default `object-fit="fill"` and `object-fit="contain"`. For a 24-point
  icon box, write `image "symbol:calendar" width=24 height=24`.
  `object-fit="cover"` fills and clips; `"none"` keeps the natural size;
  `"scale-down"` shrinks it only when it does not fit. A fixed box can therefore
  keep the glyph the same size when `font-size` changes.
- On iOS, a titled native button's symbol follows its title's font; a symbol-only
  button uses the platform's size. An image's own `font-size` overrides either
  (for example, `font-size="-exact-title1"`). Native button images take no `width`,
  `height` or `object-fit`; to fit a symbol into an authored box, use
  `button appearance="none"`. Native bars, tabs and menus use platform symbol sizes.
