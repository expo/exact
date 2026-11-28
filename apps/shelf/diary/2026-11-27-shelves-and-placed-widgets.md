surfaces: ios
agent: Claude Code, Claude Opus 5.5
task: a book row that follows a shelf row, shelves kept out of search, and the app's list of its placed widgets

### Rough
Edit Widget, the widget gallery and Spotlight have no accessibility tree that a
drive reaches on the iOS 27.2 simulator, so every tap there was by position,
and Spotlight's field took text only key by key.

WidgetKit's `getCurrentConfigurations` was not a plain list of placed widgets.
For up to about a minute it kept listing a removed widget, and the
configuration an Edit Widget sheet had just replaced. It listed two widgets
with the same choices once. The extension's record and WidgetKit's count can
therefore agree and both be wrong for a while; the app now checks again every
30 seconds for two minutes after it becomes active or the record changes.

A timeline reload that the app asked for 31 seconds after another ran a minute
later, and the app's answer was `ready: false` until then.

Two reviews of the pull request found what the build and the one device run
had not. Stats could not read the placements of a second widget kind, because a
source had one answer shape for the whole app. Publishing each book's shelf as
a field of `ContentDescription` made every publisher of every app write
`within=none`. And on Android, where a widget has no entity rows, both new
widgets said "Choose a shelf and a book" with no way to choose.

### Lean in
The shelves were already the books' statuses, so publishing them as a kind and
naming each book's shelf with the `within` option was two lines in `reindex`. Both new
widgets listed only the chosen shelf's books, in the list and in the search, and
Your Shelf, with no Shelf row, kept listing every book. Edit Widget cleared the
Book row by itself when the shelf changed.

Moving a chosen book to another shelf in the app turned both widgets that showed
it to the first book on their shelf without an edit. With nothing chosen they
show the book being read, so they have something to show on a host that offers
no rows.

`"search": false` took the shelves out of Spotlight at the next launch and put
them back when it was removed; the books stayed in.

### Needed
- widgets: provided
- system search: provided

### Checkpoints
- editing app.contract / app.ts: smooth
- iOS build: smooth
- driving the app (`exact.mjs agent`, `exact.mjs test`): rough (see Rough, the Home Screen and Spotlight)
