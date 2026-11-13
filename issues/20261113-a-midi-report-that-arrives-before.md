# A MIDI report that arrives before the grant is dropped

**Status:** Open
**Systems:** host/web-js, host/web
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

A Rust page that does not wait for its module calls `midi()` before `data.grants` is set. `Granted` stays false, and `setPorts` drops every host report. On a browser with no Web MIDI, `midi-glue.js` delivers `unavailable` as soon as it installs. On one where permission is already granted, the silent `requestMIDIAccess` delivers the ports once. Either report can land in the frames before `rust-data.js` finishes.

`data.ready` then sets `Granted` and replaces the ports with `prompt` and empty lists. It does not ask the glue to report again. A later `requestMIDIAccess` calls `request`, which does nothing once the glue's state is `unavailable`, and does not call `report` again once access is already `granted`. The plan stays at `prompt`. A native host learns `unavailable`, or the granted ports, when the grant is known.

## Where

`host/web-js/midi.js`, `setPorts`, `deliver`, and the `data.ready` arm in `midi`. `host/web/midi-glue.js`, the install-time `unavailable` delivery, `take` and `report`, and `request`. `host/web-js/build.mjs`, the non-waiting Rust boot that loads `rust-data.js` after the first frame. `host/web-js/rust-data.js`, where `data.grants` is assigned and `data.q` runs.

## How to reproduce

Build a Rust app that uses MIDI and does not wait on the module, and open it in a browser with no `requestMIDIAccess` (WebKit). After the module loads, `exactMIDI().state` stays `prompt`. Calling `requestMIDIAccess` does not move it. The same plan on a native host is `unavailable`.

## Direction

Keep the last port report received while ungranted and apply it when the grant arrives, or have the glue report its current state again once `Granted` becomes true.
