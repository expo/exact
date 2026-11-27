# Recipe: dragging something out of the app

A node a person will lift and drop into another app (Messages, Notes, Photos,
Files, the other pane of a split screen) says so with HTML's `draggable`
attribute and declares what the drag carries (LLP 1094.000). A press held on
the node lifts it on iOS and Android, and a mouse on the Mac and in Linux's
Wayland window; the drop gets the file, copied on Linux. On the web the
browser drags it, with the file URL. `tree` and `expect drag` work on all
hosts.

```text
fn thumb(l: Landmark): string = `assets/apple/${l.id}-thumb.jpg`

link href=path("landmark", l.id) press=open(path("landmark", l.id)) aria-label=l.name draggable=true dataTransfer=DataTransfer(text=l.name, files=[thumb(l)], names=[l.name]) testId=`card-${l.id}`
  image thumb(l) width=120 height=120 object-fit="cover"
```

- **`draggable`** is a boolean, which may be bound: `draggable=(not
  editing)`. It goes on a box (`view`, `column`, `row`, `box`), a `link`, a
  `button` or an `image`, not on a run of text or anywhere inside a grouped
  list (`list appearance="auto"`).
- **`dataTransfer=DataTransfer(text=, url=, files=[…], names=[…])`** is what
  the drag carries; give at least one of `text`, `url` and `files`. `text` goes as plain text, `url` as a URI
  list (Android: plain text), each file as itself, typed by extension:
  - `url` is an absolute `http:` or `https:` URL, never an app path, which
    another app cannot open;
  - a file the app ships is written as an `image` names one,
    `assets/photos/7.jpg`, and needs no grant;
  - an `app:/` path, such as `app:/data/exports/7.pdf`, needs the `fs.read`
    grant for it in `app.json`, and the web cannot carry one;
  - not an element's `"#id"`: a drag carries a file, so name the file the
    element shows.
- **`names`**, which needs `files`, gives the name each file is offered
  under, at the same place as the file: the landmark's name above, so
  Finder or Files saves `Mount Fuji.jpg`, not `1016-thumb.jpg`. A missing or
  empty name keeps the file's own. A name whose suffix is not the file's, or
  another spelling of its type, gets the file's suffix (`Mount Fuji` becomes
  `Mount Fuji.jpg`, `Fuji.png` becomes `Fuji.png.jpg`; `Fuji.jpeg` stays).
  Exact's rule for a name, so that every system can write it: 1 to 255
  bytes of UTF-8, with no control character, none of `< > : " / \ | ? *`,
  no space at either end, no dot at the end, and not a name Windows keeps
  for a device (`CON`, `NUL`, `COM1`, …). The compiler refuses a literal
  name that breaks it; the runner journals a bound one and offers the file
  under its own name. The type stays the file's. Where the platform takes a
  name: the item's suggested name on iOS, the promised file on the Mac, the
  provider's display name on Android, Chrome's download name for the first
  file on the web, and the copy's name on Linux.
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

The runner admits each file, name and `url` for the host: a file outside the
grants, an `app:/` file on the web, a name that breaks the rule above, or a
`url` that is not absolute is refused and journaled as `drag: refused: …`
(a refused name leaves the file its own), and `tree` shows on each node's
line what its drag carries as admitted, a named file with its name
(`drag{image/jpeg assets/apple/1016-thumb.jpg as "Mount Fuji.jpg", …}`). An authored test asserts
them, the same on every host:

```contract-test
test "a card carries its landmark's thumbnail for a drag"
  clock settle
  expect drag "card-1016" has "image/jpeg" name "Mount Fuji.jpg"
  expect drag "card-1016" has "text/plain" == "Mount Fuji"
  expect drag "card-1016" missing "text/uri-list"
```

`has "text/plain" == "…"` compares a string, `name` the name a file is
offered under (the one `names` gave it, else its own), and
`none` passes while the node carries nothing (`draggable` false, or nothing
admitted). A test of `app:/` files names its hosts (`test "…" on ios,
macos`).
