# A replaced socket directory leaves the listener on the old watch

**Status:** Open
**Systems:** host/listen
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

The listener opens a directory watch once, the first time it binds. Replacing or renaming the socket's directory leaves that watch on the old inode. The mask includes `IN_DELETE_SELF` and omits `IN_MOVE_SELF`. After the old directory is gone, one `look` may run and then the descriptor never becomes readable again. A later takeover stays Holding. A failed republish stays Displaced after the path is free.

Deleting the socket file still works, because that watch stays on a live directory.

## Where

`host/listen/src/lib.rs`, `bind` (the watch is created only when `watch` is none) and the poll arm that calls `look`. `host/listen/src/watch.rs`, the inotify mask and `drain`.

## How to reproduce

Start a listener, rename its socket directory away, and create a new directory at the same path with another process publishing the socket. The first listener does not displace, and it does not return to holding when the path is free again.

## Direction

On `IN_IGNORED`, `IN_MOVE_SELF`, `NOTE_DELETE`, or `NOTE_RENAME`, close the watch and open one on the path's current parent. While displaced with no live watch, poll the path until it can bind.
