# Activation empties `picked` and `shared` through `app:/tmp` when that root is a symbolic link

**Status:** Closed
**Resolution:** Before emptying `picked` and `shared`, both hosts look for a symbolic link at `app:/tmp` or between it and the cache base, as `unlinked` does for a scratch tree, and when they find one they leave both directories and say so on standard error; a link swapped in between the check and the delete is still followed, as the issue's handle-relative delete would not.
**Systems:** host/linux/src/picker.rs (Linux and Windows), host/apple/src/picker.rs
**Severity:** P3
**Author:** Claude (Opus 5.5), from Grok 4.7's whole-project review (2026-11-10)
**Date:** 2026-11-10

Line references are at `46f3d608`.

## What is wrong

At each data activation `set_roots` runs `remove_dir_all(temporary.join("picked"))` and
`remove_dir_all(temporary.join("shared"))` (`host/linux/src/picker.rs:51-55`, called from
`host/linux/src/activation.rs:16`; `host/apple/src/picker.rs:183-189`, from
`host/apple/src/host.rs:766`). `temporary` is `<cache base>/exact/<app id>/temporary`
(`host/linux/src/picker.rs:111`, `host/apple/src/picker.rs:64`).

`remove_dir_all` does not follow a link at the path it is given, but the operating system
follows every link above it. When `temporary` is a symbolic link, the two calls delete
`picked` and `shared` in the directory it points at, recursively.

The hosts already treat such a root as invalid, after the deletion:

- Apple's `open_pinned` refuses "an app root is a symbolic link"
  (`host/apple/src/picker.rs:312-318`), and `set_roots` calls it through `pin` only after
  the two deletes.
- `empty_fresh_tree` checks every component with `unlinked` before it deletes an agent
  scratch tree (`host/linux/src/picker.rs:150-176`, `:178`), because "emptying it through
  one would delete whatever the link points at". `set_roots` makes no such check.

The link has to be put there by the person or by another process running as them, which
could delete the files itself, so this crosses no permission boundary. What it costs is a
person's own directories named `picked` or `shared` when they have linked the app's cache
somewhere, for example into their home or a project.

`issues/20261110-second-session-empties-the-first-ones-picked-files.md` is the same two
calls deleting at the wrong time; a fix to either touches both.

## To see it

The mechanism was run outside the checkout: with `t/temporary` a symbolic link to
`t/target`, and `t/target/picked/deep/f` present, a Rust program calling
`std::fs::remove_dir_all("t/temporary/picked")` returned `Ok(())` and `t/target/picked`
was gone. The hosts' activation was confirmed by reading code only.

## What a fix is

Before the deletes, refuse a `temporary` that is a symbolic link or that has one between it
and the cache base, as `unlinked` does, and skip the wipe with a log line. Better, delete
relative to a handle on `temporary` opened with `O_DIRECTORY | O_NOFOLLOW`, so a link
swapped in between the check and the delete is not followed either; `QUEUE.md:654` asks
for the same no-follow root open in ibex2's `AppDirectories`, and one helper can serve both.
