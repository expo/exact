# Web serial forget drops an open connection without closing the port

**Status:** Open
**Systems:** packages/serial
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`forget` sets `open` false, deletes the connection, then calls `port.forget()` without `reader.cancel()` or `port.close()`. Web Serial rejects `forget` while the port is open, and `close` rejects while `readable` is locked by the reader.

`close` cancels the reader, drains the writer, and closes the port. Apple, Linux, and Windows `forget` close every connection on that port first. The web test forgets only a port that `close` already released.

## Where

`packages/serial/web/index.js`, `forget` and `close`.

## How to reproduce

Open a serial port and call `forget` without `close`. `port.forget()` rejects. The connection is already deleted, and the browser port stays open.

## Direction

For each connection on the port, cancel the reader, drain the writer, and `port.close()` before deleting the handle. Call `port.forget()` only after the port is closed.
