surfaces: web, linux
agent: Codex
task: Virtualize a conversation transcript

### Rough

The eager transcript mounted all 1,000 fixture messages. A collection requires
one direct keyed loop; its status content needs a row in that loop too.
Headless Linux's secondary mouse input only accepts canvases. The drive opens
message actions through existing reaction buttons instead.
The million-pixel end request stopped near row 10,000 in a 12,000-message
conversation. Keyed requests reach the final status row and include the list's
bottom padding. The fixture now takes an assigned port, with browser request
routing and a native HTTP proxy confined to each session.
At 12,000 messages, constructing the full native snapshot exceeds the source's
100 ms CPU budget. The native drive keeps 1,000 messages; the web drive uses
12,000 to cross the former end request. Bounded source answers remain separate
work.

### Lean in

The existing collection accepts the timestamp pan and measured message rows.
Source answers and message actions can keep their current interface.

### Needed

- collections: provided, including end following and keyed scroll requests
- storage: provided for local conversation fixtures

### Checkpoints

- Before the change, the web regression mounted 1,000 message rows.
- After the change, web and headless Linux mount fewer than 100 rows, evict
  distant rows and preserve older content when a message arrives. Both drives
  cover timestamp pan, reply, deletion, typing, streaming, retry and empty chats.
- Eight data tests and three authored browser tests pass.
- The 12,000-message web and 1,000-message headless Linux drives cover opening
  at the end, Latest, composer focus and sending from older history, with fewer
  than 100 rows mounted.
- JS/wasm/Linux conformance reports the same 60 storage-notice and resulting
  tree, layout and pixel differences on the original and virtualized Contracts.
- Two authored Linux tests refuse non-canvas context-menu input with either plan.
