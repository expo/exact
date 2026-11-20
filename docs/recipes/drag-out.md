# Recipe: dragging something out of the app

A node a person will lift and drop into another app (Messages, Notes, Photos,
Files, the other pane of a split screen) says so with HTML's `draggable`
attribute and declares what the drag carries (LLP 1094.000). The declaration,
`tree` and `expect drag` work on every host today; no host lifts the drag to
the system yet, which each platform's stage of LLP 1094.000 adds.

```contract
fn thumb(l: Landmark): string = `assets/apple/${l.id}-thumb.jpg`

link href=path("landmark", l.id) press=open(path("landmark", l.id)) aria-label=l.name draggable=true dataTransfer=DataTransfer(files=[thumb(l)]) testId=`card-${l.id}`
  image thumb(l) width=120 height=120 object-fit="cover"
```

- **`draggable`** is a boolean, which may be bound: `draggable=(not
  editing)`. It goes on a box (`view`, `column`, `row`, `box`), a `link`, a
  `button` or an `image`, not on a run of text or anywhere inside a grouped
  list (`list appearance="auto"`).
- **`dataTransfer=DataTransfer(text=, url=, files=[…])`** is what the drag
  carries; give at least one. `text` is offered as `text/plain`, `url` as
  `text/uri-list`, and each file as itself, its type from its extension:
  - `url` is an absolute `http:` or `https:` URL, never an app path, which
    another app cannot open;
  - a file the app ships is written as an `image` names one,
    `assets/photos/7.jpg`, and needs no grant;
  - an `app:/` path, such as `app:/data/exports/7.pdf`, needs the `fs.read`
    grant for it in `app.json`, and the web cannot carry one;
  - not an element's `"#id"`: a drag carries a file, so name the file the
    element shows.
- `draggable=true` needs `dataTransfer`, and `dataTransfer` needs
  `draggable`. Nothing drags on a native host unless it says so.
- **No drag events yet.** `dragstart`, `drag` and `dragend` are refused: an
  app cannot react when a drag begins or ends.

## Gestures

On one node, `pan`, `panrelease`, `reorderFor`, `transformDragFor` and
`heightDragFor` claim the drag's movement and are refused beside `draggable`:
put the gesture on a child, or `draggable` on a wrapper. `press`,
`contextmenu`, `contextPopover` and swipes may share the node (LLP 1094.000
D8).

## Testing it

The runner admits each file and `url` for the host: a file outside the
grants, an `app:/` file on the web, or a `url` that is not absolute is
refused and journaled as `drag: refused: …`, and `tree` shows the node's
`draggable` and `dataTransfer…` props as admitted. An authored test asserts
them, the same on every host:

```contract
test "a card carries its landmark's thumbnail for a drag"
  clock settle
  expect drag "card-1016" has "image/jpeg" name "1016-thumb.jpg"
  expect drag "card-1016" missing "text/plain"
```

`has "text/plain" == "…"` compares a string, `name` a file's own name, and
`none` passes while the node carries nothing (`draggable` false, or nothing
admitted). A test of `app:/` files names its hosts (`test "…" on ios,
macos`).
