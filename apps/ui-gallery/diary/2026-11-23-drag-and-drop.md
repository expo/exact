surfaces: web, macOS 27, iOS 27.1 (iPhone Duo simulator)
agent: Claude Code (Claude Opus 5.5)
task: a Drag and drop section whose cards each carry one kind of data out of the app, and a drop zone that lists the files another app drops in (LLP 1094.000)

### Rough

- A card component that takes `url` as a prop cannot be given `url=""` for "no link": the compiler checks a literal passed to a component's `dataTransfer` argument as it checks one written on the node, and refuses the empty URL. Cards with a link are a second component, `LinkCard`. `text=""` passes and no host carries an empty text; `files=[]`, and a file entry that is empty, leave the files prop empty, which the runner clears.
- A data source cannot read the clock (`new Date()` is refused), so the note the app writes for its `app:/` card says no time.
- The note is asked only while the section shows, a root resource with `when`: the section's component, under the navigation's `for`, may not own a request (`type-child-resource`). The web answers without writing it, since it could not drag it. A drive that names no scratch store refuses storage, and the card then says why the note was not written; an authored test's session has a store.
- The Android entry lays the gallery out as the web does (`host` is `"web"`), so the section takes the real platform as its own prop.
- The gallery's own pictures are colour-test swatches with no camera metadata. `drag-photo.jpg` and `drag-photo.heic` are `interaction-gallery`'s generated `glasshouse.png` at 1024 px, written by ImageIO with EXIF, TIFF, GPS and IPTC metadata (camera, date, location, caption, keywords), so a receiver shows whether a file's metadata arrives; `drag-notes.pdf` is a page Core Graphics wrote with a title, an author and a subject, and `drag-chord.m4a` a three-second chord encoded by `afconvert`. Like their source, they show no real place, and their GPS tag only tests that a location arrives.

### Lean in

- `DataTransfer(text=, url=, files=)` on a box: each card is one row, and `expect drag` asserts what it carries with the file's own name and its type from the extension.
- `draggable` bound to state turns a card's drag off and on with nothing else.
- `drop`, `dragenter` and `dragleave` on a column make the drop zone; its `DragEvent.files` are `doc:` handles.

### Needed

- Nothing in exact2. Drops into the app reach the Mac, the web and Windows only, and carry files, not text or links (LLP 1094.000 §4); the zone says so on the other hosts.

### Checkpoints

- Web (JS target): `bun scripts/agent.mjs web --app ui-gallery --test apps/ui-gallery/drag.test.contract`, 3 passed and the native hosts' note test skipped. A drive dropping `drag-notes.pdf` and `drag-chord.m4a` on the zone (`tap drop-zone drop …`) lists both `doc:` handles; authored tests have no drop step.
- macOS 27: `bun host/apple/build.mjs ui-gallery-apple`, then the same tests on `macos`, 3 passed (the written note's included) and the web's skipped; the same drop there lists both handles.
- iOS 27.1, iPhone Duo simulator: `bun host/apple/build.mjs --ios ui-gallery-apple`, then the same tests on `ios`, 3 passed and the web's skipped.
