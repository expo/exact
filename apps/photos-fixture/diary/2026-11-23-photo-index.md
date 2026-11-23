surfaces: ios-simulator (iOS 27.0), android (emulator, API 36)
agent: Claude Code, Claude Opus 5.5
task: keep an index of the photo library in step with it (expo/exact#714, LLP 1133.019.000 piece 6)

### Rough

- One return to the app on Android made the index sync twice: the module
  announces `photoAssets` and `photoChanges` for one change, the index reads
  both, and the runner asked it again once per topic (drive 9 saw syncs go
  from 1 to 3). Fixed in exact2: topics announced together ask a reader once
  (LLP 1016.002 D3).
- The fixture's data crate called no module before, so it had no binding:
  a `build.rs` with `contract::build_module_rust` and a `pub mod modules`
  give `photos::photo_changes` and its reply parser.
- The Android crate handed `canvas_jni!` the data type directly, and a
  data source reached through `Default` never links SQLite: every `sqlite`
  request would fail at run time. `exact_logic::configured!` with
  `ibex2_sqlite::link()` in its factory makes the type the host defaults.
- One more row of buttons under the top row would have pushed the image
  codec's controls off the emulator's screen (as in the screenshot filter's
  diary), so Import opens a panel of its own, shown only once pressed.
- `simctl addmedia` hangs once a UI test has run on the simulator, so each
  iOS run starts from an erased, freshly booted simulator.

### Lean in

The importer is a Rust resource that walks module pages and SQLite
transactions as one chain of `Answer::Later`s. Reading `photoAssets` and
`photoChanges` through the binding makes the resource a watcher of both, so
an announcement asks it again, and one that comes while a sync is in flight
waits for it to answer: the serialised sync D1 asks for, with no queue in
the app. Passing the status's access as an argument discards a read that
access changed under, as D1 asks, with no code.

### Needed

- photo library, read and changes since a token (`@exact/photos`)
- app storage, SQLite

### Checkpoints

- `cargo test -p photos-fixture-data`: eight cases against a scripted
  library and a real SQLite file.
- `apple.mjs importer` on an iOS 27.0 simulator: first import of more than
  520 photos, a walk stopped and resumed across a relaunch, an add during a walk,
  a replacement stopped mid-list, the screenshots view, a denial.
- `android.mjs --emulator` drive 9 on API 36: 501 screenshots and more,
  a return after a change, a relaunch, a replacement stopped mid-list, a
  denial, each sync one replacement, in step.
