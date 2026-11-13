# Messages

The canonical Exact2 Messages test app.

This is an Exact Contract/TypeScript port of
[Expo's chat-demo](https://github.com/expo/react-native/tree/chat-demo/packages/chat-demo)
at `da3b4d6ad3d6a4ac9fa95fb2efd9a532bdf0544e`. Its `ui-metrics.md`, Composer,
ChatScreen, and native balloon-path implementation supply the measurements. The
reference's MIT license is in `assets/reference-LICENSE.txt`.

It is a pixel-match port of that reference, with its colours and sizes written
out in hex and points, not a style guide: a new app leaves those unsaid and names
colour roles (LLP 1115, `docs/contract-for-agents.md`, "Prefer native controls").

Run from the repository root in two terminals:

```sh
bun apps/messages/service.ts
bun host/web/dev.mjs --app messages --port 8767
```

Open http://127.0.0.1:8767. Native builds use the same local AI service:

```sh
bun host/apple/build.mjs --ios messages-apple --run
bun host/apple/build.mjs messages-apple --run
```

The service reads `~/Dropbox/APIKeys/openrouter.txt`. `OPENROUTER_KEY_FILE` or
`OPENROUTER_API_KEY` can override it. The key stays in the Bun process; it is never
baked into the app or sent to the browser. The service listens on loopback port
4318 and admits browser origins on local development ports 8765–8768. This setup
supports the browser, Mac, and iOS simulator; a physical phone requires a separate
reachable, authenticated transport.

The initial **Chat** conversation is the reference's Ada/Lovelace fixture. The other
threads connect to different models. Compose creates a new model conversation;
tapping a model name switches the model for that thread. Model search fetches
OpenRouter's live text-model catalog. The local service streams OpenRouter responses
and the app polls cumulative snapshots every 250 ms, preserving text through native
answer cancellation. Stop, retry, failure messages, independent conversation context,
and persisted drafts/transcripts/reactions are implemented. Native data lives in
`app:/data/messages.json`; the browser uses Exact's local storage implementation.

The transcript uses the reference's 17/20-point typography, 14/10-point balloon
insets, 280.667-point portrait width limit, 6.65-point fitted tail, run grouping,
receipts, typing pulse, resisted timestamp drag, swipe-to-reply, message menus and
long-text reader. The growing composer uses its measured field/button insets.
Send motion uses the reference's spring coefficients, delay, and squash timing.
The demo's Add menu can receive messages, append fifty rows, toggle typing, or reset.
Light/dark colors and reduced-motion settings follow the host.

The transcript virtualizes keyed message rows, opens at the end and follows new
messages while the reader is there. Typing, errors and the empty-conversation
prompt share a final collection row. The source still returns the complete
message array; virtualization bounds mounted views, not source answers or storage.

Validation: `bun test ./apps/messages/app.test.ts` covers persistence, grouping,
context isolation, cumulative streaming, retry, and cancellation. App drives have
exercised browser and simulator layouts and real Gemini/GPT responses. Exact's
`scripts/agent.mjs --app messages` targets this app through the standard driver.
After building the web and headless Linux hosts,
`bun test ./apps/messages/transcript.test.mjs` drives local conversations through
scrolling, eviction, appends, replies, deletion, typing, streaming and errors:
12,000 messages on the web and 1,000 on headless Linux, where the full snapshot
of 12,000 messages exceeds the source's 100 ms turn budget.
Latest, composer focus and sending use keyed scroll requests, independent of
the transcript's estimated height. Each drive uses its own service port.

Fidelity limits: this is a source-measured port, not a completed pixel-diff
certification against the running Expo app. Wrapped balloons currently use CSS
shrink-to-fit rather than Expo's custom longest-line fitting. The Add card and
message menu use Exact glass; the Add card does not reproduce UIKit's keyboard-
overlapping popover. Send uses a uniform squash rather than independently deforming
width and height, and tail changes are immediate. The reference's 100,000-row
performance is not claimed. Native keyboard and physical-device gesture parity
need a direct reference recording.
