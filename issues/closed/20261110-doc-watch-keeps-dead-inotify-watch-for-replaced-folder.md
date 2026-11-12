# A watched folder removed and made again before the next commit is never watched again on Linux

**Status:** Closed
**Resolution:** The inotify reader forgets a watch descriptor when the kernel reports IN_IGNORED, so the next follow watches a folder made again at that path.
**Systems:** host/linux/src/doc_watch.rs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

## What is wrong

Line references are at commit `dd6f5dcd`.

`watch_exactly` (`host/linux/src/doc_watch.rs:212-242`) decides by path which
folders need an inotify watch. It keeps every entry of `shared.folders` (watch
descriptor to path) whose path is still wanted, and adds a watch only for the
wanted paths the map lacks (line 233).

The kernel removes a watch itself when the watched directory is deleted, and
says so with `IN_IGNORED`. inotify(7): "Watch was removed explicitly
(inotify_rm_watch(2)) or automatically (file was deleted, or filesystem was
unmounted)". `read_events` (`doc_watch.rs:259-313`) never reads `event.mask`:
it announces the folder's path and leaves the dead descriptor in the map. If
a directory exists at that path again when `follow` next runs, the path is
wanted and already in the map, so the new directory gets no watch, then or on
any later `follow`.

## The failure

An answer watches an opened folder recursively
(`storage.fs.watch(folder, { recursive: true })`), and another program
replaces a subfolder between two of the app's commits, as
`rm -rf sub && mkdir sub`, a branch checkout or a clean build does:

1. `follow` watches `folder` and `folder/sub`.
2. `sub` is removed and made again. The kernel drops `sub`'s watch; the
   parent's watch announces.
3. The commit that follows calls `follow`. `beneath` lists the new `sub`, the
   map still names that path, and no watch is added.
4. A file written into `sub` from then on announces nothing, until the topic
   leaves and returns or the app restarts.

Expected: the answer is asked again for every change beneath the folder
(LLP 1069.010.000 D1).

A `follow` that runs while `sub` is absent drops the entry, and the remade
folder is then watched: the loss needs the removal and the creation to land
between two commits. A watch that is not recursive, on a folder inside an
opened folder or a file in one, keeps its folder in the same map and is lost
the same way.

## Verification

By reading, not run: inotify is Linux only, and this review ran on macOS with
no Linux machine or container runtime. The existing test
`inotify_announces_a_change_in_a_watched_folder` (`doc_watch.rs:328`) has the
shape for a test of it: `follow` a recursive watch, remove and remake a
subfolder, `follow` again, write a file into the subfolder and wait for the
topic.

## Direction

In `read_events`, remove the descriptor from `shared.folders` when the event's
mask has `IN_IGNORED`, so the next `follow` adds the path again.

## Related

"A recursive document watch walks its whole folder tree on the UI thread after
every commit" changes when `follow` walks. This loss stands whichever way the
walk is scheduled.
