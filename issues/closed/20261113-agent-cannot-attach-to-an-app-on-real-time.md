# The agent cannot attach to an app on real time

**Status:** Closed
**Resolution:** A signalled snapshot of a normally launched development app includes tree, state, layout and stamps; --live launches on real time. A bounded perf frames live read of an arbitrary already-running process is deliberately not built: no new carrier, per the maintainer’s approved panel decision.
**Systems:** scripts/agent.mjs, scripts/agent-launch.mjs, host/apple/Sources/ExactKit/Agent.swift
**Severity:** P3
**Author:** Claude (Opus 5.5), from Exact diaries sent 2026-11-11 to 2026-11-13
**Date:** 2026-11-13
**Related:** issues/closed/20261022-agent-cannot-drag-to-dismiss-the-keyboard-or-film-real-time.md

## What is wrong

The driver launches the app it drives and holds its clock from the first frame. It cannot
attach to an app that is already running on the platform's clock. Timing work, such as the
time from a server push to the frame that shows it, needs the app on real time from its
launch, so the driver cannot be used for it.

`--timing platform` lets the platform keep the clock, and since aead4f85 an input lands
inside a live frame capture (`perf frames capture start`, then `perf frames health during
"<op>"`; `docs/contract-for-agents.md`, the frame health section). But the session is still
one the driver launched, with its storage and secret presence substituted
(`docs/reference.md:1426-1436`). Only `trace --pid` reaches a running app
(`scripts/agent-launch.mjs:591`), and it reads a trace, not `tree` or `state`.

## The failure

Exact `9e53578f`, macOS: a builder measuring push-to-frame latency asked for the driver to
attach to an app running on real time and read `tree` and `state` without holding its clock.
They steered the app through a side channel of their own and read its state through requests
it made.

## Fix

A read-only attach (`tree`, `state`, `layout`, `logs`, `perf frames live`) to a running
development build that leaves its clock alone, reached as `trace --pid` reaches one.

## Qualification

A normally launched macOS development Caltrain process returned two signalled
snapshots with 282 tree nodes, state and mounted layout, stayed alive after each,
and advanced its clock from 497 ms to 1826 ms. Both files report real clock, normal
scheduling, real storage, no substitutions and passive reads. The --live drive
accepts native taps and typing and reports its chosen scratch store and substitutions.
FrameSamplerTests passes 35 tests; the driver and help suites pass 68.

With `--live` and `EXACT_STORE=real`, the secret and preference stores are real;
app files use the configured scratch tree and are unavailable when none is configured.
State, trace and live-frame observations report the app-file policy from the same
Rust decision that supplies its roots. Both live configurations are covered on macOS.

A bounded `perf frames live` read of an arbitrary already-running process is
deliberately not built. The maintainer’s approved panel decision admits no new
carrier; `--live` launches a process rather than attaching that observation to one.
