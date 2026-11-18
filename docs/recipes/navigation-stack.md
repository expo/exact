# Recipe: a navigation stack

Keyed screens go in the navigation root or linked tabpanels.

A route's first `header` with one heading is its bar (large at `aria-level=1`,
inline at 2). `navigationBack="close"` names a route control's `id`:
`button id="close" press=back`. A missing literal id warns (`lower-navigation-back-id`).

<!-- check: app -->
```contract
routes nav
  notes "/"
    note "/note/:id"
    add "/add"

shape Note
  id: string
  title: string

shape Reply
  ok: bool
  error: string

component App
  state draft = ""
  resource lib = loadNotes() as shape Note[] else empty()
  mutation added as shape Reply refreshes lib then afterSave
  mutation removed as shape Reply refreshes lib
  derive saveError = match added { case some(r) => r.error, case none => "" }

  action openAdd
    nav = push(nav, "/add")
  action back
    nav = back(nav)
  action follow(url: string)
    nav = go(nav, url)
  action edit(v: string)
    draft = v
  action save
    if draft.trim() != "" and not pending(added)
      send added = addNote(draft.trim())
  action afterSave
    if saveError == "" and top(nav).name == "add"
      draft = ""
      nav = back(nav)
  action askRemove
    showModal("remove")
  action remove
    send removed = removeNote(top(nav).params.id)
    nav = back(nav)

  view
    main navigationKey=`${top(nav).id}` navigationBack="close" navigate=follow width="100%" height="100%"
      for e in stack(nav) key=e.id
        if e.name == "notes"
          // the list
        if e.name == "note"
          // the detail
        if e.name == "add"
          // the sheet
      dialog id="remove" role="alertdialog" aria-label="Delete Note?"
        text "This can't be undone."
        button press=remove destructive=true commandfor="remove" command="close" testId="remove-confirm"
          text "Delete"
        button commandfor="remove" command="close"
          text "Cancel"
```

A navigating row is a `link href=path(…)` that `follow` pushes.

<!-- check: route notes -->
```contract
column navigationKey=`${e.id}` navigationScroll="notes-list" position="absolute" inset=0
  header display="flex" align-items="center"
    text "Notes" role="heading" aria-level=1 flex=1
    button press=openAdd aria-label="Add Note" testId="add-note"
      image "symbol:add"
  list id="notes-list" appearance="auto" listStyle="inset-grouped" flex=1 min-height=0
    section
      for n in lib key=n.id
        link href=path("note", n.id) testId=`note-${n.id}`
          text n.title
          image "symbol:forward-chevron"
```

<!-- check: route note -->
```contract
column navigationKey=`${e.id}` navigationScroll="note-list" position="absolute" inset=0
  header display="flex" align-items="center"
    text "Note" role="heading" aria-level=2
  list id="note-list" appearance="auto" listStyle="inset-grouped" flex=1 min-height=0
    for n in lib.filter(x => x.id == e.params.id) key=n.id
      section
        text n.title testId="detail-title"
        button press=askRemove destructive=true testId="delete-note"
          text "Delete Note"
```

A sheet has `navigationPresentation="modal"`. A swipe down presses its Cancel,
named by `navigationBack`. Web: a dialog, actions below.

<!-- check: route add -->
```contract
column navigationKey=`${e.id}` navigationPresentation="modal" navigationScroll="add-form" position="absolute" inset=0
  header display="flex" align-items="center" justify-content="space-between"
    button id="close" press=back aria-label="Cancel"
      image "symbol:close"
    text "New Note" role="heading"
    button press=save buttonStyle="bordered-prominent" aria-label="Save" testId="add-save"
      image "symbol:checkmark"
  list id="add-form" appearance="auto" listStyle="inset-grouped" flex=1 min-height=0
    section
      row
        input value=draft input=edit submit=save placeholder="Title" flex=1 min-width=0 testId="add-title"
```

```contract-test
test "add, open, delete after asking"
  tap "add-note"
  clock settle
  type "add-title" "Milk"
  tap "add-save"
  clock data
  clock settle
  tap "note-1"
  clock settle
  expect text "detail-title" == "Milk"
  tap "delete-note"
  clock settle
  tap "remove-confirm"
  clock data
  expect state lib == []
```

- `aria-label` titles an alert; Escape/Back chooses Cancel. Only `closedby="any"`
  dismisses outside. Close a sheet [after its write](after-a-write.md).
- Routes stay mounted: unique `testId`s. `clock settle` after a push, sheet or alert;
  `clock data` before reading a save.
- Route motion: [motion](motion.md).
- Handle unmatched locations (`notfound`) and missing details.
- `link href="#tags"` scrolls to `id="tags"` once data lands.

Each screen owns its state, resources, mutations and tasks. Pop destroys it;
covering keeps it alive. Pass activity as a prop; gate tasks with `when active`.

During a run, keep its `navigationBack` control mounted and disabled by bool state
(`running`). iOS hides Back and refuses the edge swipe; the web refuses Back.
A test's `back` step notes `back refused`.

```text
button id="close" press=back disabled=running display="none"
  text "Back"
```
