# The heavy list's Mac series is documented as scripts under one machine's `target/`, reached by a private address

**Status:** Closed
**Resolution:** "The measurement Mac" now says which of the series' scripts are tracked and that the wrappers which built and sequenced the arms stayed in that Mac's ignored `target/g0/`, describes what each mode does in place of the `ssh` command, and neither file names the login or a tailnet address any more (git history keeps the old text, which was not rewritten).
**Systems:** bench/heavy-list/README.md, bench/heavy-list/gpui/RESULTS.md
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

`bench/heavy-list/README.md` at `dd6f5dcd` has a section, "The measurement Mac" (lines 129-299), that tells a reader how to reproduce the four-arm Mac series behind `gpui/RESULTS.md`. Its commands cannot be run from this repository, for two reasons.

**The driver is not here.** The section's one command block (lines 248-256) opens an SSH session to the measurement Mac and runs

```sh
python3 target/g0/run-42d5066572/run-series.py qualify target/g0/run-42d5066572/qualification-repeat
```

The text around it names `run-series.py` (lines 254, 279), `run-boot.py` (271-272) and `build.sh` and `bundle.py` "under `target/g0/`" (232-233) as what holds the commands. `target/` is ignored, and no file named `run-series.py`, `run-boot.py` or `bundle.py` is tracked. What is tracked under `bench/heavy-list/gpui/` is the measuring half (`measure.m`, `probe.m`, `startup.py`, `run-hook.py`, `aggregate.mjs`). The wrapper that builds, signs and sequences the arms exists only in that machine's build directory, where a `cargo clean` removes it.

**The host is one developer's.** Lines 131 and 249 give a login and a tailnet address (in `100.64.0.0/10`, reachable only inside that network). `gpui/RESULTS.md` carries such an address seven times (lines 7868, 8551, 8758, 9440, 9684, 9953, 10303; one is a second machine's). The other benches write a placeholder: `bench/extra-heavy/README.md:104-106` uses `<host>`, and `mac/push.sh` reads `BENCH_HOST`.

## Expected

A reproduction section names files a reader has. The section itself expects reruns: "Use a fresh series directory for each later run; never replace evidence cited by `gpui/RESULTS.md`" (lines 235-236).

## Verification

Read the section. Searched the tracked file list for the script names: `git ls-files | grep -E 'run-series|run-boot|bundle\.py'` prints nothing. Counted the lines that match the tailnet address range in the two files. Nothing was connected to. Not run: the series (it needs that Mac).

## Direction

Cut the section to what `gpui/RESULTS.md` needs to be read: the machine's hardware and software, the quiet-machine bounds and the hook settings. Delete the launch commands that name untracked scripts, and move the login, the address and the `ssh` line to a place only the team reads, as the issue below directs for `docs/testing.md`. If the series is meant to be rerun by others, committing the four drivers beside the tools they drive is the owner's decision; it adds scripts.

## Related

`issues/20261110-testing-guide-names-a-private-machine-and-login.md` is the same kind of pointer in `docs/testing.md`. `QUEUE.md:203` and `host/ink/MEASUREMENTS.md:5073` each carry one more tailnet address in a measurement note. No issue or QUEUE entry covers this section.
