# Recipe: foldables

On a Duo, `fold 130` sets the hinge. Posture is `continuous` at 0° and
180°, `folded` at 90° and 130°. Android reports `folded` only when half open.
Place panes by `horizontalViewportSegments` (columns) and `verticalViewportSegments`
(rows). A flat separating hinge still gives two segments, the cover display one.
`env(viewport-segment-<left|top|width|height> <x> <y>)` excludes the hinge band.
Safari 27 reports no fold. For the Duo’s cover in landscape, write `fold 0` then
`rotate landscapeLeft`.

```contract
shape Viewport
  horizontalViewportSegments: number

component Reply
  resource viewport = exactViewport() as shape Viewport
  derive book = viewport.horizontalViewportSegments == 2
  state draft = ""
  action write(value: string)
    draft = value
  view
    main width="100%" height="100%" position="relative"
      column position="absolute" top=0 bottom=0 left=(book ? "env(viewport-segment-left 1 0)" : "max(0px, calc((100% - 480px) / 2))") width=(book ? "env(viewport-segment-width 1 0)" : "min(100%, 480px)") box-sizing="border-box" padding=16 gap=12 testId="pane"
        text "Reply" role="heading"
        textarea value=draft input=write aria-label="Reply" testId="reply"
```

On a fold ([HIG](https://developer.apple.com/design/human-interface-guidelines/designing-for-iphone-duo#Dynamic-layouts),
[Apple's talk](https://developer.apple.com/videos/play/tech-talks/111463/)):

- Scrolling content can span the hinge; keep controls off it.
- Centre a non-scrolling pane (full width if narrow) or use the trailing segment.
- **A list and its detail** is a split view (`pattern list-detail`; UIKit's on iOS,
  LLP 1127). **Peer panes** use [native containers](../contract-for-agents.md#arranged-panes)
  on iOS 27.1 and responsive CSS elsewhere (Duo Lab's Panes tab).
- A route on the trailing segment keeps its title with
  `navigationBar=(book ? "none" : "auto")`: its first-child `header` paints in
  that pane, with its own Back if pushed; otherwise UIKit makes the bar.
- Segment indices count from the left. Start on segment 1 in RTL.
- Start the panes at the viewport's left edge. Duo Lab's Arrangement keeps panes below the header
  with a `space-between` row across a fold, or a box ending at the fold and an `absolute` lower pane.
  Inset controls from safe edges.
- In a laptop fold, put authored controls on the bottom segment. Cap a bottom
  sheet's `max-height` at the room below its header and
  `calc(100% - env(viewport-segment-top 0 1))`, from the viewport origin.
  An iOS keyboard can leave that segment below the viewport and zero tall;
  Duo Lab's note sheet then lifts the cap.
- End panes under the keyboard at `100%` or `bottom: 0`; hosts' segment heights differ.
- Keep panes to retain text. A panel switch may end editing.
- Panes move as UIKit's with `-exact-layout-transition="-exact-spring(439, 41.9, 1)"`; browsers drop
  `env()` early on opening (fall back to `exactViewport().segments`).
- UIKit's alerts and sheets on a fold: [Folds on iOS](../contract-for-agents.md#folds-on-ios).
