# Android Canvas JNI stream dedup forgets to clear `LAST` on move-only frames and `Native_moved`, stranding `RenderNode` groups at stale scroll offsets

**Status:** Closed
**Resolution:** Extract StreamDedup and clear both last and drawn on SHIFT (op 22) frames and Native_moved so the next full paint always delivers and resets translated RenderNode groups.
**Systems:** host/painted (Android Canvas JNI rendering pipeline)
**Author:** Gemini 4 Neon
**Date:** 2026-11-25
**Severity:** P2
**Related:** LLP 1184 (Copy-paste drift)

Verification: verified by reading.

At `9c0431fd5`, `Java_dev_exact_canvas_Native_frame` in `host/painted/src/canvas/jni.rs:362-390` and `Java_dev_exact_canvas_Native_moved` in `host/painted/src/canvas/jni_scroll.rs:26-41` clear `DRAWN` after a move-only frame or reader-handled scroll step, but neither clears `LAST`:

- `host/painted/src/canvas/jni.rs:362-369`: when `ops.first() == Some(&22)` (`SHIFT`), only `DRAWN` is set to `None` before returning `MOVE`; `LAST` keeps the pre-move full stream.
- `host/painted/src/canvas/jni_scroll.rs:33-40`: when `h.moved()` returns `true`, the comment states `"As after a move's stream: the next paint is drawn, however like the last."`, yet line 37 clears only `DRAWN`, leaving `LAST` intact.
- `host/painted/src/canvas/jni.rs:370-375`: on the next full paint, `if *last == ops { return std::ptr::null_mut(); }` runs **before** inspecting `DRAWN` (lines 379-387).

### Triggering sequence
In any Android app with a scrollable container whose rows are recorded in a group `g`:
1. Settle a full paint at scroll offset `y = 0` (or `y = 50`) whose rows are already defined in `Recorder.kept`, so `ops` contains only drawing commands with no `ROW_BEGIN`/`CLOCK`/`TRACKS` definitions (`host/painted/src/canvas.rs:1298-1306`, `host/painted/src/canvas/tracks.rs:30-31, 46-50`). `LAST` stores this stream and `ExactView.applyStream` (`host/android/canvas/src/main/java/dev/exact/canvas/ExactView.kt:987-990`) sets `nativeOrigins.translate(g, 0f, 0f)` and `feedBaseY = 0f`.
2. Scroll `g` to `y = 100` via a path that emits `op 22` (`SHIFT`) — e.g., programmatic `scrollTop = 100`, wheel scroll, TalkBack scroll, or driver `scrollBy` (where `s.own == false` in `Scrolling.kt:634`). `Native_frame` returns `MOVE` without clearing `LAST`; `ExactView.applyMove` (`ExactView.kt:564-565, 966-970`) translates `groups[g]` to `-100px` and sets `feedBaseY = -100px`.
3. Scroll back to `y = 0` on a turn that forces a full paint rather than `shift()` — e.g., tapping a "Back to top" button or pressing `Home`/`PageUp` (`host/painted/src/canvas/input.rs:65` and `host/painted/src/canvas/keys.rs:33` set `self.force = true`), or reaching `self.moved >= limit` (`MOVES = 30` or `MOVES_MOUNTED = 6`, `canvas.rs:1355-1365`). `CanvasHost::frame_held()` paints `y = 0`, updates `CanvasHost.painted` to `y = 0`, clears `self.dirty` and `self.moved`, and returns `ops` byte-for-byte equal to `*LAST`.

Similarly, after `Native_moved` (`Owner.kt:451` -> `jni_scroll.rs:33-40`), returning to the previously painted offset on an `owed_at` full paint drops the frame via `*last == ops` while `reader_scrollers` returns `None` (`presenter/scrollers.rs:741-743`), so `Owner.kt:606` queues no `Pub` at all (skipping `scrolling.acked(seq)` and `presents.ingested(picture)`).

### Actual vs. expected
- **Actual:** `Native_frame` hits `if *last == ops` at `jni.rs:373` and returns `null`. Android never calls `ExactView.applyStream` (`ExactView.kt:987-990`), so `nativeOrigins.translate(g, 0f, 0f)` never runs and `feedBaseY` is never reset to `0f` (`ExactView.kt:552`). For any scroller with `s.own == false` (skipped by `Scrolling.place()` at `Scrolling.kt:634`), the `RenderNode` group stays permanently stranded at the stale `-100px` translation while `CanvasHost.painted` and hit-testing are at `y = 0`.
- **Expected:** As stated in `host/painted/src/canvas/jni_scroll.rs:35-36` (`"As after a move's stream: the next paint is drawn, however like the last"`) and required by `ExactView.kt:551-566, 987-990`, the first full paint after any move-only stream or `Native_moved` must be delivered to Android regardless of equality with the pre-move stream.

### Verification and siblings
Verified by reading `host/painted/src/canvas/jni.rs`, `jni_scroll.rs`, `canvas.rs`, `canvas/tracks.rs`, `canvas/stream.rs`, `presenter/scrollers.rs`, and Android `ExactView.kt`, `Owner.kt`, `Scrolling.kt`.
Siblings checked:
- `adopt_running` (`host/painted/src/canvas/jni.rs:161-163`) already clears both `LAST` and `DRAWN`.
- No other host (`host/linux`, `host/windows`, `host/apple`, `host/web`) has JNI op-stream `LAST`/`DRAWN`/`SHIFT` deduplication.

### Suggested fix and regression test
Clear `LAST` alongside `DRAWN` on both move paths (Android copies the `DirectByteBuffer` into a Kotlin `IntArray` immediately in `Owner.kt:613-615` and `ExactView.kt:937-939`, so `LAST` is no longer borrowed):
- In `host/painted/src/canvas/jni.rs:362-364`, add `LAST.with(|l| l.borrow_mut().clear());` beside clearing `DRAWN`.
- In `host/painted/src/canvas/jni_scroll.rs:37`, add `LAST.with(|l| l.borrow_mut().clear());` beside clearing `DRAWN`.

Add a Rust unit test exercising the stream-dedup helper (extracting the `LAST`/`DRAWN` dedup logic from the JNI macro into a testable function in `host/painted/src/canvas/jni.rs`) that submits a full stream `S`, then a `SHIFT` (`op 22`) frame or `moved()` reset, then `S` again, asserting `S` is emitted rather than suppressed.

### Siblings checked

adopt_running (host/painted/src/canvas/jni.rs:161-163, clears both LAST and DRAWN); checked all other hosts (host/linux, host/windows, host/apple, host/web) — only Android Canvas JNI has LAST/DRAWN/SHIFT stream deduplication.
