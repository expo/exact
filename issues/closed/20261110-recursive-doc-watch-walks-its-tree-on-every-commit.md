# A recursive document watch walks its whole folder tree on the UI thread after every commit

**Status:** Closed
**Resolution:** A follow returns at once when its topics, grants and store folder are unchanged and the inotify thread has seen no folder made, moved or removed (nor a watch dropped), so an unchanged watch costs a commit nothing, and the walk runs only on Linux.
**Systems:** host/linux/src/doc_watch.rs, host/linux/src/host.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`Host::commit` ends every commit with
`crate::doc_watch::sync(&mut self.doc_watch, &mut self.runner)`
(`host/linux/src/host.rs:1295`). `DocWatch::sync` calls `follow`
(`host/linux/src/doc_watch.rs:80-86`, `104`), which rebuilds the watched
folder set from nothing each time: it parses the app's grants, resolves every
topic, and for each recursive watch calls `beneath` (`doc_watch.rs:122-124`,
`161-177`), a breadth-first `read_dir` of every folder under the watched one,
up to `MAX_FOLDERS` (4,096). Nothing compares the topics with the previous
call's, so an unchanged watch pays the whole walk again on every tap,
keystroke and network reply, on the thread that paints.

Only Linux uses the result. Built for any other system (the headless macOS
build, Android, Windows) this host walks and discards it
(`doc_watch.rs:143-144`). The Apple host's twin compares the folders with the
last set and returns (`host/apple/src/doc_watch.rs:112-114`).

## The failure

An answer that calls `await storage.fs.watch(folder, { recursive: true })`
(`docs/reference.md:2196`) on a folder with subfolders. One commit of a
field's `change`, by the size of the watched tree:

```
folders  files   not recursive  recursive
     7     20      0.012 ms       0.11 ms
    50    160      0.010 ms       0.66 ms
   122    400      0.010 ms       1.9 ms
   610  2,000      0.010 ms      14.8 ms
 2,420  8,000      0.010 ms      52.6 ms
```

A commit costs a millisecond more at about 70 folders and a 60 Hz frame at
about 700.

Expected: a commit that changes no watch costs nothing here; the folder set
changes only when the topics do or when a watched folder reports a directory
created, moved or removed.

No app or package in the repository takes a recursive watch: the Markdown
reader's two are not recursive (`apps/markdown/data/src/lib.rs:462-464`), and
the only recursive callers are tests (`js/tests/fixtures/storage.ts:178`).
The option is documented for every app.

## Verification

A test in the `host-dev` profile on macOS (APFS), on a machine busy with other
builds. It boots a presenter whose resource's answer records the watch topic
(`store.observe_topic(&documents::watch_topic(&doc, recursive))`, grants
`fs.read doc:/`) over a tree with four files in each leaf folder, then times
sixty `Host::dispatch_at` commits of a field's `change`. The table is the
medians. The test profile gave similar recursive figures (61 ms at 2,420
folders): the time is directory reads. Not run: Linux, where the same walk
runs and then feeds `watch_exactly`.

## Direction

Keep the last topics and return when they are unchanged, as the Apple host
keeps its folders. Walk a recursive watch's tree when its topic first appears,
and after that add or drop a subfolder's watch from the inotify thread's own
`IN_CREATE`, `IN_MOVED_TO`, `IN_DELETE` and `IN_IGNORED` events.

## Related

QUEUE "Document watches still owed after LLP 1069.010.000 stage 1" lists the
hosts with no watch at all, not this cost. "A watched folder removed and made
again before the next commit is never watched again on Linux" is in the same
code and wants `IN_IGNORED` handled too.
