# Recipe: dragging something out of the app

A node a person will lift and drop into another app (Messages, Notes, Photos,
Files, the other pane of a split screen) says so with HTML's `draggable`
attribute and declares what the drag carries (LLP 1094.000). A press held on
the node lifts it on iOS and Android, and a mouse on the Mac; the drop gets
the file itself. On the web the browser drags it, with the file's URL. Linux
drags text and link, not files. `tree` and `expect drag` work on all hosts.

```text
fn thumb(l: Landmark): string = `assets/apple/${l.id}-thumb.jpg`

link href=path("landmark", l.id) press=open(path("landmark", l.id)) aria-label=l.name draggable=true dataTransfer=DataTransfer(text=l.name, files=[thumb(l)], names=[l.name]) testId=`card-${l.id}`
  image thumb(l) width=120 height=120 object-fit="cover"
```

- **`draggable`** is a boolean, which may be bound: `draggable=(not
  editing)`. It goes on a box (`view`, `column`, `row`, `box`), a `link`, a
  `button` or an `image`, not on a run of text or inside a grouped list
  (`list appearance="auto"`). It needs `dataTransfer`, which needs it.
- **`dataTransfer=DataTransfer(text=, url=, files=[…], names=[…])`** is what
  the drag carries; give `text`, `url` or `files`. Each file goes as itself,
  typed by extension:
  - `url` is an absolute `http:` or `https:` URL, never an app path;
  - a shipped file is written as an `image` names one, `assets/photos/7.jpg`;
  - an `app:/` path needs its `fs.read` grant, and the web cannot carry one;
  - not an element's `"#id"`: name the file the element shows.
- **`names`**, with `files`, names each file at its place, so Finder or
  Files saves `Mount Fuji.jpg`, not `1016-thumb.jpg`. A name without the
  file's suffix gets it; an empty one keeps the file's own. The rule a name
  keeps is under `draggable` in `docs/contract-grammar.md`.
- **No drag events yet**: `dragstart`, `drag` and `dragend` are refused.

## Gestures

On one node, `pan`, `panrelease`, `reorderFor`, `transformDragFor` and
`heightDragFor` claim the drag's movement and are refused beside `draggable`:
put the gesture on a child, or `draggable` on a wrapper. `press`,
`contextmenu`, `contextPopover` and swipes may share the node (D8).

## Testing it

The runner refuses and journals (`drag: refused: …`) a file outside the
grants, an `app:/` file on the web, a name that breaks the rule (the file
keeps its own) and a `url` that is not absolute. `tree` shows on each
node's line what its drag carries as admitted. A test asserts it the same on
every host:

```contract-test
test "a card carries its landmark's thumbnail for a drag"
  clock settle
  expect drag "card-1016" has "image/jpeg" name "Mount Fuji.jpg"
  expect drag "card-1016" missing "text/uri-list"
```

`has "text/plain" == "…"` compares a string, `name` the name a file is
offered under, and `none` passes while the node carries nothing. A test of
`app:/` files names its hosts (`test "…" on ios, macos`).
