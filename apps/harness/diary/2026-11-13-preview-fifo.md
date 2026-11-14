surfaces: terminal
agent: Claude Code, Claude Opus 5.5
task: keep an edit or write preview from blocking on a FIFO

### Rough

The approval preview read its target whole with `std::fs::read` before the
turn had an entry to cancel, so a FIFO held the turn: opening one to read
waits for a writer, and reading it waits for the writer's end. The preview
now opens without blocking and refuses anything that is not a regular file.
A unit test runs the preview on a FIFO in a thread and waits five seconds
for it.

### Lean in

`rustix`, already a dependency, names the flag (`OFlags::NONBLOCK`) on each
Unix.

### Needed

none

### Checkpoints

- driving the app: the unit test only
