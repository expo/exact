# A failed old microphone or camera reopen ends the newer live capture

**Status:** Closed
**Resolution:** A failed microphone or camera reopen ends its track only while its lease is still the current one, as a successful reopen already adopts only then.
**Systems:** packages/media-capture/web/index.js, packages/media-capture/web/video.js
**Author:** Codex
**Date:** 2026-11-12
**Severity:** P2

At audited commit `f6be99af`, the audio and video `activate` functions assign each capture open a lease number and check that number before adopting a successful browser track. Their rejection handlers do not check it: `packages/media-capture/web/index.js:92-95` and `packages/media-capture/web/video.js:121-124` unconditionally clear `active` and end the track.

A consumer can leave while its device reopen is pending, then another consumer can take a new lease. If the newer open succeeds before the old one rejects, the old rejection ends the new capture. `end` stops its browser source, clears its consumers, and permanently sets `readyState` to `ended`. A new recording or viewfinder then stops even though its own device open succeeded. Hiding and showing a camera view can produce the same lease replacement.

Reproduced for audio and video by importing the actual media-capture implementation with a controlled `navigator.mediaDevices.getUserMedia` and a track-table adapter. No devices were opened:

1. Open a track, subscribe, and unsubscribe to make it idle.
2. Subscribe to start reopen A, then unsubscribe while A is pending.
3. Subscribe again to start reopen B, and resolve B with a live browser-track stand-in.
4. Reject A with `NotReadableError`.

For each kind, the module changed from `readyState: "live"` to `"ended"`, stopped B's source, and sent the new consumer one `ended` callback. Probe: `/tmp/exact-review-20261112/media-reopen-failure-probe.mjs`, run with Bun 1.4.2. This verifies completion ordering in the actual implementation; physical-device error timing was not tested.

Check the captured lease number and current state before handling a rejected reopen, as on success. Validate both completion orders for old success/failure versus new success/failure, and that a rejected current open still ends its own track.

Checked all open and closed filesystem issues and the full bodies of all 275 cached GitHub issues. `issues/closed/20261110-web-capture-reopen-race-leaves-device-open.md` fixed the successful superseded-open path, which leaked a source. This is the remaining rejection path, with the distinct consequence of ending the current capture. `issues/closed/20261110-web-camera-move-while-idle-is-lost.md` concerns camera selection, not obsolete lease completions.
