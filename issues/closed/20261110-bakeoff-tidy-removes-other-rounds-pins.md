# A bake-off round's tidy deletes the pinned exact2 checkout of every other round, running or not

**Status:** Closed
**Resolution:** A round marks its pin as in use (`~/bakeoff/locks/pin-<rev>-<pid>`) from before the checkout is made until its process exits, and `tidy` keeps every pin a live process has marked, discarding the marks of dead ones.
**Systems:** bench/bakeoff/round.mjs
**Severity:** P3
**Author:** Claude (Fable 5.2), from the 2026-11-10 project review
**Date:** 2026-11-10

`bench/bakeoff/round.mjs` at `dd6f5dcd` pins exact2 for a round as a detached worktree, `~/bakeoff/pins/<rev>` (`pinCheckout`, lines 58-70). The Exact arm of every pair builds against it by path for the whole build phase: `build.mjs:29` runs `<pin>/scripts/exact.mjs new`, and the app that makes uses that checkout.

`tidy()` (lines 144-154) keeps one pin, the one named in this round's `pin.txt`, and removes all the others:

```js
for (const rev of existsSync(pins) ? readdirSync(pins) : []) {
  if (rev === own) continue;
  if (spawnSync('git', ['-C', REPO, 'worktree', 'remove', '--force', resolve(pins, rev)]).status !== 0) rmSync(resolve(pins, rev), { recursive: true, force: true });
}
```

It runs at the end of every `all` round (line 160) and does not ask whether another round is using a pin.

Rounds do overlap. `hold()` (lines 23-38) exists "so two rounds never drive one simulator". `bench/bakeoff/results/r31/round.log` shows two `round.mjs` processes building at once on different commits: line 1 is "build: 15 pairs on 82fdd765d4", and lines 18-20 are "build: 1 pairs on 943f592a6a" followed by two "waiting for …, held by PID 2377".

## The failure

Round A is in its build phase on pin `aaa`. Round B, on pin `bbb`, reaches the end of `all`. B's `tidy` removes `~/bakeoff/pins/aaa`. A's Exact builders, which run for 5 to 25 minutes each (`results/r31/round.log:39,49`), lose the framework they are building against. Their apps fail or come out missing, and the round's table compares a broken Exact arm with an untouched SwiftUI or UIKit one. Nothing in A's log says why.

Expected: tidy removes only pins no running round uses.

## Verification

Ran a copy of `round.mjs` and `pairs.json`, placed outside the repository at the same relative paths, as `round.mjs B tidy`. `HOME` was a temporary directory holding round A (`pin.txt` naming `aaa`, a log that stops mid-build, a lock file with a live PID) and round B (`bbb`). A stand-in `git` first on `PATH` recorded its arguments and changed nothing.

```
before: pins = aaa bbb ; round A's lock SIM-1 held by live PID 84879
… tidied: build trees and older pins removed
… tidy done
after: pins = bbb
git -C <repo> worktree remove --force <home>/bakeoff/pins/aaa
git -C <repo> worktree prune
```

Not run: two real rounds (they need the simulators and both model CLIs).

## Direction

Remove pins only when no lock in `~/bakeoff/locks` is held by a live process, which `hold()` already knows how to tell. Or keep every pin that another round's `pin.txt` names while that round's log has no `all done` or `tidy done` line.

## Related

`report()` in the same script copies graders' scratch files into the committed results; that is a separate issue. No issue or QUEUE entry covers `tidy`.
