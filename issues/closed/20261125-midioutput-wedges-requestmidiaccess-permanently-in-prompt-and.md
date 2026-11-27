# MIDIOutput wedges requestMIDIAccess() permanently in "prompt" and leaks open input subscriptions after session reload or plan replacement

**Status:** Closed
**Resolution:** Reset open input and clock subscriptions on MIDI boot and re-deliver cached or unavailable port state on subsequent request ops in both Apple MIDIModule and web midi-glue.js.
**Systems:** ExactMIDI / ExactKit (macOS, iOS); web wasm midi-glue.js sibling
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Out of order)

Verification: verified by reading.

At `9c0431fd5`, `MIDIOutput` (`host/apple/Sources/ExactMIDI/MIDIModule.swift`) is reused across session reloads and plan replacements without resetting its open-input state or re-delivering cached ports on a new runner's `request`:

1. **`ExactSession.midi` outlives runner replacements:**
   - `ExactSession.midi` is a `lazy var` (`host/apple/Sources/ExactKit/Session.swift:413`) whose `shutdown()` is called only in `ExactSession.destroy()` (`Session.swift:1493`), never in `presentCommitted` (`Session.swift:1026-1051`).
   - On every reload or live update (`ExactSession.apply`, `ExactApp.applyGeneration` / `commitTogether`), a fresh `Runner` is created with `Midi::new` (`runner/src/midi.rs:583-598`, `runner/src/runner/midi.rs:14-27`); `Carried` (`runner/src/runner/carry.rs:10-76`) carries no MIDI ports or open-input state. Thus the new runner always starts at `Access::Prompt` with `open = ([], [])`, emits `{"op":"midi","boot":true,"ops":[...]}` (`host/apple/src/host.rs:782`, `host/apple/src/batch.rs:895-898`), and emits `{"op":"request"}` when the app calls `requestMIDIAccess()` (`runner/src/midi.rs:511, 789`).
2. **`requestMIDIAccess()` wedges permanently in `"prompt"` after reload:**
   - In `MIDIOutput.apply(_:)` (`host/apple/Sources/ExactMIDI/MIDIModule.swift:270-278`):
     ```swift
     case "request":
         guard let library = library() else {
             ...
             deliver("{\"ports\":{\"state\":\"unavailable\"}}")
             continue
         }
         if hub == nil { hub = MIDIHub.shared(library); hub?.join(self) }
     ```
   - Because `self.hub` is already non-nil from before the reload, `case "request"` is a complete no-op. `hub.ports` is never re-delivered to the new runner, so `exactMIDI().state` remains `"prompt"` indefinitely (and all `sendMIDI` calls are dropped at `runner/src/midi.rs:821-828`) until a physical MIDI hot-plug happens to trigger a fresh `MIDIHub.reported` broadcast.
   - *(Note on `MIDIHub.init` / `join` at `MIDIModule.swift:93-107, 120-125`: in production `host/apple/midiarm/MIDIArm.swift:152-180, 392-395`, `exact_midi_open` always returns a non-nil pointer and reports `{"state":"unavailable",...}` asynchronously on CoreMIDI client creation failure; however, because `MIDILibrary.Open` and `MIDIHub.handle` are optional, a `library.open` returning `nil` also leaves `hub.ports == nil` and delivers nothing on `join`.)*
3. **Open input and clock subscriptions leak across reload:**
   - On `payload["boot"] as? Bool == true` (`MIDIModule.swift:263-266`), `MIDIOutput.apply` settles held sends (`ids`) and clears `ids`, but never clears `open` or `clock` or calls `hub?.listen()` (contrast `shutdown()` at `:250-256`).
   - Because the reloaded runner's `Midi.open` starts empty (`([], [])`), if the new plan mounts no `midiinputs` listeners, `Midi::set_open` (`runner/src/midi.rs:664-672`) sees no change and emits no `{"op":"open",...}` op. Previously opened CoreMIDI inputs stay connected in `MIDIHub` and keep delivering `{"messages":[...]}` to the reloaded runner.

### Trigger
Run any macOS or iOS app using MIDI (`requestMIDIAccess()`, and optionally a mounted element with `midiinputs=[...]`), then trigger a hot reload (`DevMenu.reload()` / `ExactSession.apply`) or live update (`ExactApp.applyGeneration`). In the reloaded generation, call `requestMIDIAccess()`.

### Actual vs. Expected
- **Actual:** `exactMIDI().state` stays `"prompt"` forever after reload (ignoring `requestMIDIAccess()`), and inputs opened before the reload stay open in `MIDIHub` / CoreMIDI and keep delivering messages to the reloaded runner.
- **Expected:** Per LLP 1125 D6–D8 (`llp/1125-a-transport-midi-and-the-small-pieces.rfc.md`) and `MIDIModule.swift:250-266`, a `boot: true` op resets per-runner MIDI subscriptions (`open`, `clock`, `hub?.listen()`), and `case "request"` reports current ports (or `"unavailable"` if `hub.handle == nil`) even when `hub` is already attached.

### Verification & Siblings Checked
- **Verified by reading** `host/apple/Sources/ExactMIDI/MIDIModule.swift`, `host/apple/midiarm/MIDIArm.swift`, `host/apple/Sources/ExactKit/Session.swift`, `host/apple/src/host.rs`, `host/apple/src/batch.rs`, `runner/src/midi.rs`, `runner/src/runner/midi.rs`, `runner/src/runner/carry.rs`, and `host/apple/tests/ExactKitTests/MIDIMacTests.swift`.
- **Siblings checked:**
  - **Web wasm sibling HAS THE SAME TWO DEFECTS:** `host/web/wasm/commit.js:1138-1155` reuses `host.midiOut` across in-place wasm reloads, and in `host/web/midi-glue.js`: (a) `end()` (`:116-125`, called on `boot`) settles `held` but never clears `open`/`clockIn` or calls `listen()`, leaking input listeners across reload; and (b) `handlers.request()` (`:142`) ignores `state === 'unavailable'` and otherwise calls `ask(false)` (`:55-64`), where `asking ??=` is already settled from before the reload so `report()` is never invoked again, wedging the reloaded wasm runner in `"prompt"`.
  - Web JS target (`host/web-js/midi.js`): reloads the full page on dev reload, so `midi-glue.js` starts fresh.
  - Apple `host/apple/Sources/ExactSound/SoundModule.swift:159-165`: resets prior voices/backlog on `boot` (`files` present).

### Suggested Fix & Regression Test
- In `MIDIOutput.apply(_:)` (`MIDIModule.swift:263-278`):
  - Inside `if payload["boot"] as? Bool == true`, also clear `open = []`, `clock = []`, and call `hub?.listen()`.
  - In `case "request"`, if `hub == nil`, create and join it; otherwise call `hub?.join(self)` (or re-deliver `hub.ports`), and in `MIDIHub.join(_:)` deliver `{"ports":{"state":"unavailable"}}` when `handle == nil && ports == nil`.
  - Apply the matching fix in `host/web/midi-glue.js` (`end()` clears `open`/`clockIn` and calls `listen()`; `handlers.request()` calls `report()` when `access != null` or delivers `unavailable` when `state === 'unavailable'`).
- Add tests in `host/apple/tests/ExactKitTests/MIDIMacTests.swift` (and `host/web/tests/midi-glue.test.mjs`) verifying that after `request` + `open` + `boot: true`: (1) previously opened inputs no longer deliver messages, and (2) a subsequent `request` immediately delivers the granted ports JSON again.

### Siblings checked

host/web/midi-glue.js:55-64,116-149 + host/web/wasm/commit.js:1138-1155 (HAS THE EXACT SAME TWO DEFECTS on in-place wasm reload: end() leaves open/clockIn listening and handlers.request() does not re-report ports once asking is settled); host/web-js/midi.js (reloads page on dev edit, not affected); host/apple/Sources/ExactSound/SoundModule.swift:159-165 (resets voices/backlog on boot, not affected)
