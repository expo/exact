surfaces: none (the page source, by its Rust tests)
agent: Claude Code, Claude Opus 5.5
task: find a phrase that crosses emphasis, a link or inline code

### Rough

- Find searched each styled run alone, so `hello world` found nothing in
  `Hello **world**.`. The page's reuse check (`block_matches`) and the test oracle
  (`reference_value`) read the runs the same way, so all three changed together.
  About 20 minutes.

### Lean in

A span already carries its hit's ordinal, so one match split over three runs is three
spans with one ordinal, each keeping its run's weight, face and link, and the host
needed no change.

### Needed

none

### Checkpoints

- `cargo test -p llp-data`: a phrase across bold, a word half bold (twice), a match
  from inside inline code to inside a link, and one across a table cell's styles are
  each one hit; a phrase across two cells is none.
