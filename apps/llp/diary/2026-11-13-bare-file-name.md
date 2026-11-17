surfaces: none (the data source, by its Rust test)
agent: Claude Code, Claude Opus 5.5
task: open a document named by a bare file name

### Rough

- A bare name such as `1000-valid.md` has an empty parent, which the source took as
  its corpus root, and `read_dir("")` fails, so the reader refused a document it could
  read. Naming the folder `.` alone would read the corpus but leave the document as
  `1000-valid.md` beside index entries named `./1000-valid.md`, which never compare
  equal, so the document is named as the index names it too. About 15 minutes.

### Lean in

The source's `answer`, `continuation` and `parse` run in a test as the app runs them,
so the test drives the real corpus read.

### Needed

none

### Checkpoints

- `cargo test -p llp-data --test relative`: a bare name, `./` and the name, a file in a
  relative folder and an absolute path each open with their corpus's next document as
  a neighbour. The bare name fails before the change.
