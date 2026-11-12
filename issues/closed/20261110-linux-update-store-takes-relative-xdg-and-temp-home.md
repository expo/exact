# The Linux update store takes a relative `XDG_DATA_HOME`, and the temporary directory when `HOME` is unset

**Status:** Closed
**Resolution:** The Linux update store takes its base from `exact_linux::picker::xdg_base`, which the app's own directories and the recent-files list now share: an absolute `XDG_DATA_HOME`, else `$HOME/.local/share` with `HOME` absolute, else no store unless `EXACT_UPDATE_DIR` or the agent names one.
**Systems:** host/linux-update/src/store.rs, update/src/client.rs
**Severity:** P3
**Author:** Claude (Opus 5.5), from Grok 4.7's whole-project review (2026-11-10)
**Date:** 2026-11-10

Line references are at `46f3d608`.

## What is wrong

The Linux host places its update store at `data_dir()` (`host/linux-update/src/store.rs:54-62`)
joined with `exact/<app id>/update` (`update/src/client.rs:354-365`). `data_dir` takes any
non-empty `XDG_DATA_HOME`, and with neither that nor `HOME` set it uses
`std::env::temp_dir()`:

- `XDG_DATA_HOME=updates` puts the store at `./updates/exact/<app id>/update`, relative to
  wherever the app was launched from, so each working directory has its own store. The XDG
  Base Directory specification: "If an implementation encounters a relative path in any of
  these variables it should consider the path invalid and ignore it"
  (<https://specifications.freedesktop.org/basedir-spec/latest/>).
- With `HOME` unset, the store is `/tmp/.local/share/exact/<app id>/update` (`/tmp` unless
  `TMPDIR` says otherwise), under a directory every local user can write to, so another
  user can create `/tmp/.local` first.

The host's other directories follow the specification: the app's roots
(`host/linux/src/picker.rs:86-98`), the recent-files list (`host/linux/src/kept.rs:155-160`)
and the image spool (`host/linux/src/image/remote.rs:724-730`) ignore a relative `XDG_*`
value and fall back to `$HOME`, and the app's roots refuse storage without an absolute
`HOME`. So one environment puts the app's files and its updates in different trees, and in
the second case the app has no storage while its update store is under `/tmp`. The store's
contents are still checked against their signatures; what is wrong is where it lives.

## To see it

Confirmed by reading code only. Launch a Linux release build that links a store with
`XDG_DATA_HOME=updates` from two directories: each gets its own
`updates/exact/<app id>/update/owner.lock` beside where it was launched.

## What a fix is

`data_dir` should read the base as `picker::app_dirs` does: an absolute `XDG_DATA_HOME`,
else `$HOME/.local/share` with `HOME` absolute, else no store (`Updates::open`,
`store.rs:75`, already returns a `Result` and can refuse). Better, the update store takes its directory from the
one function that resolves the app's directories, so the two cannot drift again.
`EXACT_UPDATE_DIR` stays the explicit override.
