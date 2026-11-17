# Seeing what people see

This is the disposable first experiment of LLP 1172.021, not a new driver
operation or an extension of authored tests or `verify`. Run it explicitly:

```sh
bun scripts/exact.mjs see macos --app caltrain --out target/see-caltrain
bun scripts/exact.mjs see ios --app shelf --sim <owned-simulator-udid> --out target/see-shelf
```

## Checkpoint semantics

An **observation** is one read with its source, target identity, start and end
wall times, result or refusal, and saved raw evidence. Outside evidence comes
from AXUIElement and `screencapture` on macOS, or XCTest's accessibility snapshot
and `simctl io screenshot` on the simulator. Driver evidence keeps the driver's
plain tree, state and screenshot. The sources are never substituted for each
other when a read fails.

A **checkpoint** asserts only that these instruments agreed on the fields and
region explicitly compared during a stable observation interval. It does not
assert that the app is correct, accessible, or identical to another platform.
An input reply means delivery finished, not that presentation finished. The
driver's `clock settle` is recorded, then two outside tree/frame observations
bracket the inside reads; inside tree/state reads bracket those observations.
Changed trees, states, frame pixels, target identity or unsettled work make the
checkpoint inconclusive. A screenshot's reported motion or failed display
correlation also makes it inconclusive, even when the endpoint pixels match.
We do not retry until a convenient picture appears.

The state comparison covers `slots`, `derives`, `resources`, `pending`, `queued`,
`tasks`, `device`, `time`, `language`, `navigation`, `dialog`, `keyboard`, `focus`,
`store` and `collections`. Diagnostics such as launch times, read counters and
memory sizes stay in the raw evidence, outside the assertion. The report lists
these fields. Within a session the tree's epoch and incarnation must stay fixed.

There is no shared compositor transaction identifier in this experiment.
Matching endpoints establish sampled stability, not atomicity: a transient
between samples can be missed. Every report states this limit. Native animation,
the cursor and wall-clock content may prevent stability. The agent clock does
not pretend to control the window server or the input system.

A **blind spot** is evidence a checkpoint needs but cannot obtain or correlate:
permissions, an absent or truncated tree, an empty or uniform capture, an
ambiguous window or target, a failed command, a moving observation, or regions
whose coordinate mapping is unknown. Each is a finding with its source and
reason. It is never a pass, and makes the checkpoint inconclusive even when
other observations agree. A uniform picture may be intentional; it still
cannot witness that the intended app content was presented.

Tree comparisons report every unmatched named element in either direction and
every differing comparable field on a uniquely joined identifier. Raw roles
remain platform-specific; no web geometry oracle or guessed role equivalence
is used. Anonymous structural nodes are retained as evidence but excluded from
semantic claims. A missing identifier does not justify a guessed match: a
unique exact label may establish a weaker name-only join only when neither
element has an identifier. The join is recorded as such.
These are instrument disagreements, not automatically accessibility defects.

Pictures are compared only at the same pixel size and with an explicit region
mapping. Different extents are findings and an unaligned region is a blind
spot; the experiment does not resize, guess a crop, or hide native chrome.
On the Mac the AX main window's frame must identify exactly one window-server
window owned by the process; the largest window is not assumed to be the target.
Every changed pixel in a mapped region contributes to the reported count and
bounding box. No perceptual threshold declares an unexplained change a pass.
An iOS default screenshot that declares `pixels: display`, with dimensions and
scale matching XCTest's full-screen application frame, has an explicit mapping
to the simulator display. It is already outside pixel evidence; the experiment
does not describe it as an in-process drawing. XCTest's existing reader omits
zero-area and out-of-window elements; that coverage limit is in every report.

Reports use `agreement`, `disagreement`, or `inconclusive`; exit codes are 0, 1,
and 2 respectively. Even `agreement` covers only the listed comparisons.
Evidence and timings stay local in the output directory.

## Input comparison

`--tap <test-id>` or `--drag <test-id> <dx> <dy> <milliseconds>` runs two fresh
driver sessions with the same launch facts and setup (`--setup-tap <test-id>`,
repeatable). One dispatches through the existing driver, the other uses
CGEvent or XCTest. Both resolve the same semantic target. Platform coordinates
come from the outside tree through an exact identifier match; ambiguity or
absence refuses that leg. Labels never substitute for a missing identifier.
A drag is target-relative, requested in platform points and wall milliseconds.
The input
receipt interval is recorded: CGEvent's posting interval, or XCTest's request
interval including its waits. Neither measures the app's received event times
or establishes release velocity in the app.

Platform input requires matching outside observations across the checkpoint
and a fresh read immediately before posting. A changed or incomplete read
refuses input; the fresh read supplies the coordinates. Its source, identity,
read interval and raw result or refusal are saved even when input is refused.
Setup taps, their settlement and each drag phase must report success. A drag
releases its own accepted contact on failure, preserving the original error
if cleanup also fails. A Mac gesture reports success only after mouse-up posts.

The before checkpoints must agree on their comparable model and outside tree
before outcomes are compared. Differing initial conditions, a failed delivery,
or an inconclusive checkpoint prevents an input-equivalence claim. A matching
outcome proves no more than the sampled state and tree; it does not prove the
target received the event. Raw input receipts and pictures remain available.
The outside outcome comparison uses identifiers, labels, roles, values, enabled
and selected states; absolute window positions and keyboard focus are omitted
because relaunch and physical input can change them independently. Model focus
is also omitted between legs, while checkpoint stability still includes it.
Keyboard visibility remains part of the outcome comparison. Differing
outcomes with incomplete evidence remain provisional findings.

## Scope and removal

The experiment uses platform chrome in its own sessions, retaining the driver's
frozen clock in both input legs. Physical input still uses wall time. It
starts no simulator implicitly: iOS requires a caller-owned booted simulator.
The existing XCTest touch runner supplies the simulator's external tree and
input; there is no new iOS helper or core dependency. Accessibility clients can
activate platform accessibility machinery in the app; this is not a performance
measurement of an app without accessibility clients.

See `REMOVING.md` for the complete deletion list and its test. Step 2 is a
hand-judged corpus of disagreements and seeded misses, followed by precision
measurements for each proposed finding kind. Accepted baselines, cross-host
layout, replay and changes to `verify` remain outside this experiment.

## First qualification, 2026-11-14

Caltrain and Shelf were built and run on macOS and an isolated iPhone 18 Pro
simulator, iOS 27.0. The reports and original captures are under `target/see-*`.
Counts below describe instrument differences, not a count of app defects.

| Checkpoint | Outside versus driver evidence |
| --- | --- |
| Caltrain, iOS, initial screen | A stable interval: both driver pictures matched the simulator frame. XCTest exposed 23 elements against 288 driver nodes; 235 named model elements had no outside match. The visible Change station button was one of them, so the outside-targeted tap refused. |
| Shelf, iOS, initial screen | The same `search` identifier was named Search outside and Search title or author inside. Both tap paths opened `/add`. |
| Shelf, iOS, Add Book | 19,408 of 3,162,132 window pixels differed, but the outside frames also changed: inconclusive. The outside tree included native New Book navigation and field cells. |
| Shelf, macOS, Add Book | AX exposed an `AXSheet`; the default driver picture explicitly omitted `SheetWindow`. External window capture failed, while the driver's own window capture succeeded. |
| Caltrain, macOS | CGEvent and driver taps both opened `/stations`. A 100-point drag over 300 ms posted 18 CGEvent moves in 303 ms; both drag paths left the route unchanged. |

Shelf also accepted a real XCTest drag on `library-list`: a 100-point move
requested over 300 ms took a 995 ms XCTest request interval, against 23 ms for
the driver's recognized down/move/up. This does not measure delivered touch
velocity or scrolling inertia. Caltrain's missing AX target was not bypassed
with driver coordinates.

macOS Screen Recording preflight was false, and `screencapture` returned
“could not create image from window.” Those reads are blind spots. The final
Caltrain iOS checkpoint's two AX reads had a median of 62.5 ms and its two
display captures 840.5 ms; Shelf's sheet measured 89.5 ms and 481.5 ms. These
are instrument timings on a shared Mac, not app performance measurements.

The experiment earns keeping as an observer: it found a visible control missing
from the outside tree. Whole-tree equality is too noisy for a gate: structural
nodes, off-screen content, native menus, duplicate names and per-launch native
identifiers produce many differences. Step 2 should judge a small corpus of
these cases before changing driver verdicts. The LLP's starting inventory also
needs narrowing: Mac `screenshot window` already uses window-server pixels
(with canvas substitutions), and both tested iOS screenshot forms reported
`pixels: display`.
