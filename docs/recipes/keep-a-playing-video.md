# Recipe: a video that keeps playing from a card into its detail and back

A video declared again in the detail restarts. Write the player once, outside the
routes, and name where it shows with `portal`: the same element moves, so the video
plays on and the counter keeps its count.

```contract
routes nav
  tab feed "/"
  detail "/detail"

component App
  derive open = top(nav).name == "detail"
  action show
    nav = push(nav, path("detail"))
  action back
    nav = back(nav)
  view
    main navigationKey=`${top(nav).id}` navigationBack="back" width="100%" height="100%"
      for entry in stack(nav) key=entry.id
        if entry.name == "feed"
          column navigationKey=`${entry.id}` position="absolute" inset=0 gap=12 padding=16
            button press=show testId="open"
              text "Open"
            box id="card-player" width=168 height=148
        else if entry.name == "detail"
          column navigationKey=`${entry.id}` navigationPresentation="fullscreen" navigationSource="card-player" position="absolute" inset=0 gap=12 padding=16
            button id="back" press=back testId="back"
              text "Back"
            box id="detail-player" width="100%" height=320
      box
        column portal=(open ? "detail-player" : "card-player") width="100%" height="100%" gap=6
          video "assets/clip.mp4" autoplay=true muted=true playsinline=true loop=true width="100%" flex=1 min-height=0 testId="player"
          Counter()

component Counter
  state count = 0
  action increment
    count = count + 1
  view
    button press=increment testId="count"
      text `Count ${count}`
```

```contract-test
test "the counter keeps its count through Open and Back"
  tap "count"
  tap "open"
  clock settle
  expect text "count" == "Count 1"
  tap "count"
  tap "back"
  clock settle
  expect text "count" == "Count 2"
```

- **Write it** where it lives as long as both screens (here after the route loop), not
  in the card: a list row's element dies with its row. Its state stays there, and its
  presses and keys bubble there.
- **The destination** is the first element with that `id`, which must be a plain box
  (`contract vocab portal`). With none (another tab), the element waits alive and
  unseen; a muted autoplay video pauses there and plays on when shown (Firefox keeps
  it playing). A box under `display: none` still takes it; on Android that reloads it.
- **The motion** is the destination's: on iOS 18 and later and on Android the detail expands from
  the card (`navigationSource`). Removing the declaration removes the element.
- The clip is `assets/clip.mp4` in the app. The test reads only the count;
  `apps/shared-elements/app.test.mjs` checks one load and advancing `currentTime`.

Each host: [the reference's table](../reference.md#a-portal-element-on-each-host).
Linux and Windows decode no video.
