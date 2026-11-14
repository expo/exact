surfaces: ios, macos
agent: Codex
task: keep unrelated network work out of library edits

### Rough
The root's Discover resource asked Open Library while Library or Stats was
selected. A pending reply therefore kept `clock data` waiting during local
edits. The resource now uses `when nav.tab == "discover"`; leaving that tab
cancels its pending read.

### Lean in
The resource gate owns both starting and cancelling the read. The data module
needs no navigation argument or special reply.

### Needed
- conditional requests: provided
- local storage: provided

### Checkpoints
- The compiled Shelf plan asks no Discover work on Library or Stats. Controlled
  replies settle progress and removal independently, and changing the subject
  outside Discover starts no request.
- The unchanged progress and swipe tests pass on iOS and macOS with both the
  agent's and platform's chrome. Every run ends with no pending request and no
  Discover fetch, measured through the driver's fault table.
