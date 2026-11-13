# Recipe: an onboarding tip over a control

Install with `bun exact.mjs add @exact/tips`. A tip is Apple's TipKit popover, pointing
at one element by its `id`: on iOS and iPadOS 17 and macOS 14 and later. Every other host
fails both sends with `NotSupportedError` and shows nothing; there, teach the control with
its label or a help screen.

```contract
use showTip, invalidateTip from "@exact/tips"

component Library
  state books = 0
  task offering mount
    after(500, offerAddTip)
  action offerAddTip
    showTip(id="add-book", title="Add a book", message="Tap + to start your library.", image="plus.circle", source="add")
  action add
    books = books + 1
    invalidateTip(id="add-book")
  view
    main navigationKey="home" width="100%" height="100%"
      column navigationKey="home" position="absolute" inset=0
        header padding="8px 16px"
          text "Library" role="heading" aria-level=1
          button press=add id="add" aria-label="Add Book"
            image "symbol:add"
        text `${books} ${books == 1 ? "book" : "books"}` padding=16
```

- **`showTip` offers; TipKit decides.** It shows the tip unless the person closed it or
  the app invalidated it, and it remembers that across launches. The send answers nothing
  either way, so offer the tip whenever the screen shows.
- **`source` is an element's `id`.** A button the header puts in the navigation bar (the
  Mac's window toolbar) gets the tip on its bar item; any other element on its own box. An
  `id` that nothing on screen carries shows nothing and says so in `logs`.
- **`invalidateTip` when the person does the thing**: the tip closes if it is showing and
  never shows again.
- `image` is an SF Symbol's name (`plus.circle`), `message` is optional, and the title and
  message are shown as written, so pass them already translated.
- Only the app offers a tip: a `tool` or a wake calling `showTip` is refused
  (`person-required`).

Under the agent the popover shows as it does for a person, `logs` says
`showTip: add-book shown over #add's bar item`, and TipKit's record is the drive's: a
fresh drive starts with every tip unseen, and a named store (`--storage`) keeps what was
invalidated across relaunches. The agent's `screenshot` does not show the tip's content
(on iOS it draws the popover's empty frame, on the Mac nothing of its window), so read
`logs`; the Mac's agent chrome paints the header in the page, and `--chrome platform` puts
its buttons in the window toolbar.
