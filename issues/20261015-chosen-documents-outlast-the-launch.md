# A document app needs chosen files to outlast the launch and to hear when they change

**Status:** Open
**Systems:** storage (`doc:` paths, `storage.fs`), Apple host (security-scoped URLs, Open Recent), web host (File System Access), Linux host, `rules/DEFERRED.md` ("durable capability grants")
**Severity:** P2
**Author:** Claude (Opus 5.5), from Rami Maalouf's Exact Draw report
**Date:** 2026-10-15
**Related:** https://github.com/rami-maalouf/exact-draw/blob/main/docs/exact-experience.md (sections 001 and 006); LLP 1069.010 D1 ("Rejected, for now: a handle that outlives the launch"), D5 (Open Recent), Q2 (bookmarks kept expressible); `docs/reference.md`, "Documents the person chose"; `issues/20261009-macos-orderly-sigterm-quit-hold.md` (stopping a bundled helper)

Status note (moved verbatim off the **Status:** line by `bun scripts/issue.mjs`; cdcstack issue statuses are exactly `Open` or `Closed`): LLP 1069.010.000 accepted 2026-10-15. Stage 1 (watches and expected writes) built 2026-10-16 on macOS, Linux and the web; stage 2 (documents kept across launches) built on macOS 2026-10-18 (the LLP's "As built: stage 2 on macOS"); what remains is below

## Status (2026-10-26)

Stage 2 is built on Linux and the web as well (the LLP's "As built: stage 2
on Linux and the web"), and TypeScript's `atomicWriteFile` carries a kept
file. What remains in this repository is under "What remains (2026-10-26)".
Exact Draw's row is work in Exact Draw's own repository, not this one.

## Status (2026-11-13)

D4's hard rows have an editor and a fixture. `scripts/fixtures/kept-editor`
is a Markdown editor (`textarea markup="markdown"`) over a document chosen
under `doc.keep`: Save is TypeScript's `atomicWriteFile`, expecting the file
the draft began from (D2a), and the document is watched (D1).
`kept.drives.mjs` drives it; where the file is on this machine's disk,
another program replaces it (a new file renamed over the path) mid-session.

- **Linux**: the host built for macOS showed the editor's atomic save landing
  and reading back at the same `doc:` path, then refused `ECHANGED` after
  another program replaced the file. On 2026-11-14 the same fixture ran on
  Linux 6.8 from an ext4 guest filesystem: inotify made the replacement appear
  beneath the draft, the refused save kept the draft, and the other program's
  file stayed on disk. `cargo test -p exact-painted --lib doc_watch` passed
  there as well.
- **The web**: the editor's atomic save, through the agent, which hands the
  page in-memory handles that no other program can reach. Over real handles
  in Chrome (`host/web/documents-kept.test.mjs`): a kept file saved by the
  app's atomic replace reads the save after a reload; another writer's
  replacement at its name fires the `FileSystemObserver`, the browser's
  handle reads the new file, and a save expecting the earlier read is
  refused `ECHANGED`.
- **macOS**: `kept.drives.mjs macos` has the relaunch legs (the kept path
  reads the editor's save after a relaunch; replaced by another program, it
  is forgotten at the next launch) and the watch. The fixture builds for
  macOS, but the drive was not run: the screen was locked.

## Status (2026-11-14, macOS selected-file watch)

Main's shipped FSEvents watch retains its recorded claim that watching the
parent folder survives an atomic replace at a selected path. No measurement on
this base contradicts it. Branch `fs/doc-watch` attempted a retained descriptor
watch on `e7fe8fa07`, but it does not ship. Its E1 experiment observed directory
`NOTE_WRITE` for rename-over and no vnode event on the replaced leaf; E2 observed
three to five FSEvents deliveries in five trials per path-spelling cell on macOS
27.0.1. Its last review also found a FIFO that could block shutdown, two
descriptor-bound coordination races, a leaf that never re-armed after `EACCES`,
a stale stamp across an unobserved removal and re-addition, and a write that
could be announced twice. LLP 1069.010.000 retains that attempt's record; none
of its descriptor-watch code ships.

## Status (2026-11-14, Linux picker review)

`Linux document pickers use the app's declared file types, and a declared
suffix means the same on every host` fixes the portable suffix rule and sends
the declared filters to Linux. This issue remains open for the real GNOME and
KDE checks and the other work under "What remains (2026-10-26)".

The JavaScript `readManifest` and Rust `Manifest::read` readers now refuse an
invalid document extension or handler label for every build. The File System
Access suffix grammar is the rule: a leading dot is optional in `app.json`, then
both readers add it, lowercase the suffix, and give that canonical value to
every consumer. Their tests share `scripts/fixtures/document-suffixes.json`.
Compound suffixes such as `.tar.gz` remain valid. The JS target writes the
canonical web manifest; the web picker and whole-filename drop matching consume
it. Apple's metadata, Contract's picker bound and Linux compatibility metadata
come from a canonical manifest as well.

Linux sends extension-bearing handlers as bracketed, case-paired glob rules
only, so an app-owned type does not depend on the desktop MIME database. A
handler with MIME types and no extension sends MIME rules only. Compatibility
metadata that is not JSON is refused with its parse cause in the journal; valid
metadata with no `fileHandlers` remains the old unfiltered case. GTK's and KDE's
current sources support this shape, but no real GNOME or KDE session has run it.
Both Windows cross-target checks compile the portal module; the wire conversion
is Unix-only. The portal parser continues to refuse a non-canonical suffix in
embedded metadata rather than correcting it. Directory parses no handlers,
because its request sends no filters, so unusable filter metadata cannot refuse
a directory pick.

## The consumer

Exact Draw edits Excalidraw drawings on disk: a Contract shell (file explorer,
recents, search) around an embedded Excalidraw editor, on the web and macOS.
It needs three things from the files a person picks:

1. Reopen recent files and folders after a relaunch, from the app's own sidebar.
2. Learn that the open drawing, or a file in the open folder, changed on disk
   (another editor, a sync client, git) and show the change without reloading.
3. List a large folder tree without stalling the interface.

It met none of them in Exact and runs a Bun companion process instead: an
unsandboxed loopback service with absolute paths, a worker for the traversal,
and its own watcher, which both the shell and the editor call. Nothing starts
that service with the app, so the app still needs a terminal to launch.

## What exists (main `11ebf84e9`)

- **Within a session:** a picked file or folder is a `doc:/<n>/<name>` path;
  `readFile`, `stat`, `readdir` and writes beneath a chosen folder work under
  `fs.read doc:/` and `fs.write doc:/` on every host. The handles end with the
  session.
- **Need 3** is reachable already: `readdir` and `stat` walk a chosen folder,
  and a module placed `worker` (LLP 1027.002) walks it off the main thread.
  Not measured on a large tree.
- **Need 1, in part:** on macOS the host records every delivered document with
  `noteNewRecentDocumentURL`, and File ▸ Open Recent reopens one across launches
  through `open-file` (LLP 1069.010 D5). The app cannot list those entries or
  open one from its own interface, and the web and iOS have no equivalent.
- **Need 2:** nothing. No `storage.fs` call reports a change to a path.
- **The companion:** `host.macos.resources` bundles a helper tree into the
  `.app`; nothing launches or supervises it. A native module can, and the
  SIGTERM issue above covers stopping it.

## Ruling

LLP 1069.010.000 was accepted on 2026-10-15 (its Rulings section). On 2026-10-18 Charlie ruled that the in-repo Markdown and LLP readers are stage 1's user, as the LLP's own §2, §5 and §8 name them, so stage 2 starts now on macOS (the bookmark, `<n>` persistence, `recentDocuments()`, `forgetDocument()`, `doc.keep`), then Linux and the web; iOS stays out. D4's hard rows (another program replacing the file at its path, a move) need the Markdown editor and a fixture, not the reader alone.

## What remains (2026-10-18)

Stage 2 is built on macOS: `doc.keep`, the kept set and its `<n>`, the
security-scoped bookmarks, Open Recent as the list, `recentDocuments()`
and `forgetDocument()` from Rust and TypeScript, and the Markdown reader's
Recent list. Left:

- **Linux** (D4, D6): the freedesktop recent-files store as the list, the
  path with its device and inode as the handle, nothing kept outside a
  desktop session. The library's kept set (`exact_data::documents::kept`)
  is the host-independent half.
- **The web** (D4, D6, D7): `FileSystemHandle`s in IndexedDB,
  `requestPermission(id, path, mode)` under a press, the two calls in the
  glue.
- **D4's hard rows by an editor**: the Markdown editor saving a kept file
  with its own atomic replace from TypeScript, and a fixture where another
  program replaces it, on each host.
- **Exact Draw** (§7): reopen its last folder with its companion's file
  service stopped.

## What remains (2026-10-26)

- **D4's hard rows by an editor**: built and driven on Linux and the web
  (Status, 2026-11-13); left: `kept.drives.mjs macos` run with the screen
  unlocked. The watch's re-ask on real Linux passed on 2026-11-14.
- **The Linux file chooser on a desktop**: `showOpenFilePicker`,
  `showDirectoryPicker` and `showSaveFilePicker` ask the desktop portal's
  `FileChooser` and keep what it answers (`host/painted/src/portal.rs`), tested
  against a stand-in session bus only. The Linux binary now carries the
  manifest's `file_handlers`; Open and Save send extension globs, or MIME rules
  when a handler declares no extension, and select the first filter, while
  Directory sends none. A binary from before the handler metadata opens
  unfiltered. The compatibility identity changes on Linux only; an empty handler
  list adds no metadata. Left, on both a real GNOME portal and a real KDE portal:
  make Open, Directory and Save picks; confirm the declared filters appear for
  Open and Save; confirm an app-owned `drawing.board`, `Notes.MD` under a `.md`
  filter, and a compound suffix are listed; relaunch and confirm a kept document
  survives; revoke it and confirm the next relaunch forgets it.
- **Exact Draw** (§7), in its own repository.
