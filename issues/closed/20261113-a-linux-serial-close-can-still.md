# A Linux serial close can still read into a finished stream

**Status:** Closed
**Resolution:** The reader reads with its stream's lock held and stops at a flag close sets under that lock, so no byte is read into a closed stream and later bytes stay in the port for its next reader.
**Systems:** packages/serial
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`close` stores `stop`, sets `open` false, clears exclusive mode, drops its own file, and returns. The reader thread still holds the descriptor. After the backpressure wait it does not check `stop` again before `poll` and `read`, so up to 4096 bytes can still be appended.

`serialInput` sets `ended` when the connection is no longer open. A later read of that stream is `InvalidStateError`, and those bytes are never delivered. A new open of the same port in that window shares the input queue, because the old reader still has the descriptor.

## Where

`packages/serial/linux/lib.rs`, `read_loop` and `close`.

## How to reproduce

Close a serial connection while the device is transmitting. Open it again within the reader's poll. Bytes that arrived after the close are missing from the first stream, and the new connection can observe them.

## Direction

Check `stop` immediately before `poll` and `read`, and do not append once the connection is closed. Wake the reader by closing its descriptor from `close`.
