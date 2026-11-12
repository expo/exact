# The terminal host reads an `image` `src` as a filesystem path, whole, on its commit path

**Status:** Closed
**Resolution:** The terminal reads an `image` source on a thread of its own, as a regular file of at most 64 MiB opened without blocking, a `data:` URL, or a path beneath the entry's directory; it refuses `http(s)` and `app:/` by name (QUEUE.md has what they wait on) and sends `load` or `error` once per source.
**Systems:** host/terminal (`image.rs`, `host.rs`)
**Severity:** P3
**Author:** Claude (Opus 5.5), from Grok 4.7's whole-project review (2026-11-10)
**Date:** 2026-11-10

Line references are at `46f3d608`.

## What is wrong

`Images::load` (`host/terminal/src/image.rs:147`) strips a `file://` prefix from each
`image`'s `src` and hands the rest to `decode` (`:164`), which reads it with
`std::fs::read` (`:68`). There is no other resolution:

- `app:/data/photo.png` is read as a relative path named `app:/data/photo.png` and draws
  nothing. LLP 1101 D1 says a terminal entry shares the app's "app-scoped files, under the
  same app id"; nothing in `host/terminal/src` names `app:/` or configures storage.
- A relative `src` such as `assets/logo.png` is read against the process's working
  directory, not the app's directory, as the Linux host's `image_input` reads it
  (`host/linux/src/image/assets.rs:94-105`).
- `https://…` is read as a file and draws nothing. The guide says the native hosts download
  an `http(s)` source and retry it (`docs/contract-for-agents.md:1952`).
- Neither `load=` nor `error=` fires, though the guide says an `image` hears both "on every
  host" (`docs/contract-for-agents.md:1946`).
- The read has no size bound and runs on the host's thread at each commit
  (`host/terminal/src/host.rs:485`). A `src` naming `/dev/zero` grows the buffer until the
  process runs out of memory, and one naming a FIFO blocks the app until something writes
  to it.

The harness's `/image <path>` relies on the path behaviour: it passes an absolute path the
person typed and checks `is_file()` first (`apps/harness/data/src/commands.rs:151-187`).
The review this came from also called the read a disclosure, since a readable PNG is sent to
the terminal as base64 (`image.rs:181`). It is not one: the process already runs as the
person at that terminal, and the bytes go to that person's screen.

## To see it

Confirmed by reading code only. A terminal entry with
`image "https://example.com/a.png" width=10ch height=5lh` draws an empty box and fires no
`error`; the same entry on Linux or macOS draws the picture.

## What a fix is

Resolve `src` as the Linux host does: `app:/` through the app's roots (which the terminal
host must then configure), `data:` to its bound, a relative path under the app's asset
root, and refuse anything else with `error`. Keep a plain absolute path only if the harness
still needs it, and only for a regular file. Read with a cap (Apple takes a downloaded body
up to 64 MiB, `docs/contract-for-agents.md:1958`), off the commit path. Whether the
terminal downloads `http(s)` images is a decision for LLP 1101; until it does, fire `error`
for one and say so in the guide.
