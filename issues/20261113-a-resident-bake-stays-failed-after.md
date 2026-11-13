# A resident bake stays failed after a removed source is already gone

**Status:** Open
**Systems:** js
**Severity:** P2
**Author:** Grok 4.7
**Date:** 2026-11-13

`bake_in` removes stage files that left the capture, writes bytes that changed, and only then stores the new map. `remove_file` and `write` use `?`. The resident producer reuses the stage and `previous`.

A save that deletes a captured file and then hits a later I/O error leaves `previous` naming a path the stage no longer has. The next request removes that path, gets `NotFound`, and every later request fails until the process restarts. Putting the file back with the same bytes does not rewrite it, because `previous` still matches, so the compiler keeps a missing source.

## Where

`js/bake/src/lib.rs`, `bake_in`. `js/bake/src/resident.rs`, the producer that keeps `previous`.

## How to reproduce

With the resident producer running, delete a captured source and fill the disk, or otherwise fail a later write in the same bake. The bake reports the error. The next request fails with `NotFound` on the path already removed. Restoring the source does not clear it.

## Direction

Treat `NotFound` on remove as already gone. Commit `previous` per path only after that path's disk update succeeds, or clear `previous` when `bake_in` returns an error so the next request recopies.
