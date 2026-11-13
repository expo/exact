surfaces: terminal
agent: Claude (Opus 5.5)
task: the harness's and todo's side of LLP 1101.003, a terminal entry run once from a shell

### Rough

Declaring `routes` reserves the shape name `Entry` for the router's visits,
and the harness's seam already had a shape of that name. The compiler said
so (`type-shape-reserved`); the seam's shape is now `LogEntry`, and the Rust
side keeps calling it `Entry` through a renaming import.

`replace(nav, "/active")` from `/` is refused at run time ("replace cannot
change the tab's root route"), journaled only, so todo's filter buttons did
nothing and no test failed until the host test pressed one. `open` builds the
location's chain, as a launch there does, and is what a filter switch wants.

### Lean in

A route table and `searchParam(top(nav), "prompt")` were all the launch
location needed: the prompt arrives as a read, and a gated one-shot task
turns it into the one send.

### Needed

none

### Checkpoints

- `cargo test -p harness-data`: a prompt from the location is asked
  literally (`/model x`, an absolute path); a failed route fails the session
  until a turn starts; an unattended run refuses an approval and fails; an
  interrupted turn stays running until its thread leaves, and unattended it
  starts no queued prompt.
- `cargo test -p exact-terminal --test it todo`: the filter keys navigate.
